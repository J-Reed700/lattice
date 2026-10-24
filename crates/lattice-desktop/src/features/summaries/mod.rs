//! Collection-level document and section summaries for topic exploration.

pub mod di;
pub mod entity;
pub mod prompt;
pub mod repository;
pub mod runtime;
pub mod search;
pub mod source;
pub mod trigger;
pub mod use_cases;

pub use trigger::SummaryTrigger;

#[cfg(test)]
mod tests;
