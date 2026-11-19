//! Each invariant of the design's §2.5 against the standard policy with one
//! edge changed, so that it breaks that invariant and no other.

use super::*;
use crate::domain::learning::acceptance::invariants::ceiling_rises;
use crate::domain::learning::acceptance::Outcome::{
    AllCommon, Done, NeedsSource, NotApplied, Refuted, Stands, StandsElsewhere, Unjudged, Unproven,
};
use crate::domain::learning::claim_review::Class::{Common, Internal, Sourced};

/// `found` without the ninth invariant's violations, for a test of another
/// invariant whose edit also reaches a rewrite.
fn without_the_ninth(found: Vec<Violation>) -> Vec<Violation> {
    found
        .into_iter()
        .filter(|violation| !matches!(violation, Violation::RewriteWithoutRefutation { .. }))
        .collect()
}

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
fn a_statement_that_can_enter_a_class_twice_breaks_the_first_invariant() {
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
        vec![
            Violation::CutWithoutRewrite {
                class: Sourced,
                node: id("s.cut"),
            },
            Violation::RewriteWithoutRefutation {
                class: Sourced,
                node: id("s.cut"),
            },
        ]
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
fn a_ground_reading_replaced_by_a_part_wise_one_breaks_the_third_invariant() {
    assert_eq!(
        broken(Common, "c.ground", |_| {
            Node::new("c.ground", Step::Read(Reading::Parts), 1)
                .on(&[Stands], Next::End(End::GeneralKnowledge))
                .on(&[Refuted], Next::to("c.rewrite"))
        }),
        vec![Violation::VetoUnread {
            class: Common,
            node: id("c.read"),
        }]
    );
}

#[test]
fn a_citation_fault_answered_with_anything_but_the_remedy_breaks_the_fourth_invariant() {
    // `AllCommon` at the first reading, at the re-cited reading and after a
    // rewrite; `StandsElsewhere` at the page.
    for (name, outcome, to) in [
        ("s.read", AllCommon, "s.rewrite"),
        ("s.citeread", AllCommon, "s.rewrite"),
        ("s.final", AllCommon, "s.cut"),
        ("s.reread", StandsElsewhere, "s.rewrite"),
    ] {
        let found = broken(Sourced, name, |node| node.on(&[outcome], Next::to(to)));
        assert_eq!(
            without_the_ninth(found),
            vec![Violation::CitationFaultNotRepaired {
                class: Sourced,
                node: id(name),
                outcome,
            }],
            "{name}"
        );
    }
}

#[test]
fn a_first_entry_must_enter_by_the_remedy_but_its_otherwise_goes_where_it_must() {
    let found = broken(Sourced, "s.read", |node| {
        node.on(
            &[AllCommon],
            Next::first_entry_by("s.rewrite", Common, "s.research"),
        )
    });
    assert_eq!(
        without_the_ninth(found),
        vec![
            Violation::CitationFaultNotRepaired {
                class: Sourced,
                node: id("s.read"),
                outcome: AllCommon,
            },
            Violation::FirstEntryMisled {
                class: Sourced,
                node: id("s.read"),
            },
        ]
    );
    assert_eq!(
        broken(Sourced, "s.read", |node| {
            node.on(
                &[AllCommon],
                Next::first_entry_by("s.uncite", Common, "s.research"),
            )
        }),
        Vec::<Violation>::new()
    );
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
fn a_rewrites_successor_that_can_be_rewritten_again_breaks_the_eighth_invariant() {
    // The edit also loops (the first invariant) and skips the ground reading
    // (the third), so the eighth is looked for among them.
    let found = broken(Common, "c.final", |node| {
        node.on(&[Refuted], Next::to("c.rewrite"))
    });
    let rewritten_again = Violation::RewrittenTwice {
        class: Common,
        node: id("c.rewrite"),
    };
    assert!(found.contains(&rewritten_again), "{found:?}");
}

#[test]
fn a_rewrites_successor_that_changes_class_at_an_entry_breaks_the_eighth_invariant() {
    let found = broken(Sourced, "s.final.uncite", |node| {
        node.on(&[Done], Next::enter(Common))
    });
    let enters = Violation::RewrittenTwice {
        class: Sourced,
        node: id("s.final.uncite"),
    };
    assert!(found.contains(&enters), "{found:?}");

    // A first entry counts while the class has not been visited.
    let found = broken(Common, "c.final", |node| {
        node.on(&[NeedsSource], Next::first_entry(Sourced, "c.cut"))
    });
    let enters = Violation::RewrittenTwice {
        class: Common,
        node: id("c.final"),
    };
    assert!(found.contains(&enters), "{found:?}");
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
fn a_ceiling_that_ignores_the_successor_of_an_ended_statement_breaks_the_seventh_invariant() {
    let policy = StandardPolicy::new();
    assert_eq!(violations(&policy), Vec::<Violation>::new());

    // The ceiling before a statement's rewrites were counted: it holds no
    // rewrite for any statement, so a statement that ends with one left and
    // continues as a successor that reads again rises.
    let mut blind = |at: Position| {
        most_calls(
            &policy,
            Position {
                rewrites_left: 0,
                ..at
            },
        )
    };
    let rises = ceiling_rises(&policy, &mut blind);
    assert!(
        rises.contains(&Violation::CeilingRises {
            class: Class::Reasoning,
            node: id("r.reveto"),
        }),
        "{rises:?}"
    );
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

/// The rewrites and cuts reachable from an entry without crossing an edge on
/// `Refuted`, found without the invariant's own code.
fn reached_unrefuted(policy: &dyn AcceptancePolicy) -> Vec<(Class, &'static str)> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut stack: Vec<Position> = CLASSES.iter().map(|&class| start(policy, class)).collect();
    while let Some(at) = stack.pop() {
        if !seen.insert(at) {
            continue;
        }
        let Some(node) = policy.graph(at.class).node(at.node) else {
            continue;
        };
        if matches!(node.step, Step::Rewrite(_) | Step::Cut) {
            found.push((at.class, node.name));
        }
        for &(outcome, next) in node.edges() {
            if outcome == Refuted {
                continue;
            }
            if let Moved::To(to) = follow(policy, at, next) {
                stack.push(to);
            }
        }
    }
    found
}

#[test]
fn no_rewrite_or_cut_of_the_standard_policy_is_reached_without_a_refuted_edge() {
    assert_eq!(
        reached_unrefuted(&StandardPolicy::new()),
        Vec::<(Class, &str)>::new()
    );
}

#[test]
fn a_rewrite_reached_without_a_refuted_edge_breaks_the_ninth_invariant() {
    let found = broken(Sourced, "s.reread", |node| {
        node.on(&[Unproven], Next::to("s.rewrite"))
    });
    let rewrite = Violation::RewriteWithoutRefutation {
        class: Sourced,
        node: id("s.rewrite"),
    };
    assert!(found.contains(&rewrite), "{found:?}");

    // A first entry's `otherwise` counts too.
    let found = broken(Common, "c.read", |node| {
        node.on(
            &[Outcome::NeedsSource],
            Next::first_entry(Sourced, "c.rewrite"),
        )
    });
    let rewrite = Violation::RewriteWithoutRefutation {
        class: Common,
        node: id("c.rewrite"),
    };
    assert!(found.contains(&rewrite), "{found:?}");
}

#[test]
fn a_cut_reached_without_a_refuted_edge_breaks_the_ninth_invariant() {
    let found = broken(Class::Reasoning, "r.read", |node| {
        node.on(&[Outcome::NeedsSource], Next::to("r.cut"))
    });
    let cut = Violation::RewriteWithoutRefutation {
        class: Class::Reasoning,
        node: id("r.cut"),
    };
    assert!(found.contains(&cut), "{found:?}");
}

#[test]
fn a_first_entry_that_does_not_lead_into_its_class_breaks_the_tenth_invariant() {
    let misled = Violation::FirstEntryMisled {
        class: Sourced,
        node: id("s.read"),
    };
    // By a node that goes somewhere else, by an entry into another class and by
    // a node that has an edge that does not enter.
    let found = broken(Sourced, "s.read", |node| {
        node.on(
            &[AllCommon],
            Next::first_entry_by("s.research", Common, "s.research"),
        )
    });
    assert!(found.contains(&misled), "{found:?}");
    let found = broken(Sourced, "s.read", |node| {
        node.on(
            &[AllCommon],
            Next::FirstEntry {
                into: Common,
                by: Target::Enter(Class::Reasoning),
                otherwise: Otherwise::Node(id("s.research")),
            },
        )
    });
    assert!(found.contains(&misled), "{found:?}");
    let found = violations(&changed(Sourced, "s.uncite", |node| {
        node.on(&[Done], Next::to("s.research"))
    }));
    assert!(found.contains(&misled), "{found:?}");

    let standard = violations(&StandardPolicy::new());
    assert_eq!(standard, Vec::<Violation>::new());
}
