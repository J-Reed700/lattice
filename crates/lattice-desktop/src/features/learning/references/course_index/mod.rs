//! # Course index
//!
//! Every course keeps an index of its evidence on disk, apart from
//! `lattice.db`: `<data_dir>/course-index/<program_id>/` holds the shared
//! passage index (`crate::infrastructure::passage_index`) over the course's
//! active source versions, keyed by immutable version id and cut into the byte
//! spans the reference collection ranks. [`CourseIndexes::ensure_current`] is
//! the one idempotent step that brings an index in line with its course: a
//! background job runs it after source changes and at startup, and a lesson
//! job before it reads. A user's command never waits for embedding
//! ([`CourseSearch::bring_text_current`]); without an embedding model the
//! index is searched by text alone ([`IndexMode`]). The index is rebuilt from
//! `learning_source_versions`, so it needs no migration.

mod job;
mod manager;
mod search;
mod sync;

#[cfg(test)]
mod first_tests;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
pub(in crate::features::learning) mod tests;

pub use manager::{CourseIndexStatusDto, CourseIndexes};
pub use search::{CourseSearch, IndexView};
pub(in crate::features::learning) use sync::write_version;
pub use sync::CourseProgress;

/// The job kind of a course index build; its subject is the program.
pub const COURSE_INDEX: &str = "learning.course_index";

/// The most characters one indexed passage holds: the reference collection's
/// ranking window, which the claim checker has always widened from (design
/// §3.3). Not redesigned here.
pub(in crate::features::learning) const CHUNK_CHARS: usize = 1200;

/// About how far the next passage starts after the last: the reference
/// collection's step, so neighbouring passages overlap (design §3.3).
pub(in crate::features::learning) const STEP_CHARS: usize = 900;

/// How a course's evidence is searched right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexMode {
    /// Every passage is embedded by the active model: dense and full-text
    /// search, fused.
    Hybrid,
    /// No embedding model is active, or the index could not be brought up to
    /// date: the evidence is searched by its words alone.
    Lexical,
    /// Some passages are not embedded yet, because embedding stopped or
    /// failed part-way: those are found by their words alone until the next
    /// run embeds them.
    Partial { embedded: u32, total: u32 },
}

impl IndexMode {
    /// The mode as reports, passages and selection keys record it: one value
    /// per run, without counts, so a key does not move as embedding does.
    pub fn label(self) -> &'static str {
        match self {
            Self::Hybrid => "hybrid",
            Self::Lexical => "lexical",
            Self::Partial { .. } => "partial",
        }
    }

    /// The mode in words, for stage text and logs.
    pub fn describe(self) -> String {
        match self {
            Self::Hybrid => "References are searched by meaning and by their words".into(),
            Self::Lexical => "References are searched by their words only".into(),
            Self::Partial { embedded, total } => format!(
                "{embedded} of {total} reference passages are embedded; the rest are searched by their words only"
            ),
        }
    }
}
