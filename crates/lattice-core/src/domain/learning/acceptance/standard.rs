//! The standard policy: the four graphs of the design's §2.3, node for node.

use super::Outcome::{
    AllCommon, CannotCut, Done, FoundNothing, NeedsSource, NotApplied, Refuted, Rewritten, Saved,
    Stands, StandsElsewhere, Unproven,
};
use super::{AcceptancePolicy, End, Goal, Graph, Next, Node, Outcome, Reading, Step};
use crate::domain::learning::claim_review::Class;
use crate::protocol;
use crate::shared::protocol::Protocol;

/// The standard policy's version: part of every receipt's and checkpoint's
/// key, so a change to any of its graphs changes it.
pub const STANDARD_POLICY: Protocol = protocol!("acceptance", 4);

/// Samples one knowledge or veto read asks the model alone; the read holds
/// only when every sample agrees.
pub const KNOWLEDGE_SAMPLES: u32 = 3;

const PARTS: Step = Step::Read(Reading::Parts);
const GROUND: Step = Step::Read(Reading::Ground);
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
        STANDARD_POLICY.as_str()
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

/// A ground reading: the passages ground the statement and it ends, or the
/// veto stands and it goes on to `refuted`.
fn ground(name: &'static str, end: End, refuted: &'static str) -> Node {
    Node::new(name, GROUND, 1)
        .on(&[Stands], Next::End(end))
        .on(&[Refuted], Next::to(refuted))
}

/// A cut: the sentence goes, or stays when it cannot be cut, published not
/// verified.
fn cut(name: &'static str) -> Node {
    Node::new(name, Step::Cut, 0)
        .on(
            &[Outcome::Cut],
            Next::End(End::Removed {
                section_goes: false,
            }),
        )
        .on(&[CannotCut], Next::End(End::Unverified))
}

/// Entered by the writer's citation, the rule layer, the model, or evidence.
fn sourced() -> Graph {
    Graph::new(Class::Sourced, "s.read", "s.final")
        .with(
            Node::new("s.read", PARTS, 1)
                .on(&[Stands], Next::End(End::Resolved))
                .on(
                    &[AllCommon],
                    Next::first_entry_by("s.uncite", Class::Common, "s.research"),
                )
                .on(&[Unproven], Next::to("s.research"))
                .on(&[Refuted], Next::to("s.rewrite")),
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
                .on(&[Refuted], Next::to("s.rewrite"))
                .on(
                    &[Unproven],
                    Next::first_entry_by("s.uncite", Class::Common, End::Unverified),
                ),
        )
        .with(Node::new("s.recite", Step::Recite, 0).on(&[Done], Next::to("s.citeread")))
        .with(
            Node::new("s.citeread", PARTS, 1)
                .on(&[Stands], Next::End(End::Resolved))
                .on(&[Refuted], Next::to("s.rewrite"))
                .on(
                    &[Unproven, AllCommon],
                    Next::first_entry_by("s.uncite", Class::Common, End::Unverified),
                ),
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
                .on(&[AllCommon], Next::to("s.final.uncite"))
                .on(&[Refuted], Next::to("s.cut"))
                .on(&[Unproven], Next::End(End::Unverified)),
        )
        .with(
            Node::new("s.final.uncite", Step::Uncite, 0)
                .on(&[Done], Next::enter_after_rewrite(Class::Common)),
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
                    Next::first_entry(Class::Sourced, End::Unverified),
                ),
        )
        .with(
            Node::new("r.veto", VETO, KNOWLEDGE_SAMPLES)
                .on(&[Stands], Next::End(End::Consistent))
                .on(&[Refuted], Next::to("r.ground")),
        )
        .with(ground("r.ground", End::Consistent, "r.correct"))
        .with(rewrite(
            "r.correct",
            Goal::CorrectWhatWasFound,
            "r.final",
            "r.correct.again",
        ))
        .with(rewrite(
            "r.correct.again",
            Goal::CorrectWhatWasFound,
            "r.final",
            "r.cut",
        ))
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
                .on(&[Refuted], Next::to("r.cut"))
                .on(&[NeedsSource], Next::End(End::Unverified)),
        )
        .with(
            Node::new("r.reveto", VETO, KNOWLEDGE_SAMPLES)
                .on(&[Stands], Next::End(End::Consistent))
                .on(&[Refuted], Next::to("r.reground")),
        )
        .with(ground("r.reground", End::Consistent, "r.cut"))
        .with(cut("r.cut"))
}

/// Entered by the model, or by evidence from Sourced (`AllCommon`, its
/// citation dropped).
fn common() -> Graph {
    Graph::new(Class::Common, "c.read", "c.final")
        .with(
            Node::new("c.read", KNOWLEDGE, KNOWLEDGE_SAMPLES)
                .on(&[Stands], Next::End(End::GeneralKnowledge))
                .on(
                    &[NeedsSource],
                    Next::first_entry(Class::Sourced, End::Unverified),
                )
                .on(&[Refuted], Next::to("c.ground")),
        )
        .with(ground("c.ground", End::GeneralKnowledge, "c.rewrite"))
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
            Node::new("c.final", KNOWLEDGE, KNOWLEDGE_SAMPLES)
                .on(&[Stands], Next::End(End::GeneralKnowledge))
                .on(&[Refuted], Next::to("c.reground"))
                .on(&[NeedsSource], Next::End(End::Unverified)),
        )
        .with(ground("c.reground", End::GeneralKnowledge, "c.cut"))
        .with(cut("c.cut"))
}

/// A statement the model calls Internal meets the falsity veto.
fn internal() -> Graph {
    Graph::new(Class::Internal, "i.veto", "i.final")
        .with(
            Node::new("i.veto", VETO, KNOWLEDGE_SAMPLES)
                .on(&[Stands], Next::End(End::NotAClaim))
                .on(&[Refuted], Next::to("i.ground")),
        )
        .with(ground("i.ground", End::NotAClaim, "i.rewrite"))
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
            Node::new("i.final", VETO, KNOWLEDGE_SAMPLES)
                .on(&[Stands], Next::End(End::NotAClaim))
                .on(&[Refuted], Next::to("i.reground")),
        )
        .with(ground("i.reground", End::NotAClaim, "i.cut"))
        .with(cut("i.cut"))
}
