//! The one learning method every generated part of a course follows, so the
//! outline, lessons, quizzes, practice, decks and interviews teach alike. The
//! evidence and its limits are in docs/design/2026-10-25-learning-science-lessons.md
//! §2 and docs/design/2026-10-26-one-learning-method.md.

/// How learning material is built. Kept near 250 tokens so it fits beside a
/// local model's task.
pub(in crate::features::learning) const LEARNING_METHOD: &str = "Follow this learning method; the research on durable learning supports it. \
Retrieval first: the learner produces an answer from memory (recall, predict, decide, explain) before seeing it, on a fresh case rather than a copied sentence. \
Feedback after every attempt: the right answer, why, and the misconception behind each plausible wrong answer. \
Small steps: few new ideas at once, each tied to what the learner already knows, each with a concrete example; pair words with a diagram or table when an idea has structure, never a decorative visual. \
Worked example, then fade: a complete example with each step's reason, then a partly completed one, then an independent problem; what the learner already knows goes straight to problems. \
Elaborate: ask why, how ideas connect, and when one would not apply. \
Contrast what is easily confused: set it beside a near-miss, name the feature that differs, and mix similar types in practice. \
Space it: earlier material returns from memory later; something is learned after several correct recalls across sessions, not one. \
Effortful but mostly answerable: about four in five attempts succeed. \
Never rely on rereading, highlighting, summaries to read, or learning-style matching.";

/// How an answer is graded or explained back to the learner.
pub(in crate::features::learning) const FEEDBACK_METHOD: &str = "Give feedback the learner can act on: say in one line what was right, then the most important gap as a specific fix, name the misconception if one shows, and end with one concrete next step or a fresh question to retry. Never give only a score or a bare right or wrong.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_method_stays_short_enough_for_a_local_model() {
        // About four characters a token: the method must not crowd a small
        // window's task out.
        assert!(LEARNING_METHOD.len() / 4 < 320, "{}", LEARNING_METHOD.len());
        assert!(FEEDBACK_METHOD.len() / 4 < 80, "{}", FEEDBACK_METHOD.len());
    }
}
