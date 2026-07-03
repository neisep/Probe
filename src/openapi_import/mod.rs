use crate::openapi::{ImportedOperation, MergePreview};

pub struct PendingOpenApiImport {
    pub source: String,
    pub preview: MergePreview,
    pub ops: Vec<ImportedOperation>,
}
