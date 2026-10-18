//! Reading a turn's message before retrieval: what kind of turn it is, the
//! expansion a library search reads, and the queries a web search sends.
//!
//! An interpreter belongs to one turn: it is built with that turn's
//! cancellation token, so stopping the turn stops its model calls.

use async_trait::async_trait;

use crate::domain::qa::hyde::{HyDEInterpretation, QueryType};
use crate::shared::error::Result;

#[async_trait]
pub trait QueryInterpreter: Send + Sync {
    /// Classify the query, then write the retrieval-oriented expansion for
    /// that type.
    async fn interpret_with_context(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<HyDEInterpretation>;

    /// What kind of turn this is, without writing anything for it.
    async fn classify_with_context(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<QueryType>;

    /// A search-engine-ready query for the web tools.
    async fn web_search_query_with_context(
        &self,
        query: &str,
        conversation_context: Option<&str>,
    ) -> Result<String>;

    /// Further searches on the subject of `root` for deep research, each from
    /// a different angle.
    async fn research_followups_with_context(
        &self,
        root: &str,
        query: &str,
        conversation_context: Option<&str>,
        count: usize,
    ) -> Result<Vec<String>>;
}
