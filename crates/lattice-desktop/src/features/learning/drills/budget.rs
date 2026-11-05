//! How much room each model call of a drill build reserves for its answer. A reservation sizes the request; it
//! never limits what the model writes: an answer cut off at its reservation is asked for again with twice the room.

/// What one line of code takes in an answer, its indentation and JSON escaping included.
const LINE_TOKENS: usize = 20;

/// What an answer's frame takes: field names, brackets, a title and a few sentences.
const FRAME_TOKENS: usize = 600;

/// What one planted defect takes: its edit, its two sentences, its consequence and a killing test.
const MUTATION_TOKENS: usize = 1_600;

/// What a judge's reasons and verdict take.
const JUDGEMENT_TOKENS: usize = 800;

/// What one entry of the answer key takes: a pattern, a fix and two sentences.
const KEY_ENTRY_TOKENS: usize = 1_200;

fn lines(size: u32) -> usize {
    usize::try_from(size).unwrap_or(usize::MAX)
}

/// The clean code of about `lines` lines, and beside it a suite of the same size where the language has a runner.
pub(super) fn code(size: u32, with_suite: bool) -> usize {
    let written = if with_suite {
        lines(size).saturating_mul(2)
    } else {
        lines(size)
    };
    written
        .saturating_mul(LINE_TOKENS)
        .saturating_add(FRAME_TOKENS)
}

/// A review's issues: the repairs together are not longer than the code they edit.
pub(super) fn review(size: u32) -> usize {
    lines(size)
        .saturating_mul(LINE_TOKENS)
        .saturating_add(FRAME_TOKENS)
}

/// The planted defects of `count` patterns, written in one answer.
pub(super) fn mutations(count: usize) -> usize {
    count
        .saturating_mul(MUTATION_TOKENS)
        .saturating_add(FRAME_TOKENS)
}

/// One planted defect, written again.
pub(super) fn one_mutation() -> usize {
    MUTATION_TOKENS + FRAME_TOKENS
}

/// A killing test, written again.
pub(super) fn killing_test() -> usize {
    MUTATION_TOKENS + FRAME_TOKENS
}

/// A judge's answer.
pub(super) fn judgement() -> usize {
    JUDGEMENT_TOKENS
}

/// The findings of a blind review of about `lines` lines: at most one for each line.
pub(super) fn findings(size: u32) -> usize {
    lines(size)
        .saturating_mul(LINE_TOKENS)
        .saturating_add(FRAME_TOKENS)
}

/// One entry of the answer key.
pub(super) fn key_entry() -> usize {
    KEY_ENTRY_TOKENS
}
