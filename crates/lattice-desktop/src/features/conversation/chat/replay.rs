//! Past answers as a prompt replays them: every reference as its handle.

use std::collections::HashMap;
use std::sync::Arc;

use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::services::model_contract::kept::TurnNames;
use lattice_core::domain::conversation::memory::{SourceMessage, SourceRole};
use lattice_core::domain::conversation::MessageRole;
use lattice_core::shared::error::Result;

use super::reply_text::ReplyText;
use super::TurnSource;
use crate::features::conversation::ConversationServiceTrait;

/// What turns an answer saved before references into the text a prompt replays.
pub struct Replay<'a> {
    pub conv_service: &'a Arc<dyn ConversationServiceTrait>,
    pub names: &'a TurnNames,
    pub source: Option<&'a TurnSource>,
}

impl Replay<'_> {
    /// An answer saved with references is already written as its handles. One
    /// saved before them is read once, against its own sources, which are named
    /// in the conversation in the order its answers come.
    pub async fn apply(&self, conversation_id: &str, recent: &mut [SourceMessage]) -> Result<()> {
        if !recent
            .iter()
            .any(|message| message.role == SourceRole::Assistant)
        {
            return Ok(());
        }
        let Some(aggregate) = self.conv_service.get_conversation(conversation_id).await? else {
            return Ok(());
        };
        let mut replayed: HashMap<String, String> = HashMap::new();
        for saved in aggregate
            .messages()
            .iter()
            .filter(|saved| matches!(saved.role, MessageRole::Assistant))
        {
            let metadata: serde_json::Value = saved
                .metadata
                .as_deref()
                .and_then(|metadata| serde_json::from_str(metadata).ok())
                .unwrap_or(serde_json::Value::Null);
            if ReplyText::stored(&saved.content, &metadata).is_some() {
                continue;
            }
            let sources: Vec<SourceDto> = metadata
                .get("sources")
                .and_then(|sources| serde_json::from_value(sources.clone()).ok())
                .unwrap_or_default();
            let reply = ReplyText::legacy(self.names, self.source, &saved.content, &sources);
            replayed.insert(saved.id.clone(), reply.text);
        }
        for message in recent
            .iter_mut()
            .filter(|message| message.role == SourceRole::Assistant)
        {
            if let Some(text) = replayed.remove(&message.id) {
                message.replayed = Some(text);
            }
        }
        Ok(())
    }
}
