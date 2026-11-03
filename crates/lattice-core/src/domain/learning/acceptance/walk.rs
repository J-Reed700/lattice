//! Where a statement is: the steps it took, replayed through its class's graph.

use super::{AcceptancePolicy, End, Next, Outcome, Position, Step, Taken, Target, Walked};
use crate::domain::learning::claim_review::Class;

/// Where an edge leads from a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Moved {
    To(Position),
    Ended(End),
    /// A change of class, at an entry, by a statement that has changed class
    /// already.
    Illegal,
}

/// Where a statement that entered `class` begins.
pub(super) fn start(policy: &dyn AcceptancePolicy, class: Class) -> Position {
    Position {
        class,
        changed_class: false,
        reasked: false,
        node: policy.graph(class).entry(),
    }
}

/// Where `next` leads from `at`.
pub(super) fn follow(policy: &dyn AcceptancePolicy, at: Position, next: Next) -> Moved {
    match next {
        Next::Go(target) => go(policy, at, target),
        Next::FirstChange { change, otherwise } => {
            if at.changed_class {
                Moved::To(Position {
                    node: otherwise,
                    ..at
                })
            } else {
                go(policy, at, change)
            }
        }
        Next::End(end) => Moved::Ended(end),
    }
}

fn go(policy: &dyn AcceptancePolicy, at: Position, target: Target) -> Moved {
    match target {
        Target::Node(node) => Moved::To(Position { node, ..at }),
        Target::Enter(_) if at.changed_class => Moved::Illegal,
        Target::Enter(class) => Moved::To(Position {
            class,
            changed_class: true,
            reasked: at.reasked,
            node: policy.graph(class).entry(),
        }),
        Target::EnterAfterRewrite(class) => Moved::To(Position {
            class,
            changed_class: true,
            reasked: at.reasked,
            node: policy.graph(class).after_rewrite(),
        }),
    }
}

/// Where a statement that entered `class` is after `taken`, its steps in
/// order.
pub fn walk(policy: &dyn AcceptancePolicy, class: Class, taken: &[Taken]) -> Walked {
    let mut at = start(policy, class);
    let mut rest = taken.iter();
    while let Some(&step) = rest.next() {
        let diverged = Walked::Diverged { at, taken: step };
        let Some(node) = policy.graph(at.class).node(at.node) else {
            return diverged;
        };
        if node.step != step.step {
            return diverged;
        }
        if step.outcome == Outcome::Unjudged {
            if !matches!(node.step, Step::Read(_)) {
                return diverged;
            }
            if at.reasked {
                return Walked::Stopped;
            }
            at.reasked = true;
            continue;
        }
        let Some(next) = node.next(step.outcome) else {
            return diverged;
        };
        match follow(policy, at, next) {
            Moved::To(to) => at = to,
            Moved::Ended(end) => {
                return match rest.next() {
                    None => Walked::Ended(end),
                    Some(&extra) => Walked::Diverged { at, taken: extra },
                };
            }
            Moved::Illegal => return diverged,
        }
    }
    Walked::At(at)
}
