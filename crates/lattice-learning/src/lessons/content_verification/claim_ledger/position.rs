//! Where a claim stands in its class's graph, read from the steps the ledger
//! records for it (acceptance design §2.9).

use super::ClaimLedger;
use crate::lessons::call_plan::{Joining, Planned, PlannedAt};
use lattice_core::domain::learning::acceptance::{
    born, decide, start, stepped, successor, walk_from, AcceptancePolicy, End, Position, Step,
    Taken, Walked,
};
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimId, ClaimLedgerEntry, Class, ClassifiedBy,
};
use serde_json::Value;

/// A claim's walk: where its steps leave it, and where it stood last with
/// every step it took counted: the position it left to end, when it ended.
struct Course {
    walked: Walked,
    last: Position,
}

/// A kind of step, whatever its reading or goal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::lessons::content_verification) enum StepKind {
    Read,
    Research,
    Recite,
    Uncite,
    Rewrite,
    Cut,
}

impl StepKind {
    fn of(step: Step) -> Self {
        match step {
            Step::Read(_) => Self::Read,
            Step::Research => Self::Research,
            Step::Recite => Self::Recite,
            Step::Uncite => Self::Uncite,
            Step::Rewrite(_) => Self::Rewrite,
            Step::Cut => Self::Cut,
        }
    }
}

/// The class `entry` was sorted into and who sorted it: the strongest
/// authority's before its first decision, never the evidence's, which the
/// decisions record after.
pub(super) fn sorted_as(entry: &ClaimLedgerEntry) -> Option<(Class, ClassifiedBy)> {
    let mut held: Option<(Class, ClassifiedBy)> = None;
    for record in entry.events() {
        match &record.event {
            ClaimEvent::Stepped { .. } => break,
            ClaimEvent::Classified { class, by }
                if *by != ClassifiedBy::Evidence && class.prevails(*by, held) =>
            {
                held = Some((*class, *by));
            }
            _ => {}
        }
    }
    held
}

/// The class `entry` was sorted into.
fn sorted_class(entry: &ClaimLedgerEntry) -> Option<Class> {
    sorted_as(entry).map(|(class, _)| class)
}

/// The claims a machine rewrite or narrowing of `entry`'s sentence made its
/// successors, in order.
fn successors(entry: &ClaimLedgerEntry) -> impl Iterator<Item = &ClaimId> {
    entry
        .events()
        .iter()
        .filter_map(|record| match &record.event {
            ClaimEvent::Superseded {
                successor: Some(next),
                by: Actor::Machine,
            } => Some(next),
            _ => None,
        })
}

impl ClaimLedger {
    /// Where `id` stands in its class's graph, from the steps recorded for it.
    /// A claim succeeding another starts after the other's rewrite; any other
    /// starts at the entry of its class. `None` for a claim the ledger does
    /// not hold or that has no class to be checked as.
    pub(in crate::lessons::content_verification) fn position(
        &self,
        policy: &dyn AcceptancePolicy,
        id: &ClaimId,
    ) -> Option<Walked> {
        self.course(policy, id, self.entries.len())
            .map(|course| course.walked)
    }

    /// Where the machine's statement `id` waits: its position and the step of
    /// its node. None for a claim the user holds, one that has ended, one with
    /// no class and one whose record does not fit its graph.
    pub(in crate::lessons::content_verification) fn waits_at(
        &self,
        policy: &dyn AcceptancePolicy,
        id: &ClaimId,
    ) -> Option<(Position, Step)> {
        if !self.entry(id)?.state().is_machine_owned() {
            return None;
        }
        let Some(Walked::At(at)) = self.position(policy, id) else {
            return None;
        };
        let step = policy.graph(at.class).node(at.node)?.step;
        Some((at, step))
    }

    /// The machine's statements of the latest pass that stand at a node of
    /// their graph, in the pass's order, each with its position and the step of
    /// its node.
    pub(super) fn at_nodes(&self, policy: &dyn AcceptancePolicy) -> Vec<(ClaimId, Position, Step)> {
        self.findings
            .iter()
            .filter_map(|id| {
                let (at, step) = self.waits_at(policy, id)?;
                Some((id.clone(), at, step))
            })
            .collect()
    }

    /// The live machine-owned statements whose position is at a node of a step
    /// of `kind`.
    pub(in crate::lessons::content_verification) fn at(
        &self,
        policy: &dyn AcceptancePolicy,
        kind: StepKind,
    ) -> Vec<ClaimId> {
        self.at_nodes(policy)
            .into_iter()
            .filter(|(_, _, step)| StepKind::of(*step) == kind)
            .map(|(id, _, _)| id)
            .collect()
    }

    /// The statements waiting for a reading, each at the position it is read
    /// from.
    pub(in crate::lessons::content_verification) fn due(
        &self,
        policy: &dyn AcceptancePolicy,
    ) -> Vec<(ClaimId, Position)> {
        self.at_nodes(policy)
            .into_iter()
            .filter(|(_, _, step)| StepKind::of(*step) == StepKind::Read)
            .map(|(id, at, _)| (id, at))
            .collect()
    }

    /// Where `id` would stand after `taken`, from where it stands now; none
    /// when it is not at a node, the step is not the node's, or the step ends
    /// it.
    pub(in crate::lessons::content_verification) fn after(
        &self,
        policy: &dyn AcceptancePolicy,
        id: &ClaimId,
        taken: Taken,
    ) -> Option<Position> {
        let Some(Walked::At(at)) = self.position(policy, id) else {
            return None;
        };
        decide(policy, at, taken)?.1
    }

    /// The plan's statements as the ledger has them now in `candidate`: each
    /// of `claims` followed to the claim that carries its sentence on, at its
    /// place. The plan reads where each stands, not the class or sentence it
    /// carries, so those are the class the ledger holds and none.
    pub(in crate::lessons::content_verification) fn planned(
        &self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        claims: &[ClaimId],
    ) -> Vec<Planned> {
        claims
            .iter()
            .map(|id| self.plan_of(policy, candidate, &self.latest(id)))
            .collect()
    }

    /// Where the plan has `id`: at its place, a cut that narrows its sentence
    /// in `candidate` holding the narrowing's call.
    fn plan_of(&self, policy: &dyn AcceptancePolicy, candidate: &Value, id: &ClaimId) -> Planned {
        let entry = self.entry(id);
        let walked = self.position(policy, id);
        if matches!(walked, Some(Walked::Diverged { .. })) {
            tracing::warn!(claim = %id.0, "claim.plan: a claim's steps do not fit its graph; the plan keeps it where it was");
        }
        // A claim gone with its sentence makes no more calls, wherever its
        // steps left it. One that ended may still have its sentence rewritten
        // by the teaching review, and its successor read again.
        let gone = entry.is_some_and(|entry| entry.state().is_gone());
        let at = if gone {
            PlannedAt::Ended(None)
        } else {
            match (PlannedAt::of(walked), walked) {
                (PlannedAt::At(at), _)
                    if policy
                        .graph(at.class)
                        .node(at.node)
                        .is_some_and(|node| node.step == Step::Cut)
                        && self.narrows(candidate, id) =>
                {
                    PlannedAt::Narrowing(at)
                }
                (PlannedAt::Ended(_), Some(Walked::Ended(end)))
                    if !matches!(end, End::Removed { .. }) =>
                {
                    PlannedAt::Ended(self.teaching_successor(policy, id))
                }
                (other, _) => other,
            }
        };
        Planned {
            class: entry
                .and_then(ClaimLedgerEntry::class)
                .unwrap_or(Class::Sourced),
            at,
        }
    }

    /// The claims of the latest pass that continue none of `claims`, the
    /// plan's, each as the plan takes it in: where it stands in `candidate`
    /// and the plan's statements on its sentence. They join `claims`, in the
    /// pass's order.
    pub(in crate::lessons::content_verification) fn unplanned(
        &self,
        policy: &dyn AcceptancePolicy,
        candidate: &Value,
        claims: &mut Vec<ClaimId>,
    ) -> Vec<Joining> {
        let mut joining = Vec::new();
        for id in &self.findings {
            if self.continues(id, claims) {
                continue;
            }
            let mates = self.cotenants(id);
            let beside: Vec<usize> = claims
                .iter()
                .enumerate()
                .filter(|(_, held)| mates.contains(&self.latest(held)))
                .map(|(index, _)| index)
                .collect();
            claims.push(id.clone());
            joining.push(Joining {
                planned: self.plan_of(policy, candidate, id),
                beside,
            });
        }
        joining
    }

    /// Whether `id` is one of `claims` or carries the sentence of one on after
    /// the machine's rewrites.
    fn continues(&self, id: &ClaimId, claims: &[ClaimId]) -> bool {
        let mut here = id.clone();
        for _ in 0..=self.entries.len() {
            if claims.contains(&here) {
                return true;
            }
            match self.predecessor(&here) {
                Some(before) => here = before.claim_id().clone(),
                None => return false,
            }
        }
        false
    }

    /// The claim that carries `id`'s sentence on after its rewrites: the last
    /// of its machine successors, `id` itself when it has none.
    fn latest(&self, id: &ClaimId) -> ClaimId {
        let mut here = id.clone();
        for _ in 0..self.entries.len() {
            let next = self
                .entry(&here)
                .and_then(|entry| successors(entry).last())
                .cloned();
            match next {
                Some(next) => here = next,
                None => break,
            }
        }
        here
    }

    /// The claim `id` succeeds.
    pub(super) fn predecessor(&self, id: &ClaimId) -> Option<&ClaimLedgerEntry> {
        self.entries
            .iter()
            .find(|entry| successors(entry).any(|next| next == id))
    }

    /// `id`'s walk, its start following at most `hops` predecessors.
    fn course(&self, policy: &dyn AcceptancePolicy, id: &ClaimId, hops: usize) -> Option<Course> {
        let entry = self.entry(id)?;
        let from = self.begins(policy, entry, hops)?;
        let taken: Vec<Taken> = entry.steps().into_iter().map(|(taken, _)| taken).collect();
        let walked = walk_from(policy, from, &taken);
        let last = match walked {
            Walked::At(at) | Walked::Diverged { at, .. } => at,
            Walked::Ended(_) | Walked::Stopped => {
                let (_, before) = taken.split_last()?;
                match walk_from(policy, from, before) {
                    Walked::At(at) => stepped(policy, at),
                    _ => return None,
                }
            }
        };
        Some(Course { walked, last })
    }

    /// Where `id` stood last, with every step it took counted: the position
    /// its next machine rewrite would leave from.
    pub(super) fn stood_last(
        &self,
        policy: &dyn AcceptancePolicy,
        id: &ClaimId,
    ) -> Option<Position> {
        self.course(policy, id, self.entries.len())
            .map(|course| course.last)
    }

    /// Where `entry` begins: after the rewrite of a repair it was born of, with
    /// the rewrites it was given; after the rewrite of the claim it succeeds,
    /// with that claim's class and what it had spent, one rewrite more;
    /// otherwise at the start of the class it was sorted into.
    fn begins(
        &self,
        policy: &dyn AcceptancePolicy,
        entry: &ClaimLedgerEntry,
        hops: usize,
    ) -> Option<Position> {
        let born_of = entry.events().iter().find_map(|record| match record.event {
            ClaimEvent::BornOfRepair { attempts_left, .. } => Some(attempts_left),
            _ => None,
        });
        if let Some(attempts_left) = born_of {
            return Some(born(policy, sorted_class(entry)?, attempts_left));
        }
        let before = hops.checked_sub(1).and_then(|hops| {
            let before = self.predecessor(entry.claim_id())?;
            self.course(policy, before.claim_id(), hops)
        });
        if let Some(Course { last, .. }) = before {
            return Some(successor(policy, last));
        }
        Some(start(policy, sorted_class(entry)?))
    }
}
