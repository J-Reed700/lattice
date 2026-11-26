//! `ContentEdits` service facade managing units of work and post-commit reactor follow-ups.

use std::sync::Arc;

use serde_json::Value;

use super::pipeline;
use super::registry::EditRegistry;
use super::types::{
    ContentEditHistory, ContentEditOutcome, ContentEditPreview, ContentEditQuery,
    ContentEditRequest, ContentEditSummary, ContentRef, EditableContent, NewestEdit, ReadMark,
    RevertContentEditRequest, Uncommitted,
};
use crate::shared::error::Result;
use crate::shared::persistence::{Database, Reading, WriteTx};

#[derive(Clone)]
pub struct ContentEdits {
    database: Database,
    registry: Arc<EditRegistry>,
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

    pub async fn open(&self, content: &ContentRef) -> Result<EditableContent> {
        let mut read = self.database.begin_read().await?;
        let opened = self.open_in(&mut read.reading(), content).await?;
        read.commit().await?;
        Ok(opened)
    }

    pub async fn open_in(
        &self,
        unit: &mut Reading<'_>,
        content: &ContentRef,
    ) -> Result<EditableContent> {
        pipeline::open_in(unit, &self.registry, content).await
    }

    pub async fn preview(&self, request: &ContentEditRequest) -> Result<ContentEditPreview> {
        let mut read = self.database.begin_read().await?;
        let outcome = pipeline::preview(&mut read.reading(), &self.registry, request).await?;
        read.commit().await?;
        Ok(outcome)
    }

    async fn apply_tx(&self, request: &ContentEditRequest) -> Result<Uncommitted> {
        let mut tx = self.database.urgent().begin_write().await?;
        let pending = self.apply_in(&mut tx, request, None).await?;
        tx.commit().await?;
        Ok(pending)
    }

    pub async fn apply(&self, request: &ContentEditRequest) -> Result<ContentEditOutcome> {
        let pending = self.apply_tx(request).await?;
        Ok(self.committed(pending).await)
    }

    pub async fn apply_in(
        &self,
        tx: &mut WriteTx,
        request: &ContentEditRequest,
        context: Option<&Value>,
    ) -> Result<Uncommitted> {
        pipeline::apply_in(tx, &self.registry, &self.app_version, request, context).await
    }

    async fn revert_tx(&self, request: &RevertContentEditRequest) -> Result<Uncommitted> {
        let mut tx = self.database.urgent().begin_write().await?;
        let pending = self.revert_in(&mut tx, request).await?;
        tx.commit().await?;
        Ok(pending)
    }

    pub async fn revert(&self, request: &RevertContentEditRequest) -> Result<ContentEditOutcome> {
        let pending = self.revert_tx(request).await?;
        Ok(self.committed(pending).await)
    }

    pub async fn revert_in(
        &self,
        tx: &mut WriteTx,
        request: &RevertContentEditRequest,
    ) -> Result<Uncommitted> {
        pipeline::revert_in(tx, &self.registry, &self.app_version, request).await
    }

    pub async fn committed(&self, pending: Uncommitted) -> ContentEditOutcome {
        pipeline::run_after_commit(&self.registry, &pending.follow_ups).await;
        pending.outcome
    }

    pub async fn history(&self, content: &ContentRef) -> Result<ContentEditHistory> {
        let mut read = self.database.begin_read().await?;
        let history = pipeline::history(&mut read.reading(), &self.registry, content).await?;
        read.commit().await?;
        Ok(history)
    }

    pub async fn list(&self, query: &ContentEditQuery) -> Result<Vec<ContentEditSummary>> {
        let mut read = self.database.begin_read().await?;
        let summaries = pipeline::list(&mut read.reading(), query).await?;
        read.commit().await?;
        Ok(summaries)
    }

    pub async fn newest(
        &self,
        unit: &mut Reading<'_>,
        contents: &[ContentRef],
        reactor: &str,
    ) -> Result<Vec<NewestEdit>> {
        pipeline::newest(unit, &self.registry, contents, reactor).await
    }

    pub async fn edited_paths(
        &self,
        unit: &mut Reading<'_>,
        content: &ContentRef,
    ) -> Result<Vec<String>> {
        pipeline::edited_paths(unit, &self.registry, content).await
    }

    pub async fn marks(
        &self,
        unit: &mut Reading<'_>,
        contents: &[ContentRef],
    ) -> Result<Vec<ReadMark>> {
        pipeline::marks(unit, &self.registry, contents).await
    }

    pub async fn moved(
        &self,
        unit: &mut Reading<'_>,
        marks: &[ReadMark],
    ) -> Result<Vec<ContentRef>> {
        pipeline::moved(unit, &self.registry, marks).await
    }
}
