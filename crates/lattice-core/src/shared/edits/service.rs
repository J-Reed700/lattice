//! `ContentEdits` service facade (P0 contract stubs; P1 provides pipeline & store).

use std::sync::Arc;

use serde_json::Value;

use super::registry::EditRegistry;
use super::types::{
    ContentEditHistory, ContentEditOutcome, ContentEditPreview, ContentEditQuery,
    ContentEditRequest, ContentEditSummary, ContentRef, EditableContent, NewestEdit, ReadMark,
    RevertContentEditRequest, Uncommitted,
};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::{Database, Reading, WriteTx};

const NOT_BUILT: &str = "Content edits are not built yet.";

fn stub<T>() -> Result<T> {
    Err(AppError::InvalidState(NOT_BUILT.into()))
}

#[derive(Clone)]
pub struct ContentEdits {
    #[allow(dead_code)]
    database: Database,
    #[allow(dead_code)]
    registry: Arc<EditRegistry>,
    #[allow(dead_code)]
    app_version: String,
}

impl ContentEdits {
    pub fn new(database: Database, registry: Arc<EditRegistry>, app_version: String) -> Self {
        Self {
            database,
            registry,
            app_version,
        }
    }

    pub async fn open(&self, _content: &ContentRef) -> Result<EditableContent> {
        stub()
    }

    pub async fn open_in(
        &self,
        _unit: &mut Reading<'_>,
        _content: &ContentRef,
    ) -> Result<EditableContent> {
        stub()
    }

    pub async fn preview(&self, _request: &ContentEditRequest) -> Result<ContentEditPreview> {
        stub()
    }

    pub async fn apply(&self, _request: &ContentEditRequest) -> Result<ContentEditOutcome> {
        stub()
    }

    pub async fn apply_in(
        &self,
        _tx: &mut WriteTx,
        _request: &ContentEditRequest,
        _context: Option<&Value>,
    ) -> Result<Uncommitted> {
        stub()
    }

    pub async fn revert(&self, _request: &RevertContentEditRequest) -> Result<ContentEditOutcome> {
        stub()
    }

    pub async fn revert_in(
        &self,
        _tx: &mut WriteTx,
        _request: &RevertContentEditRequest,
    ) -> Result<Uncommitted> {
        stub()
    }

    pub async fn committed(&self, pending: Uncommitted) -> ContentEditOutcome {
        pending.outcome
    }

    pub async fn history(&self, _content: &ContentRef) -> Result<ContentEditHistory> {
        stub()
    }

    pub async fn list(&self, _query: &ContentEditQuery) -> Result<Vec<ContentEditSummary>> {
        stub()
    }

    pub async fn newest(
        &self,
        _unit: &mut Reading<'_>,
        _contents: &[ContentRef],
        _reactor: &str,
    ) -> Result<Vec<NewestEdit>> {
        stub()
    }

    pub async fn edited_paths(
        &self,
        _unit: &mut Reading<'_>,
        _content: &ContentRef,
    ) -> Result<Vec<String>> {
        stub()
    }

    pub async fn marks(
        &self,
        _unit: &mut Reading<'_>,
        _contents: &[ContentRef],
    ) -> Result<Vec<ReadMark>> {
        stub()
    }

    pub async fn moved(
        &self,
        _unit: &mut Reading<'_>,
        _marks: &[ReadMark],
    ) -> Result<Vec<ContentRef>> {
        stub()
    }
}
