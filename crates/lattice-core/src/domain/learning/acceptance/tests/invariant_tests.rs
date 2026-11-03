//! Each invariant of the design's §2.5 against the standard policy with one
//! edge changed, so that it breaks that invariant and no other.

use super::*;
use crate::domain::learning::acceptance::invariants::ceiling_rises;
use crate::domain::learning::acceptance::Outcome::{
    AllCommon, Done, NotApplied, Refuted, Stands, StandsElsewhere, Unjudged, Unproven,
};
use crate::domain::learning::claim_review::Class::{Common, Internal, Sourced};

/// What the standard policy breaks with `edit` applied to `class`'s node
/// `name`.
fn broken(class: Class, name: &'static str, edit: impl FnOnce(Node) -> Node) -> Vec<Violation> {
    violations(&changed(class, name, edit))
}

fn id(name: &'static str) -> NodeId {
    NodeId(name)
}

#[test]
fn an_edge_to_a_node_the_graph_lacks_is_a_violation() {
    assert_eq!(
        broken(Sourced, "s.recite", |node| {
            node.on(&[Done], Next::to("s.nowhere"))
        }),
        vec![Violation::MissingNode {
            class: Sourced,
            node: id("s.recite"),
            missing: id("s.nowhere"),
        }]
    );
}

#[test]
fn a_statement_that_can_come_back_to_a_node_it_left_breaks_the_first_invariant() {
    assert_eq!(
        broken(Sourced, "s.recite", |node| {
            node.on(&[Done], Next::to("s.read"))
        }),
        vec![Violation::Cycle {
            class: Sourced,
            node: id("s.read"),
        }]
    );
}

#[test]
fn a_statement_that_can_change_class_twice_breaks_the_first_invariant() {
    assert_eq!(
        broken(Common, "c.read", |node| {
            node.on(&[Outcome::NeedsSource], Next::enter(Sourced))
        }),
        vec![Violation::Cycle {
            class: Common,
            node: id("c.read"),
        }]
    );
}

#[test]
fn a_statement_cut_without_a_rewrite_breaks_the_second_invariant() {
    assert_eq!(
        broken(Sourced, "s.read", |node| {
            node.on(&[Unproven], Next::to("s.cut"))
        }),
        vec![Violation::CutWithoutRewrite {
            class: Sourced,
            node: id("s.cut"),
        }]
    );
}

#[test]
fn a_closed_book_refusal_that_reaches_a_rewrite_unread_breaks_the_third_invariant() {
    assert_eq!(
        broken(Common, "c.read", |node| {
            node.on(&[Refuted], Next::to("c.rewrite"))
        }),
        vec![Violation::VetoUnread {
            class: Common,
            node: id("c.read"),
        }]
    );
}

#[test]
fn a_closed_book_refusal_that_ends_the_statement_breaks_the_third_invariant() {
    assert_eq!(
        broken(Internal, "i.veto", |node| {
            node.on(&[Refuted], Next::End(End::NotAClaim))
        }),
        vec![Violation::VetoUnread {
            class: Internal,
            node: id("i.veto"),
        }]
    );
}

#[test]
fn a_first_reading_that_answers_a_citation_fault_with_a_rewrite_breaks_the_fourth_invariant() {
    assert_eq!(
        broken(Sourced, "s.read", |node| {
            node.on(&[AllCommon], Next::to("s.rewrite"))
        }),
        vec![Violation::CitationFaultNotRepaired {
            class: Sourced,
            node: id("s.read"),
            outcome: AllCommon,
        }]
    );
    assert_eq!(
        broken(Sourced, "s.read", |node| {
            node.on(&[StandsElsewhere], Next::to("s.rewrite"))
        }),
        vec![Violation::CitationFaultNotRepaired {
            class: Sourced,
            node: id("s.read"),
            outcome: StandsElsewhere,
        }]
    );
}

#[test]
fn a_first_change_must_repair_the_citation_and_only_all_common_may_research_after_it() {
    assert_eq!(
        broken(Sourced, "s.read", |node| {
            node.on(
                &[AllCommon],
                Next::first_change(Target::node("s.uncite"), "s.rewrite"),
            )
        }),
        vec![Violation::CitationFaultNotRepaired {
            class: Sourced,
            node: id("s.read"),
            outcome: AllCommon,
        }]
    );
    assert_eq!(
        broken(Sourced, "s.read", |node| {
            node.on(
                &[StandsElsewhere],
                Next::first_change(Target::node("s.recite"), "s.research"),
            )
        }),
        vec![Violation::CitationFaultNotRepaired {
            class: Sourced,
            node: id("s.read"),
            outcome: StandsElsewhere,
        }]
    );
}

#[test]
fn a_later_reading_may_answer_all_common_with_a_rewrite_or_a_cut() {
    let policy = StandardPolicy::new();
    let sourced = policy.graph(Sourced);
    for (name, to) in [("s.citeread", "s.rewrite"), ("s.final", "s.cut")] {
        let reading = sourced.node(id(name)).unwrap();
        assert_eq!(reading.next(AllCommon), Some(Next::to(to)));
    }
    assert_eq!(violations(&policy), Vec::<Violation>::new());
}

#[test]
fn an_unjudged_edge_breaks_the_fifth_invariant() {
    assert_eq!(
        broken(Sourced, "s.read", |node| {
            node.on(&[Unjudged], Next::to("s.research"))
        }),
        vec![Violation::UnjudgedEdge {
            class: Sourced,
            node: id("s.read"),
        }]
    );
}

#[test]
fn a_rewrite_whose_failure_goes_straight_to_the_cut_breaks_the_fifth_invariant() {
    assert_eq!(
        broken(Sourced, "s.rewrite", |node| {
            node.on(&[NotApplied], Next::to("s.cut"))
        }),
        vec![Violation::RewriteNotRetried {
            class: Sourced,
            node: id("s.rewrite"),
        }]
    );
}

#[test]
fn a_retry_whose_failure_goes_anywhere_but_the_cut_breaks_the_fifth_invariant() {
    assert_eq!(
        broken(Sourced, "s.rewrite.again", |node| {
            node.on(&[NotApplied], Next::to("s.final"))
        }),
        vec![Violation::RewriteNotRetried {
            class: Sourced,
            node: id("s.rewrite"),
        }]
    );
}

#[test]
fn an_end_that_is_not_the_classes_own_breaks_the_sixth_invariant() {
    assert_eq!(
        broken(Sourced, "s.read", |node| {
            node.on(&[Stands], Next::End(End::Consistent))
        }),
        vec![Violation::ForeignEnd {
            class: Sourced,
            node: id("s.read"),
            end: End::Consistent,
        }]
    );
}

#[test]
fn a_ceiling_that_rises_breaks_the_seventh_invariant() {
    let policy = StandardPolicy::new();
    let mut exact = |at: Position| most_calls(&policy, at);
    assert_eq!(ceiling_rises(&policy, &mut exact), Vec::<Violation>::new());

    // A ceiling that forgets the re-ask rises at every reading that can be
    // asked again.
    let mut forgetful = |at: Position| {
        most_calls(
            &policy,
            Position {
                reasked: true,
                ..at
            },
        )
    };
    let rises = ceiling_rises(&policy, &mut forgetful);
    assert!(rises
        .iter()
        .all(|violation| matches!(violation, Violation::CeilingRises { .. })));
    for (class, name) in [
        (Sourced, "s.read"),
        (Common, "c.final"),
        (Internal, "i.veto"),
    ] {
        assert!(
            rises.contains(&Violation::CeilingRises {
                class,
                node: id(name)
            }),
            "{name}"
        );
    }
}

#[test]
fn a_policy_that_loops_is_not_also_held_to_a_ceiling() {
    // `most_calls` is finite on it, but its figures are not a longest path.
    let policy = changed(Sourced, "s.recite", |node| {
        node.on(&[Done], Next::to("s.read"))
    });
    let found = violations(&policy);
    assert!(found
        .iter()
        .all(|violation| !matches!(violation, Violation::CeilingRises { .. })));
}
