//! The program as a pack carries it: `program/program.json` plus, when the
//! learner's evidence was exported, the attempts the exporter moved into
//! `program/attempts.json`. Preview and apply both read it from here so the
//! attempts are validated and imported with the curriculum they cite.
use super::{invalid, parse, ATTEMPTS_ENTRY, PROGRAM_ENTRY};
use crate::features::learning::{dto::LearningProgramDto, pack::DecodedLearningPack};
use crate::shared::error::Result;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::{
        dto::{LearningAssessmentKind, LearningAttemptDto},
        pack::{LearningPackManifest, LearningPackPrivacyManifest},
        tests::fixture,
    };
    use std::collections::BTreeMap;

    fn pack(entries: BTreeMap<String, Vec<u8>>) -> DecodedLearningPack {
        DecodedLearningPack {
            manifest: LearningPackManifest {
                format: String::new(),
                version: 0,
                schema_version: 1,
                pack_id: String::new(),
                title: String::new(),
                created_at: 0,
                application_version: String::new(),
                root_sha256: String::new(),
                privacy: LearningPackPrivacyManifest::default(),
                entries: vec![],
            },
            entries,
        }
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
        let mut entries = BTreeMap::new();
        entries.insert(PROGRAM_ENTRY.to_owned(), serde_json::to_vec(&program)?);
        entries.insert(ATTEMPTS_ENTRY.to_owned(), serde_json::to_vec(&attempts)?);
        let loaded = load_program(&pack(entries.clone()))?;
        assert_eq!(loaded.attempts.len(), 2);
        assert_eq!(loaded.attempts[1].id, attempts[1].id);

        entries.remove(ATTEMPTS_ENTRY);
        assert!(load_program(&pack(entries.clone()))?.attempts.is_empty());

        let mut smuggled = program.clone();
        smuggled.attempts = attempts;
        entries.insert(PROGRAM_ENTRY.to_owned(), serde_json::to_vec(&smuggled)?);
        assert!(load_program(&pack(entries)).is_err());
        assert!(load_program(&pack(BTreeMap::new())).is_err());
        Ok(())
    }
}
