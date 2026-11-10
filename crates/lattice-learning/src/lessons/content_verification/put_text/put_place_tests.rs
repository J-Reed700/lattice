#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
//! Where a sentence the machine cut goes back, from the places the cuts
//! recorded: each cut was made against the text the earlier cuts left, so a
//! neighbour it names may itself have been cut.

use super::*;

const A: &str = "Alpha comes first in the list.";
const B: &str = "Bravo comes second in the list.";
const C: &str = "Charlie comes third in the list.";
const D: &str = "Delta comes fourth in the list.";
const E: &str = "Echo comes last in the list.";

fn cut(sentence: &str, before: &str, after: &str) -> CutPlace {
    CutPlace {
        sentence: sentence.into(),
        before: Some(before.into()),
        after: Some(after.into()),
    }
}

/// B, C and D cut from "A B C D E" one after the other.
fn sequential() -> Vec<CutPlace> {
    vec![cut(B, A, C), cut(C, A, D), cut(D, A, E)]
}

fn refs(cuts: &[CutPlace]) -> Vec<&CutPlace> {
    cuts.iter().collect()
}

fn text(sentences: &[&str]) -> String {
    sentences.join(" ")
}

#[test]
fn each_of_three_adjacent_cut_sentences_goes_after_the_nearest_earlier_sentence_that_is_in_the_text(
) {
    let cuts = sequential();
    let original = [A, B, C, D, E];
    for mask in 0..8u8 {
        let present: Vec<&str> = [B, C, D]
            .into_iter()
            .enumerate()
            .filter(|(index, _)| mask & (1 << index) != 0)
            .map(|(_, sentence)| sentence)
            .collect();
        for sentence in [B, C, D] {
            if present.contains(&sentence) {
                continue;
            }
            let mut sentences = vec![A];
            sentences.extend(present.iter().copied());
            sentences.push(E);
            let body = text(&sentences);
            let at = original
                .iter()
                .position(|other| *other == sentence)
                .unwrap();
            let nearest = original[..at]
                .iter()
                .rev()
                .find(|other| sentences.contains(other))
                .unwrap();
            assert_eq!(
                put_place(&body, sentence, &refs(&cuts)),
                Placing::At(Anchor::After((*nearest).into())),
                "{sentence:?} into {body:?}"
            );
        }
    }
}

#[test]
fn a_sentence_cut_beside_another_cut_sentence_goes_after_the_one_put_back_last() {
    let cuts = vec![cut(B, A, C), cut(C, A, D), cut(D, A, E)];
    let body = text(&[A, B, C, E]);
    assert_eq!(
        put_place(&body, D, &refs(&cuts)),
        Placing::At(Anchor::After(C.into()))
    );
}

#[test]
fn a_chain_of_cuts_places_the_last_after_the_sentence_put_back_before_it() {
    let b1 = "Bravo one.";
    let b2 = "Bravo two.";
    let cuts = vec![cut(b1, A, b2), cut(b2, A, C), cut(C, A, E)];
    let body = text(&[A, b1, b2, E]);
    assert_eq!(
        put_place(&body, C, &refs(&cuts)),
        Placing::At(Anchor::After(b2.into()))
    );
}

#[test]
fn a_sentence_with_only_later_neighbours_in_the_text_goes_before_the_first() {
    let cuts = vec![CutPlace {
        sentence: B.into(),
        before: None,
        after: Some(C.into()),
    }];
    let body = text(&[C, D]);
    assert_eq!(
        put_place(&body, B, &refs(&cuts)),
        Placing::At(Anchor::Before(C.into()))
    );
}

#[test]
fn a_sentence_the_user_wrote_between_the_neighbours_is_ambiguous() {
    let cuts = sequential();
    let body = text(&[A, "The user wrote this one.", E]);
    assert_eq!(put_place(&body, C, &refs(&cuts)), Placing::Ambiguous);
}

#[test]
fn a_neighbour_that_occurs_twice_is_ambiguous() {
    let cuts = sequential();
    let body = text(&[A, E, A]);
    assert_eq!(put_place(&body, B, &refs(&cuts)), Placing::Ambiguous);
}

#[test]
fn neighbours_that_overlap_themselves_count_twice() {
    let cuts = vec![cut(B, "aa", "zz")];
    assert_eq!(put_place("aaa zz", B, &refs(&cuts)), Placing::Ambiguous);
}

#[test]
fn a_text_that_would_come_both_before_and_after_is_ambiguous() {
    let cuts = vec![cut(B, A, C), cut(C, B, A)];
    let body = text(&[A, E]);
    assert_eq!(put_place(&body, B, &refs(&cuts)), Placing::Ambiguous);
}

#[test]
fn neighbours_in_the_wrong_order_are_ambiguous() {
    let cuts = vec![cut(B, A, C)];
    let body = text(&[C, A]);
    assert_eq!(put_place(&body, B, &refs(&cuts)), Placing::Ambiguous);
}

#[test]
fn a_text_with_none_of_the_neighbours_is_unknown() {
    let cuts = sequential();
    let body = text(&["Something else entirely."]);
    assert_eq!(put_place(&body, C, &refs(&cuts)), Placing::Unknown);
    assert_eq!(
        put_place(&body, "No record.", &refs(&cuts)),
        Placing::Unknown
    );
}

#[test]
fn a_sentence_the_text_already_has_is_not_inserted_again() {
    let present = text(&[A, B]);
    for anchor in [
        Anchor::After(A.into()),
        Anchor::Before(B.into()),
        Anchor::End,
    ] {
        assert_eq!(insert_sentence(&present, &anchor, A), None);
    }
    assert_eq!(
        insert_sentence(&present, &Anchor::End, &format!("  {B}\n")),
        None,
        "whitespace does not make it another sentence"
    );
    assert!(insert_sentence(&present, &Anchor::End, C).is_some());
}
