//! The program as a pack carries it: `program/program.json`. Preview and
//! apply both read it from here, so the curriculum is validated and imported
//! the same way.
use super::{
    conflicts::IdClaim,
    entries::{fresh, ExportScope, ImportScope, SectionRows},
    export::entry,
    importers, invalid, parse,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{
        sections::{PackSection, PROGRAM_ENTRY},
        DecodedLearningPack, LearningPackEntry, LearningPackEntryKind,
    },
    portability_dto::LearningPackConflictKind,
};
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use sqlx::SqliteConnection;
use std::collections::HashMap;

pub(super) fn load_program(pack: &DecodedLearningPack) -> Result<LearningProgramDto> {
    parse(
        pack.entries
            .get(PROGRAM_ENTRY)
            .ok_or_else(|| invalid("Pack is missing its program."))?,
    )
}

/// The curriculum: modules and lessons, written with the program row. Their owner queries also select the local title.
pub(super) struct ProgramRows;
pub(super) const PROGRAM: ProgramRows = ProgramRows;

#[async_trait]
impl SectionRows for ProgramRows {
    fn section(&self) -> PackSection {
        PackSection::Program
    }
    async fn encode(
        &self,
        _connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        Ok(vec![entry(
            PROGRAM_ENTRY,
            LearningPackEntryKind::Program,
            scope.program,
        )?])
    }
    fn claims(
        &self,
        _pack: &DecodedLearningPack,
        program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        let mut claims = Vec::new();
        for module in &program.modules {
            claims.push(IdClaim::owned_by(
                LearningPackConflictKind::Module,
                "learning_modules",
                "SELECT program_id,title FROM learning_modules WHERE id=?",
                &module.id,
            ));
            for lesson in &module.lessons {
                claims.push(IdClaim::owned_by(
                    LearningPackConflictKind::Lesson,
                    "learning_lessons",
                    "SELECT program_id,title FROM learning_lessons WHERE id=?",
                    &lesson.id,
                ));
            }
        }
        Ok(claims)
    }
    fn map_ids(
        &self,
        _pack: &DecodedLearningPack,
        program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        for module in &program.modules {
            fresh(ids, &module.id);
            for lesson in &module.lessons {
                fresh(ids, &lesson.id);
            }
        }
        for source in &program.sources {
            fresh(ids, &source.id);
        }
        Ok(())
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        importers::insert_program(connection, scope.program, scope.ids).await
    }
}
