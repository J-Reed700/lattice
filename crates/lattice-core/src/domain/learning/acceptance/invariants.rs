//! The invariants of the design's §2.5, held against every graph of a policy.

use std::collections::HashSet;

use super::ceiling::Ceiling;
use super::walk::{follow, next_rewrite, onward, start, Moved};
use super::{
    AcceptancePolicy, Classes, End, Graph, Next, Node, NodeId, Otherwise, Outcome, Position,
    Reading, Step, Target, CLASSES,
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
    /// 1: a statement can come back to a node it left, or enter a class at an
    /// entry twice.
    Cycle { class: Class, node: NodeId },
    /// 2: a statement can be removed without a rewrite first.
    CutWithoutRewrite { class: Class, node: NodeId },
    /// 3: a closed-book refusal can lead to a rewrite, a cut or an end before
    /// the passages are read as a ground reading.
    VetoUnread { class: Class, node: NodeId },
    /// 4: a citation's fault is answered with anything but the citation
    /// remedy: a recite for `StandsElsewhere`, an uncite for `AllCommon`.
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
    /// 8: a rewrite's successor can be rewritten again, or can change class
    /// into an entry.
    RewrittenTwice { class: Class, node: NodeId },
    /// 9: a rewrite or a cut is reachable from an entry without crossing an
    /// edge on `Refuted`.
    RewriteWithoutRefutation { class: Class, node: NodeId },
    /// 10: a first entry does not lead into the class it names.
    FirstEntryMisled { class: Class, node: NodeId },
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
    found.extend(rewritten_twice(policy));
    found.extend(rewrites_without_refutation(policy));
    found.extend(misled_first_entries(policy));
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
        Next::FirstEntry { by, otherwise, .. } => {
            let mut nodes = Vec::new();
            if let Target::Node(id) = by {
                nodes.push(id);
            }
            if let Otherwise::Node(id) = otherwise {
                nodes.push(id);
            }
            nodes
        }
        Next::Go(Target::Enter(_) | Target::EnterAfterRewrite(_)) | Next::End(_) => Vec::new(),
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
        if policy.graph(at.class).node(at.node).is_none() {
            continue;
        }
        stack.extend(onward(policy, at));
        stack.extend(next_rewrite(policy, at));
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

/// 3: a closed-book refusal reaches a ground reading before any rewrite, cut
/// or end.
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

/// Whether every path from `next` reaches a ground reading before a rewrite,
/// a cut or an end.
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
            Step::Read(Reading::Ground) => continue,
            Step::Rewrite(_) | Step::Cut => return false,
            _ => {}
        }
        for &(_, edge) in node.edges() {
            match follow(policy, here, edge) {
                Moved::To(to) => stack.push(to),
                Moved::Ended(_) | Moved::Illegal => return false,
            }
        }
    }
    true
}

/// 4: a citation's fault is answered on the citation: every `StandsElsewhere`
/// edge leads to a recite and every `AllCommon` edge to an uncite, directly or
/// as a first entry's `by`. Its `otherwise`, taken by a statement that has been
/// in the class already, is held to nothing.
fn unrepaired_citations(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    for class in CLASSES {
        let graph = policy.graph(class);
        for node in graph.nodes() {
            for (outcome, remedy) in [
                (Outcome::StandsElsewhere, Step::Recite),
                (Outcome::AllCommon, Step::Uncite),
            ] {
                let unrepaired = node
                    .next(outcome)
                    .is_some_and(|next| !leads_to(graph, next, remedy));
                if unrepaired {
                    found.push(Violation::CitationFaultNotRepaired {
                        class,
                        node: node.id(),
                        outcome,
                    });
                }
            }
        }
    }
    found
}

/// Whether `next` goes to a node of the step `wanted`, directly or as a first
/// entry's `by`.
fn leads_to(graph: &Graph, next: Next, wanted: Step) -> bool {
    match next {
        Next::Go(Target::Node(id))
        | Next::FirstEntry {
            by: Target::Node(id),
            ..
        } => step_of(graph, id) == Some(wanted),
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

/// 6: a class's ends are its own, or Removed or Unverified.
fn foreign_ends(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    for class in CLASSES {
        for node in policy.graph(class).nodes() {
            for &(_, next) in node.edges() {
                let end = match next {
                    Next::End(end)
                    | Next::FirstEntry {
                        otherwise: Otherwise::End(end),
                        ..
                    } => end,
                    _ => continue,
                };
                if !is_own(class, end) {
                    found.push(Violation::ForeignEnd {
                        class,
                        node: node.id(),
                        end,
                    });
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
            | (_, End::Removed { .. } | End::Unverified)
    )
}

/// 8: from every class's post-rewrite node, in every state, no rewrite is
/// reachable and no change of class goes to an entry: a rewrite's successor is
/// never rewritten again.
fn rewritten_twice(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = Vec::new();
    for class in CLASSES {
        for visited in visited_with(class) {
            for reasked in [false, true] {
                stack.push(Position {
                    class,
                    visited,
                    reasked,
                    node: policy.graph(class).after_rewrite(),
                    rewrites_left: 0,
                });
            }
        }
    }
    while let Some(at) = stack.pop() {
        if !seen.insert(at) {
            continue;
        }
        let Some(node) = policy.graph(at.class).node(at.node) else {
            continue;
        };
        let rewrites = matches!(node.step, Step::Rewrite(_));
        let enters = node
            .edges()
            .iter()
            .any(|&(_, next)| enters_an_entry(at, next));
        if rewrites || enters {
            found.push(Violation::RewrittenTwice {
                class: at.class,
                node: at.node,
            });
        }
        for &(_, next) in node.edges() {
            if let Moved::To(to) = follow(policy, at, next) {
                stack.push(to);
            }
        }
    }
    found
}

/// Every set of graph classes that holds `class`.
fn visited_with(class: Class) -> Vec<Classes> {
    (0..1_u16 << CLASSES.len())
        .map(|mask| {
            CLASSES
                .iter()
                .enumerate()
                .filter(|&(place, _)| (mask >> place) & 1 == 1)
                .fold(Classes::of(class), |set, (_, &other)| set.with(other))
        })
        .collect()
}

/// 9: no rewrite or cut is reachable from a class's entry without crossing an
/// edge on `Refuted`: only a statement shown false is rewritten or cut. A
/// reading that finds the sources silent says nothing is wrong.
fn rewrites_without_refutation(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
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
        let Some(node) = policy.graph(at.class).node(at.node) else {
            continue;
        };
        if matches!(node.step, Step::Rewrite(_) | Step::Cut) {
            found.push(Violation::RewriteWithoutRefutation {
                class: at.class,
                node: at.node,
            });
        }
        for &(outcome, next) in node.edges() {
            if outcome == Outcome::Refuted {
                continue;
            }
            if let Moved::To(to) = follow(policy, at, next) {
                stack.push(to);
            }
        }
    }
    found
}

/// 10: every first entry's `by` is an entry into its class, or a node of the
/// same graph whose every edge is one.
fn misled_first_entries(policy: &dyn AcceptancePolicy) -> Vec<Violation> {
    let mut found = Vec::new();
    for class in CLASSES {
        let graph = policy.graph(class);
        for node in graph.nodes() {
            for &(_, next) in node.edges() {
                let Next::FirstEntry { into, by, .. } = next else {
                    continue;
                };
                let leads = match by {
                    Target::Enter(entered) => entered == into,
                    Target::Node(id) => graph.node(id).is_some_and(|by| {
                        !by.edges().is_empty()
                            && by
                                .edges()
                                .iter()
                                .all(|&(_, edge)| edge == Next::Go(Target::Enter(into)))
                    }),
                    Target::EnterAfterRewrite(_) => false,
                };
                if !leads {
                    found.push(Violation::FirstEntryMisled {
                        class,
                        node: node.id(),
                    });
                }
            }
        }
    }
    found
}

/// Whether `next`, taken from `at`, enters a class at its entry.
fn enters_an_entry(at: Position, next: Next) -> bool {
    match next {
        Next::Go(Target::Enter(_)) => true,
        Next::FirstEntry {
            by: Target::Enter(_),
            into,
            ..
        } => !at.visited.has(into),
        _ => false,
    }
}

/// 7: along every edge from a position, in either state of the re-ask,
/// `cost(n) + most(m) <= most(n)`, and so for using the re-ask and for the
/// successor a further rewrite of the sentence gives, waiting or ended. `most`
/// is the ceiling being held to it.
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
            ways.extend(
                onward(policy, at)
                    .into_iter()
                    .map(|to| node.calls.saturating_add(most(to))),
            );
            if let Some(rewritten) = next_rewrite(policy, at) {
                ways.push(most(rewritten));
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
