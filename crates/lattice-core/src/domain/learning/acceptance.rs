//! The acceptance policy of a lesson's statements: each class's check is one
//! finite graph of steps, and the next step, the call ceiling, a statement's
//! end and the proof that its loop ends are derived from the graphs. Pure
//! types and rules, no I/O.
//!
//! Design: docs/design/2026-11-03-lesson-acceptance-policy.md §2.3–§2.5, §4.

mod ceiling;
mod graph;
mod invariants;
mod standard;
#[cfg(test)]
mod tests;
mod walk;

pub use ceiling::{after_rewrite_calls, most_calls};
pub use graph::{Graph, Node};
pub use invariants::{violations, Violation};
pub use standard::{StandardPolicy, STANDARD_POLICY};
pub use walk::walk;

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::domain::learning::claim_review::Class;

/// Every class, in the order a policy lists its graphs.
pub const CLASSES: [Class; 4] = [
    Class::Sourced,
    Class::Reasoning,
    Class::Common,
    Class::Internal,
];

/// A set of graphs, one per class: how each statement is checked and what
/// follows each outcome.
pub trait AcceptancePolicy: Send + Sync {
    /// Part of every receipt's and checkpoint's key: a change to any graph
    /// changes it.
    fn version(&self) -> &str;
    /// The graph of a class's check. A statement the rule layer leaves out
    /// never asks.
    fn graph(&self, class: Class) -> &Graph;
}

/// Which judge reads a statement, and on what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reading {
    /// The part-wise judge on the passages the statement's sentence cites.
    Parts,
    /// The strict judge on the course's page for the statement.
    Page,
    /// The consistency judge on the section's passages.
    Consistency,
    /// The knowledge judge's closed-book samples, passing on unanimity.
    Knowledge,
    /// The same samples, failing only on a majority `false`.
    Veto,
}

/// What a rewrite is asked to achieve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    /// Say what the passage states, or the point without the specific.
    WhatThePassageStates,
    /// Agree with the passages; correct what the reason names.
    AgreeWithThePassages,
    /// State it as the field has it, per the reason.
    AsTheFieldHasIt,
    /// Narrow the sentence to what its other statements say.
    KeepTheOthers,
}

/// One thing done to a statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    Read(Reading),
    /// Look for a passage, once in a statement's life.
    Research,
    /// Move the statement's citation to the passage found.
    Recite,
    /// Drop the statement's citation.
    Uncite,
    Rewrite(Goal),
    /// Take the statement out of its sentence.
    Cut,
}

impl Step {
    /// The outcomes this step can give, which every node of the step answers.
    pub fn outcomes(self) -> &'static [Outcome] {
        match self {
            Self::Read(Reading::Parts) => &[
                Outcome::Stands,
                Outcome::AllCommon,
                Outcome::Unproven,
                Outcome::Refuted,
            ],
            Self::Read(Reading::Page) => &[
                Outcome::Stands,
                Outcome::StandsElsewhere,
                Outcome::Unproven,
                Outcome::Refuted,
            ],
            Self::Read(Reading::Consistency | Reading::Knowledge) => {
                &[Outcome::Stands, Outcome::Refuted, Outcome::NeedsSource]
            }
            Self::Read(Reading::Veto) => &[Outcome::Stands, Outcome::Refuted],
            Self::Research => &[Outcome::Saved, Outcome::FoundNothing],
            Self::Recite | Self::Uncite => &[Outcome::Done],
            Self::Rewrite(_) => &[Outcome::Rewritten, Outcome::NotApplied],
            Self::Cut => &[Outcome::Cut, Outcome::CannotCut],
        }
    }
}

/// What came of a step, in the one vocabulary every judge and remedy is
/// read into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The statement stands on this reading.
    Stands,
    /// It stands on a passage its sentence does not cite.
    StandsElsewhere,
    /// Every part is common knowledge and none is stated: the citation is
    /// at fault, not the statement.
    AllCommon,
    /// It asserts a specific nothing states.
    Unproven,
    /// A passage, or most samples, say otherwise.
    Refuted,
    /// It needs a source: a `needs_source`, a lone `false`, or an unfounded
    /// specific.
    NeedsSource,
    /// The answer could not be read, after the judge's own second asking.
    Unjudged,
    /// A citation moved or dropped.
    Done,
    /// Research saved a source.
    Saved,
    /// Research found nothing.
    FoundNothing,
    /// A rewrite was applied.
    Rewritten,
    /// The confinement could not apply a rewrite.
    NotApplied,
    /// The statement was cut out of its sentence.
    Cut,
    /// The sentence could not be cut: its section goes.
    CannotCut,
}

/// Where a statement's path ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum End {
    Resolved,
    Consistent,
    GeneralKnowledge,
    NotAClaim,
    Removed {
        /// The sentence could not be cut, so its section goes.
        section_goes: bool,
    },
}

/// A node of a graph, by its name in the design: `s.read`, `c.rewrite.again`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub &'static str);

/// Where an edge goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Node(NodeId),
    /// Another class's graph, at its entry: the statement changes class by
    /// evidence.
    Enter(Class),
}

impl Target {
    /// The node named `name` of the same graph.
    pub fn node(name: &'static str) -> Self {
        Self::Node(NodeId(name))
    }
}

/// What follows an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Go(Target),
    /// `change` the first time the evidence moves the statement; `otherwise`
    /// once it has changed class (no class changes twice).
    FirstChange {
        change: Target,
        otherwise: NodeId,
    },
    End(End),
}

impl Next {
    /// On to the node named `name` of the same graph.
    pub fn to(name: &'static str) -> Self {
        Self::Go(Target::node(name))
    }

    /// Change to `class`, at its graph's entry.
    pub fn enter(class: Class) -> Self {
        Self::Go(Target::Enter(class))
    }

    /// `change` the first time, then the node named `otherwise`.
    pub fn first_change(change: Target, otherwise: &'static str) -> Self {
        Self::FirstChange {
            change,
            otherwise: NodeId(otherwise),
        }
    }
}

/// Where a statement is in its class's graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub class: Class,
    /// The evidence has moved the statement to this class.
    pub changed_class: bool,
    /// The statement's one re-ask of an unreadable answer is spent.
    pub reasked: bool,
    pub node: NodeId,
}

// `Class` is not `Hash`, so a position hashes the slot of its class.
impl Hash for Position {
    fn hash<H: Hasher>(&self, state: &mut H) {
        slot(self.class).hash(state);
        self.changed_class.hash(state);
        self.reasked.hash(state);
        self.node.hash(state);
    }
}

fn slot(class: Class) -> u8 {
    match class {
        Class::Sourced => 0,
        Class::Reasoning => 1,
        Class::Common => 2,
        Class::Internal => 3,
    }
}

/// One step a statement took and what came of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken {
    pub step: Step,
    pub outcome: Outcome,
}

/// Where the steps a statement took leave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Walked {
    At(Position),
    Ended(End),
    /// A second unreadable answer: the lesson stops.
    Stopped,
    /// `taken` does not fit the graph at `at`. After an end, `at` is the
    /// position the statement left to end.
    Diverged {
        at: Position,
        taken: Taken,
    },
}
