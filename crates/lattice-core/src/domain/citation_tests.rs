#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

fn at(version: &str, start_byte: usize, end_byte: usize) -> Anchor {
    Anchor {
        source_version_id: version.to_owned(),
        start_byte,
        end_byte,
    }
}

#[test]
fn parts_without_anchors_give_nothing_until_one_has_an_anchor() {
    assert_eq!(relation_of(&[]), None);
    let empty: [Anchor; 0] = [];
    assert_eq!(
        relation_of(&[(Grounds::Stated, &empty), (Grounds::Follows, &empty)]),
        None
    );
    let one = [at("v1", 0, 10)];
    assert_eq!(
        relation_of(&[(Grounds::Stated, &empty), (Grounds::Stated, &one)]),
        Some((Relation::Restates, vec![at("v1", 0, 10)]))
    );
}

#[test]
fn a_part_without_anchors_is_ignored_beside_one_with_them() {
    let empty: [Anchor; 0] = [];
    let one = [at("v1", 0, 10)];
    let two = [at("v1", 20, 30)];
    assert_eq!(
        relation_of(&[(Grounds::Stated, &one), (Grounds::Stated, &empty)]),
        Some((Relation::Restates, vec![at("v1", 0, 10)]))
    );
    assert_eq!(
        relation_of(&[(Grounds::Follows, &empty), (Grounds::Stated, &two)]),
        Some((Relation::Restates, vec![at("v1", 20, 30)]))
    );
}

#[test]
fn a_follows_part_among_stated_parts_makes_the_statement_inferred() {
    let stated = [at("v1", 20, 30)];
    let follows = [at("v1", 0, 10)];
    assert_eq!(
        relation_of(&[(Grounds::Stated, &stated), (Grounds::Follows, &follows)]),
        Some((Relation::Infers, vec![at("v1", 0, 10), at("v1", 20, 30)]))
    );
}

#[test]
fn stated_parts_sharing_a_sentence_restate_it_alone() {
    let first = [at("v1", 0, 10), at("v1", 20, 30)];
    let second = [at("v1", 20, 30), at("v1", 40, 50)];
    assert_eq!(
        relation_of(&[(Grounds::Stated, &first), (Grounds::Stated, &second)]),
        Some((Relation::Restates, vec![at("v1", 20, 30)]))
    );
}

#[test]
fn stated_parts_on_different_sentences_summarize_all_of_them() {
    let first = [at("v1", 40, 50)];
    let second = [at("v1", 0, 10)];
    assert_eq!(
        relation_of(&[(Grounds::Stated, &first), (Grounds::Stated, &second)]),
        Some((
            Relation::Summarizes,
            vec![at("v1", 0, 10), at("v1", 40, 50)]
        ))
    );
}

#[test]
fn one_stated_part_on_two_sentences_restates_with_both() {
    let both = [at("v1", 20, 30), at("v1", 0, 10)];
    assert_eq!(
        relation_of(&[(Grounds::Stated, &both)]),
        Some((Relation::Restates, vec![at("v1", 0, 10), at("v1", 20, 30)]))
    );
}

#[test]
fn anchors_are_sorted_across_versions_and_each_comes_once() {
    let follows = [at("v2", 0, 4), at("v1", 9, 12), at("v2", 0, 4)];
    let stated = [at("v1", 9, 12)];
    assert_eq!(
        relation_of(&[(Grounds::Follows, &follows), (Grounds::Stated, &stated)]),
        Some((Relation::Infers, vec![at("v1", 9, 12), at("v2", 0, 4)]))
    );
    let first = [at("v2", 0, 4), at("v1", 9, 12)];
    let second = [at("v1", 9, 12), at("v2", 0, 4), at("v1", 2, 3)];
    assert_eq!(
        relation_of(&[(Grounds::Stated, &first), (Grounds::Stated, &second)]),
        Some((Relation::Restates, vec![at("v1", 9, 12), at("v2", 0, 4)]))
    );
}
