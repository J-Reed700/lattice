//! The program as a pack carries it: `program/program.json` plus, when the
//! learner's evidence was exported, the attempts the exporter moved into
//! `program/attempts.json`. Preview and apply both read it from here so the
//! attempts are validated and imported with the curriculum they cite.
use super::{
    conflicts::IdClaim,
    entries::{fresh, ExportScope, ImportScope, SectionRows},
    export::entry,
    importers, invalid, parse,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{
        sections::{PackSection, ATTEMPTS_ENTRY, PROGRAM_ENTRY},
        DecodedLearningPack, LearningPackEntry, LearningPackEntryKind,
    },
    portability_dto::LearningPackConflictKind,
};
use crate::shared::error::Result;
use async_trait::async_trait;
use sqlx::SqliteConnection;
use std::collections::HashMap;

pub(super) fn load_program(pack: &DecodedLearningPack) -> Result<LearningProgramDto> {
    let mut program: LearningProgramDto = parse(
        pack.entries
            .get(PROGRAM_ENTRY)
            .ok_or_else(|| invalid("Pack is missing its program."))?,
    )?;
    // The exporter always moves attempts out of the program entry, so that
    // they only travel when the learner chose to include evidence.
    if !program.attempts.is_empty() {
        return Err(invalid(
            "Pack program entry carries attempts outside its attempt entry.",
        ));
    }
    if let Some(bytes) = pack.entries.get(ATTEMPTS_ENTRY) {
        program.attempts = parse(bytes)?;
    }
    Ok(program)
}

/// The curriculum: modules, lessons and questions, written with the
/// program row. Their owner queries also select the local title.
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
        let mut program = scope.program.clone();
        program.attempts.clear();
        Ok(vec![entry(
            PROGRAM_ENTRY,
            LearningPackEntryKind::Program,
            &program,
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
                for question in &lesson.questions {
                    // Questions belong to a program through their lesson;
                    // they have a prompt, not their own program_id/title.
                    claims.push(IdClaim::owned_by(
                        LearningPackConflictKind::Question,
                        "learning_questions",
                        "SELECT l.program_id,q.prompt AS title FROM learning_questions q JOIN learning_lessons l ON l.id=q.lesson_id WHERE q.id=?",
                        &question.id,
                    ));
                }
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
                for question in &lesson.questions {
                    fresh(ids, &question.id);
                }
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

/// Submitted assessments travel only with the learner's evidence. They are
/// written with the program row, so this row only exports and claims them.
pub(super) struct AttemptRows;
pub(super) const ATTEMPTS: AttemptRows = AttemptRows;

#[async_trait]
impl SectionRows for AttemptRows {
    fn section(&self) -> PackSection {
        PackSection::Attempts
    }
    async fn encode(
        &self,
        _connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        if !scope.options.include_evidence {
            return Ok(Vec::new());
        }
        Ok(vec![entry(
            ATTEMPTS_ENTRY,
            LearningPackEntryKind::Attempt,
            &scope.program.attempts,
        )?])
    }
    fn claims(
        &self,
        _pack: &DecodedLearningPack,
        program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        Ok(program
            .attempts
            .iter()
            .map(|attempt| {
                IdClaim::owned_by(
                    LearningPackConflictKind::Attempt,
                    "learning_attempts",
                    "SELECT program_id FROM learning_attempts WHERE id=?",
                    &attempt.id,
                )
            })
            .collect())
    }
    fn map_ids(
        &self,
        _pack: &DecodedLearningPack,
        program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        for attempt in &program.attempts {
            fresh(ids, &attempt.id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::{
        dto::{LearningAssessmentKind, LearningAttemptDto},
        pack::{
            decode_learning_pack, encode_learning_pack, LearningPackInput,
            LearningPackPrivacyManifest,
        },
        tests::fixture,
    };

    /// A pack through the real codec, so a manifest field added later is
    /// filled the way every export fills it.
    fn pack(
        entries: Vec<(&str, LearningPackEntryKind, Vec<u8>)>,
    ) -> anyhow::Result<DecodedLearningPack> {
        let includes_learner_evidence = entries
            .iter()
            .any(|(_, kind, _)| kind == &LearningPackEntryKind::Attempt);
        let bytes = encode_learning_pack(LearningPackInput {
            schema_version: 1,
            pack_id: uuid::Uuid::new_v4().to_string(),
            title: "Field biology".into(),
            created_at: crate::features::learning::persistence::now(),
            application_version: env!("CARGO_PKG_VERSION").into(),
            privacy: LearningPackPrivacyManifest {
                includes_learner_evidence,
                ..LearningPackPrivacyManifest::default()
            },
            entries: entries
                .into_iter()
                .map(|(path, kind, bytes)| LearningPackEntry {
                    path: path.into(),
                    kind,
                    bytes,
                })
                .collect(),
        })?;
        Ok(decode_learning_pack(&bytes)?)
    }

    fn attempt(program: &LearningProgramDto) -> LearningAttemptDto {
        LearningAttemptDto {
            id: uuid::Uuid::new_v4().to_string(),
            module_id: program.modules[0].id.clone(),
            lesson_id: Some(program.modules[0].lessons[0].id.clone()),
            kind: LearningAssessmentKind::Quiz,
            correct: 0,
            total: 0,
            results: vec![],
            submitted_at: 7,
        }
    }

    #[test]
    fn attempts_entry_is_joined_to_the_program() -> anyhow::Result<()> {
        let program = fixture();
        let attempts = vec![attempt(&program), attempt(&program)];
        let program_entry = (
            PROGRAM_ENTRY,
            LearningPackEntryKind::Program,
            serde_json::to_vec(&program)?,
        );
        let attempts_entry = (
            ATTEMPTS_ENTRY,
            LearningPackEntryKind::Attempt,
            serde_json::to_vec(&attempts)?,
        );
        let loaded = load_program(&pack(vec![program_entry.clone(), attempts_entry.clone()])?)?;
        assert_eq!(loaded.attempts.len(), 2);
        assert_eq!(loaded.attempts[1].id, attempts[1].id);

        assert!(load_program(&pack(vec![program_entry])?)?
            .attempts
            .is_empty());

        let mut smuggled = program.clone();
        smuggled.attempts = attempts;
        let smuggled = (
            PROGRAM_ENTRY,
            LearningPackEntryKind::Program,
            serde_json::to_vec(&smuggled)?,
        );
        assert!(load_program(&pack(vec![smuggled, attempts_entry.clone()])?).is_err());
        assert!(load_program(&pack(vec![attempts_entry])?).is_err());
        Ok(())
    }
}
