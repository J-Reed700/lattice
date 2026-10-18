//! The folder an Explorer conversation reads beside the chat, opened for one
//! turn: what the chat offers the model, tells it, and runs for it.

use async_trait::async_trait;

use crate::application::contracts::tools::{FunctionCall, FunctionResult};
use crate::application::ports::llm_port::ToolDefinition;
use crate::shared::error::Result;

#[async_trait]
pub trait ExplorerTurnPort: Send + Sync + std::fmt::Debug {
    /// Whether `search_folder` is offered: the folder's index has passages.
    fn has_folder_index(&self) -> bool;

    /// Adds the folder tools to the turn's tool list, once each.
    fn add_tools(&self, tools: &mut Vec<ToolDefinition>);

    /// The prompt block, at most about `max_chars` long, with the passages the
    /// folder index finds for `question` (`None` on a closed-book turn, which
    /// searches nothing).
    async fn context_block(
        &self,
        max_chars: usize,
        tools_offered: bool,
        question: Option<&str>,
    ) -> String;

    /// Runs one folder tool call, its result cut to `max_chars`.
    async fn execute_tool(&self, call: FunctionCall, max_chars: usize) -> Result<FunctionResult>;
}
