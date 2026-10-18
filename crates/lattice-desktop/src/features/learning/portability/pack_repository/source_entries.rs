//! The course's sources as a pack carries them: the library with its
//! versions, refresh history, quote selectors, and, only by request, full
//! source text.
use super::{
    conflicts::IdClaim,
    entries::{
        decode, fresh, map_table_ids, row_ids, table_claims, ExportScope, ImportScope, SectionRows,
    },
    export, importers, value_str,
};
use crate::features::learning::{
    dto::{LearningProgramDto, LearningSourceWorkspaceDto},
    pack::{
        sections::{
            PackSection, SOURCE_HISTORY_ENTRY, SOURCE_LIBRARY_ENTRY, SOURCE_SELECTORS_ENTRY,
        },
        DecodedLearningPack, LearningPackEntry,
    },
};
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqliteConnection;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceHistorySnapshot {
    pub(super) checks: Vec<serde_json::Value>,
}

pub(super) struct SourceLibrary;
#[async_trait]
impl SectionRows for SourceLibrary {
    fn section(&self) -> PackSection {
        PackSection::SourceLibrary
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        export::source_library(connection, &scope.program.summary.id).await
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        let Some(workspace) = decode::<LearningSourceWorkspaceDto>(pack, SOURCE_LIBRARY_ENTRY)?
        else {
            return Ok(Vec::new());
        };
        let mut claims = Vec::new();
        for source in &workspace.sources {
            let title = source
                .active_version
                .as_ref()
                .map(|version| version.title.as_str());
            claims.push(IdClaim::column(
                "learning_source_library",
                "id",
                &source.id,
                title,
            ));
            for version in &source.versions {
                claims.push(IdClaim::column(
                    "learning_source_versions",
                    "id",
                    &version.id,
                    Some(&version.title),
                ));
            }
        }
        Ok(claims)
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        let Some(workspace) = decode::<LearningSourceWorkspaceDto>(pack, SOURCE_LIBRARY_ENTRY)?
        else {
            return Ok(());
        };
        for source in &workspace.sources {
            fresh(ids, &source.id);
            for version in &source.versions {
                fresh(ids, &version.id);
            }
        }
        Ok(())
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        let Some(workspace) =
            decode::<LearningSourceWorkspaceDto>(scope.pack, SOURCE_LIBRARY_ENTRY)?
        else {
            return Ok(());
        };
        importers::insert_source_workspace(
            connection,
            &scope.program.summary.id,
            &workspace,
            scope.pack,
            scope.ids,
        )
        .await
    }
}

pub(super) struct SourceHistory;
#[async_trait]
impl SectionRows for SourceHistory {
    fn section(&self) -> PackSection {
        PackSection::SourceHistory
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        export::source_history(connection, &scope.program.summary.id).await
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        let history: Option<SourceHistorySnapshot> = decode(pack, SOURCE_HISTORY_ENTRY)?;
        let checks = history.map(|history| history.checks).unwrap_or_default();
        Ok(row_ids(&checks, "operationId")?
            .into_iter()
            .map(|id| IdClaim::column("learning_source_refresh_checks", "operation_id", id, None))
            .collect())
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        let history: Option<SourceHistorySnapshot> = decode(pack, SOURCE_HISTORY_ENTRY)?;
        for check in history.map(|history| history.checks).unwrap_or_default() {
            fresh(ids, value_str(&check, "operationId")?);
        }
        Ok(())
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        let Some(mut history) = decode::<SourceHistorySnapshot>(scope.pack, SOURCE_HISTORY_ENTRY)?
        else {
            return Ok(());
        };
        importers::insert_source_history(
            connection,
            &scope.program.summary.id,
            &mut history,
            scope.ids,
        )
        .await
    }
}

pub(super) struct SourceSelectors;
#[async_trait]
impl SectionRows for SourceSelectors {
    fn section(&self) -> PackSection {
        PackSection::SourceSelectors
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        export::selectors(connection, &scope.program.summary.id).await
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        table_claims(pack, SOURCE_SELECTORS_ENTRY, "learning_source_selectors")
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        map_table_ids(pack, SOURCE_SELECTORS_ENTRY, ids)
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        importers::insert_selectors(
            connection,
            &scope.program.summary.id,
            scope.pack,
            scope.ids,
            scope.policy.remaps_ids(),
        )
        .await
    }
}

/// Full source text travels only by request; the source library's import
/// reads it from here.
pub(super) struct SourceBodies;
#[async_trait]
impl SectionRows for SourceBodies {
    fn section(&self) -> PackSection {
        PackSection::SourceBodies
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        if !scope.options.include_source_bodies {
            return Ok(Vec::new());
        }
        export::source_bodies(connection, &scope.program.summary.id).await
    }
}
