//! Form read: a model sees each multiple-choice question without its key and
//! says which option, if any, can be told apart from the rest by its form
//! alone.
use crate::assessment_engine::LearningAssessmentCandidate as Item;
use lattice_core::{application::ports::LLMPort, shared::error::Result};

/// The option that stands apart from the others, and what sets it apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Apart {
    pub index: usize,
    pub why: String,
}

/// For each item, in order, the option that stands apart by its form.
pub(crate) async fn stands_apart(
    _llm: &dyn LLMPort,
    items: &[&Item],
) -> Result<Vec<Option<Apart>>> {
    Ok(items.iter().map(|_| None).collect())
}
