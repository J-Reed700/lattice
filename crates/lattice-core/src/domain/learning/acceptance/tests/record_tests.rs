//! What the ledger records of each step: `decide`, where a rewrite is still
//! ahead, and the check that a record walks.

use super::*;
use crate::domain::learning::acceptance::Outcome::{
    AllCommon, FoundNothing, NeedsSource, NotApplied, Refuted, Rewritten, Saved, Stands,
    StandsElsewhere, Unjudged, Unproven,
};
use crate::domain::learning::claim_review::Class::{Common, Internal, Reasoning, Sourced};

fn named(name: &str, step: Step) -> Then {
    Then::Node {
        name: name.into(),
        step,
    }
}

fn field_rewrite(outcome: Outcome) -> Taken {
    rewrite(Goal::AsTheFieldHasIt, outcome)
}

/// `taken` as the ledger records it from `from`: each step with where
/// `decide` sends it, up to the first that does not fit.
fn recorded(from: Position, taken: &[Taken]) -> Vec<(Taken, Then)> {
    let policy = StandardPolicy::new();
    let mut at = Some(from);
    let mut steps = Vec::new();
    for &step in taken {
        let Some((then, next)) = at.and_then(|here| decide(&policy, here, step)) else {
            break;
        };
        steps.push((step, then));
        at = next;
    }
    steps
}

/// A path that reads a cited sentence, researches, recites, loses the citation
/// to Common's check, is refuted there, rewritten and passes.
fn long_path() -> Vec<Taken> {
    vec![
        parts(Unproven),
        research(Saved),
        page(StandsElsewhere),
        recite(),
        parts(AllCommon),
        uncite(),
        knowledge(Refuted),
        ground(Refuted),
        field_rewrite(Rewritten),
        knowledge(Stands),
    ]
}

#[test]
fn a_step_that_fits_is_sent_to_the_node_the_graph_names() {
    let policy = StandardPolicy::new();
    let first = start(&policy, Sourced);
    assert_eq!(
        decide(&policy, first, parts(Unproven)),
        Some((
            named("s.research", Step::Research),
            Some(position(Sourced, &[], false, "s.research", 2))
        ))
    );
    assert_eq!(
        decide(&policy, first, parts(Stands)),
        Some((Then::End(End::Resolved), None))
    );
    for (outcome, end) in [
        (
            Outcome::Cut,
            End::Removed {
                section_goes: false,
            },
        ),
        (Outcome::CannotCut, End::Unverified),
    ] {
        assert_eq!(
            decide(
                &policy,
                position(Sourced, &[], false, "s.cut", 0),
                cut(outcome)
            ),
            Some((Then::End(end), None))
        );
    }
}

#[test]
fn a_change_of_class_names_the_other_classs_node() {
    let policy = StandardPolicy::new();
    assert_eq!(
        decide(
            &policy,
            position(Sourced, &[], false, "s.uncite", 2),
            uncite()
        ),
        Some((
            named("c.read", Step::Read(Reading::Knowledge)),
            Some(position(Common, &[Class::Sourced], false, "c.read", 2))
        ))
    );
    // After a rewrite it is the other class's post-rewrite node, which has no
    // rewrite in it.
    assert_eq!(
        decide(
            &policy,
            position(Sourced, &[], false, "s.final.uncite", 1),
            uncite()
        ),
        Some((
            named("c.final", Step::Read(Reading::Knowledge)),
            Some(position(Common, &[Class::Sourced], false, "c.final", 1))
        ))
    );
    // Even a statement that has changed class may.
    assert_eq!(
        decide(
            &policy,
            position(Sourced, &[Class::Common], false, "s.final.uncite", 1),
            uncite()
        ),
        Some((
            named("c.final", Step::Read(Reading::Knowledge)),
            Some(position(Common, &[Class::Sourced], false, "c.final", 1))
        ))
    );
}

#[test]
fn the_re_ask_stays_at_the_node_with_the_re_ask_spent_and_the_next_one_stops() {
    let policy = StandardPolicy::new();
    let reading = named("s.read", Step::Read(Reading::Parts));
    assert_eq!(
        decide(&policy, start(&policy, Sourced), parts(Unjudged)),
        Some((reading, Some(position(Sourced, &[], true, "s.read", 2))))
    );
    assert_eq!(
        decide(
            &policy,
            position(Sourced, &[], true, "s.read", 2),
            parts(Unjudged)
        ),
        Some((Then::Stop, None))
    );
    // Only a reading can be asked again.
    assert_eq!(
        decide(
            &policy,
            position(Sourced, &[], false, "s.research", 2),
            research(Unjudged)
        ),
        None
    );
}

#[test]
fn a_step_that_does_not_fit_is_not_decided() {
    let policy = StandardPolicy::new();
    let first = start(&policy, Sourced);
    // Another step than the node's, and an outcome the node has no edge for.
    assert_eq!(decide(&policy, first, research(Saved)), None);
    assert_eq!(decide(&policy, first, parts(NotApplied)), None);
    assert_eq!(decide(&policy, first, ground(Stands)), None);
    // A second change of class at an entry.
    let twice = changed(Common, "c.read", |node| {
        node.on(&[NeedsSource], Next::enter(Sourced))
    });
    assert_eq!(
        decide(
            &twice,
            position(Common, &[Class::Sourced], false, "c.read", 2),
            knowledge(NeedsSource)
        ),
        None
    );
    // An edge to a node the graph does not have.
    let nowhere = changed(Sourced, "s.read", |node| {
        node.on(&[Unproven], Next::to("s.nowhere"))
    });
    assert_eq!(decide(&nowhere, first, parts(Unproven)), None);
}

#[test]
fn walking_from_a_position_is_walking_from_the_classs_start_when_it_is_the_start() {
    let policy = StandardPolicy::new();
    let taken = [parts(Unproven), research(FoundNothing), page(Stands)];
    assert_eq!(
        walk_from(&policy, start(&policy, Sourced), &taken),
        walk(&policy, Sourced, &taken)
    );
    assert_eq!(
        walk_from(&policy, start(&policy, Sourced), &taken),
        Walked::Ended(End::Resolved)
    );
    assert_eq!(
        walk_from(
            &policy,
            position(Sourced, &[], false, "s.final", 1),
            &[parts(AllCommon), uncite(), knowledge(Stands)]
        ),
        Walked::Ended(End::GeneralKnowledge)
    );
}

#[test]
fn a_rewrite_is_ahead_of_every_node_that_can_reach_one() {
    let policy = StandardPolicy::new();
    for (class, name) in [
        (Sourced, "s.read"),
        (Sourced, "s.research"),
        (Sourced, "s.reread"),
        (Sourced, "s.recite"),
        (Sourced, "s.citeread"),
        (Sourced, "s.rewrite"),
        (Sourced, "s.rewrite.again"),
        (Reasoning, "r.read"),
        (Reasoning, "r.veto"),
        (Reasoning, "r.ground"),
        (Reasoning, "r.correct"),
        (Reasoning, "r.rewrite"),
        (Common, "c.read"),
        (Common, "c.ground"),
        (Common, "c.rewrite"),
        (Internal, "i.veto"),
        (Internal, "i.ground"),
        (Internal, "i.rewrite.again"),
    ] {
        assert!(
            rewrite_ahead(&policy, position(class, &[], false, name, 2)),
            "{name}"
        );
    }
    // The uncite reaches Common's whole check by a change of class.
    assert!(rewrite_ahead(
        &policy,
        position(Sourced, &[], false, "s.uncite", 2)
    ));
    assert!(rewrite_ahead(
        &policy,
        position(Sourced, &[Class::Common], false, "s.read", 2)
    ));
}

#[test]
fn no_rewrite_is_ahead_of_a_post_rewrite_node_or_the_cut() {
    let policy = StandardPolicy::new();
    for (class, name) in [
        (Sourced, "s.final"),
        (Sourced, "s.final.uncite"),
        (Sourced, "s.cut"),
        (Reasoning, "r.final"),
        (Reasoning, "r.reveto"),
        (Reasoning, "r.reground"),
        (Reasoning, "r.cut"),
        (Common, "c.final"),
        (Common, "c.reground"),
        (Common, "c.cut"),
        (Internal, "i.final"),
        (Internal, "i.reground"),
        (Internal, "i.cut"),
    ] {
        for came_from in [&[][..], &[Class::Common][..], &[Class::Sourced][..]] {
            assert!(
                !rewrite_ahead(&policy, position(class, came_from, false, name, 0)),
                "{name}"
            );
        }
    }
}

#[test]
fn a_record_of_steps_the_graph_gives_walks() {
    let policy = StandardPolicy::new();
    let first = start(&policy, Sourced);
    let steps = recorded(first, &long_path());
    assert_eq!(steps.len(), 10);
    assert_eq!(check_record(&policy, first, &steps), Ok(()));
    assert_eq!(
        steps.last().map(|(_, then)| then),
        Some(&Then::End(End::GeneralKnowledge))
    );
    assert_eq!(check_record(&policy, first, &[]), Ok(()));
}

#[test]
fn a_then_the_graph_does_not_give_is_a_fault_at_its_step() {
    let policy = StandardPolicy::new();
    let first = start(&policy, Sourced);
    let mut steps = recorded(first, &long_path());
    let wrong = named("s.rewrite", Step::Rewrite(Goal::WhatThePassageStates));
    let right = std::mem::replace(&mut steps[2].1, wrong.clone());
    assert_eq!(right, named("s.recite", Step::Recite));
    assert_eq!(
        check_record(&policy, first, &steps),
        Err(RecordFault {
            index: 2,
            expected: Some(right),
            recorded: wrong,
        })
    );
}

#[test]
fn a_step_after_an_end_or_a_stop_is_a_fault() {
    let policy = StandardPolicy::new();
    let first = start(&policy, Sourced);
    let after_an_end = [
        (parts(Stands), Then::End(End::Resolved)),
        (
            research(Saved),
            named("s.reread", Step::Read(Reading::Page)),
        ),
    ];
    assert_eq!(
        check_record(&policy, first, &after_an_end),
        Err(RecordFault {
            index: 1,
            expected: None,
            recorded: named("s.reread", Step::Read(Reading::Page)),
        })
    );
    let after_a_stop = [
        (parts(Unjudged), named("s.read", Step::Read(Reading::Parts))),
        (parts(Unjudged), Then::Stop),
        (parts(Stands), Then::End(End::Resolved)),
    ];
    assert_eq!(
        check_record(&policy, first, &after_a_stop),
        Err(RecordFault {
            index: 2,
            expected: None,
            recorded: Then::End(End::Resolved),
        })
    );
}

#[test]
fn a_step_the_graph_does_not_have_is_a_fault() {
    let policy = StandardPolicy::new();
    let steps = [(
        research(Saved),
        named("s.reread", Step::Read(Reading::Page)),
    )];
    assert_eq!(
        check_record(&policy, start(&policy, Sourced), &steps),
        Err(RecordFault {
            index: 0,
            expected: None,
            recorded: named("s.reread", Step::Read(Reading::Page)),
        })
    );
}

#[test]
fn a_successor_starts_at_its_parents_post_rewrite_node() {
    let policy = StandardPolicy::new();
    let first = start(&policy, Sourced);
    let after = position(Sourced, &[], false, "s.final", 1);
    let taken = [parts(AllCommon), uncite(), knowledge(Stands)];
    let steps = recorded(after, &taken);
    assert_eq!(steps.len(), 3);
    assert_eq!(
        steps.first().map(|(_, then)| then),
        Some(&named("s.final.uncite", Step::Uncite))
    );
    assert_eq!(check_record(&policy, after, &steps), Ok(()));

    // The same record does not walk from the class's start: it would have
    // sent the statement to the uncite of its first reading.
    assert_eq!(
        check_record(&policy, first, &steps),
        Err(RecordFault {
            index: 0,
            expected: Some(named("s.uncite", Step::Uncite)),
            recorded: named("s.final.uncite", Step::Uncite),
        })
    );

    // And Common's successor starts at its own post-rewrite node.
    let common = position(Common, &[], false, "c.final", 1);
    let steps = recorded(common, &[knowledge(Refuted), ground(Stands)]);
    assert_eq!(
        steps,
        vec![
            (
                knowledge(Refuted),
                named("c.reground", Step::Read(Reading::Ground))
            ),
            (ground(Stands), Then::End(End::GeneralKnowledge)),
        ]
    );
    assert_eq!(check_record(&policy, common, &steps), Ok(()));
}

#[test]
fn a_record_that_changes_class_walks_through_the_change() {
    let policy = StandardPolicy::new();
    let first = start(&policy, Reasoning);
    let steps = recorded(
        first,
        &[
            consistency(NeedsSource),
            parts(Unproven),
            research(FoundNothing),
            page(Refuted),
            rewrite(Goal::WhatThePassageStates, Rewritten),
            parts(Stands),
        ],
    );
    assert_eq!(steps.len(), 6);
    assert_eq!(
        steps.first().map(|(_, then)| then),
        Some(&named("s.read", Step::Read(Reading::Parts)))
    );
    assert_eq!(check_record(&policy, first, &steps), Ok(()));
}
