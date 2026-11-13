//! What a statement can still cost, derived from the graphs: the longest path
//! in model calls to an end, with the one re-ask on the costliest reading
//! while it is unused, and the rewrite the teaching review may still give a
//! sentence: a statement that ends with a rewrite left, or waits with one,
//! can continue as a successor that is read again.

use std::collections::{HashMap, HashSet};

use super::walk::{next_rewrite, onward};
use super::{AcceptancePolicy, Position, Step};

/// The most calls a statement at `at` can still cost: its longest path to an
/// end, and its re-ask on the costliest reading ahead while it is unused.
pub fn most_calls(policy: &dyn AcceptancePolicy, at: Position) -> u32 {
    Ceiling::new(policy).most(at)
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
        let mut ahead = onward(policy, at)
            .into_iter()
            .map(|to| self.most(to))
            .max()
            .unwrap_or(0);
        if !at.reasked && matches!(node.step, Step::Read(_)) {
            ahead = ahead.max(self.most(Position {
                reasked: true,
                ..at
            }));
        }
        let along = node.calls.saturating_add(ahead);
        match next_rewrite(policy, at) {
            Some(rewritten) => along.max(self.most(rewritten)),
            None => along,
        }
    }
}
