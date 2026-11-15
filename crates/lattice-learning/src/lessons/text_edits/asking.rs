//! What every writer and repair of a lesson is told about citing sources.
use serde_json::Value;

/// The writer's rules for naming sources and for quotation marks, shared with
/// the repairs so that a repair cannot undo what the writer was told.
pub(in crate::lessons) const NEVER_SAY_WHERE: &str = "Never say in the lesson's text where a statement comes from, by number or by name, and never refer to an excerpt, passage or tutorial: no \"Source 2\", no \"as source 0 describes\", no \"In the project's tutorial, ...\", no \"According to the guide, ...\" and no \"Citation:\" line.";
pub(in crate::lessons) const QUOTED_WORDING: &str =
    "Wording quoted from a source goes in double quotation marks, and only in a cited sentence; never put wording in single quotation marks.";
/// What the lesson's own figures are (design §3.13), one sentence for every judge and writer.
pub(in crate::lessons) const OWN_FIGURES: &str = lattice_core::own_figures!();
/// What separates the lesson's sentence from the citation's quote.
pub(in crate::lessons) const OWN_SENTENCE: &str = "The lesson states the fact in its own sentence and the quote holds the source's words: never paste the quote as the sentence, and never put the source's wording in a sentence outside double quotation marks.";
pub(in crate::lessons) const UNQUOTED_REMARKS: &str = "A misconception, a learner's thought, something a friend says or an example remark is written in the lesson's own sentence without quotation marks. Commands and their output go in code formatting.";
/// Told to the writer and the repairs alike, so a step a repair adds starts where the lesson has left the learner.
pub(in crate::lessons) const STEPS_IN_ORDER: &str = "The learner does the lesson's steps in order, with their own tools and materials, so each step starts from what the steps before it left: a later step uses what an earlier one made or set up, and never has the learner make or set it up again.";

/// Where a lesson's schema describes one cited sentence of a part.
const CITATION: &str = "/properties/blocks/items/properties/citations/items";

/// Whether `lesson`'s schema gives its parts citations.
pub(in crate::lessons) fn takes_citations(lesson: &Value) -> bool {
    lesson.pointer(CITATION).is_some()
}
