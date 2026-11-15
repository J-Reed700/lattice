//! Shared builders for the acceptance policy's tests.

mod ceiling_tests;
mod invariant_tests;
mod record_tests;
mod standard_tests;
mod walk_tests;

use super::walk::{follow, Moved};
use super::*;
use crate::domain::learning::claim_review::Class;

const REMOVED: Walked = Walked::Ended(End::Removed {
    section_goes: false,
});
const UNVERIFIED: Walked = Walked::Ended(End::Unverified);

/// `class`'s `node` with `rewrites_left` rewrites still to give, for a statement
/// that was in the classes of `came_from` before it.
fn position(
    class: Class,
    came_from: &[Class],
    reasked: bool,
    node: &'static str,
    rewrites_left: u8,
) -> Position {
    Position {
        class,
        visited: came_from
            .iter()
            .fold(Classes::of(class), |visited, &before| visited.with(before)),
        reasked,
        node: NodeId(node),
        rewrites_left,
    }
}

/// A statement left at `node`.
fn placed(
    class: Class,
    came_from: &[Class],
    reasked: bool,
    node: &'static str,
    rewrites_left: u8,
) -> Walked {
    Walked::At(position(class, came_from, reasked, node, rewrites_left))
}

fn read(reading: Reading, outcome: Outcome) -> Taken {
    Taken {
        step: Step::Read(reading),
        outcome,
    }
}

fn parts(outcome: Outcome) -> Taken {
    read(Reading::Parts, outcome)
}

fn ground(outcome: Outcome) -> Taken {
    read(Reading::Ground, outcome)
}

fn page(outcome: Outcome) -> Taken {
    read(Reading::Page, outcome)
}

fn consistency(outcome: Outcome) -> Taken {
    read(Reading::Consistency, outcome)
}

fn knowledge(outcome: Outcome) -> Taken {
    read(Reading::Knowledge, outcome)
}

fn veto(outcome: Outcome) -> Taken {
    read(Reading::Veto, outcome)
}

fn research(outcome: Outcome) -> Taken {
    Taken {
        step: Step::Research,
        outcome,
    }
}

fn recite() -> Taken {
    Taken {
        step: Step::Recite,
        outcome: Outcome::Done,
    }
}

fn uncite() -> Taken {
    Taken {
        step: Step::Uncite,
        outcome: Outcome::Done,
    }
}

fn rewrite(goal: Goal, outcome: Outcome) -> Taken {
    Taken {
        step: Step::Rewrite(goal),
        outcome,
    }
}

fn cut(outcome: Outcome) -> Taken {
    Taken {
        step: Step::Cut,
        outcome,
    }
}

/// The standard policy with one graph replaced.
struct Changed {
    standard: StandardPolicy,
    replaced: Graph,
}

impl AcceptancePolicy for Changed {
    fn version(&self) -> &str {
        "changed"
    }

    fn graph(&self, class: Class) -> &Graph {
        if class == self.replaced.class() {
            &self.replaced
        } else {
            self.standard.graph(class)
        }
    }
}

/// The standard policy with `class`'s node `name` replaced by `edit(node)`.
fn changed(class: Class, name: &'static str, edit: impl FnOnce(Node) -> Node) -> Changed {
    let standard = StandardPolicy::new();
    let node = standard.graph(class).node(NodeId(name)).cloned().unwrap();
    let replaced = standard.graph(class).clone().with(edit(node));
    Changed { standard, replaced }
}
