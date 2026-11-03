//! The most calls a lesson's checks can make (acceptance design §2.4). The
//! panel shows the calls made so far and this ceiling, never an expected
//! count. Each statement's share is its longest path through its class's graph
//! from where it is, so the ceiling comes down as statements move. A successor
//! that reads again is paid from its statement's reserve; a rise beyond the
//! reserve is shown.
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
    /// The class the statement is in: the one it was sorted into until its
    /// walk is in another.
    class: Class,
    group: Option<u32>,
    /// The most calls it can still cost: its longest path from where it is.
    most: u32,
    /// M, sized at the sort: the readings of its class after a neighbour's
    /// rewrite and after a neighbour's narrowing cut. A successor that reads
    /// again draws on it; it goes when the statement and its whole sentence
    /// have ended.
    reserve: u32,
    ended: bool,
}

/// The terms of the ceiling as the checks of one lesson leave them.
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

/// A statement at the entry of a class with nothing spent has not moved,
/// whatever class the ledger holds for it: it is at the entry of its own.
fn settled(policy: &dyn AcceptancePolicy, at: PlannedAt) -> PlannedAt {
    match at {
        PlannedAt::At(position)
            if !position.changed_class
                && !position.reasked
                && position.node == policy.graph(position.class).entry() =>
        {
            PlannedAt::Entry
        }
        other => other,
    }
}

/// The class a statement sorted into `sorted` is in at `at`: where its walk is.
fn class_at(sorted: Class, at: PlannedAt) -> Class {
    match at {
        PlannedAt::At(position) => position.class,
        PlannedAt::Entry | PlannedAt::Ended => sorted,
    }
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
            .map(|statement| {
                let at = settled(policy, statement.at);
                let class = class_at(statement.class, at);
                Slot {
                    class,
                    group: statement.sentence,
                    most: most(policy, class, at),
                    reserve: if statement.sentence.is_some() {
                        reserve(policy, statement.class)
                    } else {
                        0
                    },
                    ended: at == PlannedAt::Ended,
                }
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
    /// place; the sentence they carry is not read, the plan fixed it when it
    /// was made, and their class is where their walk is. Those beyond the
    /// plan's are ignored.
    ///
    /// A statement's number falls to what its place costs. Where the place
    /// costs more, a successor of the statement reads again and the increase
    /// comes out of its reserve; what the reserve cannot cover is a rise,
    /// logged and shown. A reserve released stays released.
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
            let at = settled(policy, statement.at);
            slot.class = class_at(slot.class, at);
            slot.ended = at == PlannedAt::Ended;
            let now = most(policy, slot.class, at);
            if now <= slot.most {
                slot.most = now;
                continue;
            }
            let taken = (now - slot.most).min(slot.reserve);
            slot.reserve -= taken;
            slot.most += taken;
            if now > slot.most {
                tracing::error!(
                    class = ?slot.class,
                    held = slot.most,
                    now,
                    "claim.plan: a statement can cost more than the plan held for it and its reserve; the ceiling rises"
                );
                slot.most = now;
            }
        }
        self.release();
    }

    /// Releases the reserve of each statement that has ended when every other
    /// statement on its sentence has: none is left to rewrite the sentence or
    /// to be read again.
    fn release(&mut self) {
        let over: Vec<bool> = (0..self.slots.len())
            .map(|index| self.sentence_over(index))
            .collect();
        for (slot, over) in self.slots.iter_mut().zip(over) {
            if over {
                slot.reserve = 0;
            }
        }
    }

    /// Whether the statement at `index` and every other statement on its
    /// sentence have ended.
    fn sentence_over(&self, index: usize) -> bool {
        self.slots.get(index).is_some_and(|slot| slot.ended) && !self.neighbour_stands(index)
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
