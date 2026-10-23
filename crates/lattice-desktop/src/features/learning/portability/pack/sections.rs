//! The pack's layout: every path the importer understands and the entry kind
//! its manifest must declare there. Pure data, so the codec validates paths
//! from the same rows the repository encodes and imports by, without
//! depending on the repository.
use super::LearningPackEntryKind;
use sha2::{Digest, Sha256};

pub const PROGRAM_ENTRY: &str = "program/program.json";
pub const PROVENANCE_ENTRY: &str = "program/provenance.json";
pub const OUTCOMES_ENTRY: &str = "program/outcomes.json";
pub const LESSON_STATE_ENTRY: &str = "curriculum/lesson-state.json";
pub const EVIDENCE_ENTRY: &str = "evidence/events.json";
pub const FOLLOW_UP_ENTRY: &str = "evidence/follow-ups.json";
pub const PRACTICAL_ENTRY: &str = "practical/artifacts.json";
pub const SOURCE_LIBRARY_ENTRY: &str = "sources/library.json";
pub const SOURCE_SELECTORS_ENTRY: &str = "sources/selectors.json";
pub const SOURCE_HISTORY_ENTRY: &str = "sources/history.json";
/// Each version's provenance, the relations between versions and the
/// course's source trust overrides.
pub const SOURCE_TRUST_ENTRY: &str = "sources/trust.json";
/// One `<table>.json` entry per program-scoped evidence table.
pub const EVIDENCE_TABLE_PREFIX: &str = "evidence/tables/";
/// The quiz bank's private answer keys, which travel in the evidence tables.
pub const QUIZ_KEYS_TABLE_ENTRY: &str = "evidence/tables/learning_assessment_answer_keys.json";
/// One `<table>.json` entry per lesson claim review table.
pub const REVIEW_TABLE_PREFIX: &str = "curriculum/reviews/";
/// One `<canvas-id>.json` entry per canvas, with its snapshots.
pub const CANVAS_PREFIX: &str = "program/canvases/";
/// One `<sha256(version id)>.txt` entry per full source body.
pub const SOURCE_BODY_PREFIX: &str = "sources/bodies/";

/// One part of a pack the repository writes and reads as a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackSection {
    Program,
    Provenance,
    Outcomes,
    LessonStates,
    Events,
    FollowUps,
    Practical,
    SourceLibrary,
    SourceSelectors,
    SourceHistory,
    SourceTrust,
    EvidenceTables,
    Canvases,
    SourceBodies,
    ReviewTables,
}

/// A fixed entry, or a folder of entries named `<stem><suffix>` directly
/// inside `prefix`.
#[derive(Debug, Clone, Copy)]
pub enum SectionPath {
    Entry(&'static str),
    Folder {
        prefix: &'static str,
        suffix: &'static str,
    },
}

#[derive(Debug)]
pub struct SectionDescriptor {
    pub section: PackSection,
    pub path: SectionPath,
    pub kind: LearningPackEntryKind,
}

const fn entry(
    section: PackSection,
    path: &'static str,
    kind: LearningPackEntryKind,
) -> SectionDescriptor {
    SectionDescriptor {
        section,
        path: SectionPath::Entry(path),
        kind,
    }
}

const fn folder(
    section: PackSection,
    prefix: &'static str,
    suffix: &'static str,
    kind: LearningPackEntryKind,
) -> SectionDescriptor {
    SectionDescriptor {
        section,
        path: SectionPath::Folder { prefix, suffix },
        kind,
    }
}

use LearningPackEntryKind as Kind;
use PackSection as S;

/// Every section, one row each. A new kind of entry is one row here and one
/// behaviour row in the repository.
pub const SECTIONS: &[SectionDescriptor] = &[
    entry(S::Program, PROGRAM_ENTRY, Kind::Program),
    entry(S::Provenance, PROVENANCE_ENTRY, Kind::Curriculum),
    entry(S::Outcomes, OUTCOMES_ENTRY, Kind::Curriculum),
    entry(S::LessonStates, LESSON_STATE_ENTRY, Kind::Curriculum),
    entry(S::Events, EVIDENCE_ENTRY, Kind::Evidence),
    entry(S::FollowUps, FOLLOW_UP_ENTRY, Kind::Evidence),
    entry(S::Practical, PRACTICAL_ENTRY, Kind::PracticalArtifact),
    entry(S::SourceLibrary, SOURCE_LIBRARY_ENTRY, Kind::SourceMetadata),
    entry(
        S::SourceSelectors,
        SOURCE_SELECTORS_ENTRY,
        Kind::SourceMetadata,
    ),
    entry(S::SourceHistory, SOURCE_HISTORY_ENTRY, Kind::SourceMetadata),
    entry(S::SourceTrust, SOURCE_TRUST_ENTRY, Kind::SourceMetadata),
    folder(
        S::EvidenceTables,
        EVIDENCE_TABLE_PREFIX,
        ".json",
        Kind::Evidence,
    ),
    folder(S::Canvases, CANVAS_PREFIX, ".json", Kind::Canvas),
    folder(
        S::SourceBodies,
        SOURCE_BODY_PREFIX,
        ".txt",
        Kind::SourceBody,
    ),
    folder(
        S::ReviewTables,
        REVIEW_TABLE_PREFIX,
        ".json",
        Kind::Curriculum,
    ),
];

/// The content-addressed entry for one source version's full text.
pub fn source_body_path(version_id: &str) -> String {
    let digest = Sha256::digest(version_id.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{SOURCE_BODY_PREFIX}{hex}.txt")
}
