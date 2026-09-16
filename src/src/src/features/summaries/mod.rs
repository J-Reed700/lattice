//! # Summaries feature (RAPTOR-lite, one level)
//!
//! At index time the utility model writes a three-sentence summary plus five
//! key topics for each document, and for each top-level section of documents
//! that have at least three. Those summaries are embedded into their own vector
//! index and used as a *collection-level* first stage: they answer "what is
//! this document about" and "where should I start" — questions the chunk index
//! answers badly, because no single 512-token passage is about a whole
//! document.
//!
//! Opt-in and off by default: [`use_cases::GenerateDocumentSummariesUseCase`]
//! takes an `enabled` constructor flag, and [`di::register`] does nothing
//! without it.
//!
//! ## Public surface
//!
//! - [`entity`] — `DocumentSummary`, `SummaryLevel`, `SummaryHit`
//! - [`prompt`] — prompt rendering, response parsing, token truncation
//! - [`repository`] — `SummaryRepositoryPort` + `SqliteSummaryRepository`
//! - [`source`] — `SummarySourcePort`: document text, sections, cluster label
//! - [`runtime`] — `SummaryRuntimePort`: lazily-resolved models
//! - [`search`] — `SummarySearchPort` + `SummarySearch`
//! - [`openings`] — summary hits to an opening-document list
//! - [`trigger`] — the post-index notification indexing calls
//! - [`di::register`] — one-call wiring
//!
//! ## Non-goals
//!
//! - More than one summary level (no recursive RAPTOR tree).
//! - Summaries as answer evidence. They are generated text, so they name
//!   documents to read; the quoted passage always comes from the chunk index.
//! - Re-summarizing on a schedule, or on edits that do not re-index.

pub mod di;
pub mod entity;
pub mod openings;
pub mod prompt;
pub mod repository;
pub mod runtime;
pub mod search;
pub mod source;
pub mod trigger;
pub mod use_cases;

pub use trigger::notify_document_indexed;

#[cfg(test)]
mod tests;
