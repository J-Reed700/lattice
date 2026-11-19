//! Where a statement is: the steps it took, replayed through its class's graph,
//! and what the ledger records of each.

use std::collections::{HashMap, HashSet};

use super::{
    AcceptancePolicy, Classes, End, Next, Otherwise, Outcome, Position, Step, Taken, Target, Then,
    Walked,
};
use crate::domain::learning::claim_review::Class;

/// Where an edge leads from a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Moved {
    To(Position),
    Ended(End),
    /// A change of class, at an entry, into a class the statement has been in
    /// already.
    Illegal,
}

/// Where a statement that entered `class` begins: the one place a first
/// position is made.
pub fn start(policy: &dyn AcceptancePolicy, class: Class) -> Position {
    Position {
        class,
        visited: Classes::of(class),
        reasked: false,
        node: policy.graph(class).entry(),
        rewrites_left: rewrites_given(policy, class),
    }
}

/// Where a statement a repair wrote beside its target's successor begins: the
/// post-rewrite node of its `class`, with the `rewrites_left` the successor has.
pub fn born(policy: &dyn AcceptancePolicy, class: Class, rewrites_left: u8) -> Position {
    Position {
        class,
        visited: Classes::of(class),
        reasked: false,
        node: policy.graph(class).after_rewrite(),
        rewrites_left,
    }
}

/// The rewrites a statement that entered `class` is given: the most `Rewrite`
/// steps any path from the class's entry takes, class changes included. Re-asks
/// and successors are not paths. A position met again on its own path counts
/// nothing, so a graph that loops still ends.
pub fn rewrites_given(policy: &dyn AcceptancePolicy, class: Class) -> u8 {
    let entry = Position {
        class,
        visited: Classes::of(class),
        reasked: false,
        node: policy.graph(class).entry(),
        rewrites_left: 0,
    };
    most_rewrites(policy, entry, &mut HashMap::new(), &mut HashSet::new())
}

fn most_rewrites(
    policy: &dyn AcceptancePolicy,
    at: Position,
    known: &mut HashMap<Position, u8>,
    open: &mut HashSet<Position>,
) -> u8 {
    if let Some(&most) = known.get(&at) {
        return most;
    }
    if !open.insert(at) {
        return 0;
    }
    let Some(node) = policy.graph(at.class).node(at.node) else {
        open.remove(&at);
        return 0;
    };
    let mut onward: u8 = 0;
    for &(_, next) in node.edges() {
        if let Moved::To(to) = follow(policy, at, next) {
            onward = onward.max(most_rewrites(policy, to, known, open));
        }
    }
    open.remove(&at);
    let most = onward.saturating_add(u8::from(matches!(node.step, Step::Rewrite(_))));
    known.insert(at, most);
    most
}

/// `at` once the step of its node is taken: one rewrite fewer left when the
/// node is a rewrite.
pub fn stepped(policy: &dyn AcceptancePolicy, at: Position) -> Position {
    let rewrites = policy
        .graph(at.class)
        .node(at.node)
        .is_some_and(|node| matches!(node.step, Step::Rewrite(_)));
    if rewrites {
        Position {
            rewrites_left: at.rewrites_left.saturating_sub(1),
            ..at
        }
    } else {
        at
    }
}

/// Where a statement continues once a machine rewrite has changed its
/// sentence, from `left`: where it waited, or the node it ended from with its
/// last step counted. Every machine successor spends a rewrite, budgeted or
/// not.
pub fn successor(policy: &dyn AcceptancePolicy, left: Position) -> Position {
    Position {
        node: policy.graph(left.class).after_rewrite(),
        rewrites_left: left.rewrites_left.saturating_sub(1),
        ..left
    }
}

/// The successor a further rewrite of the sentence would give, none when no
/// rewrite is left: the one rule for whether a rewrite is left.
pub fn next_rewrite(policy: &dyn AcceptancePolicy, left: Position) -> Option<Position> {
    (left.rewrites_left > 0).then(|| successor(policy, left))
}

/// The repair a statement at a `Rewrite` or `Cut` node is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepairStep {
    pub step: Step,
    /// One more than the rewrites its sentence has been given.
    pub attempt: u8,
    /// The cut: a statement's last repair.
    pub last: bool,
}

/// The number of the repair a statement at `at` is given: one more than the
/// rewrites its sentence has already been given.
pub fn repair_attempt(policy: &dyn AcceptancePolicy, at: Position) -> u8 {
    rewrites_given(policy, at.class)
        .saturating_sub(at.rewrites_left)
        .saturating_add(1)
}

/// The repair the node of `at` asks for: a rewrite, or the cut, which is the
/// last. `None` at any other step.
pub fn repair_at(policy: &dyn AcceptancePolicy, at: Position) -> Option<RepairStep> {
    let step = policy.graph(at.class).node(at.node)?.step;
    matches!(step, Step::Rewrite(_) | Step::Cut).then(|| RepairStep {
        step,
        attempt: repair_attempt(policy, at),
        last: step == Step::Cut,
    })
}

/// The positions one step from `at` along its node's edges: where each edge
/// leads, and for an edge that ends the statement other than removed, the
/// successor a further rewrite of its sentence gives. A removal and an edge
/// the class may not take lead nowhere.
pub(super) fn onward(policy: &dyn AcceptancePolicy, at: Position) -> Vec<Position> {
    let Some(node) = policy.graph(at.class).node(at.node) else {
        return Vec::new();
    };
    node.edges()
        .iter()
        .filter_map(|&(_, next)| match follow(policy, at, next) {
            Moved::To(to) => Some(to),
            Moved::Ended(End::Removed { .. }) | Moved::Illegal => None,
            Moved::Ended(_) => next_rewrite(policy, stepped(policy, at)),
        })
        .collect()
}

/// Where `next` leads from `at`.
pub(super) fn follow(policy: &dyn AcceptancePolicy, at: Position, next: Next) -> Moved {
    let at = stepped(policy, at);
    match next {
        Next::Go(target) => go(policy, at, target),
        Next::FirstEntry {
            into,
            by,
            otherwise,
        } => {
            if at.visited.has(into) {
                match otherwise {
                    Otherwise::Node(node) => Moved::To(Position { node, ..at }),
                    Otherwise::End(end) => Moved::Ended(end),
                }
            } else {
                go(policy, at, by)
            }
        }
        Next::End(end) => Moved::Ended(end),
    }
}

fn go(policy: &dyn AcceptancePolicy, at: Position, target: Target) -> Moved {
    match target {
        Target::Node(node) => Moved::To(Position { node, ..at }),
        Target::Enter(class) if at.visited.has(class) => Moved::Illegal,
        Target::Enter(class) => Moved::To(Position {
            class,
            visited: at.visited.with(class),
            reasked: at.reasked,
            node: policy.graph(class).entry(),
            rewrites_left: at.rewrites_left,
        }),
        Target::EnterAfterRewrite(class) => Moved::To(Position {
            class,
            visited: at.visited.with(class),
            reasked: at.reasked,
            node: policy.graph(class).after_rewrite(),
            rewrites_left: at.rewrites_left,
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
