//! The alignment of two versions of a text.

use proptest::prelude::*;

use super::tests::texts;
use super::*;
use crate::shared::text::floor_char_boundary;

/// The span of the first `part` in `text`.
fn at(text: &str, part: &str) -> Span {
    let start = text.find(part).expect("the text has the part");
    start..start + part.len()
}

/// The empty span before the first `part` in `text`.
fn empty_before(text: &str, part: &str) -> Span {
    let start = at(text, part).start;
    start..start
}

fn pieces<'t>(text: &'t str, spans: &[(usize, usize)]) -> Vec<&'t str> {
    spans
        .iter()
        .map(|&(start, end)| &text[start..end])
        .collect()
}

#[test]
fn the_common_subsequence_matches_in_order_and_advances_the_first_list_on_a_tie() {
    assert_eq!(common_subsequence(&[1, 2, 3, 4], &[2, 4]), [(1, 0), (3, 1)]);
    assert_eq!(common_subsequence(&["A", "B"], &["B", "A"]), [(1, 0)]);
    assert!(common_subsequence::<u8>(&[], &[1]).is_empty());
    assert!(common_subsequence::<u8>(&[], &[]).is_empty());
}

#[test]
fn a_sentence_is_kept_rewritten_or_removed() {
    let old = "Alpha. Bravo. Charlie. Delta.";
    let new = "Alpha. Bravo two. Charlie.";
    let alignment = align(old, new);
    assert_eq!(
        alignment.fate(at(old, "Alpha.")),
        Fate::Kept(at(new, "Alpha."))
    );
    assert_eq!(
        alignment.fate(at(old, "Bravo.")),
        Fate::Rewritten(at(new, "Bravo two."))
    );
    assert_eq!(
        alignment.fate(at(old, "Charlie.")),
        Fate::Kept(at(new, "Charlie."))
    );
    assert_eq!(alignment.fate(at(old, "Delta.")), Fate::Removed);
    assert_eq!(
        alignment.hunks(),
        [
            (at(old, "Bravo."), at(new, "Bravo two.")),
            (at(old, "Delta."), new.len()..new.len()),
        ]
    );
}

#[test]
fn two_edits_in_one_text_are_told_apart() {
    let old = "Alpha holds. Bravo stands. Charlie rests. Delta runs. Echo waits.";
    let new = "Alpha holds. Charlie rests. Delta now sprints. Echo waits.";
    let alignment = align(old, new);
    for kept in ["Alpha holds.", "Charlie rests.", "Echo waits."] {
        assert_eq!(alignment.fate(at(old, kept)), Fate::Kept(at(new, kept)));
    }
    assert_eq!(alignment.fate(at(old, "Bravo stands.")), Fate::Removed);
    assert_eq!(
        alignment.fate(at(old, "Delta runs.")),
        Fate::Rewritten(at(new, "Delta now sprints."))
    );
    assert_eq!(
        alignment.hunks(),
        [
            (
                at(old, "Bravo stands."),
                empty_before(new, "Charlie rests.")
            ),
            (at(old, "Delta runs."), at(new, "Delta now sprints.")),
        ]
    );
}

#[test]
fn as_many_sentences_on_both_sides_pair_one_for_one() {
    let old = "Alpha. Bravo. Charlie. Delta.";
    let new = "Alpha. Beta. Gamma. Delta.";
    let alignment = align(old, new);
    assert_eq!(
        alignment.fate(at(old, "Bravo.")),
        Fate::Rewritten(at(new, "Beta."))
    );
    assert_eq!(
        alignment.fate(at(old, "Charlie.")),
        Fate::Rewritten(at(new, "Gamma."))
    );
    assert_eq!(
        alignment.hunks(),
        [
            (at(old, "Bravo."), at(new, "Beta.")),
            (at(old, "Charlie."), at(new, "Gamma.")),
        ]
    );
    assert_eq!(
        alignment.fate(at(old, "Bravo. Charlie.")),
        Fate::Rewritten(at(new, "Beta. Gamma."))
    );
}

#[test]
fn a_different_number_of_sentences_is_one_hunk() {
    let old = "Alpha. Bravo. Charlie. Delta.";
    let new = "Alpha. Beta. Delta.";
    let alignment = align(old, new);
    let rewritten = Fate::Rewritten(at(new, "Beta."));
    assert_eq!(alignment.fate(at(old, "Bravo.")), rewritten);
    assert_eq!(alignment.fate(at(old, "Charlie.")), rewritten);
    assert_eq!(
        alignment.hunks(),
        [(at(old, "Bravo. Charlie."), at(new, "Beta."))]
    );
}

#[test]
fn repeated_sentences_match_front_to_back() {
    let alignment = align("Echo. Echo. Echo.", "Echo.");
    assert_eq!(alignment.fate(0..5), Fate::Kept(0..5));
    assert_eq!(alignment.fate(6..11), Fate::Removed);
    assert_eq!(alignment.fate(12..17), Fate::Removed);
    assert_eq!(alignment.hunks(), [(6..17, 5..5)]);

    let alignment = align("Echo.", "Echo. Echo.");
    assert_eq!(alignment.fate(0..5), Fate::Kept(0..5));
    assert_eq!(alignment.hunks(), [(5..5, 6..11)]);

    let alignment = align("Echo. Foxtrot. Echo.", "Echo. Echo.");
    assert_eq!(alignment.fate(0..5), Fate::Kept(0..5));
    assert_eq!(alignment.fate(6..14), Fate::Removed);
    assert_eq!(alignment.fate(15..20), Fate::Kept(6..11));
}

#[test]
fn texts_without_sentences_have_nothing_to_keep() {
    let nothing = align("", "");
    assert!(nothing.hunks().is_empty());
    assert_eq!(nothing.fate(0..0), Fate::Removed);

    assert_eq!(align("", "One.").hunks(), [(0..0, 0..4)]);
    assert_eq!(align(" \n ", "One.").hunks(), [(3..3, 0..4)]);
    assert!(align(" ", "\n").hunks().is_empty());

    let lost = align("One.", "");
    assert_eq!(lost.hunks(), [(0..4, 0..0)]);
    assert_eq!(lost.fate(0..4), Fate::Removed);
    let blanked = align("One.", "  ");
    assert_eq!(blanked.hunks(), [(0..4, 2..2)]);
    assert_eq!(blanked.fate(0..4), Fate::Removed);
}

#[test]
fn whitespace_around_sentences_is_not_part_of_a_span() {
    let old = "  Alpha. Bravo.  ";
    let new = "Alpha. Bravo.";
    let alignment = align(old, new);
    assert_eq!(alignment.fate(0..old.len()), Fate::Kept(0..new.len()));
    assert_eq!(alignment.fate(0..2), Fate::Removed);
    assert_eq!(alignment.fate(15..17), Fate::Removed);
    assert_eq!(alignment.fate(8..9), Fate::Removed);
}

#[test]
fn a_span_over_a_kept_and_a_removed_sentence_is_rewritten_to_the_kept_one() {
    let old = "Alpha. Bravo. Charlie.";
    let new = "Alpha. Charlie.";
    let alignment = align(old, new);
    assert_eq!(
        alignment.fate(at(old, "Alpha. Bravo.")),
        Fate::Rewritten(at(new, "Alpha."))
    );
    assert_eq!(
        alignment.fate(at(old, "Bravo. Charlie.")),
        Fate::Rewritten(at(new, "Charlie."))
    );
    assert_eq!(alignment.fate(0..old.len()), Fate::Rewritten(0..new.len()));
}

#[test]
fn an_insertion_between_two_kept_sentences_makes_a_span_over_both_a_rewrite() {
    let old = "Alpha. Charlie.";
    let new = "Alpha. Bravo. Charlie.";
    let alignment = align(old, new);
    assert_eq!(
        alignment.fate(at(old, "Alpha.")),
        Fate::Kept(at(new, "Alpha."))
    );
    assert_eq!(
        alignment.fate(at(old, "Charlie.")),
        Fate::Kept(at(new, "Charlie."))
    );
    assert_eq!(alignment.fate(0..old.len()), Fate::Rewritten(0..new.len()));
    assert_eq!(
        alignment.hunks(),
        [(empty_before(old, "Charlie."), at(new, "Bravo."))]
    );
}

#[test]
fn a_fragment_of_a_kept_sentence_maps_to_the_same_fragment() {
    let old = "Alpha holds. Bravo stands firm. Charlie rests.";
    let new = "Bravo stands firm. Charlie rests.";
    let alignment = align(old, new);
    assert_eq!(alignment.fate(at(old, "Alpha holds.")), Fate::Removed);
    for fragment in [
        "stands",
        "Bravo",
        "firm.",
        "firm. Charlie",
        "Bravo stands firm. Charlie rests.",
    ] {
        let found = alignment.fate(at(old, fragment));
        assert_eq!(found, Fate::Kept(at(new, fragment)), "{fragment}");
    }
    let whole = alignment.fate(at(old, " Bravo stands firm."));
    assert_eq!(whole, Fate::Kept(at(new, "Bravo stands firm.")));
}

#[test]
fn a_fragment_of_a_rewritten_sentence_is_the_whole_new_sentence() {
    let old = "Alpha holds. Bravo stands firm. Charlie rests.";
    let new = "Alpha holds. Bravo falls. Charlie rests.";
    let alignment = align(old, new);
    assert_eq!(
        alignment.fate(at(old, "stands")),
        Fate::Rewritten(at(new, "Bravo falls."))
    );
    assert_eq!(
        alignment.fate(at(old, "firm. Charlie")),
        Fate::Rewritten(at(new, "Bravo falls. Charlie"))
    );
}

#[test]
fn multibyte_text_keeps_every_offset_on_a_char_boundary() {
    let old = "Zażółć gęślą jaźń. Smile 😀 wide. 日本語です。 Final 🎉 line.";
    let new = "Intro. Zażółć gęślą jaźń. Smile 😀 broad. 日本語です。 Final 🎉 line.";
    let alignment = align(old, new);
    assert_eq!(
        alignment.fate(at(old, "Smile 😀 wide.")),
        Fate::Rewritten(at(new, "Smile 😀 broad."))
    );
    for fragment in ["gęślą", "jaźń.", "本語", "日本語です。", "🎉 line."] {
        assert_eq!(
            alignment.fate(at(old, fragment)),
            Fate::Kept(at(new, fragment)),
            "{fragment}"
        );
    }
    let boundaries: Vec<usize> = old
        .char_indices()
        .map(|(offset, _)| offset)
        .chain([old.len()])
        .collect();
    for &from in &boundaries {
        for &to in boundaries.iter().filter(|&&to| to >= from) {
            if let Fate::Kept(span) | Fate::Rewritten(span) = alignment.fate(from..to) {
                assert!(span.start <= span.end && span.end <= new.len());
                assert!(new.is_char_boundary(span.start) && new.is_char_boundary(span.end));
            }
        }
    }
}

proptest! {
    #[test]
    fn hunks_ascend_on_char_boundaries_and_only_matched_sentences_are_kept(
        old in texts(),
        new in texts(),
    ) {
        let alignment = align(&old, &new);
        let (mut old_end, mut new_end) = (0, 0);
        for (before, after) in alignment.hunks() {
            prop_assert!(old_end <= before.start && before.start <= before.end);
            prop_assert!(new_end <= after.start && after.start <= after.end);
            prop_assert!(before.end <= old.len() && after.end <= new.len());
            prop_assert!(old.is_char_boundary(before.start) && old.is_char_boundary(before.end));
            prop_assert!(new.is_char_boundary(after.start) && new.is_char_boundary(after.end));
            (old_end, new_end) = (before.end, after.end);
        }
        let (old_spans, new_spans) = (sentences(&old), sentences(&new));
        let matched = common_subsequence(&pieces(&old, &old_spans), &pieces(&new, &new_spans));
        for (index, &(start, end)) in old_spans.iter().enumerate() {
            let kept = matches!(alignment.fate(start..end), Fate::Kept(_));
            let was_matched = matched.iter().any(|&(i, _)| i == index);
            prop_assert_eq!(kept, was_matched, "sentence {} of {:?}", index, old);
        }
    }

    #[test]
    fn any_span_lands_on_char_boundaries_of_the_new_text(
        old in texts(),
        new in texts(),
        from in 0usize..400,
        to in 0usize..400,
    ) {
        let alignment = align(&old, &new);
        let (from, to) = (floor_char_boundary(&old, from), floor_char_boundary(&old, to));
        if let Fate::Kept(span) | Fate::Rewritten(span) = alignment.fate(from.min(to)..from.max(to)) {
            prop_assert!(span.start <= span.end && span.end <= new.len());
            prop_assert!(new.is_char_boundary(span.start) && new.is_char_boundary(span.end));
        }
    }
}
