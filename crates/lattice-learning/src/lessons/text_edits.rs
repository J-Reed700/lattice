//! What the lesson's writers and repairs share about citing and about where a lesson teaches.
mod asking;
mod citations;
pub(super) use asking::{
    takes_citations, NEVER_SAY_WHERE, OWN_FIGURES, OWN_SENTENCE, QUOTED_WORDING, STEPS_IN_ORDER,
    UNQUOTED_REMARKS,
};
pub(in crate::lessons) use citations::{citable_texts, citation_entry, cited_sentence, taught_at};

#[cfg(test)]
mod citation_entry_tests;
