use std::collections::HashMap;

use crate::state::request::RequestDraft;

use super::ImportedOperation;

#[derive(Debug, Clone, Default)]
pub struct MergePreview {
    pub new_count: usize,
    pub updated_count: usize,
    pub unchanged_count: usize,
}

/// Three-way merge: update spec-managed fields, preserve user-owned fields.
///
/// Spec-managed: name, folder, url, query_params, import_key.
/// User-owned: auth, headers, body, attach_oauth.
/// Hand-crafted requests (no import_key) are always appended unchanged.
pub fn compute_merge(
    existing: &[RequestDraft],
    incoming: &[ImportedOperation],
) -> (Vec<RequestDraft>, MergePreview) {
    let mut index: HashMap<&str, &RequestDraft> = HashMap::new();
    for req in existing {
        if let Some(key) = req.import_key.as_deref() {
            index.insert(key, req);
        }
    }

    let mut result: Vec<RequestDraft> = Vec::new();
    let mut preview = MergePreview::default();

    for op in incoming {
        if let Some(existing_req) = index.get(op.import_key.as_str()) {
            let mut draft = (*existing_req).clone();
            let before = draft.clone();

            draft.name = op.name.clone();
            draft.folder = op.folder.clone();
            draft.url = op.url.clone();
            draft.query_params = op.query_params.clone();
            draft.import_key = Some(op.import_key.clone());

            if draft == before {
                preview.unchanged_count += 1;
            } else {
                preview.updated_count += 1;
            }
            result.push(draft);
        } else {
            result.push(draft_from_operation(op));
            preview.new_count += 1;
        }
    }

    // Always preserve hand-crafted requests (no import_key).
    for req in existing {
        if req.import_key.is_none() {
            result.push(req.clone());
        }
    }

    (result, preview)
}

fn draft_from_operation(op: &ImportedOperation) -> RequestDraft {
    RequestDraft {
        name: op.name.clone(),
        folder: op.folder.clone(),
        method: op.method.clone(),
        url: op.url.clone(),
        query_params: op.query_params.clone(),
        auth: op.auth_hint.clone().unwrap_or_default(),
        headers: Vec::new(),
        body: op.body_example.clone(),
        attach_oauth: true,
        import_key: Some(op.import_key.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::request::{RequestAuth, RequestDraft};

    fn make_op(key: &str, name: &str, folder: &str, url: &str) -> ImportedOperation {
        ImportedOperation {
            import_key: key.to_owned(),
            name: name.to_owned(),
            folder: folder.to_owned(),
            method: key.split(':').next().unwrap_or("GET").to_owned(),
            url: url.to_owned(),
            query_params: Vec::new(),
            auth_hint: None,
            body_example: None,
        }
    }

    fn make_req(key: &str, name: &str) -> RequestDraft {
        RequestDraft {
            name: name.to_owned(),
            folder: "pets".to_owned(),
            method: key.split(':').next().unwrap_or("GET").to_owned(),
            url: "https://example.com/pet".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::None,
            headers: Vec::new(),
            body: None,
            attach_oauth: true,
            import_key: Some(key.to_owned()),
        }
    }

    #[test]
    fn all_new_when_no_existing() {
        let ops = vec![
            make_op("GET:/pet", "findPets", "pet", "https://x.com/pet"),
            make_op("POST:/pet", "addPet", "pet", "https://x.com/pet"),
        ];
        let (result, preview) = compute_merge(&[], &ops);
        assert_eq!(preview.new_count, 2);
        assert_eq!(preview.updated_count, 0);
        assert_eq!(preview.unchanged_count, 0);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].import_key.as_deref(), Some("GET:/pet"));
    }

    #[test]
    fn unchanged_when_spec_same() {
        let existing = vec![RequestDraft {
            name: "findPets".to_owned(),
            folder: "pet".to_owned(),
            method: "GET".to_owned(),
            url: "https://x.com/pet".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::None,
            headers: Vec::new(),
            body: None,
            attach_oauth: true,
            import_key: Some("GET:/pet".to_owned()),
        }];
        let ops = vec![make_op("GET:/pet", "findPets", "pet", "https://x.com/pet")];
        let (_, preview) = compute_merge(&existing, &ops);
        assert_eq!(preview.unchanged_count, 1);
        assert_eq!(preview.updated_count, 0);
        assert_eq!(preview.new_count, 0);
    }

    #[test]
    fn updated_when_url_changes() {
        let existing = vec![make_req("GET:/pet", "findPets")];
        let ops = vec![make_op("GET:/pet", "findPets", "pet", "https://NEW.com/pet")];
        let (result, preview) = compute_merge(&existing, &ops);
        assert_eq!(preview.updated_count, 1);
        assert_eq!(result[0].url, "https://NEW.com/pet");
    }

    #[test]
    fn user_auth_preserved_on_update() {
        let mut existing = vec![make_req("GET:/pet", "findPets")];
        existing[0].auth = RequestAuth::Bearer {
            token: "my-secret".to_owned(),
        };
        let ops = vec![make_op("GET:/pet", "findPets Renamed", "pet", "https://x.com/pet")];
        let (result, _) = compute_merge(&existing, &ops);
        assert_eq!(result[0].name, "findPets Renamed");
        assert_eq!(
            result[0].auth,
            RequestAuth::Bearer {
                token: "my-secret".to_owned()
            }
        );
    }

    #[test]
    fn hand_crafted_requests_always_preserved() {
        let handcrafted = RequestDraft {
            name: "My custom request".to_owned(),
            folder: String::new(),
            method: "GET".to_owned(),
            url: "https://custom.example.com".to_owned(),
            query_params: Vec::new(),
            auth: RequestAuth::None,
            headers: Vec::new(),
            body: None,
            attach_oauth: true,
            import_key: None,
        };
        let existing = vec![make_req("GET:/pet", "findPets"), handcrafted.clone()];
        let ops = vec![make_op("GET:/pet", "findPets", "pet", "https://x.com/pet")];
        let (result, _) = compute_merge(&existing, &ops);
        assert!(result.iter().any(|r| r.name == "My custom request"));
    }

    #[test]
    fn idempotent_on_second_import() {
        let ops = vec![
            make_op("GET:/pet", "findPets", "pet", "https://x.com/pet"),
            make_op("POST:/pet", "addPet", "pet", "https://x.com/pet"),
        ];
        let (first, _) = compute_merge(&[], &ops);
        let (second, preview) = compute_merge(&first, &ops);
        assert_eq!(preview.new_count, 0);
        assert_eq!(preview.updated_count, 0);
        assert_eq!(preview.unchanged_count, 2);
        assert_eq!(second.len(), first.len());
    }
}
