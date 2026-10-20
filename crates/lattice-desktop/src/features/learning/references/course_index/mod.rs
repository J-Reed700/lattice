//! # Course index
//!
//! Every course keeps an index of its evidence on disk, apart from
//! `lattice.db`: `<data_dir>/course-index/<program_id>/` holds one
//! `passages.db` (passages and their full-text index) and one
//! `vectors-<identity>.usearch`, made by whichever embedding model is
//! active. It is the shared passage index
//! (`crate::infrastructure::passage_index`), the one Explorer keeps for a
//! folder; the course owns only what its index means.
//!
//! - A document is a source's active version, keyed by the version's
//!   immutable id: a refreshed, adopted or reimported source is a new key,
//!   so nothing is ever updated in place. Web pages, pasted text and library
//!   documents alike: a document captured from the library is indexed again
//!   in each course that holds it, so one course is one ranking over one
//!   corpus (design D3).
//! - A passage is a byte span of the version's text, cut by the span chunker
//!   at the widths the reference collection ranks by, so a hit is the span
//!   the claim checker widens, with no relocation. A passage is embedded
//!   under its source's title; the title is never stored with it.
//! - [`CourseIndexes::ensure_current`] is the one idempotent step that brings
//!   an index in line with its course: versions no longer active leave,
//!   new ones are chunked in, and pending passages are embedded. A
//!   background job (`learning.course_index`) runs it after a source is
//!   captured, adopted, reimported or deleted, and at startup for every
//!   course whose index is not current; a lesson job runs it inline before
//!   it reads, waiting for a background build that is embedding.
//! - A user's command (attaching evidence to a claim, searching the course's
//!   sources) never waits for embedding: its text-only step
//!   ([`CourseSearch::bring_text_current`]) chunks new versions in, drops
//!   retired ones, searches what is embedded by meaning and the rest by
//!   words, and queues a build for the embedding.
//! - Without an embedding model the index is searched by text alone, and an
//!   embedding that stops part-way leaves the rest to text until the next
//!   run. Either is an [`IndexMode`], said in the lesson's stage text and
//!   recorded in its verification report; never silent, never a stop.
//! - The index is rebuilt from `learning_source_versions`, so it needs no
//!   migration, and backups and packs carry none of it. Deleting a course
//!   removes its directory; a startup sweep removes directories whose course
//!   is gone.
//!
//! ## Public surface
//!
//! - [`CourseIndexes`] — every course's index: `search`, `ensure_current`,
//!   `refresh`, `delete`, `sweep`, `backfill`, `register`, `open` and
//!   `close` (the course in its workspace goes first), and `status`, the
//!   cheap read the course workspace shows ([`CourseIndexStatusDto`]): while a
//!   course indexes, lesson preparation waits for it and says why
//! - [`CourseSearch`] — one course's index held by a reader, and the
//!   [`IndexView`] it last brought current
//! - [`IndexMode`], [`CourseProgress`]

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
