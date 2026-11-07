//! Where a statement is: the steps it took, replayed through its class's graph,
//! and what the ledger records of each.

use std::collections::HashSet;

use super::{AcceptancePolicy, End, Next, Outcome, Position, Step, Taken, Target, Then, Walked};
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

/// Where a statement at `at` goes after `taken`, and the position it is then
/// at: none after an end or a stop. The re-ask stays at the node, spent;
/// `None` when the step does not fit the graph.
pub fn decide(
    policy: &dyn AcceptancePolicy,
    at: Position,
    taken: Taken,
) -> Option<(Then, Option<Position>)> {
    let node = policy.graph(at.class).node(at.node)?;
    if node.step != taken.step {
        return None;
    }
    if taken.outcome == Outcome::Unjudged {
        if !matches!(node.step, Step::Read(_)) {
            return None;
        }
        if at.reasked {
            return Some((Then::Stop, None));
        }
        let again = Position {
            reasked: true,
            ..at
        };
        return Some((named(policy, again)?, Some(again)));
    }
    match follow(policy, at, node.next(taken.outcome)?) {
        Moved::To(to) => Some((named(policy, to)?, Some(to))),
        Moved::Ended(end) => Some((Then::End(end), None)),
        Moved::Illegal => None,
    }
}

/// The node `at` is at, as a decision names it.
fn named(policy: &dyn AcceptancePolicy, at: Position) -> Option<Then> {
    let node = policy.graph(at.class).node(at.node)?;
    Some(Then::Node {
        name: node.name.to_owned(),
        step: node.step,
    })
}

/// Where a statement that entered `class` is after `taken`, its steps in
/// order.
pub fn walk(policy: &dyn AcceptancePolicy, class: Class, taken: &[Taken]) -> Walked {
    walk_from(policy, start(policy, class), taken)
}

/// Where a statement that was at `from` is after `taken`, its steps in order.
pub fn walk_from(policy: &dyn AcceptancePolicy, from: Position, taken: &[Taken]) -> Walked {
    let mut at = from;
    let mut rest = taken.iter();
    while let Some(&step) = rest.next() {
        let Some((then, to)) = decide(policy, at, step) else {
            return Walked::Diverged { at, taken: step };
        };
        match (then, to) {
            (_, Some(to)) => at = to,
            (Then::Stop, None) => return Walked::Stopped,
            (Then::End(end), None) => {
                return match rest.next() {
                    None => Walked::Ended(end),
                    Some(&extra) => Walked::Diverged { at, taken: extra },
                };
            }
            (Then::Node { .. }, None) => return Walked::Diverged { at, taken: step },
        }
    }
    Walked::At(at)
}

/// Whether a rewrite is still ahead of a statement at `at`: a `Rewrite` node
/// is reachable from it, class changes included.
pub fn rewrite_ahead(policy: &dyn AcceptancePolicy, at: Position) -> bool {
    let mut seen = HashSet::new();
    let mut stack = vec![at];
    while let Some(here) = stack.pop() {
        if !seen.insert(here) {
            continue;
        }
        let Some(node) = policy.graph(here.class).node(here.node) else {
            continue;
        };
        if matches!(node.step, Step::Rewrite(_)) {
            return true;
        }
        for &(_, next) in node.edges() {
            if let Moved::To(to) = follow(policy, here, next) {
                stack.push(to);
            }
        }
    }
    false
}

/// A recorded step the graph does not give: where the policy would have sent
/// the statement, when the step fits at all, and where the record says it went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFault {
    /// The step's place in the record.
    pub index: usize,
    pub expected: Option<Then>,
    pub recorded: Then,
}

/// Whether the steps a statement recorded, from `start`, are the graph's: each
/// must give the `Then` recorded beside it. A step after an end or a stop is a
/// fault.
pub fn check_record(
    policy: &dyn AcceptancePolicy,
    start: Position,
    steps: &[(Taken, Then)],
) -> Result<(), RecordFault> {
    let mut at = Some(start);
    for (index, (taken, recorded)) in steps.iter().enumerate() {
        let decided = at.and_then(|here| decide(policy, here, *taken));
        match decided {
            Some((then, next)) if then == *recorded => at = next,
            other => {
                return Err(RecordFault {
                    index,
                    expected: other.map(|(then, _)| then),
                    recorded: recorded.clone(),
                });
            }
        }
    }
    Ok(())
}
