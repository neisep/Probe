use std::collections::{HashMap, HashSet};

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
    let mut incoming_index: HashMap<&str, &ImportedOperation> = HashMap::new();
    for op in incoming {
        incoming_index.insert(op.import_key.as_str(), op);
    }

    let mut result: Vec<RequestDraft> = Vec::new();
    let mut preview = MergePreview::default();
    let mut seen_keys: HashSet<&str> = HashSet::new();

    // Pass 1: walk existing in user order — update spec-managed, preserve hand-crafted.
    for req in existing {
        if let Some(key) = req.import_key.as_deref() {
            if let Some(op) = incoming_index.get(key) {
                let mut draft = req.clone();
                let before = draft.clone();

                draft.name = op.name.clone();
                draft.folder = op.folder.clone();
                draft.url = op.url.clone();
                draft.query_params = merge_query_params(&draft.query_params, &op.query_params);
                draft.import_key = Some(op.import_key.clone());

                if draft == before {
                    preview.unchanged_count += 1;
                } else {
                    preview.updated_count += 1;
                }
                result.push(draft);
                seen_keys.insert(key);
            }
            // Spec-managed request removed from spec: silently drop it.
        } else {
            result.push(req.clone());
        }
    }

    // Pass 2: append operations that are new to this spec (in spec order).
    for op in incoming {
        if !seen_keys.contains(op.import_key.as_str()) {
            result.push(draft_from_operation(op));
            preview.new_count += 1;
        }
    }

    (result, preview)
}

/// Merge spec-defined query params with the user's existing ones.
/// Spec params keep any user-filled value; user-added params (not in spec) are appended.
fn merge_query_params(
    existing: &[(String, String)],
    incoming: &[(String, String)],
) -> Vec<(String, String)> {
    let spec_keys: HashSet<&str> = incoming.iter().map(|(k, _)| k.as_str()).collect();
    let mut result: Vec<(String, String)> = incoming
        .iter()
        .map(|(k, _)| {
            let user_value = existing
                .iter()
                .find(|(ek, _)| ek == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            (k.clone(), user_value)
        })
        .collect();
    for (k, v) in existing {
        if !spec_keys.contains(k.as_str()) {
            result.push((k.clone(), v.clone()));
        }
    }
    result
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
    fn user_query_values_and_custom_params_preserved_on_update() {
        let mut op = make_op("GET:/pets", "listPets", "pets", "https://x.com/pets");
        op.query_params = vec![
            ("limit".to_owned(), String::new()),
            ("status".to_owned(), String::new()),
        ];
        let existing = vec![RequestDraft {
            name: "listPets".to_owned(),
            folder: "pets".to_owned(),
            method: "GET".to_owned(),
            url: "https://x.com/pets".to_owned(),
            query_params: vec![
                ("limit".to_owned(), "20".to_owned()),   // user-filled spec param
                ("x-debug".to_owned(), "true".to_owned()), // user-added custom param
            ],
            auth: RequestAuth::None,
            headers: Vec::new(),
            body: None,
            attach_oauth: true,
            import_key: Some("GET:/pets".to_owned()),
        }];

        let (result, _) = compute_merge(&existing, &[op]);
        let params = &result[0].query_params;

        // spec param "limit": user's value preserved
        assert_eq!(params.iter().find(|(k, _)| k == "limit").map(|(_, v)| v.as_str()), Some("20"));
        // spec param "status": new, gets empty value
        assert_eq!(params.iter().find(|(k, _)| k == "status").map(|(_, v)| v.as_str()), Some(""));
        // user-added param "x-debug": preserved
        assert_eq!(params.iter().find(|(k, _)| k == "x-debug").map(|(_, v)| v.as_str()), Some("true"));
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
