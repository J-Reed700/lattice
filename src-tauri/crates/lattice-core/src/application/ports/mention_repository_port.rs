use crate::shared::error::AppError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionData {
    pub id: String,
    pub name: String,
    pub mention_type: String,
    pub metadata: Option<String>,
    pub created_at: String,
}

impl MentionData {
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentionWithContextData {
    pub mention: MentionData,
    pub document_id: String,
    pub context: Option<String>,
    pub position: Option<i64>,
}

#[async_trait]
pub trait MentionRepositoryPort: Send + Sync {
    async fn create_mention(
        &self,
        name: &str,
        mention_type: &str,
        metadata: Option<&str>,
    ) -> Result<MentionData, AppError>;

    async fn find_mention_by_name(&self, name: &str) -> Result<Option<MentionData>, AppError>;

    async fn search_mentions(&self, query: &str, limit: i64) -> Result<Vec<MentionData>, AppError>;

    async fn get_mentions_by_type(&self, mention_type: &str) -> Result<Vec<MentionData>, AppError>;

    async fn get_mentions_for_document(
        &self,
        document_id: &str,
    ) -> Result<Vec<MentionWithContextData>, AppError>;

    async fn get_documents_with_mention(&self, mention_id: &str) -> Result<Vec<String>, AppError>;

    async fn extract_and_store_mentions(
        &self,
        document_id: &str,
        text: &str,
    ) -> Result<Vec<MentionWithContextData>, AppError>;

    async fn update_mention(
        &self,
        id: &str,
        mention_type: Option<&str>,
        metadata: Option<&str>,
    ) -> Result<MentionData, AppError>;

    async fn delete_mention(&self, id: &str) -> Result<(), AppError>;
}
