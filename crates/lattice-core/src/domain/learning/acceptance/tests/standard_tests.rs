//! The standard policy against the design's tables: its nodes, steps, costs
//! and edges, and the invariants it holds.

use super::*;
use crate::domain::learning::claim_review::Class::{Common, Internal, Reasoning, Sourced};

#[test]
fn the_standard_policy_breaks_no_invariant() {
    assert_eq!(violations(&StandardPolicy::new()), Vec::<Violation>::new());
}

#[test]
fn the_standard_policy_is_acceptance_v1() {
    assert_eq!(StandardPolicy::new().version(), "acceptance-v1");
    assert_eq!(STANDARD_POLICY, "acceptance-v1");
}

#[test]
fn each_graph_enters_at_its_first_reading_and_continues_a_rewrite_at_its_post_rewrite_node() {
    let policy = StandardPolicy::new();
    for (class, entry, after_rewrite) in [
        (Sourced, "s.read", "s.final"),
        (Reasoning, "r.read", "r.final"),
        (Common, "c.read", "c.final"),
        (Internal, "i.veto", "i.final"),
    ] {
        let graph = policy.graph(class);
        assert_eq!(graph.class(), class);
        assert_eq!(graph.entry(), NodeId(entry));
        assert_eq!(graph.after_rewrite(), NodeId(after_rewrite));
    }
}

#[test]
fn the_standard_policys_nodes_are_the_designs_tables() {
    let parts = Step::Read(Reading::Parts);
    let page = Step::Read(Reading::Page);
    let consistency = Step::Read(Reading::Consistency);
    let knowledge = Step::Read(Reading::Knowledge);
    let veto = Step::Read(Reading::Veto);
    let sourced_rewrite = Step::Rewrite(Goal::WhatThePassageStates);
    let reasoning_rewrite = Step::Rewrite(Goal::AgreeWithThePassages);
    let field_rewrite = Step::Rewrite(Goal::AsTheFieldHasIt);
    let rows: Vec<(Class, &str, Step, u32)> = vec![
        (Sourced, "s.read", parts, 1),
        (Sourced, "s.uncite", Step::Uncite, 0),
        (Sourced, "s.research", Step::Research, 0),
        (Sourced, "s.reread", page, 1),
        (Sourced, "s.recite", Step::Recite, 0),
        (Sourced, "s.citeread", parts, 1),
        (Sourced, "s.rewrite", sourced_rewrite, 1),
        (Sourced, "s.rewrite.again", sourced_rewrite, 1),
        (Sourced, "s.final", parts, 1),
        (Sourced, "s.cut", Step::Cut, 0),
        (Reasoning, "r.read", consistency, 1),
        (Reasoning, "r.veto", veto, 3),
        (Reasoning, "r.ground", parts, 1),
        (Reasoning, "r.rewrite", reasoning_rewrite, 1),
        (Reasoning, "r.rewrite.again", reasoning_rewrite, 1),
        (Reasoning, "r.final", consistency, 1),
        (Reasoning, "r.reveto", veto, 3),
        (Reasoning, "r.reground", parts, 1),
        (Reasoning, "r.cut", Step::Cut, 0),
        (Common, "c.read", knowledge, 3),
        (Common, "c.ground", parts, 1),
        (Common, "c.rewrite", field_rewrite, 1),
        (Common, "c.rewrite.again", field_rewrite, 1),
        (Common, "c.final", knowledge, 3),
        (Common, "c.reground", parts, 1),
        (Common, "c.cut", Step::Cut, 0),
        (Internal, "i.veto", veto, 3),
        (Internal, "i.ground", parts, 1),
        (Internal, "i.rewrite", field_rewrite, 1),
        (Internal, "i.rewrite.again", field_rewrite, 1),
        (Internal, "i.final", veto, 3),
        (Internal, "i.reground", parts, 1),
        (Internal, "i.cut", Step::Cut, 0),
    ];
    let policy = StandardPolicy::new();
    for &(class, name, step, calls) in &rows {
        let node = policy.graph(class).node(NodeId(name)).expect(name);
        assert_eq!((node.step, node.calls), (step, calls), "{name}");
    }
    for class in CLASSES {
        let named = rows.iter().filter(|row| row.0 == class).count();
        assert_eq!(policy.graph(class).nodes().len(), named, "{class:?}");
    }
}

#[test]
fn the_edges_that_change_a_class_are_first_changes() {
    let policy = StandardPolicy::new();
    let edge = |class: Class, node: &'static str, outcome: Outcome| {
        policy
            .graph(class)
            .node(NodeId(node))
            .and_then(|node| node.next(outcome))
    };
    assert_eq!(
        edge(Sourced, "s.read", Outcome::AllCommon),
        Some(Next::first_change(Target::node("s.uncite"), "s.research"))
    );
    assert_eq!(
        edge(Sourced, "s.uncite", Outcome::Done),
        Some(Next::enter(Common))
    );
    assert_eq!(
        edge(Common, "c.read", Outcome::NeedsSource),
        Some(Next::first_change(Target::Enter(Sourced), "c.rewrite"))
    );
    assert_eq!(
        edge(Reasoning, "r.read", Outcome::NeedsSource),
        Some(Next::first_change(Target::Enter(Sourced), "r.rewrite"))
    );
}

#[test]
fn every_node_answers_every_outcome_its_step_can_give_and_none_answers_unjudged() {
    let policy = StandardPolicy::new();
    for class in CLASSES {
        for node in policy.graph(class).nodes() {
            for &outcome in node.step.outcomes() {
                assert!(
                    node.next(outcome).is_some(),
                    "{} has no edge for {outcome:?}",
                    node.name
                );
            }
            assert_eq!(node.next(Outcome::Unjudged), None, "{}", node.name);
        }
    }
}

#[test]
fn a_cited_sentence_read_as_a_page_can_stand_elsewhere() {
    let policy = StandardPolicy::new();
    let read = policy.graph(Sourced).node(NodeId("s.read")).unwrap();
    assert_eq!(read.step, Step::Read(Reading::Parts));
    assert_eq!(
        read.next(Outcome::StandsElsewhere),
        Some(Next::to("s.recite"))
    );
}

#[test]
fn a_cut_removes_the_sentence_and_a_sentence_that_cannot_be_cut_takes_its_section() {
    let policy = StandardPolicy::new();
    for (class, name) in [
        (Sourced, "s.cut"),
        (Reasoning, "r.cut"),
        (Common, "c.cut"),
        (Internal, "i.cut"),
    ] {
        let node = policy.graph(class).node(NodeId(name)).unwrap();
        assert_eq!(
            node.next(Outcome::Cut),
            Some(Next::End(End::Removed {
                section_goes: false
            }))
        );
        assert_eq!(
            node.next(Outcome::CannotCut),
            Some(Next::End(End::Removed { section_goes: true }))
        );
    }
}

#[test]
fn a_node_keeps_one_edge_per_outcome_and_otherwise_fills_only_the_outcomes_left() {
    let node = Node::new("x", Step::Read(Reading::Parts), 1)
        .on(&[Outcome::Stands], Next::End(End::Resolved))
        .on(&[Outcome::Stands], Next::End(End::Consistent))
        .otherwise(Next::to("y"));
    assert_eq!(node.next(Outcome::Stands), Some(Next::End(End::Consistent)));
    for outcome in [Outcome::AllCommon, Outcome::Unproven, Outcome::Refuted] {
        assert_eq!(node.next(outcome), Some(Next::to("y")));
    }
    assert_eq!(node.next(Outcome::StandsElsewhere), None);
    assert_eq!(node.edges().len(), 4);
}

#[test]
fn a_graph_holds_one_node_per_name() {
    let graph = Graph::new(Sourced, "a", "a")
        .with(Node::new("a", Step::Cut, 0))
        .with(Node::new("a", Step::Research, 1));
    assert_eq!(graph.nodes().len(), 1);
    assert_eq!(
        graph.node(NodeId("a")).map(|node| node.step),
        Some(Step::Research)
    );
    assert_eq!(graph.node(NodeId("b")), None);
}

#[test]
fn a_reading_a_goal_and_an_outcome_are_stored_in_snake_case() {
    assert_eq!(
        serde_json::to_value(Reading::Consistency).unwrap(),
        serde_json::json!("consistency")
    );
    assert_eq!(
        serde_json::to_value(Goal::WhatThePassageStates).unwrap(),
        serde_json::json!("what_the_passage_states")
    );
    for outcome in [
        Outcome::Stands,
        Outcome::StandsElsewhere,
        Outcome::AllCommon,
        Outcome::Unproven,
        Outcome::Refuted,
        Outcome::NeedsSource,
        Outcome::Unjudged,
        Outcome::Done,
        Outcome::Saved,
        Outcome::FoundNothing,
        Outcome::Rewritten,
        Outcome::NotApplied,
        Outcome::Cut,
        Outcome::CannotCut,
    ] {
        let stored = serde_json::to_value(outcome).unwrap();
        assert_eq!(serde_json::from_value::<Outcome>(stored).unwrap(), outcome);
    }
    assert_eq!(
        serde_json::to_value(Outcome::StandsElsewhere).unwrap(),
        serde_json::json!("stands_elsewhere")
    );
}
