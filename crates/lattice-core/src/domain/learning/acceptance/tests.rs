//! Shared builders for the acceptance policy's tests.

mod ceiling_tests;
mod invariant_tests;
mod record_tests;
mod standard_tests;
mod walk_tests;

use super::walk::{follow, start, Moved};
use super::*;
use crate::domain::learning::claim_review::Class;

const REMOVED: Walked = Walked::Ended(End::Removed {
    section_goes: false,
});
const REMOVED_WITH_SECTION: Walked = Walked::Ended(End::Removed { section_goes: true });

fn position(class: Class, changed_class: bool, reasked: bool, node: &'static str) -> Position {
    Position {
        class,
        changed_class,
        reasked,
        node: NodeId(node),
    }
}

/// A statement left at `node`.
fn placed(class: Class, changed_class: bool, reasked: bool, node: &'static str) -> Walked {
    Walked::At(position(class, changed_class, reasked, node))
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
