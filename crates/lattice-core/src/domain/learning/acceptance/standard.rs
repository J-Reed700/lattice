//! The standard policy: the four graphs of the design's §2.3, node for node.

use super::Outcome::{
    AllCommon, CannotCut, Done, FoundNothing, NeedsSource, NotApplied, Refuted, Rewritten, Saved,
    Stands, StandsElsewhere,
};
use super::{AcceptancePolicy, End, Goal, Graph, Next, Node, Outcome, Reading, Step, Target};
use crate::domain::learning::claim_review::Class;

/// The standard policy's version: part of every receipt's and checkpoint's
/// key, so a change to any of its graphs changes it.
pub const STANDARD_POLICY: &str = "acceptance-v1";

const PARTS: Step = Step::Read(Reading::Parts);
const PAGE: Step = Step::Read(Reading::Page);
const CONSISTENCY: Step = Step::Read(Reading::Consistency);
const KNOWLEDGE: Step = Step::Read(Reading::Knowledge);
const VETO: Step = Step::Read(Reading::Veto);

/// Sourced, Reasoning, Common and model-Internal statements, each checked by
/// its own graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandardPolicy {
    sourced: Graph,
    reasoning: Graph,
    common: Graph,
    internal: Graph,
}

impl StandardPolicy {
    pub fn new() -> Self {
        Self {
            sourced: sourced(),
            reasoning: reasoning(),
            common: common(),
            internal: internal(),
        }
    }
}

impl Default for StandardPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl AcceptancePolicy for StandardPolicy {
    fn version(&self) -> &str {
        STANDARD_POLICY
    }

    fn graph(&self, class: Class) -> &Graph {
        match class {
            Class::Sourced => &self.sourced,
            Class::Reasoning => &self.reasoning,
            Class::Common => &self.common,
            Class::Internal => &self.internal,
        }
    }
}

/// A rewrite toward `goal`: on to `rewritten`, or to `not_applied` when the
/// confinement could not apply it.
fn rewrite(
    name: &'static str,
    goal: Goal,
    rewritten: &'static str,
    not_applied: &'static str,
) -> Node {
    Node::new(name, Step::Rewrite(goal), 1)
        .on(&[Rewritten], Next::to(rewritten))
        .on(&[NotApplied], Next::to(not_applied))
}

/// A cut: the sentence, or its section when the sentence cannot be cut.
fn cut(name: &'static str) -> Node {
    Node::new(name, Step::Cut, 0)
        .on(
            &[Outcome::Cut],
            Next::End(End::Removed {
                section_goes: false,
            }),
        )
        .on(&[CannotCut], Next::End(End::Removed { section_goes: true }))
}

/// Entered by the writer's citation, the rule layer, the model, or evidence.
fn sourced() -> Graph {
    Graph::new(Class::Sourced, "s.read", "s.final")
        .with(
            Node::new("s.read", PARTS, 1)
                .on(&[Stands], Next::End(End::Resolved))
                .on(&[StandsElsewhere], Next::to("s.recite"))
                .on(
                    &[AllCommon],
                    Next::first_change(Target::node("s.uncite"), "s.research"),
                )
                .otherwise(Next::to("s.research")),
        )
        .with(Node::new("s.uncite", Step::Uncite, 0).on(&[Done], Next::enter(Class::Common)))
        .with(
            Node::new("s.research", Step::Research, 0)
                .on(&[Saved, FoundNothing], Next::to("s.reread")),
        )
        .with(
            Node::new("s.reread", PAGE, 1)
                .on(&[Stands], Next::End(End::Resolved))
                .on(&[StandsElsewhere], Next::to("s.recite"))
                .otherwise(Next::to("s.rewrite")),
        )
        .with(Node::new("s.recite", Step::Recite, 0).on(&[Done], Next::to("s.citeread")))
        .with(
            Node::new("s.citeread", PARTS, 1)
                .on(&[Stands], Next::End(End::Resolved))
                .otherwise(Next::to("s.rewrite")),
        )
        .with(rewrite(
            "s.rewrite",
            Goal::WhatThePassageStates,
            "s.final",
            "s.rewrite.again",
        ))
        .with(rewrite(
            "s.rewrite.again",
            Goal::WhatThePassageStates,
            "s.final",
            "s.cut",
        ))
        .with(
            Node::new("s.final", PARTS, 1)
                .on(&[Stands], Next::End(End::Resolved))
                .otherwise(Next::to("s.cut")),
        )
        .with(cut("s.cut"))
}

/// Entered by the model.
fn reasoning() -> Graph {
    Graph::new(Class::Reasoning, "r.read", "r.final")
        .with(
            Node::new("r.read", CONSISTENCY, 1)
                .on(&[Stands], Next::to("r.veto"))
                .on(&[Refuted], Next::to("r.rewrite"))
                .on(
                    &[NeedsSource],
                    Next::first_change(Target::Enter(Class::Sourced), "r.rewrite"),
                ),
        )
        .with(
            Node::new("r.veto", VETO, 3)
                .on(&[Stands], Next::End(End::Consistent))
                .on(&[Refuted], Next::to("r.ground")),
        )
        .with(
            Node::new("r.ground", PARTS, 1)
                .on(&[Stands], Next::End(End::Consistent))
                .otherwise(Next::to("r.rewrite")),
        )
        .with(rewrite(
            "r.rewrite",
            Goal::AgreeWithThePassages,
            "r.final",
            "r.rewrite.again",
        ))
        .with(rewrite(
            "r.rewrite.again",
            Goal::AgreeWithThePassages,
            "r.final",
            "r.cut",
        ))
        .with(
            Node::new("r.final", CONSISTENCY, 1)
                .on(&[Stands], Next::to("r.reveto"))
                .otherwise(Next::to("r.cut")),
        )
        .with(
            Node::new("r.reveto", VETO, 3)
                .on(&[Stands], Next::End(End::Consistent))
                .on(&[Refuted], Next::to("r.reground")),
        )
        .with(
            Node::new("r.reground", PARTS, 1)
                .on(&[Stands], Next::End(End::Consistent))
                .otherwise(Next::to("r.cut")),
        )
        .with(cut("r.cut"))
}

/// Entered by the model, or by evidence from Sourced (`AllCommon`, its
/// citation dropped).
fn common() -> Graph {
    Graph::new(Class::Common, "c.read", "c.final")
        .with(
            Node::new("c.read", KNOWLEDGE, 3)
                .on(&[Stands], Next::End(End::GeneralKnowledge))
                .on(
                    &[NeedsSource],
                    Next::first_change(Target::Enter(Class::Sourced), "c.rewrite"),
                )
                .on(&[Refuted], Next::to("c.ground")),
        )
        .with(
            Node::new("c.ground", PARTS, 1)
                .on(&[Stands], Next::End(End::GeneralKnowledge))
                .otherwise(Next::to("c.rewrite")),
        )
        .with(rewrite(
            "c.rewrite",
            Goal::AsTheFieldHasIt,
            "c.final",
            "c.rewrite.again",
        ))
        .with(rewrite(
            "c.rewrite.again",
            Goal::AsTheFieldHasIt,
            "c.final",
            "c.cut",
        ))
        .with(
            Node::new("c.final", KNOWLEDGE, 3)
                .on(&[Stands], Next::End(End::GeneralKnowledge))
                .on(&[Refuted], Next::to("c.reground"))
                .on(&[NeedsSource], Next::to("c.cut")),
        )
        .with(
            Node::new("c.reground", PARTS, 1)
                .on(&[Stands], Next::End(End::GeneralKnowledge))
                .otherwise(Next::to("c.cut")),
        )
        .with(cut("c.cut"))
}

/// A statement the model calls Internal meets the falsity veto.
fn internal() -> Graph {
    Graph::new(Class::Internal, "i.veto", "i.final")
        .with(
            Node::new("i.veto", VETO, 3)
                .on(&[Stands], Next::End(End::NotAClaim))
                .on(&[Refuted], Next::to("i.ground")),
        )
        .with(
            Node::new("i.ground", PARTS, 1)
                .on(&[Stands], Next::End(End::NotAClaim))
                .otherwise(Next::to("i.rewrite")),
        )
        .with(rewrite(
            "i.rewrite",
            Goal::AsTheFieldHasIt,
            "i.final",
            "i.rewrite.again",
        ))
        .with(rewrite(
            "i.rewrite.again",
            Goal::AsTheFieldHasIt,
            "i.final",
            "i.cut",
        ))
        .with(
            Node::new("i.final", VETO, 3)
                .on(&[Stands], Next::End(End::NotAClaim))
                .on(&[Refuted], Next::to("i.reground")),
        )
        .with(
            Node::new("i.reground", PARTS, 1)
                .on(&[Stands], Next::End(End::NotAClaim))
                .otherwise(Next::to("i.cut")),
        )
        .with(cut("i.cut"))
}
