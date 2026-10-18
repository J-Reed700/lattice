//! Saved passages: the `passage_references` rows the references feature
//! stores and a weekly journal synthesis reads.

#[derive(Debug, Clone)]
pub struct PassageReferenceRecord {
    pub id: String,
    pub document_id: String,
    pub chunk_id: Option<String>,
    pub file_path: String,
    pub file_name: String,
    pub locator: Option<String>,
    pub text: String,
    pub title: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}
