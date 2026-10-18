//! The branching port over this repository: tangents, forks, truncation and
//! the regenerate lift.
use super::ConversationRepository;
use crate::features::conversation::chat::ports::ConversationBranches;
use crate::features::conversation::dto::ConversationDto;
use crate::features::conversation::tangent_dto::{ConversationTangentDto, CreateTangentRequestDto};
use async_trait::async_trait;
use lattice_core::domain::conversation::{Conversation, ConversationMessage, MessageRole};
use lattice_core::shared::error::Result;

#[async_trait]
impl ConversationBranches for ConversationRepository {
    async fn create_tangent(
        &self,
        request: CreateTangentRequestDto,
    ) -> Result<ConversationTangentDto> {
        ConversationRepository::create_tangent(self, request).await
    }

    async fn list_tangents(&self, conversation_id: &str) -> Result<Vec<ConversationTangentDto>> {
        ConversationRepository::list_tangents(self, conversation_id).await
    }

    async fn tangent_selection(&self, conversation_id: &str) -> Result<Option<String>> {
        ConversationRepository::tangent_selection(self, conversation_id).await
    }

    async fn promote_tangent(&self, conversation_id: &str) -> Result<()> {
        ConversationRepository::promote_tangent(self, conversation_id).await
    }

    async fn create_continuation(
        &self,
        conversation_id: &str,
        new_id: &str,
        new_title: &str,
        summary: &str,
        metadata: &str,
    ) -> Result<()> {
        ConversationRepository::create_continuation(
            self,
            conversation_id,
            new_id,
            new_title,
            summary,
            metadata,
        )
        .await
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Conversation>> {
        ConversationRepository::find_by_id(self, id).await
    }

    async fn project_conversation(&self, conversation: &Conversation) -> Result<ConversationDto> {
        ConversationRepository::project_conversation(self, conversation).await
    }

    async fn truncate_after(
        &self,
        conversation_id: &str,
        message_id: &str,
        inclusive: bool,
    ) -> Result<u64> {
        ConversationRepository::truncate_after(self, conversation_id, message_id, inclusive).await
    }

    async fn get_messages(&self, conversation_id: &str) -> Result<Vec<ConversationMessage>> {
        ConversationRepository::get_messages(self, conversation_id).await
    }

    async fn fork(
        &self,
        conversation_id: &str,
        up_to_message_id: Option<&str>,
        new_id: &str,
        new_title: &str,
    ) -> Result<(String, u32)> {
        ConversationRepository::fork(self, conversation_id, up_to_message_id, new_id, new_title)
            .await
    }

    async fn take_last_user_turn(
        &self,
        conversation_id: &str,
    ) -> Result<Option<(String, i64, Option<String>)>> {
        ConversationRepository::take_last_user_turn(self, conversation_id).await
    }

    async fn add_message_with_status(
        &self,
        conversation_id: &str,
        role: MessageRole,
        content: &str,
        tokens: i64,
        metadata: Option<&str>,
        status: &str,
    ) -> Result<ConversationMessage> {
        ConversationRepository::add_message_with_status(
            self,
            conversation_id,
            role,
            content,
            tokens,
            metadata,
            status,
        )
        .await
    }
}
