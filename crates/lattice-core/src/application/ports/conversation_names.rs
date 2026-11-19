//! The names a conversation has given the things it showed its model.

use crate::application::services::model_contract::shown::Record;
use crate::shared::error::Result;
use async_trait::async_trait;

#[async_trait]
pub trait ConversationNames: Send + Sync {
    /// The conversation's names, in the order they were given.
    async fn names(&self, conversation_id: &str) -> Result<Vec<Record>>;

    /// Keeps `fresh`, which are the names given from position `first` on. A
    /// position already written is an error: a conversation has one live scope.
    async fn keep(&self, conversation_id: &str, first: usize, fresh: &[Record]) -> Result<()>;
}
