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

pub use ceiling::most_calls;
pub use graph::{Graph, Node};
pub use invariants::{violations, Violation};
pub use standard::{StandardPolicy, KNOWLEDGE_SAMPLES, STANDARD_POLICY};
pub use walk::{
    born, check_record, decide, next_rewrite, repair_at, repair_attempt, rewrite_ahead,
    rewrites_given, start, stepped, successor, walk, walk_from, RecordFault, RepairStep,
};

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
    /// The part-wise judge on the statement's own citation: the passages its
    /// sentence cites, or its section's when it cites none. The one reading
    /// that can say `AllCommon`.
    Parts,
    /// The part-wise judge on the section's passages, asked only whether
    /// they ground a statement a closed-book reading called false.
    Ground,
    /// The strict judge on the course's page for the statement.
    Page,
    /// The consistency judge on the section's passages.
    Consistency,
    /// The knowledge judge's closed-book samples, passing on unanimity.
    Knowledge,
    /// The same samples, failing only when every sample says `false`.
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
    /// Correct what a veto found wrong in the sentence; keep the rest of it.
    CorrectWhatWasFound,
}

/// One thing done to a statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
            Self::Read(Reading::Ground | Reading::Veto) => &[Outcome::Stands, Outcome::Refuted],
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
    /// A passage, or every sample, says otherwise.
    Refuted,
    /// It needs a source: a `needs_source`, a split with a `false`, or an unfounded
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum End {
    Resolved,
    Consistent,
    GeneralKnowledge,
    NotAClaim,
    /// The sources are silent, or the sentence cannot be cut: it stays in the
    /// lesson as written, published not verified.
    Unverified,
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
    /// Another class's graph, at its entry: the statement enters the class by
    /// evidence, once.
    Enter(Class),
    /// Another class's graph, at its post-rewrite node: a rewrite's successor
    /// changes class, and is never rewritten again.
    EnterAfterRewrite(Class),
}

impl Target {
    /// The node named `name` of the same graph.
    pub fn node(name: &'static str) -> Self {
        Self::Node(NodeId(name))
    }
}

/// What a statement that has been in a class already does where a first entry
/// would move it into that class again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Otherwise {
    Node(NodeId),
    End(End),
}

impl From<&'static str> for Otherwise {
    fn from(name: &'static str) -> Self {
        Self::Node(NodeId(name))
    }
}

impl From<End> for Otherwise {
    fn from(end: End) -> Self {
        Self::End(end)
    }
}

/// What follows an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Go(Target),
    /// `by` unless the statement has been in `into`; `otherwise` once it has
    /// (a statement enters each class at most once).
    FirstEntry {
        into: Class,
        by: Target,
        otherwise: Otherwise,
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

    /// Change to `class`, at its graph's post-rewrite node.
    pub fn enter_after_rewrite(class: Class) -> Self {
        Self::Go(Target::EnterAfterRewrite(class))
    }

    /// Change to `class`, at its graph's entry, unless the statement has been
    /// in it: then the node named `otherwise` or the end given.
    pub fn first_entry(class: Class, otherwise: impl Into<Otherwise>) -> Self {
        Self::FirstEntry {
            into: class,
            by: Target::Enter(class),
            otherwise: otherwise.into(),
        }
    }

    /// As `first_entry`, by the node named `by` of the same graph, whose step
    /// leads into `class`.
    pub fn first_entry_by(by: &'static str, class: Class, otherwise: impl Into<Otherwise>) -> Self {
        Self::FirstEntry {
            into: class,
            by: Target::node(by),
            otherwise: otherwise.into(),
        }
    }
}

/// A set of classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Classes(u8);

impl Classes {
    /// The set of `class` alone.
    pub const fn of(class: Class) -> Self {
        Self(Self::bit(class))
    }

    /// This set and `class`.
    pub const fn with(self, class: Class) -> Self {
        Self(self.0 | Self::bit(class))
    }

    /// Whether `class` is in the set.
    pub const fn has(self, class: Class) -> bool {
        self.0 & Self::bit(class) != 0
    }

    /// The bit of `class` in the set.
    const fn bit(class: Class) -> u8 {
        match class {
            Class::Sourced => 1,
            Class::Reasoning => 2,
            Class::Common => 4,
            Class::Internal => 8,
        }
    }
}

/// Where a statement is in its class's graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub class: Class,
    /// The classes the statement has been in, its current one included: it
    /// enters each at most once.
    pub visited: Classes,
    /// The statement's one re-ask of an unreadable answer is spent.
    pub reasked: bool,
    pub node: NodeId,
    /// The rewrites its sentence can still be given: a rewrite step it takes
    /// spends one, and so does each machine change of its sentence it
    /// continues after.
    pub rewrites_left: u8,
}

/// One step a statement took and what came of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taken {
    pub step: Step,
    pub outcome: Outcome,
}

/// Where the policy sent a statement after a step: what the ledger records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Then {
    /// On to a node, named with its step so a reader needs no policy.
    Node {
        name: String,
        step: Step,
    },
    End(End),
    /// A second unanswered reading: the lesson stops.
    Stop,
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
