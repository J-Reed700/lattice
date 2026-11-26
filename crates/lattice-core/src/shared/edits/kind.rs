//! Traits implemented by registered content kinds and derived-data reactors.

use async_trait::async_trait;
use serde_json::Value;

use super::types::{
    Consequence, ContentAddress, ContentRef, EditOp, EditOrigin, EditScope, Editability,
    FieldProblem, FieldSpec,
};
use crate::shared::error::Result;
use crate::shared::persistence::{Reading, WriteTx};

/// One entity loaded by a `ContentKind`, with its form and current JSON document.
#[derive(Debug, Clone)]
pub struct Loaded {
    pub label: String,
    pub scope: EditScope,
    pub editability: Editability,
    pub form: Vec<FieldSpec>,
    pub document: Value,
    pub notes: Vec<String>,
}

/// Context handed to `ContentKind::store` and `EditReactor::{preview, apply}`.
pub struct Change<'a> {
    pub edit_id: &'a str,
    pub content: &'a ContentRef,
    pub scope: &'a EditScope,
    pub origin: &'a EditOrigin,
    pub target: Option<&'a ContentAddress>,
    pub context: Option<&'a Value>,
    pub ops: &'a [EditOp],
    pub before: &'a Value,
    pub after: &'a Value,
    pub reverted_receipt: Option<&'a Value>,
    pub at: i64,
}

/// Result returned by a reactor's `apply` inside the edit's transaction.
#[derive(Debug, Default, Clone)]
pub struct Reaction {
    pub receipt: Option<Value>,
    pub affected: Vec<String>,
    pub consequences: Vec<Consequence>,
    pub follow_up: Option<Value>,
}

/// Feature adapter that loads, validates, and persists one editable entity kind.
#[async_trait]
pub trait ContentKind: Send + Sync {
    fn name(&self) -> &'static str;
    fn affects(&self) -> &'static [&'static str];
    async fn load(&self, unit: &mut Reading<'_>, key: &str) -> Result<Option<Loaded>>;
    async fn check(
        &self,
        unit: &mut Reading<'_>,
        key: &str,
        before: &Value,
        after: &Value,
    ) -> Result<Vec<FieldProblem>>;
    async fn store(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<()>;
}

/// Synchronous derived-data handler invoked inside the edit's transaction.
#[async_trait]
pub trait EditReactor: Send + Sync {
    fn name(&self) -> &'static str;
    fn kinds(&self) -> &'static [&'static str];
    async fn preview(
        &self,
        unit: &mut Reading<'_>,
        change: &Change<'_>,
    ) -> Result<Vec<Consequence>>;
    async fn apply(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<Reaction>;
    async fn after_commit(&self, _follow_up: &Value) -> Result<()> {
        Ok(())
    }
}
