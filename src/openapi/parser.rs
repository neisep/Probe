use openapiv3::{OpenAPI, Parameter, ReferenceOr, SecurityScheme};
use serde::Deserialize;
use serde_json::Value;

use crate::state::request::{ApiKeyLocation, RequestAuth};

use super::{ImportedOperation, OpenApiError};

pub fn parse_spec(text: &str) -> Result<Vec<ImportedOperation>, OpenApiError> {
    let raw: Value = if text.trim_start().starts_with('{') {
        serde_json::from_str(text).map_err(OpenApiError::Json)?
    } else {
        serde_yaml::from_str(text).map_err(OpenApiError::Yaml)?
    };

    if raw.get("swagger").and_then(Value::as_str).is_some() {
        parse_swagger2(&raw)
    } else if raw.get("openapi").and_then(Value::as_str).is_some() {
        let spec: OpenAPI = serde_json::from_value(raw).map_err(OpenApiError::Json)?;
        parse_openapi3(&spec)
    } else {
        Err(OpenApiError::UnsupportedVersion("unknown".to_owned()))
    }
}

// ── OpenAPI 3.x ─────────────────────────────────────────────────────────────

fn parse_openapi3(spec: &OpenAPI) -> Result<Vec<ImportedOperation>, OpenApiError> {
    let base_url = spec
        .servers
        .first()
        .map(|s| s.url.trim_end_matches('/').to_owned())
        .unwrap_or_default();

    let title = spec.info.title.as_str();

    let mut ops = Vec::new();

    for (path, path_item) in spec.paths.iter() {
        let path_item = match path_item {
            ReferenceOr::Item(item) => item,
            ReferenceOr::Reference { .. } => continue,
        };

        for (method, operation) in path_item.iter() {
            let import_key = format!("{}:{}", method.to_uppercase(), path);

            let name = operation
                .operation_id
                .clone()
                .unwrap_or_else(|| format!("{} {}", method.to_uppercase(), path));

            let folder = operation
                .tags
                .first()
                .cloned()
                .unwrap_or_else(|| title.to_owned());

            let url = format!("{}{}", base_url, path);

            let mut seen = std::collections::HashSet::new();
            let query_params: Vec<(String, String)> = operation
                .parameters
                .iter()
                .chain(path_item.parameters.iter())
                .filter_map(|p| match p {
                    ReferenceOr::Item(Parameter::Query { parameter_data, .. }) => {
                        Some((parameter_data.name.clone(), String::new()))
                    }
                    _ => None,
                })
                .filter(|(k, _)| seen.insert(k.clone()))
                .collect();

            let auth_hint = resolve_auth_hint3(operation, spec);

            let body_example = extract_body_example3(operation, spec);

            ops.push(ImportedOperation {
                import_key,
                name,
                folder,
                method: method.to_uppercase(),
                url,
                query_params,
                auth_hint,
                body_example,
            });
        }
    }

    Ok(ops)
}

fn resolve_auth_hint3(
    operation: &openapiv3::Operation,
    spec: &OpenAPI,
) -> Option<RequestAuth> {
    let schemes = spec.components.as_ref()?.security_schemes.clone();

    let requirements = operation
        .security
        .as_deref()
        .or_else(|| spec.security.as_deref())?;

    for requirement in requirements {
        for scheme_name in requirement.keys() {
            if let Some(ReferenceOr::Item(scheme)) = schemes.get(scheme_name) {
                if let Some(auth) = security_scheme_to_auth(scheme) {
                    return Some(auth);
                }
            }
        }
    }
    None
}

fn security_scheme_to_auth(scheme: &SecurityScheme) -> Option<RequestAuth> {
    match scheme {
        SecurityScheme::HTTP { scheme, .. } => match scheme.to_lowercase().as_str() {
            "bearer" => Some(RequestAuth::Bearer {
                token: String::new(),
            }),
            "basic" => Some(RequestAuth::Basic {
                username: String::new(),
                password: String::new(),
            }),
            _ => None,
        },
        SecurityScheme::APIKey { location, name, .. } => {
            let loc = match location {
                openapiv3::APIKeyLocation::Header => ApiKeyLocation::Header,
                openapiv3::APIKeyLocation::Query => ApiKeyLocation::Query,
                openapiv3::APIKeyLocation::Cookie => ApiKeyLocation::Header,
            };
            Some(RequestAuth::ApiKey {
                location: loc,
                name: name.clone(),
                value: String::new(),
            })
        }
        SecurityScheme::OAuth2 { .. } | SecurityScheme::OpenIDConnect { .. } => {
            Some(RequestAuth::Bearer {
                token: String::new(),
            })
        }
    }
}

fn extract_body_example3(
    operation: &openapiv3::Operation,
    spec: &OpenAPI,
) -> Option<String> {
    let rb = match operation.request_body.as_ref()? {
        ReferenceOr::Item(rb) => rb,
        ReferenceOr::Reference { reference } => {
            let name = reference.strip_prefix("#/components/requestBodies/")?;
            match spec.components.as_ref()?.request_bodies.get(name)? {
                ReferenceOr::Item(rb) => rb,
                _ => return None,
            }
        }
    };

    let media = rb.content.get("application/json")?;
    if let Some(ex) = media.examples.values().next() {
        if let ReferenceOr::Item(ex) = ex {
            if let Some(val) = &ex.value {
                return serde_json::to_string_pretty(val).ok();
            }
        }
    }
    if let Some(schema_ref) = &media.schema {
        if let ReferenceOr::Item(schema) = schema_ref {
            if let Some(ex) = &schema.schema_data.example {
                return serde_json::to_string_pretty(ex).ok();
            }
        }
    }
    None
}

// ── Swagger 2.x ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Swagger2 {
    host: Option<String>,
    #[serde(rename = "basePath")]
    base_path: Option<String>,
    schemes: Option<Vec<String>>,
    info: Swagger2Info,
    paths: std::collections::BTreeMap<String, Swagger2PathItem>,
    #[serde(rename = "securityDefinitions")]
    security_definitions: Option<std::collections::BTreeMap<String, Swagger2SecurityDef>>,
}

#[derive(Debug, Deserialize)]
struct Swagger2Info {
    title: String,
}

#[derive(Debug, Deserialize, Default)]
struct Swagger2PathItem {
    parameters: Option<Vec<Swagger2Parameter>>,
    get: Option<Swagger2Operation>,
    post: Option<Swagger2Operation>,
    put: Option<Swagger2Operation>,
    patch: Option<Swagger2Operation>,
    delete: Option<Swagger2Operation>,
    head: Option<Swagger2Operation>,
    options: Option<Swagger2Operation>,
}

#[derive(Debug, Deserialize)]
struct Swagger2Operation {
    #[serde(rename = "operationId")]
    operation_id: Option<String>,
    tags: Option<Vec<String>>,
    parameters: Option<Vec<Swagger2Parameter>>,
    security: Option<Vec<std::collections::BTreeMap<String, Value>>>,
}

#[derive(Debug, Deserialize)]
struct Swagger2Parameter {
    #[serde(rename = "in")]
    location: String,
    name: String,
    schema: Option<Swagger2Schema>,
}

#[derive(Debug, Deserialize)]
struct Swagger2Schema {
    example: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Swagger2SecurityDef {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "in")]
    location: Option<String>,
    name: Option<String>,
}

fn parse_swagger2(raw: &Value) -> Result<Vec<ImportedOperation>, OpenApiError> {
    let spec: Swagger2 = serde_json::from_value(raw.clone()).map_err(OpenApiError::Json)?;

    let scheme = spec
        .schemes
        .as_ref()
        .and_then(|s| s.first())
        .map(|s| s.as_str())
        .unwrap_or("https");
    let host = spec.host.as_deref().unwrap_or("localhost");
    let base_path = spec
        .base_path
        .as_deref()
        .map(|p| p.trim_end_matches('/'))
        .unwrap_or("");
    let base_url = format!("{scheme}://{host}{base_path}");

    let title = &spec.info.title;

    let mut ops = Vec::new();

    for (path, path_item) in &spec.paths {
        let methods: &[(&str, Option<&Swagger2Operation>)] = &[
            ("GET", path_item.get.as_ref()),
            ("POST", path_item.post.as_ref()),
            ("PUT", path_item.put.as_ref()),
            ("PATCH", path_item.patch.as_ref()),
            ("DELETE", path_item.delete.as_ref()),
            ("HEAD", path_item.head.as_ref()),
            ("OPTIONS", path_item.options.as_ref()),
        ];

        for (method, maybe_op) in methods {
            let Some(operation) = maybe_op else { continue };

            let import_key = format!("{}:{}", method, path);

            let name = operation
                .operation_id
                .clone()
                .unwrap_or_else(|| format!("{} {}", method, path));

            let folder = operation
                .tags
                .as_ref()
                .and_then(|t| t.first())
                .cloned()
                .unwrap_or_else(|| title.clone());

            let url = format!("{}{}", base_url, path);

            let path_level = path_item.parameters.as_deref().unwrap_or(&[]);
            let op_level = operation.parameters.as_deref().unwrap_or(&[]);
            let mut seen = std::collections::HashSet::new();
            let query_params: Vec<(String, String)> = op_level
                .iter()
                .chain(path_level.iter())
                .filter(|p| p.location == "query")
                .filter_map(|p| seen.insert(p.name.clone()).then(|| (p.name.clone(), String::new())))
                .collect();

            let auth_hint =
                resolve_auth_hint2(operation, spec.security_definitions.as_ref());

            ops.push(ImportedOperation {
                import_key,
                name,
                folder,
                method: method.to_string(),
                url,
                query_params,
                auth_hint,
                body_example: extract_body_example2(operation),
            });
        }
    }

    Ok(ops)
}

fn extract_body_example2(operation: &Swagger2Operation) -> Option<String> {
    let params = operation.parameters.as_deref()?;
    let body = params.iter().find(|p| p.location == "body")?;
    let example = body.schema.as_ref()?.example.as_ref()?;
    serde_json::to_string_pretty(example).ok()
}

fn resolve_auth_hint2(
    operation: &Swagger2Operation,
    defs: Option<&std::collections::BTreeMap<String, Swagger2SecurityDef>>,
) -> Option<RequestAuth> {
    let defs = defs?;
    let requirements = operation.security.as_deref()?;

    for requirement in requirements {
        for scheme_name in requirement.keys() {
            if let Some(def) = defs.get(scheme_name) {
                if let Some(auth) = swagger2_def_to_auth(def) {
                    return Some(auth);
                }
            }
        }
    }
    None
}

fn swagger2_def_to_auth(def: &Swagger2SecurityDef) -> Option<RequestAuth> {
    match def.kind.as_str() {
        "basic" => Some(RequestAuth::Basic {
            username: String::new(),
            password: String::new(),
        }),
        "apiKey" => {
            let loc = match def.location.as_deref().unwrap_or("header") {
                "query" => ApiKeyLocation::Query,
                _ => ApiKeyLocation::Header,
            };
            Some(RequestAuth::ApiKey {
                location: loc,
                name: def.name.clone().unwrap_or_default(),
                value: String::new(),
            })
        }
        "oauth2" => Some(RequestAuth::Bearer {
            token: String::new(),
        }),
        _ => None,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const PETSTORE_3: &str = r#"{
  "openapi": "3.0.0",
  "info": { "title": "Petstore", "version": "1.0.0" },
  "servers": [{ "url": "https://petstore3.swagger.io/api/v3" }],
  "paths": {
    "/pet": {
      "post": {
        "tags": ["pet"],
        "operationId": "addPet",
        "parameters": [],
        "requestBody": {
          "content": {
            "application/json": {
              "schema": { "type": "object" }
            }
          }
        },
        "responses": {}
      },
      "get": {
        "tags": ["pet"],
        "operationId": "findPets",
        "parameters": [
          { "name": "status", "in": "query", "schema": { "type": "string" } }
        ],
        "responses": {}
      }
    },
    "/pet/{petId}": {
      "get": {
        "tags": ["pet"],
        "operationId": "getPetById",
        "parameters": [
          { "name": "petId", "in": "path", "schema": { "type": "integer" } }
        ],
        "responses": {}
      },
      "delete": {
        "tags": ["pet"],
        "operationId": "deletePet",
        "parameters": [],
        "responses": {}
      }
    }
  }
}"#;

    const PETSTORE_2: &str = r#"{
  "swagger": "2.0",
  "info": { "title": "Petstore", "version": "1.0.0" },
  "host": "petstore.swagger.io",
  "basePath": "/v2",
  "schemes": ["https"],
  "paths": {
    "/pet": {
      "post": {
        "tags": ["pet"],
        "operationId": "addPet",
        "parameters": [],
        "responses": {}
      }
    },
    "/pet/findByStatus": {
      "get": {
        "tags": ["pet"],
        "operationId": "findPetsByStatus",
        "parameters": [
          { "name": "status", "in": "query", "type": "string" }
        ],
        "responses": {}
      }
    }
  }
}"#;

    #[test]
    fn parses_openapi3_petstore() {
        let ops = parse_spec(PETSTORE_3).expect("parse");
        assert_eq!(ops.len(), 4);

        let get_pet = ops.iter().find(|o| o.import_key == "GET:/pet").expect("GET:/pet");
        assert_eq!(get_pet.name, "findPets");
        assert_eq!(get_pet.folder, "pet");
        assert_eq!(get_pet.url, "https://petstore3.swagger.io/api/v3/pet");
        assert_eq!(get_pet.query_params, vec![("status".to_owned(), String::new())]);

        let post_pet = ops.iter().find(|o| o.import_key == "POST:/pet").expect("POST:/pet");
        assert_eq!(post_pet.name, "addPet");

        let get_by_id = ops.iter().find(|o| o.import_key == "GET:/pet/{petId}").expect("GET by id");
        assert_eq!(get_by_id.name, "getPetById");
        assert!(get_by_id.query_params.is_empty(), "path param must not appear in query_params");
    }

    #[test]
    fn parses_swagger2_petstore() {
        let ops = parse_spec(PETSTORE_2).expect("parse");
        assert_eq!(ops.len(), 2);

        let post = ops.iter().find(|o| o.import_key == "POST:/pet").expect("POST:/pet");
        assert_eq!(post.url, "https://petstore.swagger.io/v2/pet");
        assert_eq!(post.folder, "pet");

        let get = ops.iter().find(|o| o.import_key == "GET:/pet/findByStatus").expect("GET status");
        assert_eq!(get.query_params, vec![("status".to_owned(), String::new())]);
    }

    #[test]
    fn parses_yaml_format() {
        let yaml = r#"
openapi: "3.0.0"
info:
  title: YamlAPI
  version: "1"
servers:
  - url: https://api.example.com
paths:
  /health:
    get:
      operationId: getHealth
      parameters: []
      responses: {}
"#;
        let ops = parse_spec(yaml).expect("parse yaml");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].import_key, "GET:/health");
        assert_eq!(ops[0].url, "https://api.example.com/health");
    }

    #[test]
    fn path_level_params_merged_openapi3() {
        let spec = r#"{
  "openapi": "3.0.0",
  "info": { "title": "T", "version": "1" },
  "servers": [{ "url": "https://api.example.com" }],
  "paths": {
    "/search": {
      "parameters": [
        { "name": "format", "in": "query", "schema": { "type": "string" } }
      ],
      "get": {
        "operationId": "searchGet",
        "parameters": [
          { "name": "q", "in": "query", "schema": { "type": "string" } }
        ],
        "responses": {}
      },
      "post": {
        "operationId": "searchPost",
        "parameters": [],
        "responses": {}
      }
    }
  }
}"#;
        let ops = parse_spec(spec).expect("parse");
        let get = ops.iter().find(|o| o.import_key == "GET:/search").expect("GET");
        assert!(get.query_params.iter().any(|(k, _)| k == "q"), "op-level param missing");
        assert!(get.query_params.iter().any(|(k, _)| k == "format"), "path-level param missing");
        let post = ops.iter().find(|o| o.import_key == "POST:/search").expect("POST");
        assert!(post.query_params.iter().any(|(k, _)| k == "format"), "path-level param missing on POST");
    }

    #[test]
    fn path_level_params_merged_swagger2() {
        let spec = r#"{
  "swagger": "2.0",
  "info": { "title": "T", "version": "1" },
  "host": "api.example.com",
  "paths": {
    "/search": {
      "parameters": [
        { "name": "format", "in": "query", "type": "string" }
      ],
      "get": {
        "operationId": "searchGet",
        "parameters": [
          { "name": "q", "in": "query", "type": "string" }
        ],
        "responses": {}
      },
      "post": {
        "operationId": "searchPost",
        "parameters": [],
        "responses": {}
      }
    }
  }
}"#;
        let ops = parse_spec(spec).expect("parse");
        let get = ops.iter().find(|o| o.import_key == "GET:/search").expect("GET");
        assert!(get.query_params.iter().any(|(k, _)| k == "q"), "op-level param missing");
        assert!(get.query_params.iter().any(|(k, _)| k == "format"), "path-level param missing");
        let post = ops.iter().find(|o| o.import_key == "POST:/search").expect("POST");
        assert!(post.query_params.iter().any(|(k, _)| k == "format"), "path-level param missing on POST");
    }

    #[test]
    fn import_keys_use_uppercase_method() {
        let ops = parse_spec(PETSTORE_3).expect("parse");
        for op in &ops {
            assert_eq!(op.method, op.method.to_uppercase(), "method must be uppercase");
            assert!(
                op.import_key.starts_with(&op.method),
                "import_key must start with method"
            );
        }
    }
}
