//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use super::trait_def::ConversationServiceTrait;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use lattice_core::shared::error::Result;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
/// Mock conversation service for testing
///
/// Simulates conversation management without database.
/// Allows configuration and tracking of conversation operations.
/// All operations are deterministic and thread-safe.
pub struct MockConversationService {
    conversations: Arc<
        RwLock<
            std::collections::HashMap<
                String,
                lattice_core::domain::conversation::ConversationAggregate,
            >,
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

    /// The model a conversation asked for, as the repository keeps it.
    pub fn model_choice(
        &self,
        id: &str,
    ) -> Result<lattice_core::domain::conversation::ChatModelChoice> {
        let conversations = self.conversations.read().unwrap();
        let aggregate = conversations.get(id).ok_or_else(|| not_found(id))?;
        Ok(aggregate.model_choice().clone())
    }

    /// Record the model a conversation asks for, as the repository does.
    pub fn set_model_choice(
        &self,
        id: &str,
        choice: lattice_core::domain::conversation::ChatModelChoice,
    ) -> Result<()> {
        use lattice_core::domain::conversation::ConversationAggregate;

        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(id).ok_or_else(|| not_found(id))?;
        let mut conversation = aggregate.conversation().clone();
        conversation.model_choice = choice;
        *aggregate = ConversationAggregate::from_persistence(
            conversation,
            aggregate.messages().to_vec(),
            aggregate.document_context().to_vec(),
            aggregate.compaction().cloned(),
        );
        Ok(())
    }
}

#[cfg(test)]
fn not_found(id: &str) -> lattice_core::shared::error::AppError {
    lattice_core::shared::error::AppError::NotFound(format!("Conversation not found: {}", id))
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
    async fn fail_pending_turn(&self, user_message_id: &str) -> Result<()> {
        let mut conversations = self.conversations.write().unwrap();
        for conversation in conversations.values_mut() {
            if conversation.messages().iter().any(|m| {
                m.id == user_message_id
                    && m.status == "pending"
                    && m.role == lattice_core::domain::conversation::MessageRole::User
            }) {
                conversation.update_message_status(user_message_id, "failed".into())?;
            }
        }
        Ok(())
    }
    async fn complete_turn(
        &self,
        conversation_id: &str,
        user_message_id: &str,
        content: String,
        tokens: i64,
        metadata: Option<String>,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage> {
        use lattice_core::domain::conversation::{ConversationAggregate, MessageRole};
        let mut conversations = self.conversations.write().unwrap();
        let current = conversations.get_mut(conversation_id).ok_or_else(|| {
            lattice_core::shared::error::AppError::NotFound("Conversation not found".into())
        })?;
        let mut updated = current.clone();
        if !updated.messages().iter().any(|m| {
            m.id == user_message_id && m.role == MessageRole::User && m.status == "pending"
        }) {
            return Err(lattice_core::shared::error::AppError::InvalidState(
                "Turn is no longer pending in this conversation".into(),
            ));
        }
        updated.update_message_status(user_message_id, "completed".into())?;
        updated.add_message(MessageRole::Assistant, content, tokens)?;
        let mut messages = updated.messages().to_vec();
        let message = messages.last_mut().unwrap();
        message.metadata = metadata;
        let result = message.clone();
        *current = ConversationAggregate::from_persistence(
            updated.conversation().clone(),
            messages,
            updated.document_context().to_vec(),
            updated.compaction().cloned(),
        );
        Ok(result)
    }
    async fn create_conversation(
        &self,
        title: String,
        system_prompt: Option<String>,
    ) -> Result<lattice_core::domain::conversation::Conversation> {
        use lattice_core::domain::conversation::ConversationAggregate;

        let aggregate = ConversationAggregate::new(title, system_prompt)?;
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
    ) -> Result<Option<lattice_core::domain::conversation::ConversationAggregate>> {
        Ok(self.conversations.read().unwrap().get(id).cloned())
    }

    async fn list_conversations(
        &self,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<lattice_core::domain::conversation::Conversation>> {
        let conversations = self.conversations.read().unwrap();
        let mut all: Vec<_> = conversations
            .values()
            .map(|agg| agg.conversation().clone())
            .collect();

        all.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at));

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
                lattice_core::shared::error::AppError::NotFound(format!(
                    "Conversation not found: {}",
                    id
                ))
            })?;
        Ok(())
    }

    async fn rename_conversation(&self, id: &str, new_title: String) -> Result<()> {
        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(id).ok_or_else(|| {
            lattice_core::shared::error::AppError::NotFound(format!(
                "Conversation not found: {}",
                id
            ))
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
            lattice_core::shared::error::AppError::NotFound(format!(
                "Conversation not found: {}",
                id
            ))
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
    ) -> Result<lattice_core::domain::conversation::ConversationMessage> {
        use lattice_core::domain::conversation::MessageRole;

        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            lattice_core::shared::error::AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            ))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_message(MessageRole::User, content.clone(), tokens)?;

        let message = new_aggregate.messages().last().unwrap().clone();
        *aggregate = new_aggregate;

        Ok(message)
    }

    async fn add_assistant_message(
        &self,
        conversation_id: &str,
        content: String,
        tokens: i64,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage> {
        use lattice_core::domain::conversation::MessageRole;

        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            lattice_core::shared::error::AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            ))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_message(MessageRole::Assistant, content.clone(), tokens)?;

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
    ) -> Result<lattice_core::domain::conversation::ConversationMessage> {
        use lattice_core::domain::conversation::MessageRole;

        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            lattice_core::shared::error::AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            ))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_message(MessageRole::Assistant, content.clone(), tokens)?;

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
            lattice_core::shared::error::AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            ))
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
            lattice_core::shared::error::AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            ))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_document_reference(document_id, chunk_id, relevance_score);
        *aggregate = new_aggregate;

        Ok(())
    }

    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: lattice_core::domain::conversation::MessageRole,
        content: String,
        tokens: i64,
        status: String,
    ) -> Result<lattice_core::domain::conversation::ConversationMessage> {
        let mut conversations = self.conversations.write().unwrap();
        let aggregate = conversations.get_mut(conversation_id).ok_or_else(|| {
            lattice_core::shared::error::AppError::NotFound(format!(
                "Conversation not found: {}",
                conversation_id
            ))
        })?;

        let mut new_aggregate = aggregate.clone();
        new_aggregate.add_message_with_status(role, content.clone(), tokens, status)?;

        let message = new_aggregate.messages().last().unwrap().clone();
        *aggregate = new_aggregate;

        Ok(message)
    }

    async fn update_message_status(&self, message_id: &str, status: String) -> Result<()> {
        let mut conversations = self.conversations.write().unwrap();
        for conversation in conversations.values_mut() {
            if conversation.messages().iter().any(|m| m.id == message_id) {
                conversation.update_message_status(message_id, status.clone())?;
            }
        }
        Ok(())
    }

    async fn set_message_metadata_fields(
        &self,
        _message_id: &str,
        _fields: Vec<(String, serde_json::Value)>,
    ) -> Result<()> {
        Ok(())
    }
}
