//! The call ceiling derived from the graphs: the longest path in calls, the
//! one re-ask and the rewrite the teaching review may still give, from an
//! entry and from a post-rewrite node.

use super::*;
use crate::domain::learning::claim_review::Class::{Common, Internal, Reasoning, Sourced};

/// Every position a statement can reach from a class's entry, the re-ask
/// spent or not.
fn positions(policy: &dyn AcceptancePolicy) -> Vec<Position> {
    let mut found: Vec<Position> = Vec::new();
    let mut stack: Vec<Position> = CLASSES.iter().map(|&class| start(policy, class)).collect();
    while let Some(at) = stack.pop() {
        if found.contains(&at) {
            continue;
        }
        found.push(at);
        let Some(node) = policy.graph(at.class).node(at.node) else {
            continue;
        };
        for &(_, next) in node.edges() {
            if let Moved::To(to) = follow(policy, at, next) {
                stack.push(to);
            }
        }
        if !at.reasked {
            stack.push(Position {
                reasked: true,
                ..at
            });
        }
    }
    found
}

#[test]
fn a_statement_at_its_entry_can_cost_the_longest_path_the_re_ask_and_a_teaching_rewrite() {
    let policy = StandardPolicy::new();
    for (class, most) in [(Sourced, 19), (Reasoning, 20), (Common, 19), (Internal, 16)] {
        assert_eq!(
            most_calls(&policy, start(&policy, class)),
            most,
            "{class:?}"
        );
    }
}

#[test]
fn a_statement_whose_re_ask_is_spent_can_cost_the_longest_path_alone() {
    let policy = StandardPolicy::new();
    for (class, most) in [(Sourced, 16), (Reasoning, 17), (Common, 16), (Internal, 13)] {
        let spent = Position {
            reasked: true,
            ..start(&policy, class)
        };
        assert_eq!(most_calls(&policy, spent), most, "{class:?}");
    }
}

#[test]
fn a_spent_re_ask_lowers_the_ceiling_by_the_costliest_reading_ahead() {
    let policy = StandardPolicy::new();
    let grounding = position(Common, &[], false, "c.ground", 2);
    assert_eq!(most_calls(&policy, grounding), 13);
    assert_eq!(
        most_calls(
            &policy,
            Position {
                reasked: true,
                ..grounding
            }
        ),
        10
    );
    let reciting = position(Sourced, &[], false, "s.recite", 2);
    assert_eq!(most_calls(&policy, reciting), 17);
    assert_eq!(
        most_calls(
            &policy,
            Position {
                reasked: true,
                ..reciting
            }
        ),
        14
    );
}

#[test]
fn a_statement_that_changed_its_class_costs_what_the_new_class_costs_from_where_it_entered() {
    let policy = StandardPolicy::new();
    assert_eq!(
        most_calls(
            &policy,
            position(Common, &[Class::Sourced], false, "c.read", 2)
        ),
        16
    );
    assert_eq!(
        most_calls(
            &policy,
            position(Sourced, &[Class::Common], false, "s.read", 2)
        ),
        16
    );
}

#[test]
fn along_every_edge_of_every_reachable_state_the_ceiling_never_rises() {
    let policy = StandardPolicy::new();
    for at in positions(&policy) {
        let node = policy.graph(at.class).node(at.node).unwrap();
        let here = most_calls(&policy, at);
        for &(outcome, next) in node.edges() {
            if let Moved::To(to) = follow(&policy, at, next) {
                assert!(
                    node.calls + most_calls(&policy, to) <= here,
                    "{at:?} on {outcome:?}"
                );
            }
        }
        if !at.reasked && matches!(node.step, Step::Read(_)) {
            let again = Position {
                reasked: true,
                ..at
            };
            assert!(
                node.calls + most_calls(&policy, again) <= here,
                "re-asking at {at:?}"
            );
        }
    }
}

#[test]
fn a_policy_that_loops_still_has_a_finite_ceiling() {
    let policy = changed(Sourced, "s.recite", |node| {
        node.on(&[Outcome::Done], Next::to("s.read"))
    });
    assert!(most_calls(&policy, start(&policy, Sourced)) > 0);
}

#[test]
fn each_rewrite_a_statement_has_left_adds_a_post_rewrite_check_to_its_ceiling() {
    let policy = StandardPolicy::new();
    // At the last veto of a Reasoning statement, its re-ask spent: the veto's
    // three calls and the ground reading, and for each rewrite left the
    // successor's consistency reading, veto and ground reading.
    for (left, most) in [(0, 4), (1, 9), (2, 14)] {
        let at = position(Reasoning, &[], true, "r.reveto", left);
        assert_eq!(most_calls(&policy, at), most, "{left} left");
    }
}

#[test]
fn a_statement_that_ended_can_cost_its_successors_readings() {
    let policy = StandardPolicy::new();
    let ending = position(Reasoning, &[], false, "r.reveto", 2);
    let successor = position(Reasoning, &[], false, "r.final", 1);
    assert!(most_calls(&policy, ending) >= 3 + most_calls(&policy, successor));
}
