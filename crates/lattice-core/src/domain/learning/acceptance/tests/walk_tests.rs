//! The walk of every path of the design's §2.3, the re-ask, and divergence.

use super::*;
use crate::domain::learning::acceptance::Outcome::{
    AllCommon, CannotCut, FoundNothing, NeedsSource, NotApplied, Refuted, Rewritten, Saved, Stands,
    StandsElsewhere, Unjudged, Unproven,
};
use crate::domain::learning::claim_review::Class::{Common, Internal, Reasoning, Sourced};

fn walked(class: Class, taken: &[Taken]) -> Walked {
    walk(&StandardPolicy::new(), class, taken)
}

/// `steps` followed by `more`.
fn then(steps: &[Taken], more: &[Taken]) -> Vec<Taken> {
    steps.iter().chain(more).copied().collect()
}

fn sourced_rewrite(outcome: Outcome) -> Taken {
    rewrite(Goal::WhatThePassageStates, outcome)
}

fn reasoning_rewrite(outcome: Outcome) -> Taken {
    rewrite(Goal::AgreeWithThePassages, outcome)
}

fn correct_rewrite(outcome: Outcome) -> Taken {
    rewrite(Goal::CorrectWhatWasFound, outcome)
}

fn field_rewrite(outcome: Outcome) -> Taken {
    rewrite(Goal::AsTheFieldHasIt, outcome)
}

#[test]
fn a_statement_that_has_taken_no_step_is_at_its_class_entry() {
    for (class, node) in [
        (Sourced, "s.read"),
        (Reasoning, "r.read"),
        (Common, "c.read"),
        (Internal, "i.veto"),
    ] {
        assert_eq!(walked(class, &[]), placed(class, &[], false, node, 2));
    }
}

#[test]
fn a_sourced_statement_its_passages_state_is_resolved_at_the_first_reading() {
    assert_eq!(
        walked(Sourced, &[parts(Stands)]),
        Walked::Ended(End::Resolved)
    );
}

#[test]
fn a_sourced_statement_stated_on_a_passage_it_does_not_cite_is_recited_and_read_again() {
    let elsewhere = [parts(Unproven), research(Saved), page(StandsElsewhere)];
    assert_eq!(
        walked(Sourced, &elsewhere),
        placed(Sourced, &[], false, "s.recite", 2)
    );
    let recited = then(&elsewhere, &[recite()]);
    assert_eq!(
        walked(Sourced, &recited),
        placed(Sourced, &[], false, "s.citeread", 2)
    );
    assert_eq!(
        walked(Sourced, &then(&recited, &[parts(Stands)])),
        Walked::Ended(End::Resolved)
    );
    assert_eq!(
        walked(Sourced, &then(&recited, &[parts(Refuted)])),
        placed(Sourced, &[], false, "s.rewrite", 2)
    );
    assert_eq!(
        walked(Sourced, &then(&recited, &[parts(Unproven)])),
        placed(Sourced, &[], false, "s.uncite", 2)
    );
}

#[test]
fn a_cited_statement_whose_parts_are_all_common_loses_its_citation_and_meets_the_common_bar() {
    let all_common = [parts(AllCommon)];
    assert_eq!(
        walked(Sourced, &all_common),
        placed(Sourced, &[], false, "s.uncite", 2)
    );
    let uncited = then(&all_common, &[uncite()]);
    assert_eq!(
        walked(Sourced, &uncited),
        placed(Common, &[Class::Sourced], false, "c.read", 2)
    );
    assert_eq!(
        walked(Sourced, &then(&uncited, &[knowledge(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
}

#[test]
fn a_statement_that_lost_its_citation_and_needs_a_source_ends_unverified_not_sent_back() {
    let uncited = [parts(AllCommon), uncite()];
    assert_eq!(
        walked(Sourced, &then(&uncited, &[knowledge(NeedsSource)])),
        Walked::Ended(End::Unverified)
    );
}

#[test]
fn a_statement_that_lost_its_citation_and_is_refuted_has_its_passages_read() {
    let uncited = [parts(AllCommon), uncite()];
    assert_eq!(
        walked(Sourced, &then(&uncited, &[knowledge(Refuted)])),
        placed(Common, &[Class::Sourced], false, "c.ground", 2)
    );
}

#[test]
fn a_statement_nothing_states_is_researched_and_read_on_its_page() {
    let unproven = [parts(Unproven)];
    assert_eq!(
        walked(Sourced, &unproven),
        placed(Sourced, &[], false, "s.research", 2)
    );
    let researched = then(&unproven, &[research(Saved)]);
    assert_eq!(
        walked(Sourced, &researched),
        placed(Sourced, &[], false, "s.reread", 2)
    );
    assert_eq!(
        walked(Sourced, &then(&researched, &[page(Stands)])),
        Walked::Ended(End::Resolved)
    );
}

#[test]
fn a_statement_its_own_citation_contradicts_is_rewritten_without_research() {
    let refuted = [parts(Refuted)];
    assert_eq!(
        walked(Sourced, &refuted),
        placed(Sourced, &[], false, "s.rewrite", 2)
    );
    let rewritten = then(&refuted, &[sourced_rewrite(Rewritten)]);
    assert_eq!(
        walked(Sourced, &rewritten),
        placed(Sourced, &[], false, "s.final", 1)
    );
    assert_eq!(
        walked(Sourced, &then(&rewritten, &[parts(Stands)])),
        Walked::Ended(End::Resolved)
    );
}

#[test]
fn research_that_finds_nothing_is_followed_by_the_same_reading() {
    let nothing = [parts(Unproven), research(FoundNothing)];
    assert_eq!(
        walked(Sourced, &nothing),
        placed(Sourced, &[], false, "s.reread", 2)
    );
}

#[test]
fn a_page_that_stands_elsewhere_after_research_is_recited() {
    let found = [parts(Unproven), research(Saved), page(StandsElsewhere)];
    assert_eq!(
        walked(Sourced, &found),
        placed(Sourced, &[], false, "s.recite", 2)
    );
}

#[test]
fn a_statement_the_research_shows_false_is_rewritten_then_read_again() {
    let failed = [parts(Unproven), research(Saved), page(Refuted)];
    assert_eq!(
        walked(Sourced, &failed),
        placed(Sourced, &[], false, "s.rewrite", 2)
    );
    let rewritten = then(&failed, &[sourced_rewrite(Rewritten)]);
    assert_eq!(
        walked(Sourced, &rewritten),
        placed(Sourced, &[], false, "s.final", 1)
    );
    assert_eq!(
        walked(Sourced, &then(&rewritten, &[parts(Stands)])),
        Walked::Ended(End::Resolved)
    );
}

#[test]
fn a_rewrite_that_could_not_be_applied_is_tried_once_more() {
    let failed = [parts(Unproven), research(Saved), page(Refuted)];
    let refused = then(&failed, &[sourced_rewrite(NotApplied)]);
    assert_eq!(
        walked(Sourced, &refused),
        placed(Sourced, &[], false, "s.rewrite.again", 1)
    );
    assert_eq!(
        walked(Sourced, &then(&refused, &[sourced_rewrite(Rewritten)])),
        placed(Sourced, &[], false, "s.final", 0)
    );
    assert_eq!(
        walked(Sourced, &then(&refused, &[sourced_rewrite(NotApplied)])),
        placed(Sourced, &[], false, "s.cut", 0)
    );
}

#[test]
fn a_sourced_statement_that_fails_every_rung_is_removed() {
    let paths: [&[Taken]; 4] = [
        &[
            parts(Unproven),
            research(FoundNothing),
            page(Refuted),
            sourced_rewrite(NotApplied),
            sourced_rewrite(NotApplied),
            cut(Outcome::Cut),
        ],
        &[
            parts(Refuted),
            sourced_rewrite(Rewritten),
            parts(Refuted),
            cut(Outcome::Cut),
        ],
        &[
            parts(Unproven),
            research(Saved),
            page(Refuted),
            sourced_rewrite(Rewritten),
            parts(Refuted),
            cut(Outcome::Cut),
        ],
        &[
            parts(Unproven),
            research(Saved),
            page(StandsElsewhere),
            recite(),
            parts(Refuted),
            sourced_rewrite(Rewritten),
            parts(Refuted),
            cut(Outcome::Cut),
        ],
    ];
    for path in paths {
        assert_eq!(walked(Sourced, path), REMOVED);
    }
}

#[test]
fn a_sentence_that_cannot_be_cut_stays_and_ends_unverified() {
    let path = [
        parts(Unproven),
        research(Saved),
        page(Refuted),
        sourced_rewrite(NotApplied),
        sourced_rewrite(NotApplied),
        cut(CannotCut),
    ];
    assert_eq!(walked(Sourced, &path), Walked::Ended(End::Unverified));
}

#[test]
fn a_re_cited_statement_whose_parts_are_all_common_loses_the_new_citation_the_first_time() {
    let recited = [
        parts(Unproven),
        research(Saved),
        page(StandsElsewhere),
        recite(),
    ];
    let all_common = then(&recited, &[parts(AllCommon)]);
    assert_eq!(
        walked(Sourced, &all_common),
        placed(Sourced, &[], false, "s.uncite", 2)
    );
    assert_eq!(
        walked(Sourced, &then(&all_common, &[uncite()])),
        placed(Common, &[Class::Sourced], false, "c.read", 2)
    );
}

#[test]
fn a_re_cited_statement_that_came_from_common_ends_unverified_when_its_parts_are_all_common() {
    let path = [
        knowledge(NeedsSource),
        parts(Unproven),
        research(Saved),
        page(StandsElsewhere),
        recite(),
        parts(AllCommon),
    ];
    assert_eq!(walked(Common, &path), Walked::Ended(End::Unverified));
}

#[test]
fn a_rewrite_whose_parts_read_all_common_meets_commons_check_from_its_post_rewrite_node() {
    let rewritten = [
        parts(Unproven),
        research(Saved),
        page(Refuted),
        sourced_rewrite(Rewritten),
    ];
    let all_common = then(&rewritten, &[parts(AllCommon)]);
    assert_eq!(
        walked(Sourced, &all_common),
        placed(Sourced, &[], false, "s.final.uncite", 1)
    );
    let uncited = then(&all_common, &[uncite()]);
    assert_eq!(
        walked(Sourced, &uncited),
        placed(Common, &[Class::Sourced], false, "c.final", 1)
    );
    assert_eq!(
        walked(Sourced, &then(&uncited, &[knowledge(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
    let refuted = then(&uncited, &[knowledge(Refuted)]);
    assert_eq!(
        walked(Sourced, &refuted),
        placed(Common, &[Class::Sourced], false, "c.reground", 1)
    );
    assert_eq!(
        walked(Sourced, &then(&refuted, &[ground(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
    let needs_a_source = then(&uncited, &[knowledge(NeedsSource)]);
    assert_eq!(
        walked(Sourced, &needs_a_source),
        Walked::Ended(End::Unverified)
    );
}

#[test]
fn a_rewrite_that_is_still_refuted_is_cut_unless_its_parts_read_all_common() {
    let rewritten = [
        parts(Unproven),
        research(Saved),
        page(Refuted),
        sourced_rewrite(Rewritten),
    ];
    assert_eq!(
        walked(Sourced, &then(&rewritten, &[parts(Refuted)])),
        placed(Sourced, &[], false, "s.cut", 1)
    );
    assert_eq!(
        walked(Sourced, &then(&rewritten, &[parts(Unproven)])),
        Walked::Ended(End::Unverified)
    );
}

#[test]
fn a_statement_that_changed_its_class_may_still_enter_a_class_after_its_rewrite() {
    let rewritten = [
        knowledge(NeedsSource),
        parts(Unproven),
        research(Saved),
        page(Refuted),
        sourced_rewrite(Rewritten),
    ];
    assert_eq!(
        walked(Common, &rewritten),
        placed(Sourced, &[Class::Common], false, "s.final", 1)
    );
    let uncited = then(&rewritten, &[parts(AllCommon), uncite()]);
    assert_eq!(
        walked(Common, &uncited),
        placed(
            Common,
            &[Class::Common, Class::Sourced],
            false,
            "c.final",
            1
        )
    );
    assert_eq!(
        walked(Common, &then(&uncited, &[knowledge(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
}

#[test]
fn a_statement_that_changed_its_class_by_evidence_is_never_sent_back() {
    let needs_a_source = [knowledge(NeedsSource)];
    assert_eq!(
        walked(Common, &needs_a_source),
        placed(Sourced, &[Class::Common], false, "s.read", 2)
    );
    assert_eq!(
        walked(Common, &then(&needs_a_source, &[parts(Stands)])),
        Walked::Ended(End::Resolved)
    );
    // All common again: it researches, Common having been visited.
    assert_eq!(
        walked(Common, &then(&needs_a_source, &[parts(AllCommon)])),
        placed(Sourced, &[Class::Common], false, "s.research", 2)
    );
}

#[test]
fn an_explanation_its_passages_and_the_samples_leave_alone_is_consistent() {
    assert_eq!(
        walked(Reasoning, &[consistency(Stands)]),
        placed(Reasoning, &[], false, "r.veto", 2)
    );
    assert_eq!(
        walked(Reasoning, &[consistency(Stands), veto(Stands)]),
        Walked::Ended(End::Consistent)
    );
}

#[test]
fn a_vetoed_explanation_its_passages_ground_is_consistent() {
    let vetoed = [consistency(Stands), veto(Refuted)];
    assert_eq!(
        walked(Reasoning, &vetoed),
        placed(Reasoning, &[], false, "r.ground", 2)
    );
    assert_eq!(
        walked(Reasoning, &then(&vetoed, &[ground(Stands)])),
        Walked::Ended(End::Consistent)
    );
}

#[test]
fn a_vetoed_explanation_its_passages_do_not_ground_is_rewritten() {
    let vetoed = [consistency(Stands), veto(Refuted)];
    assert_eq!(
        walked(Reasoning, &then(&vetoed, &[ground(Refuted)])),
        placed(Reasoning, &[], false, "r.correct", 2)
    );
}

#[test]
fn a_ground_reading_is_not_a_part_wise_one() {
    let vetoed = [consistency(Stands), veto(Refuted)];
    for taken in [parts(Stands), ground(AllCommon), ground(Unproven)] {
        assert_eq!(
            walked(Reasoning, &then(&vetoed, &[taken])),
            Walked::Diverged {
                at: position(Reasoning, &[], false, "r.ground", 2),
                taken
            }
        );
    }
}

#[test]
fn an_explanation_a_passage_contradicts_is_rewritten_without_the_samples() {
    assert_eq!(
        walked(Reasoning, &[consistency(Refuted)]),
        placed(Reasoning, &[], false, "r.rewrite", 2)
    );
}

#[test]
fn an_explanation_asserting_a_specific_the_passages_lack_becomes_sourced() {
    assert_eq!(
        walked(Reasoning, &[consistency(NeedsSource)]),
        placed(Sourced, &[Class::Reasoning], false, "s.read", 2)
    );
}

#[test]
fn a_reasoning_statement_that_has_been_in_sourced_ends_unverified_not_sent_back() {
    let policy = StandardPolicy::new();
    let next = policy
        .graph(Reasoning)
        .node(NodeId("r.read"))
        .and_then(|node| node.next(NeedsSource))
        .unwrap();
    assert_eq!(
        follow(&policy, position(Reasoning, &[], false, "r.read", 2), next),
        Moved::To(position(Sourced, &[Class::Reasoning], false, "s.read", 2))
    );
    assert_eq!(
        follow(
            &policy,
            position(Reasoning, &[Class::Sourced], false, "r.read", 2),
            next
        ),
        Moved::Ended(End::Unverified)
    );
}

#[test]
fn a_rewritten_explanation_is_read_again_and_vetoed_again() {
    let rewritten = [consistency(Refuted), reasoning_rewrite(Rewritten)];
    assert_eq!(
        walked(Reasoning, &rewritten),
        placed(Reasoning, &[], false, "r.final", 1)
    );
    let read = then(&rewritten, &[consistency(Stands)]);
    assert_eq!(
        walked(Reasoning, &read),
        placed(Reasoning, &[], false, "r.reveto", 1)
    );
    assert_eq!(
        walked(Reasoning, &then(&read, &[veto(Stands)])),
        Walked::Ended(End::Consistent)
    );
    let vetoed = then(&read, &[veto(Refuted)]);
    assert_eq!(
        walked(Reasoning, &vetoed),
        placed(Reasoning, &[], false, "r.reground", 1)
    );
    assert_eq!(
        walked(Reasoning, &then(&vetoed, &[ground(Stands)])),
        Walked::Ended(End::Consistent)
    );
}

#[test]
fn an_explanation_that_fails_every_rung_is_removed() {
    let paths: [&[Taken]; 3] = [
        &[
            consistency(Refuted),
            reasoning_rewrite(NotApplied),
            reasoning_rewrite(NotApplied),
            cut(Outcome::Cut),
        ],
        &[
            consistency(Stands),
            veto(Refuted),
            ground(Refuted),
            correct_rewrite(Rewritten),
            consistency(Stands),
            veto(Refuted),
            ground(Refuted),
            cut(Outcome::Cut),
        ],
        &[
            consistency(Refuted),
            reasoning_rewrite(Rewritten),
            consistency(Refuted),
            cut(Outcome::Cut),
        ],
    ];
    for path in paths {
        assert_eq!(walked(Reasoning, path), REMOVED);
    }
    assert_eq!(
        walked(
            Reasoning,
            &[
                consistency(Refuted),
                reasoning_rewrite(Rewritten),
                consistency(Refuted),
                cut(CannotCut)
            ]
        ),
        UNVERIFIED
    );
    assert_eq!(
        walked(
            Reasoning,
            &[
                consistency(Refuted),
                reasoning_rewrite(Rewritten),
                consistency(NeedsSource)
            ]
        ),
        UNVERIFIED
    );
}

#[test]
fn a_common_statement_every_sample_passes_is_general_knowledge() {
    assert_eq!(
        walked(Common, &[knowledge(Stands)]),
        Walked::Ended(End::GeneralKnowledge)
    );
}

#[test]
fn a_common_statement_most_samples_call_false_has_its_passages_read() {
    let refuted = [knowledge(Refuted)];
    assert_eq!(
        walked(Common, &refuted),
        placed(Common, &[], false, "c.ground", 2)
    );
    assert_eq!(
        walked(Common, &then(&refuted, &[ground(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
}

#[test]
fn a_common_statement_the_passages_do_not_ground_is_rewritten_before_any_cut() {
    assert_eq!(
        walked(Common, &[knowledge(Refuted), ground(Refuted)]),
        placed(Common, &[], false, "c.rewrite", 2)
    );
}

#[test]
fn a_common_statement_that_needs_a_source_becomes_sourced() {
    let needs_a_source = [knowledge(NeedsSource)];
    assert_eq!(
        walked(Common, &needs_a_source),
        placed(Sourced, &[Class::Common], false, "s.read", 2)
    );
}

#[test]
fn a_rewritten_common_statement_is_read_again_and_passes_or_is_cut() {
    let rewritten = [
        knowledge(Refuted),
        ground(Refuted),
        field_rewrite(Rewritten),
    ];
    assert_eq!(
        walked(Common, &rewritten),
        placed(Common, &[], false, "c.final", 1)
    );
    assert_eq!(
        walked(Common, &then(&rewritten, &[knowledge(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
    let refused_once = [
        knowledge(Refuted),
        ground(Refuted),
        field_rewrite(NotApplied),
        field_rewrite(Rewritten),
        knowledge(Refuted),
        ground(Stands),
    ];
    assert_eq!(
        walked(Common, &refused_once),
        Walked::Ended(End::GeneralKnowledge)
    );
    assert_eq!(
        walked(
            Sourced,
            &[
                parts(AllCommon),
                uncite(),
                knowledge(Refuted),
                ground(Refuted),
                field_rewrite(Rewritten)
            ]
        ),
        placed(Common, &[Class::Sourced], false, "c.final", 1)
    );
}

#[test]
fn a_common_statement_that_fails_every_rung_is_removed() {
    let paths: [&[Taken]; 2] = [
        &[
            knowledge(Refuted),
            ground(Refuted),
            field_rewrite(NotApplied),
            field_rewrite(NotApplied),
            cut(Outcome::Cut),
        ],
        &[
            knowledge(Refuted),
            ground(Refuted),
            field_rewrite(Rewritten),
            knowledge(Refuted),
            ground(Refuted),
            cut(Outcome::Cut),
        ],
    ];
    for path in paths {
        assert_eq!(walked(Common, path), REMOVED);
    }
    assert_eq!(
        walked(
            Common,
            &[
                knowledge(Refuted),
                ground(Refuted),
                field_rewrite(Rewritten),
                knowledge(Refuted),
                ground(Refuted),
                cut(CannotCut)
            ]
        ),
        UNVERIFIED
    );
}

#[test]
fn an_internal_statement_the_samples_leave_alone_is_not_a_claim() {
    assert_eq!(
        walked(Internal, &[veto(Stands)]),
        Walked::Ended(End::NotAClaim)
    );
}

#[test]
fn a_vetoed_internal_statement_its_passages_ground_is_not_a_claim() {
    let vetoed = [veto(Refuted)];
    assert_eq!(
        walked(Internal, &vetoed),
        placed(Internal, &[], false, "i.ground", 2)
    );
    assert_eq!(
        walked(Internal, &then(&vetoed, &[ground(Stands)])),
        Walked::Ended(End::NotAClaim)
    );
}

#[test]
fn a_rewritten_internal_statement_passes_the_veto_again_or_is_cut() {
    let rewritten = [veto(Refuted), ground(Refuted), field_rewrite(Rewritten)];
    assert_eq!(
        walked(Internal, &rewritten),
        placed(Internal, &[], false, "i.final", 1)
    );
    assert_eq!(
        walked(Internal, &then(&rewritten, &[veto(Stands)])),
        Walked::Ended(End::NotAClaim)
    );
    assert_eq!(
        walked(
            Internal,
            &then(&rewritten, &[veto(Refuted), ground(Stands)])
        ),
        Walked::Ended(End::NotAClaim)
    );
}

#[test]
fn an_internal_statement_that_fails_every_rung_is_removed() {
    assert_eq!(
        walked(
            Internal,
            &[
                veto(Refuted),
                ground(Refuted),
                field_rewrite(Rewritten),
                veto(Refuted),
                ground(Refuted),
                cut(Outcome::Cut)
            ]
        ),
        REMOVED
    );
    assert_eq!(
        walked(
            Internal,
            &[
                veto(Refuted),
                ground(Refuted),
                field_rewrite(NotApplied),
                field_rewrite(NotApplied),
                cut(CannotCut)
            ]
        ),
        UNVERIFIED
    );
}

#[test]
fn an_unreadable_answer_leaves_the_statement_where_it_is_with_its_one_re_ask_spent() {
    assert_eq!(
        walked(Sourced, &[parts(Unjudged)]),
        placed(Sourced, &[], true, "s.read", 2)
    );
}

#[test]
fn the_reading_asked_again_is_read_like_any_other() {
    assert_eq!(
        walked(Sourced, &[parts(Unjudged), parts(Stands)]),
        Walked::Ended(End::Resolved)
    );
    assert_eq!(
        walked(Sourced, &[parts(Unjudged), parts(Unproven)]),
        placed(Sourced, &[], true, "s.research", 2)
    );
}

#[test]
fn a_second_unreadable_answer_at_the_same_node_stops_the_lesson() {
    assert_eq!(
        walked(Sourced, &[parts(Unjudged), parts(Unjudged)]),
        Walked::Stopped
    );
}

#[test]
fn the_one_re_ask_is_the_statements_not_the_nodes() {
    assert_eq!(
        walked(Sourced, &[parts(Unproven), research(Saved), page(Unjudged)]),
        placed(Sourced, &[], true, "s.reread", 2)
    );
    assert_eq!(
        walked(
            Sourced,
            &[
                parts(Unjudged),
                parts(Unproven),
                research(Saved),
                page(Unjudged)
            ]
        ),
        Walked::Stopped
    );
}

#[test]
fn the_re_ask_stays_spent_across_a_change_of_class() {
    let changed = [knowledge(Unjudged), knowledge(NeedsSource)];
    assert_eq!(
        walked(Common, &changed),
        placed(Sourced, &[Class::Common], true, "s.read", 2)
    );
    assert_eq!(
        walked(Common, &then(&changed, &[parts(Unjudged)])),
        Walked::Stopped
    );
}

#[test]
fn only_a_reading_can_be_unreadable() {
    assert_eq!(
        walked(Sourced, &[parts(Unproven), research(Unjudged)]),
        Walked::Diverged {
            at: position(Sourced, &[], false, "s.research", 2),
            taken: research(Unjudged)
        }
    );
}

#[test]
fn an_unreadable_answer_of_another_step_than_the_nodes_diverges() {
    assert_eq!(
        walked(Sourced, &[page(Unjudged)]),
        Walked::Diverged {
            at: position(Sourced, &[], false, "s.read", 2),
            taken: page(Unjudged)
        }
    );
}

#[test]
fn a_step_the_node_does_not_take_diverges() {
    assert_eq!(
        walked(Sourced, &[research(Saved)]),
        Walked::Diverged {
            at: position(Sourced, &[], false, "s.read", 2),
            taken: research(Saved)
        }
    );
    // A rewrite toward another goal is another step.
    let wrong_goal = rewrite(Goal::AsTheFieldHasIt, Rewritten);
    assert_eq!(
        walked(
            Sourced,
            &[parts(Unproven), research(Saved), page(Refuted), wrong_goal]
        ),
        Walked::Diverged {
            at: position(Sourced, &[], false, "s.rewrite", 2),
            taken: wrong_goal
        }
    );
}

#[test]
fn an_outcome_the_node_has_no_edge_for_diverges() {
    assert_eq!(
        walked(Sourced, &[parts(Outcome::Done)]),
        Walked::Diverged {
            at: position(Sourced, &[], false, "s.read", 2),
            taken: parts(Outcome::Done)
        }
    );
    assert_eq!(
        walked(Sourced, &[parts(StandsElsewhere)]),
        Walked::Diverged {
            at: position(Sourced, &[], false, "s.read", 2),
            taken: parts(StandsElsewhere)
        }
    );
    assert_eq!(
        walked(Reasoning, &[consistency(AllCommon)]),
        Walked::Diverged {
            at: position(Reasoning, &[], false, "r.read", 2),
            taken: consistency(AllCommon)
        }
    );
}

#[test]
fn a_step_taken_after_an_end_diverges_at_the_position_the_statement_left() {
    assert_eq!(
        walked(Sourced, &[parts(Stands), research(Saved)]),
        Walked::Diverged {
            at: position(Sourced, &[], false, "s.read", 2),
            taken: research(Saved)
        }
    );
}

#[test]
fn a_second_entry_into_a_class_diverges() {
    let policy = changed(Common, "c.read", |node| {
        node.on(&[NeedsSource], Next::enter(Sourced))
    });
    assert_eq!(
        walk(
            &policy,
            Sourced,
            &[parts(AllCommon), uncite(), knowledge(NeedsSource)]
        ),
        Walked::Diverged {
            at: position(Common, &[Class::Sourced], false, "c.read", 2),
            taken: knowledge(NeedsSource)
        }
    );
}

#[test]
fn a_statement_nothing_states_after_research_loses_its_citation_and_meets_commons_bar_never_a_rewrite(
) {
    let unproven = [parts(Unproven), research(FoundNothing), page(Unproven)];
    assert_eq!(
        walked(Sourced, &unproven),
        placed(Sourced, &[], false, "s.uncite", 2)
    );
    assert_eq!(
        walked(Sourced, &then(&unproven, &[uncite()])),
        placed(Common, &[Class::Sourced], false, "c.read", 2)
    );
    assert_eq!(
        walked(Sourced, &then(&unproven, &[uncite(), knowledge(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
}

#[test]
fn a_recited_statement_nothing_states_loses_its_citation_instead_of_being_rewritten() {
    let recited = [
        parts(Unproven),
        research(Saved),
        page(StandsElsewhere),
        recite(),
    ];
    assert_eq!(
        walked(Sourced, &then(&recited, &[parts(Unproven)])),
        placed(Sourced, &[], false, "s.uncite", 2)
    );
}

#[test]
fn a_reasoning_statement_no_page_establishes_is_uncited_into_commons_knowledge_read() {
    let unproven = [
        consistency(NeedsSource),
        parts(Unproven),
        research(Saved),
        page(Unproven),
    ];
    assert_eq!(
        walked(Reasoning, &unproven),
        placed(Sourced, &[Class::Reasoning], false, "s.uncite", 2)
    );
    let uncited = then(&unproven, &[uncite()]);
    assert_eq!(
        walked(Reasoning, &uncited),
        placed(
            Common,
            &[Class::Reasoning, Class::Sourced],
            false,
            "c.read",
            2
        )
    );
    assert_eq!(
        walked(Reasoning, &then(&uncited, &[knowledge(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
    assert_eq!(
        walked(Reasoning, &then(&uncited, &[knowledge(NeedsSource)])),
        UNVERIFIED
    );
}

#[test]
fn a_reasoning_statement_whose_parts_read_all_common_is_uncited_into_common() {
    assert_eq!(
        walked(Reasoning, &[consistency(NeedsSource), parts(AllCommon)]),
        placed(Sourced, &[Class::Reasoning], false, "s.uncite", 2)
    );
}

#[test]
fn a_statement_that_came_from_common_and_is_still_unproven_ends_unverified() {
    let unproven = [parts(Unproven), research(FoundNothing), page(Unproven)];
    assert_eq!(
        walked(Common, &then(&[knowledge(NeedsSource)], &unproven)),
        UNVERIFIED
    );
}

#[test]
fn a_rewrite_whose_parts_are_unproven_ends_unverified_and_one_that_is_refuted_is_cut() {
    let rewritten = [
        parts(Unproven),
        research(Saved),
        page(Refuted),
        sourced_rewrite(Rewritten),
    ];
    assert_eq!(
        walked(Sourced, &then(&rewritten, &[parts(Unproven)])),
        UNVERIFIED
    );
    assert_eq!(
        walked(Sourced, &then(&rewritten, &[parts(Refuted)])),
        placed(Sourced, &[], false, "s.cut", 1)
    );
}

#[test]
fn a_common_statement_that_came_from_sourced_and_needs_a_source_ends_unverified() {
    assert_eq!(
        walked(
            Sourced,
            &[parts(AllCommon), uncite(), knowledge(NeedsSource)]
        ),
        UNVERIFIED
    );
}

#[test]
fn a_rewritten_common_statement_that_needs_a_source_ends_unverified() {
    let policy = StandardPolicy::new();
    assert_eq!(
        walk_from(
            &policy,
            position(Common, &[], false, "c.final", 1),
            &[knowledge(NeedsSource)]
        ),
        UNVERIFIED
    );
}

#[test]
fn a_rewritten_explanation_that_needs_a_source_ends_unverified_and_one_that_is_refuted_is_cut() {
    let policy = StandardPolicy::new();
    let at = position(Reasoning, &[], false, "r.final", 1);
    assert_eq!(
        walk_from(&policy, at, &[consistency(NeedsSource)]),
        UNVERIFIED
    );
    assert_eq!(
        walk_from(&policy, at, &[consistency(Refuted)]),
        placed(Reasoning, &[], false, "r.cut", 1)
    );
}

#[test]
fn a_cut_that_cannot_be_made_ends_unverified_in_every_class() {
    let policy = StandardPolicy::new();
    for (class, node) in [
        (Sourced, "s.cut"),
        (Reasoning, "r.cut"),
        (Common, "c.cut"),
        (Internal, "i.cut"),
    ] {
        assert_eq!(
            walk_from(
                &policy,
                position(class, &[], false, node, 0),
                &[cut(CannotCut)]
            ),
            UNVERIFIED,
            "{node}"
        );
        assert_eq!(
            walk_from(
                &policy,
                position(class, &[], false, node, 0),
                &[cut(Outcome::Cut)]
            ),
            REMOVED,
            "{node}"
        );
    }
}

/// A Sourced graph whose one path takes a rewrite, then changes class by
/// evidence into Common's graph, which has a rewrite and its retry.
fn rewrite_then_change_class() -> Changed {
    let standard = StandardPolicy::new();
    let replaced = Graph::new(Sourced, "t.read", "t.read")
        .with(
            Node::new("t.read", Step::Read(Reading::Parts), 1)
                .on(&[Refuted], Next::to("t.rewrite"))
                .otherwise(Next::End(End::Resolved)),
        )
        .with(
            Node::new("t.rewrite", Step::Rewrite(Goal::WhatThePassageStates), 1)
                .on(&[Rewritten, NotApplied], Next::to("t.uncite")),
        )
        .with(Node::new("t.uncite", Step::Uncite, 0).on(&[Outcome::Done], Next::enter(Common)));
    Changed { standard, replaced }
}

#[test]
fn a_class_is_given_the_most_rewrites_any_path_from_its_entry_takes() {
    let policy = StandardPolicy::new();
    for class in CLASSES {
        assert_eq!(rewrites_given(&policy, class), 2, "{class:?}");
        assert_eq!(start(&policy, class).rewrites_left, 2, "{class:?}");
    }
    let policy = rewrite_then_change_class();
    assert_eq!(rewrites_given(&policy, Sourced), 3);
    assert_eq!(rewrites_given(&policy, Common), 2);
}

#[test]
fn a_graph_that_loops_through_a_rewrite_still_gives_a_count() {
    let policy = changed(Sourced, "s.rewrite", |node| {
        node.on(&[Rewritten], Next::to("s.read"))
    });
    assert!(rewrites_given(&policy, Sourced) >= 2);
}

#[test]
fn every_rewrite_step_a_statement_takes_spends_one_whatever_its_outcome() {
    let failed = [parts(Unproven), research(Saved), page(Refuted)];
    assert_eq!(
        walked(Sourced, &failed),
        placed(Sourced, &[], false, "s.rewrite", 2)
    );
    assert_eq!(
        walked(Sourced, &then(&failed, &[sourced_rewrite(Rewritten)])),
        placed(Sourced, &[], false, "s.final", 1)
    );
    let refused = then(&failed, &[sourced_rewrite(NotApplied)]);
    assert_eq!(
        walked(Sourced, &refused),
        placed(Sourced, &[], false, "s.rewrite.again", 1)
    );
    assert_eq!(
        walked(Sourced, &then(&refused, &[sourced_rewrite(Rewritten)])),
        placed(Sourced, &[], false, "s.final", 0)
    );
}

#[test]
fn a_change_of_class_carries_the_rewrites_left() {
    let policy = rewrite_then_change_class();
    let taken = [parts(Refuted), sourced_rewrite(NotApplied), uncite()];
    assert_eq!(
        walk(&policy, Sourced, &taken),
        placed(Common, &[Class::Sourced], false, "c.read", 2)
    );
}

#[test]
fn the_step_of_a_node_is_counted_only_when_it_is_a_rewrite() {
    let policy = StandardPolicy::new();
    let reading = position(Sourced, &[], false, "s.read", 2);
    assert_eq!(stepped(&policy, reading), reading);
    let rewriting = position(Sourced, &[], false, "s.rewrite", 2);
    assert_eq!(
        stepped(&policy, rewriting),
        position(Sourced, &[], false, "s.rewrite", 1)
    );
    let spent = position(Sourced, &[], false, "s.rewrite.again", 0);
    assert_eq!(stepped(&policy, spent), spent);
}

#[test]
fn a_machine_successor_spends_a_rewrite_and_continues_after_its_rewrite() {
    let policy = StandardPolicy::new();
    let ended = position(Reasoning, &[], true, "r.veto", 2);
    let continued = successor(&policy, ended);
    assert_eq!(continued, position(Reasoning, &[], true, "r.final", 1));
    assert_eq!(next_rewrite(&policy, ended), Some(continued));
    assert_eq!(
        next_rewrite(&policy, continued),
        Some(position(Reasoning, &[], true, "r.final", 0))
    );
    let last = position(Reasoning, &[], true, "r.final", 0);
    assert_eq!(next_rewrite(&policy, last), None);
}

#[test]
fn a_successor_of_a_statement_with_none_left_is_still_made() {
    let policy = StandardPolicy::new();
    let last = position(Common, &[], false, "c.final", 0);
    assert_eq!(successor(&policy, last), last);
}

#[test]
fn a_statement_born_of_a_repair_begins_after_the_rewrite_of_its_class_with_the_rewrites_given() {
    let policy = StandardPolicy::new();
    for (class, node) in [
        (Sourced, "s.final"),
        (Reasoning, "r.final"),
        (Common, "c.final"),
        (Internal, "i.final"),
    ] {
        for left in [0, 1, 2] {
            assert_eq!(
                born(&policy, class, left),
                position(class, &[], false, node, left),
                "{class:?} with {left} left"
            );
        }
    }
}
