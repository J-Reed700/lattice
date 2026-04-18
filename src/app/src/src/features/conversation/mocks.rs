//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use super::trait_def::ConversationServiceTrait;
#[cfg(test)]
use crate::shared::error::Result;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, Mutex, RwLock};

#[cfg(test)]
/// Mock conversation service for testing
///
/// Simulates conversation management without database.
/// Allows configuration and tracking of conversation operations.
/// All operations are deterministic and thread-safe.
pub struct MockConversationService {
    conversations: Arc<
        RwLock<
            std::collections::HashMap<String, crate::domain::conversation::ConversationAggregate>,
        >,
    >,
}

#[cfg(test)]
impl MockConversationService {
    /// Create new mock service with empty state
    pub fn new() -> Self {
        Self {
            conversations: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Clear all conversations
    pub fn clear(&self) {
        self.conversations.write().unwrap().clear();
    }

    /// Get number of conversations
    pub fn count(&self) -> usize {
        self.conversations.read().unwrap().len()
    }
}

#[cfg(test)]
impl Default for MockConversationService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl ConversationServiceTrait for MockConversationService {
    async fn create_conversation(
        &self,
        title: String,
        model_name: String,
        system_prompt: Option<String>,
    ) -> Result<crate::domain::conversation::Conversation> {
        use crate::domain::conversation::ConversationAggregate;

        let aggregate = ConversationAggregate::new(title, model_name, system_prompt)?;
        let conversation = aggregate.conversation().clone();

        self.conversations
            .write()
            .unwrap()
            .insert(conversation.id.to_string(), aggregate);

        Ok(conversation)
    }

    async fn get_conversation(
        &self,
        id: &str,
    ) -> Result<Option<crate::domain::conversation::ConversationAggregate>> {
        Ok(self.conversations.read().unwrap().get(id).cloned())
    }

    async fn list_conversations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<crate::domain::conversation::Conversation>> {
        let conversations = self.conversations.read().unwrap();
        let mut all: Vec<_> = conversations
            .values()
            .map(|agg| agg.conversation().clone())
            .collect();

        // Sort by updated_at descending (most recent first)
        all.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        let offset = offset.unwrap_or(0) as usize;
        let limit = limit.map(|l| l as usize);

        let result = all
            .into_iter()
            .skip(offset)
            .take(limit.unwrap_or(usize::MAX))
            .collect();

        Ok(result)
    }

    async fn delete_conversation(&self, id: &str) -> Result<()> {
        self.conversations
            .write()
            .unwrap()
            .remove(id)
            .ok_or_else(|| {
                crate::error::AppError::NotFound(format!("Conversation not found: {}", id))
            })?;
        Ok(())
    }

    async fn rename_conversation(&self, id: &str, new_title: String) -> Result<()> {
        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Conversation not found: {}", id))
        })?;

        // Use the domain method to ensure validation
        let mut new_aggregate = aggregate.clone();
        new_aggregate.rename(new_title)?;
        *aggregate = new_aggregate;

        Ok(())
    }

    async fn update_system_prompt(&self, id: &str, system_prompt: Option<String>) -> Result<()> {
        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Conversation not found: {}", id))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.update_system_prompt(system_prompt);
        *aggregate = new_aggregate;

        Ok(())
    }

    async fn add_user_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<crate::domain::conversation::ConversationMessage> {
        use crate::domain::conversation::MessageRole;

        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_message(MessageRole::User, content.clone(), tokens)?;

        // Get the last message that was just added
        let message = new_aggregate.messages().last().unwrap().clone();
        *aggregate = new_aggregate;

        Ok(message)
    }

    async fn add_assistant_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<crate::domain::conversation::ConversationMessage> {
        use crate::domain::conversation::MessageRole;

        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_message(MessageRole::Assistant, content.clone(), tokens)?;

        // Get the last message that was just added
        let message = new_aggregate.messages().last().unwrap().clone();
        *aggregate = new_aggregate;

        Ok(message)
    }

    async fn add_assistant_message_with_metadata(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<crate::domain::conversation::ConversationMessage> {
        use crate::domain::conversation::MessageRole;

        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_message(MessageRole::Assistant, content.clone(), tokens)?;

        // Get the last message that was just added and add metadata
        let mut message = new_aggregate.messages().last().unwrap().clone();
        message.metadata = metadata;
        *aggregate = new_aggregate;

        Ok(message)
    }

    async fn prune_conversation_to_limit(
        &self,
        conversation_id: &str,
        max_tokens: i64,
    ) -> Result<()> {
        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.prune_to_token_limit(max_tokens)?;
        *aggregate = new_aggregate;

        Ok(())
    }

    async fn add_document_reference(
        &self,
        conversation_id: &str,
        document_id: String,
        chunk_id: Option<String>,
        relevance_score: Option<f32>,
    ) -> Result<()> {
        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_document_reference(document_id, chunk_id, relevance_score);
        *aggregate = new_aggregate;

        Ok(())
    }

    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: crate::domain::conversation::MessageRole,
        content: String,
        tokens: i64,
        _status: String,
    ) -> Result<crate::domain::conversation::ConversationMessage> {
        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            crate::error::AppError::NotFound(format!("Conversation not found: {}", conversation_id))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_message(role, content.clone(), tokens)?;

        let message = new_aggregate.messages().last().unwrap().clone();
        *aggregate = new_aggregate;

        Ok(message)
    }

    async fn update_message_status(&self, _message_id: &str, _status: String) -> Result<()> {
        Ok(())
    }
}
