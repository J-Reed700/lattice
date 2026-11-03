//! The call ceiling derived from the graphs: the longest path in calls and
//! the one re-ask, from an entry and from a post-rewrite node.

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
fn a_statement_at_its_entry_can_cost_the_longest_path_and_one_re_ask() {
    let policy = StandardPolicy::new();
    for (class, most) in [(Sourced, 14), (Reasoning, 15), (Common, 13), (Internal, 13)] {
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
    for (class, most) in [(Sourced, 11), (Reasoning, 12), (Common, 10), (Internal, 10)] {
        let spent = Position {
            reasked: true,
            ..start(&policy, class)
        };
        assert_eq!(most_calls(&policy, spent), most, "{class:?}");
    }
}

#[test]
fn a_neighbours_rewrite_can_cost_the_longest_path_from_the_post_rewrite_node() {
    let policy = StandardPolicy::new();
    for (class, most) in [(Sourced, 1), (Reasoning, 5), (Common, 4), (Internal, 4)] {
        assert_eq!(after_rewrite_calls(&policy, class), most, "{class:?}");
    }
}

#[test]
fn a_spent_re_ask_lowers_the_ceiling_by_the_costliest_reading_ahead() {
    let policy = StandardPolicy::new();
    let ground = position(Common, false, false, "c.ground");
    assert_eq!(most_calls(&policy, ground), 10);
    assert_eq!(
        most_calls(
            &policy,
            Position {
                reasked: true,
                ..ground
            }
        ),
        7
    );
    let recite = position(Sourced, false, false, "s.recite");
    assert_eq!(most_calls(&policy, recite), 5);
    assert_eq!(
        most_calls(
            &policy,
            Position {
                reasked: true,
                ..recite
            }
        ),
        4
    );
}

#[test]
fn a_statement_that_changed_class_costs_what_the_new_class_costs_from_where_it_entered() {
    let policy = StandardPolicy::new();
    assert_eq!(
        most_calls(&policy, position(Common, true, false, "c.read")),
        13
    );
    assert_eq!(
        most_calls(&policy, position(Sourced, true, false, "s.read")),
        7
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
