//! A source a conversation reads beside the chat, such as an Explorer folder,
//! opened for one turn: what the chat offers the model, tells it and runs for
//! it, and how the turn treats it.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::application::contracts::sources::SourceDto;
use crate::application::contracts::tools::{FunctionCall, FunctionResult};
use crate::application::ports::llm_port::ToolDefinition;
use crate::shared::error::Result;

/// How a turn with this source treats the library, its intent, its rounds and
/// its claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnSourcePolicy {
    /// The router may start a library search on its own. Otherwise the library
    /// is searched only when the user turns it on.
    pub routes_to_library: bool,
    /// The library's search tools are offered without the user turning the
    /// library on.
    pub offers_library_tools: bool,
    /// Web search and deep research may be inferred from the message.
    /// Otherwise the turn takes only what the user turned on.
    pub infers_intent: bool,
    /// Only the claims the answer cites are verified.
    pub verifies_cited_only: bool,
    /// Tool rounds one turn may take.
    pub tool_rounds: usize,
    /// The template an evidence-carrying message renders in, in place of the
    /// saved document-answer template. `{context}` and `{question}` are filled.
    pub evidence_template: Option<&'static str>,
}

/// A passage of a source that the turn numbers with its library passages, so
/// the answer cites it and the chat's citation checks read it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePassage {
    /// What the model reads the passage under.
    pub label: String,
    pub source: SourceDto,
}

/// A source tool's result when it carries passages: each is shown under the
/// number the turn gives it. Any other result is plain text.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourcePassages {
    pub passages: Vec<SourcePassage>,
    /// Closes the result: how to read on, or why nothing came back.
    pub note: Option<String>,
}

#[async_trait]
pub trait TurnSourcePort: Send + Sync + std::fmt::Debug {
    fn policy(&self) -> TurnSourcePolicy;

    /// Whether `tool` is one of this source's tools.
    fn owns_tool(&self, tool: &str) -> bool;

    /// Whether this source's `tool` only reads, so it may run alongside the
    /// other calls of its round.
    fn reads_only(&self, tool: &str) -> bool;

    /// What the turn's activity shows while this source's `tool` runs.
    fn activity_label(&self, tool: &str, arguments: &Value) -> String;

    /// Adds the source's tools to the turn's tool list, once each.
    fn add_tools(&self, tools: &mut Vec<ToolDefinition>);

    /// The prompt block, at most about `max_chars` long, with what the source
    /// finds for `question` (`None` on a closed-book turn, which searches
    /// nothing).
    async fn context_block(
        &self,
        max_chars: usize,
        tools_offered: bool,
        question: Option<&str>,
    ) -> String;

    /// What the source found for `question` to number with the library's
    /// passages, best first, at most about `max_chars` long. A source that
    /// cites by its own means, as a folder cites lines, has none.
    async fn passages(&self, _question: &str, _max_chars: usize) -> Vec<SourcePassage> {
        Vec::new()
    }

    /// Runs one of the source's tool calls, its result cut to `max_chars`.
    async fn execute_tool(&self, call: FunctionCall, max_chars: usize) -> Result<FunctionResult>;
}
