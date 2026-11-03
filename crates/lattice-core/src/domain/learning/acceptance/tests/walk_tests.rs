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
        assert_eq!(walked(class, &[]), placed(class, false, false, node));
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
    let elsewhere = [parts(StandsElsewhere)];
    assert_eq!(
        walked(Sourced, &elsewhere),
        placed(Sourced, false, false, "s.recite")
    );
    let recited = then(&elsewhere, &[recite()]);
    assert_eq!(
        walked(Sourced, &recited),
        placed(Sourced, false, false, "s.citeread")
    );
    assert_eq!(
        walked(Sourced, &then(&recited, &[parts(Stands)])),
        Walked::Ended(End::Resolved)
    );
    assert_eq!(
        walked(Sourced, &then(&recited, &[parts(Unproven)])),
        placed(Sourced, false, false, "s.rewrite")
    );
}

#[test]
fn a_cited_statement_whose_parts_are_all_common_loses_its_citation_and_meets_the_common_bar() {
    let all_common = [parts(AllCommon)];
    assert_eq!(
        walked(Sourced, &all_common),
        placed(Sourced, false, false, "s.uncite")
    );
    let uncited = then(&all_common, &[uncite()]);
    assert_eq!(
        walked(Sourced, &uncited),
        placed(Common, true, false, "c.read")
    );
    assert_eq!(
        walked(Sourced, &then(&uncited, &[knowledge(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
}

#[test]
fn a_statement_that_lost_its_citation_and_needs_a_source_is_rewritten_not_sent_back() {
    let uncited = [parts(AllCommon), uncite()];
    assert_eq!(
        walked(Sourced, &then(&uncited, &[knowledge(NeedsSource)])),
        placed(Common, true, false, "c.rewrite")
    );
}

#[test]
fn a_statement_that_lost_its_citation_and_is_refuted_has_its_passages_read() {
    let uncited = [parts(AllCommon), uncite()];
    assert_eq!(
        walked(Sourced, &then(&uncited, &[knowledge(Refuted)])),
        placed(Common, true, false, "c.ground")
    );
}

#[test]
fn a_statement_nothing_states_is_researched_and_read_on_its_page() {
    for first in [Unproven, Refuted] {
        let unproven = [parts(first)];
        assert_eq!(
            walked(Sourced, &unproven),
            placed(Sourced, false, false, "s.research")
        );
        let researched = then(&unproven, &[research(Saved)]);
        assert_eq!(
            walked(Sourced, &researched),
            placed(Sourced, false, false, "s.reread")
        );
        assert_eq!(
            walked(Sourced, &then(&researched, &[page(Stands)])),
            Walked::Ended(End::Resolved)
        );
    }
}

#[test]
fn research_that_finds_nothing_is_followed_by_the_same_reading() {
    let nothing = [parts(Unproven), research(FoundNothing)];
    assert_eq!(
        walked(Sourced, &nothing),
        placed(Sourced, false, false, "s.reread")
    );
}

#[test]
fn a_page_that_stands_elsewhere_after_research_is_recited() {
    let found = [parts(Unproven), research(Saved), page(StandsElsewhere)];
    assert_eq!(
        walked(Sourced, &found),
        placed(Sourced, false, false, "s.recite")
    );
}

#[test]
fn a_statement_the_research_does_not_rescue_is_rewritten_then_read_again() {
    for failed in [Unproven, Refuted] {
        let failed = [parts(Unproven), research(Saved), page(failed)];
        assert_eq!(
            walked(Sourced, &failed),
            placed(Sourced, false, false, "s.rewrite")
        );
        let rewritten = then(&failed, &[sourced_rewrite(Rewritten)]);
        assert_eq!(
            walked(Sourced, &rewritten),
            placed(Sourced, false, false, "s.final")
        );
        assert_eq!(
            walked(Sourced, &then(&rewritten, &[parts(Stands)])),
            Walked::Ended(End::Resolved)
        );
    }
}

#[test]
fn a_rewrite_that_could_not_be_applied_is_tried_once_more() {
    let failed = [parts(Unproven), research(Saved), page(Unproven)];
    let refused = then(&failed, &[sourced_rewrite(NotApplied)]);
    assert_eq!(
        walked(Sourced, &refused),
        placed(Sourced, false, false, "s.rewrite.again")
    );
    assert_eq!(
        walked(Sourced, &then(&refused, &[sourced_rewrite(Rewritten)])),
        placed(Sourced, false, false, "s.final")
    );
    assert_eq!(
        walked(Sourced, &then(&refused, &[sourced_rewrite(NotApplied)])),
        placed(Sourced, false, false, "s.cut")
    );
}

#[test]
fn a_sourced_statement_that_fails_every_rung_is_removed() {
    let paths: [&[Taken]; 3] = [
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
            research(Saved),
            page(Unproven),
            sourced_rewrite(Rewritten),
            parts(Unproven),
            cut(Outcome::Cut),
        ],
        &[
            parts(StandsElsewhere),
            recite(),
            parts(Refuted),
            sourced_rewrite(Rewritten),
            parts(AllCommon),
            cut(Outcome::Cut),
        ],
    ];
    for path in paths {
        assert_eq!(walked(Sourced, path), REMOVED);
    }
}

#[test]
fn a_sentence_that_cannot_be_cut_takes_its_section_with_it() {
    let path = [
        parts(Unproven),
        research(Saved),
        page(Unproven),
        sourced_rewrite(NotApplied),
        sourced_rewrite(NotApplied),
        cut(CannotCut),
    ];
    assert_eq!(walked(Sourced, &path), REMOVED_WITH_SECTION);
}

#[test]
fn a_statement_that_changed_class_by_evidence_is_never_sent_back() {
    let needs_a_source = [knowledge(NeedsSource)];
    assert_eq!(
        walked(Common, &needs_a_source),
        placed(Sourced, true, false, "s.read")
    );
    assert_eq!(
        walked(Common, &then(&needs_a_source, &[parts(Stands)])),
        Walked::Ended(End::Resolved)
    );
    // All common again: it researches, the one change of class being spent.
    assert_eq!(
        walked(Common, &then(&needs_a_source, &[parts(AllCommon)])),
        placed(Sourced, true, false, "s.research")
    );
    assert_eq!(
        walked(Reasoning, &[consistency(NeedsSource), parts(AllCommon)]),
        placed(Sourced, true, false, "s.research")
    );
}

#[test]
fn an_explanation_its_passages_and_the_samples_leave_alone_is_consistent() {
    assert_eq!(
        walked(Reasoning, &[consistency(Stands)]),
        placed(Reasoning, false, false, "r.veto")
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
        placed(Reasoning, false, false, "r.ground")
    );
    assert_eq!(
        walked(Reasoning, &then(&vetoed, &[parts(Stands)])),
        Walked::Ended(End::Consistent)
    );
}

#[test]
fn a_vetoed_explanation_its_passages_do_not_ground_is_rewritten() {
    let vetoed = [consistency(Stands), veto(Refuted)];
    for read in [AllCommon, Unproven, Refuted] {
        assert_eq!(
            walked(Reasoning, &then(&vetoed, &[parts(read)])),
            placed(Reasoning, false, false, "r.rewrite")
        );
    }
}

#[test]
fn an_explanation_a_passage_contradicts_is_rewritten_without_the_samples() {
    assert_eq!(
        walked(Reasoning, &[consistency(Refuted)]),
        placed(Reasoning, false, false, "r.rewrite")
    );
}

#[test]
fn an_explanation_asserting_a_specific_the_passages_lack_becomes_sourced() {
    assert_eq!(
        walked(Reasoning, &[consistency(NeedsSource)]),
        placed(Sourced, true, false, "s.read")
    );
}

#[test]
fn a_reasoning_statement_that_has_changed_class_is_rewritten_not_sent_back() {
    let policy = StandardPolicy::new();
    let next = policy
        .graph(Reasoning)
        .node(NodeId("r.read"))
        .and_then(|node| node.next(NeedsSource))
        .unwrap();
    assert_eq!(
        follow(&policy, position(Reasoning, false, false, "r.read"), next),
        Moved::To(position(Sourced, true, false, "s.read"))
    );
    assert_eq!(
        follow(&policy, position(Reasoning, true, false, "r.read"), next),
        Moved::To(position(Reasoning, true, false, "r.rewrite"))
    );
}

#[test]
fn a_rewritten_explanation_is_read_again_and_vetoed_again() {
    let rewritten = [consistency(Refuted), reasoning_rewrite(Rewritten)];
    assert_eq!(
        walked(Reasoning, &rewritten),
        placed(Reasoning, false, false, "r.final")
    );
    let read = then(&rewritten, &[consistency(Stands)]);
    assert_eq!(
        walked(Reasoning, &read),
        placed(Reasoning, false, false, "r.reveto")
    );
    assert_eq!(
        walked(Reasoning, &then(&read, &[veto(Stands)])),
        Walked::Ended(End::Consistent)
    );
    let vetoed = then(&read, &[veto(Refuted)]);
    assert_eq!(
        walked(Reasoning, &vetoed),
        placed(Reasoning, false, false, "r.reground")
    );
    assert_eq!(
        walked(Reasoning, &then(&vetoed, &[parts(Stands)])),
        Walked::Ended(End::Consistent)
    );
}

#[test]
fn an_explanation_that_fails_every_rung_is_removed() {
    let paths: [&[Taken]; 4] = [
        &[
            consistency(Refuted),
            reasoning_rewrite(NotApplied),
            reasoning_rewrite(NotApplied),
            cut(Outcome::Cut),
        ],
        &[
            consistency(Refuted),
            reasoning_rewrite(Rewritten),
            consistency(NeedsSource),
            cut(Outcome::Cut),
        ],
        &[
            consistency(Stands),
            veto(Refuted),
            parts(Unproven),
            reasoning_rewrite(Rewritten),
            consistency(Stands),
            veto(Refuted),
            parts(AllCommon),
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
        REMOVED_WITH_SECTION
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
        placed(Common, false, false, "c.ground")
    );
    assert_eq!(
        walked(Common, &then(&refuted, &[parts(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
}

#[test]
fn a_common_statement_the_passages_do_not_ground_is_rewritten_before_any_cut() {
    for read in [AllCommon, Unproven, Refuted] {
        assert_eq!(
            walked(Common, &[knowledge(Refuted), parts(read)]),
            placed(Common, false, false, "c.rewrite")
        );
    }
}

#[test]
fn a_common_statement_that_needs_a_source_becomes_sourced() {
    let needs_a_source = [knowledge(NeedsSource)];
    assert_eq!(
        walked(Common, &needs_a_source),
        placed(Sourced, true, false, "s.read")
    );
}

#[test]
fn a_rewritten_common_statement_is_read_again_and_passes_or_is_cut() {
    let rewritten = [
        knowledge(Refuted),
        parts(Unproven),
        field_rewrite(Rewritten),
    ];
    assert_eq!(
        walked(Common, &rewritten),
        placed(Common, false, false, "c.final")
    );
    assert_eq!(
        walked(Common, &then(&rewritten, &[knowledge(Stands)])),
        Walked::Ended(End::GeneralKnowledge)
    );
    let refused_once = [
        knowledge(Refuted),
        parts(Unproven),
        field_rewrite(NotApplied),
        field_rewrite(Rewritten),
        knowledge(Refuted),
        parts(Stands),
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
                knowledge(NeedsSource),
                field_rewrite(Rewritten)
            ]
        ),
        placed(Common, true, false, "c.final")
    );
}

#[test]
fn a_common_statement_that_fails_every_rung_is_removed() {
    let paths: [&[Taken]; 3] = [
        &[
            knowledge(Refuted),
            parts(Refuted),
            field_rewrite(NotApplied),
            field_rewrite(NotApplied),
            cut(Outcome::Cut),
        ],
        &[
            knowledge(Refuted),
            parts(Unproven),
            field_rewrite(Rewritten),
            knowledge(Refuted),
            parts(AllCommon),
            cut(Outcome::Cut),
        ],
        &[
            knowledge(Refuted),
            parts(Unproven),
            field_rewrite(Rewritten),
            knowledge(NeedsSource),
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
                parts(Unproven),
                field_rewrite(Rewritten),
                knowledge(NeedsSource),
                cut(CannotCut)
            ]
        ),
        REMOVED_WITH_SECTION
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
        placed(Internal, false, false, "i.ground")
    );
    assert_eq!(
        walked(Internal, &then(&vetoed, &[parts(Stands)])),
        Walked::Ended(End::NotAClaim)
    );
}

#[test]
fn a_rewritten_internal_statement_passes_the_veto_again_or_is_cut() {
    let rewritten = [veto(Refuted), parts(Unproven), field_rewrite(Rewritten)];
    assert_eq!(
        walked(Internal, &rewritten),
        placed(Internal, false, false, "i.final")
    );
    assert_eq!(
        walked(Internal, &then(&rewritten, &[veto(Stands)])),
        Walked::Ended(End::NotAClaim)
    );
    assert_eq!(
        walked(Internal, &then(&rewritten, &[veto(Refuted), parts(Stands)])),
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
                parts(AllCommon),
                field_rewrite(Rewritten),
                veto(Refuted),
                parts(Refuted),
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
                parts(Unproven),
                field_rewrite(NotApplied),
                field_rewrite(NotApplied),
                cut(CannotCut)
            ]
        ),
        REMOVED_WITH_SECTION
    );
}

#[test]
fn an_unreadable_answer_leaves_the_statement_where_it_is_with_its_one_re_ask_spent() {
    assert_eq!(
        walked(Sourced, &[parts(Unjudged)]),
        placed(Sourced, false, true, "s.read")
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
        placed(Sourced, false, true, "s.research")
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
        placed(Sourced, false, true, "s.reread")
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
        placed(Sourced, true, true, "s.read")
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
            at: position(Sourced, false, false, "s.research"),
            taken: research(Unjudged)
        }
    );
}

#[test]
fn an_unreadable_answer_of_another_step_than_the_nodes_diverges() {
    assert_eq!(
        walked(Sourced, &[page(Unjudged)]),
        Walked::Diverged {
            at: position(Sourced, false, false, "s.read"),
            taken: page(Unjudged)
        }
    );
}

#[test]
fn a_step_the_node_does_not_take_diverges() {
    assert_eq!(
        walked(Sourced, &[research(Saved)]),
        Walked::Diverged {
            at: position(Sourced, false, false, "s.read"),
            taken: research(Saved)
        }
    );
    // A rewrite toward another goal is another step.
    let wrong_goal = rewrite(Goal::AsTheFieldHasIt, Rewritten);
    assert_eq!(
        walked(
            Sourced,
            &[parts(Unproven), research(Saved), page(Unproven), wrong_goal]
        ),
        Walked::Diverged {
            at: position(Sourced, false, false, "s.rewrite"),
            taken: wrong_goal
        }
    );
}

#[test]
fn an_outcome_the_node_has_no_edge_for_diverges() {
    assert_eq!(
        walked(Sourced, &[parts(Outcome::Done)]),
        Walked::Diverged {
            at: position(Sourced, false, false, "s.read"),
            taken: parts(Outcome::Done)
        }
    );
    assert_eq!(
        walked(Reasoning, &[consistency(AllCommon)]),
        Walked::Diverged {
            at: position(Reasoning, false, false, "r.read"),
            taken: consistency(AllCommon)
        }
    );
}

#[test]
fn a_step_taken_after_an_end_diverges_at_the_position_the_statement_left() {
    assert_eq!(
        walked(Sourced, &[parts(Stands), research(Saved)]),
        Walked::Diverged {
            at: position(Sourced, false, false, "s.read"),
            taken: research(Saved)
        }
    );
}

#[test]
fn a_second_change_of_class_diverges() {
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
            at: position(Common, true, false, "c.read"),
            taken: knowledge(NeedsSource)
        }
    );
}
