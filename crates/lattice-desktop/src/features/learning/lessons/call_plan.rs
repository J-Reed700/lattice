//! The most calls a lesson's checks can make (acceptance design §2.4). The
//! panel shows the calls made so far and this ceiling, never an expected
//! count. Each statement's share is its longest path through its class's graph
//! from where it is, so the ceiling only comes down as statements move.
use lattice_core::application::services::claim_verification::KNOWLEDGE_SAMPLES;
use lattice_core::domain::learning::acceptance::{
    after_rewrite_calls, most_calls, AcceptancePolicy, Position, Walked,
};
use lattice_core::domain::learning::claim_review::Class;

/// The calls of one knowledge check: its samples.
pub(crate) const SAMPLES: u32 = KNOWLEDGE_SAMPLES as u32;

/// Where a statement is in its class's graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlannedAt {
    /// At its class's entry, nothing done.
    Entry,
    At(Position),
    /// Its path is over.
    Ended,
}

impl PlannedAt {
    /// Where the ledger's walk leaves a statement. A statement with no walk,
    /// or whose record does not fit the graph, stays at its entry: the plan
    /// never moves on a guess.
    pub(crate) fn of(walked: Option<Walked>) -> Self {
        match walked {
            Some(Walked::At(at)) => Self::At(at),
            Some(Walked::Ended(_) | Walked::Stopped) => Self::Ended,
            Some(Walked::Diverged { .. }) | None => Self::Entry,
        }
    }
}

/// A statement the checks will make calls for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Planned {
    /// The class it was sorted into.
    pub(crate) class: Class,
    pub(crate) at: PlannedAt,
    /// The group of statements standing on its sentence, none when it stands
    /// alone. Read when the plan is made: a statement never joins a group
    /// later.
    pub(crate) sentence: Option<u32>,
}

/// What the plan holds for one statement.
struct Slot {
    class: Class,
    group: Option<u32>,
    /// The most calls it can still cost: its longest path from where it is.
    most: u32,
    /// M: the readings of its class after a neighbour's rewrite and after a
    /// neighbour's narrowing cut, held while a neighbour stands on its sentence.
    reserve: u32,
    ended: bool,
}

/// The terms of the ceiling as the checks of one lesson leave them. None of
/// them rises.
pub(crate) struct CallPlan {
    /// The sorting calls, made before the statements had classes.
    sorting: u32,
    slots: Vec<Slot>,
    /// K: sections that can still take cuts.
    flow_repairs: u32,
}

/// The most calls a statement of `class` can still cost at `at`.
fn most(policy: &dyn AcceptancePolicy, class: Class, at: PlannedAt) -> u32 {
    match at {
        PlannedAt::Entry => most_calls(
            policy,
            Position {
                class,
                changed_class: false,
                reasked: false,
                node: policy.graph(class).entry(),
            },
        ),
        PlannedAt::At(at) => most_calls(policy, at),
        PlannedAt::Ended => 0,
    }
}

/// M for a statement of `class`: two readings from its post-rewrite node and
/// the narrowing call.
fn reserve(policy: &dyn AcceptancePolicy, class: Class) -> u32 {
    after_rewrite_calls(policy, class)
        .saturating_mul(2)
        .saturating_add(1)
}

impl CallPlan {
    /// The plan once `statements` are sorted, after `sorting` calls, in
    /// `sections` sections that could take cuts.
    pub(crate) fn new(
        policy: &dyn AcceptancePolicy,
        sorting: u32,
        statements: &[Planned],
        sections: u32,
    ) -> Self {
        let slots = statements
            .iter()
            .map(|statement| Slot {
                class: statement.class,
                group: statement.sentence,
                most: most(policy, statement.class, statement.at),
                reserve: if statement.sentence.is_some() {
                    reserve(policy, statement.class)
                } else {
                    0
                },
                ended: statement.at == PlannedAt::Ended,
            })
            .collect();
        let mut plan = Self {
            sorting,
            slots,
            flow_repairs: sections,
        };
        plan.release();
        plan
    }

    /// The most calls the checks can make in all.
    pub(crate) fn ceiling(&self) -> u32 {
        self.slots.iter().fold(
            self.sorting.saturating_add(self.flow_repairs),
            |total, slot| total.saturating_add(slot.most).saturating_add(slot.reserve),
        )
    }

    /// The cuts are known: only `sections` of the lesson's sections took one
    /// and are left, so only they can need a flow repair.
    pub(crate) fn cuts_known(&mut self, sections: u32) {
        self.flow_repairs = self.flow_repairs.min(sections);
    }

    /// `statements` are the plan's statements in its order, each now at its
    /// place; the class and sentence they carry are not read, the plan fixed
    /// them when it was made. Those beyond the plan's are ignored. A number
    /// never rises, and a reserve released stays released.
    pub(crate) fn moved(&mut self, policy: &dyn AcceptancePolicy, statements: &[Planned]) {
        let held = self.slots.len();
        if statements.len() > held {
            tracing::warn!(
                held,
                moved = statements.len(),
                "claim.plan: statements the plan does not hold moved; they are ignored"
            );
        }
        for (slot, statement) in self.slots.iter_mut().zip(statements) {
            slot.most = slot.most.min(most(policy, slot.class, statement.at));
            slot.ended = statement.at == PlannedAt::Ended;
        }
        self.release();
    }

    /// Releases the reserve of each statement whose neighbours on its
    /// sentence have all ended: none is left to rewrite it.
    fn release(&mut self) {
        let standing: Vec<bool> = (0..self.slots.len())
            .map(|index| self.neighbour_stands(index))
            .collect();
        for (slot, standing) in self.slots.iter_mut().zip(standing) {
            if !standing {
                slot.reserve = 0;
            }
        }
    }

    /// Whether a statement other than the one at `index` stands on its
    /// sentence still.
    fn neighbour_stands(&self, index: usize) -> bool {
        let Some(group) = self.slots.get(index).and_then(|slot| slot.group) else {
            return false;
        };
        self.slots
            .iter()
            .enumerate()
            .any(|(other, slot)| other != index && slot.group == Some(group) && !slot.ended)
    }
}

#[cfg(test)]
#[path = "call_plan_tests.rs"]
mod tests;
