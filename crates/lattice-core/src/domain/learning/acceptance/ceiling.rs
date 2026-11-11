//! What a statement can still cost, derived from the graphs: the longest path
//! in model calls to an end, with the one re-ask on the costliest reading
//! while it is unused, and the rewrite the teaching review may still give a
//! sentence: a statement that ends with a rewrite left, or waits with one,
//! can continue as a successor that is read again.

use std::collections::{HashMap, HashSet};

use super::walk::{follow, next_rewrite, stepped, Moved};
use super::{AcceptancePolicy, End, Position, Step};
use crate::domain::learning::claim_review::Class;

/// The most calls a statement at `at` can still cost: its longest path to an
/// end, and its re-ask on the costliest reading ahead while it is unused.
pub fn most_calls(policy: &dyn AcceptancePolicy, at: Position) -> u32 {
    Ceiling::new(policy).most(at)
}

/// The longest path from `class`'s post-rewrite node: what a neighbour's
/// rewrite can cost. The statement it is asked of has no rewrite left, so the
/// figure holds the path alone, not a further rewrite of the teaching review.
pub fn after_rewrite_calls(policy: &dyn AcceptancePolicy, class: Class) -> u32 {
    most_calls(
        policy,
        Position {
            class,
            changed_class: false,
            reasked: true,
            node: policy.graph(class).after_rewrite(),
            rewrites_left: 0,
        },
    )
}

/// The longest paths of a policy, remembered per position.
pub(super) struct Ceiling<'a> {
    policy: &'a dyn AcceptancePolicy,
    known: HashMap<Position, u32>,
    open: HashSet<Position>,
}

impl<'a> Ceiling<'a> {
    pub(super) fn new(policy: &'a dyn AcceptancePolicy) -> Self {
        Self {
            policy,
            known: HashMap::new(),
            open: HashSet::new(),
        }
    }

    /// A position met again on its own path counts for nothing, so a graph
    /// that loops still ends here.
    pub(super) fn most(&mut self, at: Position) -> u32 {
        if let Some(&known) = self.known.get(&at) {
            return known;
        }
        if !self.open.insert(at) {
            return 0;
        }
        let longest = self.longest(at);
        self.open.remove(&at);
        self.known.insert(at, longest);
        longest
    }

    fn longest(&mut self, at: Position) -> u32 {
        let policy = self.policy;
        let Some(node) = policy.graph(at.class).node(at.node) else {
            return 0;
        };
        let mut onward: u32 = 0;
        for &(_, next) in node.edges() {
            match follow(policy, at, next) {
                Moved::To(to) => onward = onward.max(self.most(to)),
                Moved::Ended(End::Removed { .. }) | Moved::Illegal => {}
                Moved::Ended(_) => {
                    if let Some(rewritten) = next_rewrite(policy, stepped(policy, at)) {
                        onward = onward.max(self.most(rewritten));
                    }
                }
            }
        }
        if !at.reasked && matches!(node.step, Step::Read(_)) {
            onward = onward.max(self.most(Position {
                reasked: true,
                ..at
            }));
        }
        let along = node.calls.saturating_add(onward);
        match next_rewrite(policy, at) {
            Some(rewritten) => along.max(self.most(rewritten)),
            None => along,
        }
    }
}
