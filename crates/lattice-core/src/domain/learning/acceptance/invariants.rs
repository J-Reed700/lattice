//! The invariants of the design's §2.5, held against every graph of a policy.

use std::collections::HashSet;

use super::ceiling::Ceiling;
use super::walk::{follow, start, Moved};
use super::{
    AcceptancePolicy, End, Graph, Next, Node, NodeId, Outcome, Position, Reading, Step, Target,
    CLASSES,
};
use crate::domain::learning::claim_review::Class;

/// An invariant a policy breaks, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// The entry, the post-rewrite node or an edge names a node its graph
    /// lacks.
    MissingNode {
        class: Class,
        node: NodeId,
        missing: NodeId,
    },
    /// 1: a statement can come back to a node it left, or change class twice.
    Cycle { class: Class, node: NodeId },
    /// 2: a statement can be removed without a rewrite first.
    CutWithoutRewrite { class: Class, node: NodeId },
    /// 3: a closed-book refusal can lead to a rewrite, a cut or an end before
    /// the passages are read.
    VetoUnread { class: Class, node: NodeId },
    /// 4: a first reading answers a citation's fault with anything but a
    /// citation remedy.
    CitationFaultNotRepaired {
        class: Class,
        node: NodeId,
        outcome: Outcome,
    },
    /// 5: a node has an `Unjudged` edge.
    UnjudgedEdge { class: Class, node: NodeId },
    /// 5: a rewrite is not followed by one retry and then the cut.
    RewriteNotRetried { class: Class, node: NodeId },
    /// 6: a graph ends in an end that is not its class's.
    ForeignEnd {
        class: Class,
        node: NodeId,
        end: End,
    },
    /// 7: the ceiling rises along an edge, or by using the re-ask.
    CeilingRises { class: Class, node: NodeId },
}

/// Every invariant of §2.5 that `policy` breaks, each once. The ceiling is
/// held only against a policy that ends: a graph that loops has no longest
/// path.
pub fn violations(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let looping = cycles(policy);
    let ends = looping.is_empty();
    let mut found = missing_nodes(policy);
    found.extend(looping);
    found.extend(cuts_without_rewrite(policy));
    found.extend(unread_vetoes(policy));
    found.extend(unrepaired_citations(policy));
    found.extend(unjudged_edges(policy));
    found.extend(unretried_rewrites(policy));
    found.extend(foreign_ends(policy));
    if ends {
        let mut ceiling = Ceiling::new(policy);
        found.extend(ceiling_rises(policy, &mut |at: Position| ceiling.most(at)));
    }
    dedupe(found)
}

fn dedupe(all: Vec<Violation>) -> Vec<Violation> {
    let mut unique: Vec<Violation> = Vec::new();
    for violation in all {
        if !unique.contains(&violation) {
            unique.push(violation);
        }
    }
    unique
}

/// The nodes of its own graph that `next` can lead to.
fn nodes_of(next: Next) -> Vec<NodeId> {
    match next {
        Next::Go(Target::Node(id)) => vec![id],
        Next::FirstChange {
            change: Target::Node(id),
            otherwise,
        } => vec![id, otherwise],
        Next::FirstChange { otherwise, .. } => vec![otherwise],
        Next::Go(Target::Enter(_)) | Next::End(_) => Vec::new(),
    }
}

fn step_of(graph: &Graph, id: NodeId) -> Option<Step> {
    graph.node(id).map(|node| node.step)
}

/// Every position a statement can reach from a class's entry, in the order
/// found.
fn reachable(policy: &dyn AcceptancePolicy) -> Vec<Position> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<Position> = CLASSES
        .iter()
        .rev()
        .map(|&class| start(policy, class))
        .collect();
    while let Some(at) = stack.pop() {
        if !seen.insert(at) {
            continue;
        }
        found.push(at);
        let Some(node) = policy.graph(at.class).node(at.node) else {
            continue;
        };
        for &(_, next) in node.edges() {
            if let Moved::To(to) = follow(policy, at, next) {
                stack.push(to);
            }
        }
    }
    found
}

fn missing_nodes(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    for class in CLASSES {
        let graph = policy.graph(class);
        for id in [graph.entry(), graph.after_rewrite()] {
            if graph.node(id).is_none() {
                found.push(Violation::MissingNode {
                    class,
                    node: id,
                    missing: id,
                });
            }
        }
        for node in graph.nodes() {
            for &(_, next) in node.edges() {
                for target in nodes_of(next) {
                    if graph.node(target).is_none() {
                        found.push(Violation::MissingNode {
                            class,
                            node: node.id(),
                            missing: target,
                        });
                    }
                }
            }
        }
    }
    found
}

/// 1: a depth-first walk over the positions a statement can reach, class
/// changes included.
fn cycles(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    let mut done = HashSet::new();
    for class in CLASSES {
        visit(
            policy,
            start(policy, class),
            &mut Vec::new(),
            &mut done,
            &mut found,
        );
    }
    found
}

fn visit(
    policy: &dyn AcceptancePolicy,
    at: Position,
    path: &mut Vec<Position>,
    done: &mut HashSet<Position>,
    found: &mut Vec<Violation>,
) {
    if !done.insert(at) {
        return;
    }
    path.push(at);
    if let Some(node) = policy.graph(at.class).node(at.node) {
        for &(_, next) in node.edges() {
            match follow(policy, at, next) {
                Moved::To(to) if path.contains(&to) => found.push(Violation::Cycle {
                    class: to.class,
                    node: to.node,
                }),
                Moved::To(to) => visit(policy, to, path, done, found),
                Moved::Illegal => found.push(Violation::Cycle {
                    class: at.class,
                    node: at.node,
                }),
                Moved::Ended(_) => {}
            }
        }
    }
    path.pop();
}

/// 2: every path to Removed passes a rewrite.
fn cuts_without_rewrite(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<(Position, bool)> = CLASSES
        .iter()
        .rev()
        .map(|&class| (start(policy, class), false))
        .collect();
    while let Some((at, rewritten)) = stack.pop() {
        if !seen.insert((at, rewritten)) {
            continue;
        }
        let Some(node) = policy.graph(at.class).node(at.node) else {
            continue;
        };
        let rewritten = rewritten || matches!(node.step, Step::Rewrite(_));
        for &(_, next) in node.edges() {
            match follow(policy, at, next) {
                Moved::To(to) => stack.push((to, rewritten)),
                Moved::Ended(End::Removed { .. }) if !rewritten => {
                    found.push(Violation::CutWithoutRewrite {
                        class: at.class,
                        node: at.node,
                    });
                }
                Moved::Ended(_) | Moved::Illegal => {}
            }
        }
    }
    found
}

/// 3: a closed-book refusal reaches a part-wise reading before any rewrite,
/// cut or end.
fn unread_vetoes(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    for at in reachable(policy) {
        let Some(node) = policy.graph(at.class).node(at.node) else {
            continue;
        };
        if !matches!(node.step, Step::Read(Reading::Knowledge | Reading::Veto)) {
            continue;
        }
        let unread = node
            .next(Outcome::Refuted)
            .is_some_and(|next| !reads_the_passages(policy, at, next));
        if unread {
            found.push(Violation::VetoUnread {
                class: at.class,
                node: at.node,
            });
        }
    }
    found
}

/// Whether every path from `next` reaches a part-wise reading before a
/// rewrite, a cut or an end.
fn reads_the_passages(policy: &dyn AcceptancePolicy, at: Position, next: Next) -> bool {
    let mut stack = match follow(policy, at, next) {
        Moved::To(first) => vec![first],
        Moved::Ended(_) | Moved::Illegal => return false,
    };
    let mut seen = HashSet::new();
    while let Some(here) = stack.pop() {
        if !seen.insert(here) {
            continue;
        }
        let Some(node) = policy.graph(here.class).node(here.node) else {
            continue;
        };
        match node.step {
            Step::Read(Reading::Parts) => continue,
            Step::Rewrite(_) | Step::Cut => return false,
            _ => {}
        }
        for &(_, onward) in node.edges() {
            match follow(policy, here, onward) {
                Moved::To(to) => stack.push(to),
                Moved::Ended(_) | Moved::Illegal => return false,
            }
        }
    }
    true
}

/// 4: the first readings (the part-wise and page readings the entry reaches
/// through readings and research alone) answer a citation's fault with a
/// citation remedy. Later readings judge standing, not attribution.
fn unrepaired_citations(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    for class in CLASSES {
        let graph = policy.graph(class);
        for id in first_readings(graph) {
            let Some(node) = graph.node(id) else {
                continue;
            };
            for outcome in [Outcome::StandsElsewhere, Outcome::AllCommon] {
                let unrepaired = node
                    .next(outcome)
                    .is_some_and(|next| !repairs_the_citation(graph, outcome, next));
                if unrepaired {
                    found.push(Violation::CitationFaultNotRepaired {
                        class,
                        node: id,
                        outcome,
                    });
                }
            }
        }
    }
    found
}

fn first_readings(graph: &Graph) -> Vec<NodeId> {
    let mut readings = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![graph.entry()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(node) = graph.node(id) else {
            continue;
        };
        match node.step {
            Step::Read(Reading::Parts | Reading::Page) => readings.push(id),
            Step::Research => {}
            _ => continue,
        }
        for &(_, next) in node.edges() {
            stack.extend(nodes_of(next));
        }
    }
    readings
}

/// Whether `next` answers `outcome` with a recite or an uncite (on the first
/// change, when it has one); an `AllCommon` that has changed class may
/// research instead.
fn repairs_the_citation(graph: &Graph, outcome: Outcome, next: Next) -> bool {
    let remedy = |id: NodeId| matches!(step_of(graph, id), Some(Step::Recite | Step::Uncite));
    let research = |id: NodeId| matches!(step_of(graph, id), Some(Step::Research));
    match next {
        Next::Go(Target::Node(id)) => remedy(id),
        Next::FirstChange {
            change: Target::Node(id),
            otherwise,
        } => {
            remedy(id)
                && (remedy(otherwise) || (outcome == Outcome::AllCommon && research(otherwise)))
        }
        _ => false,
    }
}

/// 5: no node has an `Unjudged` edge.
fn unjudged_edges(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    for class in CLASSES {
        for node in policy.graph(class).nodes() {
            if node.edges().iter().any(|edge| edge.0 == Outcome::Unjudged) {
                found.push(Violation::UnjudgedEdge {
                    class,
                    node: node.id(),
                });
            }
        }
    }
    found
}

/// 5: a rewrite that is not itself a retry sends its `NotApplied` to a retry
/// whose own `NotApplied` goes to the cut.
fn unretried_rewrites(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let reached = reachable(policy);
    let mut found = Vec::new();
    for class in CLASSES {
        let graph = policy.graph(class);
        let rewrites: Vec<&Node> = graph
            .nodes()
            .iter()
            .filter(|node| matches!(node.step, Step::Rewrite(_)))
            .filter(|node| {
                reached
                    .iter()
                    .any(|at| at.class == class && at.node == node.id())
            })
            .collect();
        let retries: Vec<NodeId> = rewrites.iter().copied().filter_map(retry_of).collect();
        for node in rewrites {
            if retries.contains(&node.id()) {
                continue;
            }
            let retried = retry_of(node)
                .and_then(|retry| graph.node(retry))
                .filter(|retry| matches!(retry.step, Step::Rewrite(_)))
                .and_then(retry_of)
                .and_then(|cut| step_of(graph, cut))
                .is_some_and(|step| step == Step::Cut);
            if !retried {
                found.push(Violation::RewriteNotRetried {
                    class,
                    node: node.id(),
                });
            }
        }
    }
    found
}

/// Where a rewrite goes when its edit could not be applied, if to a node.
fn retry_of(node: &Node) -> Option<NodeId> {
    match node.next(Outcome::NotApplied) {
        Some(Next::Go(Target::Node(retry))) => Some(retry),
        _ => None,
    }
}

/// 6: a class's ends are its own, or Removed.
fn foreign_ends(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    for class in CLASSES {
        for node in policy.graph(class).nodes() {
            for &(_, next) in node.edges() {
                match next {
                    Next::End(end) if !is_own(class, end) => {
                        found.push(Violation::ForeignEnd {
                            class,
                            node: node.id(),
                            end,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    found
}

fn is_own(class: Class, end: End) -> bool {
    matches!(
        (class, end),
        (Class::Sourced, End::Resolved)
            | (Class::Reasoning, End::Consistent)
            | (Class::Common, End::GeneralKnowledge)
            | (Class::Internal, End::NotAClaim)
            | (_, End::Removed { .. })
    )
}

/// 7: along every edge from a position, in either state of the re-ask,
/// `cost(n) + most(m) <= most(n)`, and so for using the re-ask. `most` is
/// the ceiling being held to it.
pub(super) fn ceiling_rises(
    policy: &dyn AcceptancePolicy,
    most: &mut dyn FnMut(Position) -> u32,
) -> Vec<Violation> {
    let mut found = Vec::new();
    for base in reachable(policy) {
        for reasked in [false, true] {
            let at = Position { reasked, ..base };
            let Some(node) = policy.graph(at.class).node(at.node) else {
                continue;
            };
            let here = most(at);
            let mut ways: Vec<u32> = Vec::new();
            for &(_, next) in node.edges() {
                if let Moved::To(to) = follow(policy, at, next) {
                    ways.push(node.calls.saturating_add(most(to)));
                }
            }
            if !reasked && matches!(node.step, Step::Read(_)) {
                let again = most(Position {
                    reasked: true,
                    ..at
                });
                ways.push(node.calls.saturating_add(again));
            }
            if ways.iter().any(|&way| way > here) {
                found.push(Violation::CeilingRises {
                    class: at.class,
                    node: at.node,
                });
            }
        }
    }
    found
}
