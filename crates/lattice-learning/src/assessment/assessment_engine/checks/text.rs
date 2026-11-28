//! Deterministic wording guards for option-position references and
//! non-choice distractor leaks in quiz items and lesson questions.

fn contains_word_bounded_ascii(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(needle).any(|(pos, _)| {
        let left_ok = pos
            .checked_sub(1)
            .and_then(|idx| bytes.get(idx))
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'));
        let right_ok = bytes
            .get(pos + needle.len())
            .is_none_or(|b| !(b.is_ascii_alphanumeric() || *b == b'_'));
        left_ok && right_ok
    })
}

fn contains_any_bounded(haystack: &str, patterns: &[&str]) -> bool {
    patterns
        .iter()
        .any(|pat| contains_word_bounded_ascii(haystack, pat))
}

pub(crate) fn contains_option_position_reference(text: &str) -> bool {
    let normalized = text.replace('\u{2019}', "'");
    if contains_any_bounded(
        &normalized,
        &["option A", "Option A", "choice A", "Choice A"],
    ) {
        return true;
    }
    let lower = normalized.to_ascii_lowercase();
    contains_any_bounded(
        &lower,
        &[
            "option b",
            "option c",
            "option d",
            "option e",
            "choice b",
            "choice c",
            "choice d",
            "choice e",
            "option 1",
            "option 2",
            "option 3",
            "option 4",
            "choice 1",
            "choice 2",
            "choice 3",
            "choice 4",
            "matches option a",
            "is option a",
            "see option a",
            "select option a",
            "matches choice a",
            "is choice a",
            "see choice a",
            "select choice a",
            "first option",
            "second option",
            "third option",
            "fourth option",
            "last option",
            "first choice",
            "second choice",
            "third choice",
            "fourth choice",
            "last choice",
        ],
    )
}

pub(super) fn contains_non_choice_distractor_reference(text: &str) -> bool {
    let lower = text.replace('\u{2019}', "'").to_ascii_lowercase();
    contains_any_bounded(
        &lower,
        &[
            "distractor",
            "distractors",
            "incorrect option",
            "incorrect options",
            "wrong option",
            "wrong options",
        ],
    )
}
