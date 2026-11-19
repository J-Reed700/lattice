//! The standard policy against the design's tables: its nodes, steps, costs
//! and edges, and the invariants it holds.

use super::*;
use crate::domain::learning::claim_review::Class::{Common, Internal, Reasoning, Sourced};

#[test]
fn the_standard_policy_breaks_no_invariant() {
    assert_eq!(violations(&StandardPolicy::new()), Vec::<Violation>::new());
}

#[test]
fn the_standard_policy_is_acceptance_v4() {
    assert_eq!(StandardPolicy::new().version(), "acceptance-v4");
    assert_eq!(STANDARD_POLICY.as_str(), "acceptance-v4");
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
    let ground = Step::Read(Reading::Ground);
    let page = Step::Read(Reading::Page);
    let consistency = Step::Read(Reading::Consistency);
    let knowledge = Step::Read(Reading::Knowledge);
    let veto = Step::Read(Reading::Veto);
    let sourced_rewrite = Step::Rewrite(Goal::WhatThePassageStates);
    let reasoning_rewrite = Step::Rewrite(Goal::AgreeWithThePassages);
    let correct_rewrite = Step::Rewrite(Goal::CorrectWhatWasFound);
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
        (Sourced, "s.final.uncite", Step::Uncite, 0),
        (Sourced, "s.cut", Step::Cut, 0),
        (Reasoning, "r.read", consistency, 1),
        (Reasoning, "r.veto", veto, 3),
        (Reasoning, "r.ground", ground, 1),
        (Reasoning, "r.correct", correct_rewrite, 1),
        (Reasoning, "r.correct.again", correct_rewrite, 1),
        (Reasoning, "r.rewrite", reasoning_rewrite, 1),
        (Reasoning, "r.rewrite.again", reasoning_rewrite, 1),
        (Reasoning, "r.final", consistency, 1),
        (Reasoning, "r.reveto", veto, 3),
        (Reasoning, "r.reground", ground, 1),
        (Reasoning, "r.cut", Step::Cut, 0),
        (Common, "c.read", knowledge, 3),
        (Common, "c.ground", ground, 1),
        (Common, "c.rewrite", field_rewrite, 1),
        (Common, "c.rewrite.again", field_rewrite, 1),
        (Common, "c.final", knowledge, 3),
        (Common, "c.reground", ground, 1),
        (Common, "c.cut", Step::Cut, 0),
        (Internal, "i.veto", veto, 3),
        (Internal, "i.ground", ground, 1),
        (Internal, "i.rewrite", field_rewrite, 1),
        (Internal, "i.rewrite.again", field_rewrite, 1),
        (Internal, "i.final", veto, 3),
        (Internal, "i.reground", ground, 1),
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
fn the_edges_that_change_a_class_are_first_entries_or_enter_after_a_rewrite() {
    let policy = StandardPolicy::new();
    let edge = |class: Class, node: &'static str, outcome: Outcome| {
        policy
            .graph(class)
            .node(NodeId(node))
            .and_then(|node| node.next(outcome))
    };
    assert_eq!(
        edge(Sourced, "s.read", Outcome::AllCommon),
        Some(Next::first_entry_by("s.uncite", Common, "s.research"))
    );
    for (node, outcome) in [
        ("s.reread", Outcome::Unproven),
        ("s.citeread", Outcome::Unproven),
        ("s.citeread", Outcome::AllCommon),
    ] {
        assert_eq!(
            edge(Sourced, node, outcome),
            Some(Next::first_entry_by("s.uncite", Common, End::Unverified)),
            "{node} {outcome:?}"
        );
    }
    assert_eq!(
        edge(Sourced, "s.uncite", Outcome::Done),
        Some(Next::enter(Common))
    );
    assert_eq!(
        edge(Sourced, "s.final", Outcome::AllCommon),
        Some(Next::to("s.final.uncite"))
    );
    assert_eq!(
        edge(Sourced, "s.final.uncite", Outcome::Done),
        Some(Next::enter_after_rewrite(Common))
    );
    assert_eq!(
        edge(Common, "c.read", Outcome::NeedsSource),
        Some(Next::first_entry(Sourced, End::Unverified))
    );
    assert_eq!(
        edge(Reasoning, "r.read", Outcome::NeedsSource),
        Some(Next::first_entry(Sourced, End::Unverified))
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
fn only_a_part_wise_reading_can_say_all_common_and_only_a_page_can_stand_elsewhere() {
    for reading in [
        Reading::Parts,
        Reading::Ground,
        Reading::Page,
        Reading::Consistency,
        Reading::Knowledge,
        Reading::Veto,
    ] {
        let outcomes = Step::Read(reading).outcomes();
        assert_eq!(
            outcomes.contains(&Outcome::AllCommon),
            reading == Reading::Parts,
            "{reading:?}"
        );
        assert_eq!(
            outcomes.contains(&Outcome::StandsElsewhere),
            reading == Reading::Page,
            "{reading:?}"
        );
    }
    assert_eq!(
        Step::Read(Reading::Ground).outcomes().to_vec(),
        vec![Outcome::Stands, Outcome::Refuted]
    );
    let policy = StandardPolicy::new();
    let read = policy.graph(Sourced).node(NodeId("s.read")).unwrap();
    assert_eq!(read.step, Step::Read(Reading::Parts));
    assert_eq!(read.next(Outcome::StandsElsewhere), None);
    // A contradiction its own citation names is overruled by nothing later:
    // it goes to its rewrite, not to research a silent page can answer.
    assert_eq!(read.next(Outcome::Refuted), Some(Next::to("s.rewrite")));
    assert_eq!(read.next(Outcome::Unproven), Some(Next::to("s.research")));
    let reread = policy.graph(Sourced).node(NodeId("s.reread")).unwrap();
    assert_eq!(
        reread.next(Outcome::StandsElsewhere),
        Some(Next::to("s.recite"))
    );
}

#[test]
fn a_ground_reading_has_two_edges_its_end_and_its_next_node() {
    let policy = StandardPolicy::new();
    for (class, name, end, refuted) in [
        (Reasoning, "r.ground", End::Consistent, "r.correct"),
        (Reasoning, "r.reground", End::Consistent, "r.cut"),
        (Common, "c.ground", End::GeneralKnowledge, "c.rewrite"),
        (Common, "c.reground", End::GeneralKnowledge, "c.cut"),
        (Internal, "i.ground", End::NotAClaim, "i.rewrite"),
        (Internal, "i.reground", End::NotAClaim, "i.cut"),
    ] {
        let node = policy.graph(class).node(NodeId(name)).unwrap();
        assert_eq!(node.step, Step::Read(Reading::Ground), "{name}");
        assert_eq!(node.edges().len(), 2, "{name}");
        assert_eq!(node.next(Outcome::Stands), Some(Next::End(end)), "{name}");
        assert_eq!(
            node.next(Outcome::Refuted),
            Some(Next::to(refuted)),
            "{name}"
        );
    }
}

#[test]
fn a_cut_removes_the_sentence_and_a_sentence_that_cannot_be_cut_stays_unverified() {
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
            Some(Next::End(End::Unverified))
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
fn a_node_name_read_back_from_a_record_is_the_graphs_own_node_id() {
    let policy = StandardPolicy::new();
    for class in CLASSES {
        let graph = policy.graph(class);
        for node in graph.nodes() {
            let name = String::from(node.name);
            assert_eq!(graph.id_of(&name), Some(node.id()), "{name}");
        }
    }
    let sourced = policy.graph(Sourced);
    assert_eq!(sourced.id_of("c.read"), None, "another class's node");
    assert_eq!(sourced.id_of("s.read.again"), None);
    assert_eq!(sourced.id_of(""), None);
}

#[test]
fn a_reading_a_goal_and_an_outcome_are_stored_in_snake_case() {
    assert_eq!(
        serde_json::to_value(Reading::Consistency).unwrap(),
        serde_json::json!("consistency")
    );
    assert_eq!(
        serde_json::to_value(Reading::Ground).unwrap(),
        serde_json::json!("ground")
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
