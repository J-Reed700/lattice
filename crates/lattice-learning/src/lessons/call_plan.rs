//! The most calls a lesson's checks can make (acceptance design §2.4). The
//! panel shows the calls made so far and this ceiling, never an expected
//! count. The ceiling is the calls made and the most still to come: the plan
//! holds only what is ahead. Each statement's share is its longest path
//! through its class's graph from where it is, so the calls a statement makes
//! come out of its share as it moves and the ceiling does not rise. A successor
//! that reads again is paid from its statement's reserve, as it starts, and a
//! cut that narrows its sentence holds the narrowing's call. The plan starts
//! empty: every statement enters it where it first stands, when the pass that
//! identifies it is named, sized as a statement standing there would be. A
//! rise, beyond a reserve or for a statement taken in, is shown. A statement
//! whose path is over still holds the readings of the successor the teaching
//! review may give it, while a rewrite is left.
use lattice_core::domain::learning::acceptance::{
    most_calls, start, AcceptancePolicy, Classes, Position, Walked,
};
use lattice_core::domain::learning::claim_review::Class;

/// Where a statement is in its class's graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlannedAt {
    /// At its class's entry, nothing done.
    Entry,
    At(Position),
    /// At a cut that narrows its sentence: the narrowing's call is still to
    /// come.
    Narrowing(Position),
    /// Its path is over. `Some` holds where a successor would begin when the
    /// teaching review may still rewrite its sentence.
    Ended(Option<Position>),
}

impl PlannedAt {
    /// Where the ledger's walk leaves a statement. A statement with no walk,
    /// or whose record does not fit the graph, stays at its entry: the plan
    /// never moves on a guess.
    pub(crate) fn of(walked: Option<Walked>) -> Self {
        match walked {
            Some(Walked::At(at)) => Self::At(at),
            Some(Walked::Ended(_) | Walked::Stopped) => Self::Ended(None),
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
}

/// A statement the plan does not hold yet, taken in where it stands.
pub(crate) struct Joining {
    pub(crate) planned: Planned,
    /// The plan's statements, by their place in its order, that stand on its
    /// sentence.
    pub(crate) beside: Vec<usize>,
}

/// What the plan holds for one statement.
struct Slot {
    /// The class the statement is in: the one it was sorted into until its
    /// walk is in another.
    class: Class,
    group: Option<u32>,
    /// The most calls it can still cost: its longest path from where it is.
    most: u32,
    /// M, sized when the statement is taken in: the readings of
    /// its class after a neighbour's rewrite and after a neighbour's narrowing
    /// cut. A successor that reads again draws on it; it goes when the
    /// statement and its whole sentence have ended.
    reserve: u32,
    ended: bool,
}

/// What is still to come in the checks of one lesson.
pub(crate) struct CallPlan {
    slots: Vec<Slot>,
    /// K: flow repairs still to be made, one for each section that can take a
    /// cut.
    flow_repairs: u32,
    /// The label the next group of statements gets.
    next_group: u32,
}

/// The most calls a statement of `class` can still cost at `at`.
fn most(policy: &dyn AcceptancePolicy, class: Class, at: PlannedAt) -> u32 {
    match at {
        PlannedAt::Entry => most_calls(policy, start(policy, class)),
        PlannedAt::At(at) | PlannedAt::Ended(Some(at)) => most_calls(policy, at),
        PlannedAt::Narrowing(at) => most_calls(policy, at).saturating_add(1),
        PlannedAt::Ended(None) => 0,
    }
}

/// M for a statement of `class`: two readings from its post-rewrite node,
/// each with the statement's re-ask still possible, and the narrowing call. A
/// successor beyond the statement's budget, or a neighbour's change of a
/// statement with no rewrite left, is covered by it, so it holds no rewrite.
fn reserve(policy: &dyn AcceptancePolicy, class: Class) -> u32 {
    let reading = most_calls(
        policy,
        Position {
            class,
            visited: Classes::of(class),
            reasked: false,
            node: policy.graph(class).after_rewrite(),
            rewrites_left: 0,
        },
    );
    reading.saturating_mul(2).saturating_add(1)
}

/// A statement at the entry of a class with nothing spent has not moved,
/// whatever class the ledger holds for it: it is at the entry of its own.
fn settled(policy: &dyn AcceptancePolicy, at: PlannedAt) -> PlannedAt {
    match at {
        PlannedAt::At(position)
            if position.visited == Classes::of(position.class)
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
        PlannedAt::At(position)
        | PlannedAt::Narrowing(position)
        | PlannedAt::Ended(Some(position)) => position.class,
        PlannedAt::Entry | PlannedAt::Ended(None) => sorted,
    }
}

impl CallPlan {
    /// A plan with no statements, in `sections` sections that could take cuts.
    pub(crate) fn empty(sections: u32) -> Self {
        Self {
            slots: Vec::new(),
            flow_repairs: sections,
            next_group: 0,
        }
    }

    /// Takes in `joining`, statements the plan does not hold, each where it
    /// stands: its longest path from there and, when other statements stand on
    /// its sentence, a reserve. Reserves of statements whose sentences are over
    /// are released. Gives the calls it took in.
    pub(crate) fn take_in(&mut self, policy: &dyn AcceptancePolicy, joining: &[Joining]) -> u32 {
        let before = self.shares().saturating_add(self.reserves());
        for statement in joining {
            let at = settled(policy, statement.planned.at);
            let class = class_at(statement.planned.class, at);
            let group = self.group_beside(policy, &statement.beside);
            self.slots.push(Slot {
                class,
                group,
                most: most(policy, class, at),
                reserve: if group.is_some() {
                    reserve(policy, statement.planned.class)
                } else {
                    0
                },
                ended: at == PlannedAt::Ended(None),
            });
        }
        self.release();
        self.shares()
            .saturating_add(self.reserves())
            .saturating_sub(before)
    }

    /// The group of a statement taken in beside the plan's statements at
    /// `beside`: theirs, one if they stood in several, or a new one; none when
    /// it stands alone. Each of them that holds no reserve gets its class's,
    /// since the newcomer's rewrite can have it read again.
    fn group_beside(&mut self, policy: &dyn AcceptancePolicy, beside: &[usize]) -> Option<u32> {
        if beside.is_empty() {
            return None;
        }
        let theirs: Vec<u32> = beside
            .iter()
            .filter_map(|&index| self.slots.get(index).and_then(|slot| slot.group))
            .collect();
        let group = match theirs.first() {
            Some(&group) => group,
            None => {
                let group = self.next_group;
                self.next_group = self.next_group.saturating_add(1);
                group
            }
        };
        for slot in &mut self.slots {
            if slot.group.is_some_and(|other| theirs.contains(&other)) {
                slot.group = Some(group);
            }
        }
        for &index in beside {
            if let Some(slot) = self.slots.get_mut(index) {
                slot.group = Some(group);
                if slot.reserve == 0 {
                    slot.reserve = reserve(policy, slot.class);
                }
            }
        }
        Some(group)
    }

    /// The most calls the checks can make in all: the `made` so far and the
    /// most still to come.
    pub(crate) fn ceiling(&self, made: u32) -> u32 {
        made.saturating_add(self.shares())
            .saturating_add(self.reserves())
            .saturating_add(self.flow_repairs)
    }

    /// What the statements can still cost along their paths, all told.
    pub(crate) fn shares(&self) -> u32 {
        self.slots
            .iter()
            .fold(0, |total: u32, slot| total.saturating_add(slot.most))
    }

    /// What is held in reserve for the statements' readings after a
    /// neighbour's rewrite or narrowing.
    pub(crate) fn reserves(&self) -> u32 {
        self.slots
            .iter()
            .fold(0, |total: u32, slot| total.saturating_add(slot.reserve))
    }

    /// The flow repairs still held.
    pub(crate) fn flow_repairs(&self) -> u32 {
        self.flow_repairs
    }

    /// A section's flow repair is done: the call held for it is made, or was
    /// not needed.
    pub(crate) fn flow_repaired(&mut self) {
        self.flow_repairs = self.flow_repairs.saturating_sub(1);
    }

    /// The cuts are known: only `sections` of the lesson's sections took one
    /// and are left, so only they can need a flow repair.
    pub(crate) fn cuts_known(&mut self, sections: u32) {
        self.flow_repairs = self.flow_repairs.min(sections);
    }

    /// `statements` are the plan's statements in its order, each now at its
    /// place; the sentence they carry is not read, the plan fixed it when the
    /// statement was taken in, and their class is where their walk is. Those
    /// beyond the plan's are ignored.
    ///
    /// A statement's number falls to what its place costs. Where the place
    /// costs more, a successor of the statement reads again and the increase
    /// comes out of its reserve; what the reserve cannot cover is a rise,
    /// logged and shown, and the number this gives. A reserve released stays
    /// released.
    pub(crate) fn moved(&mut self, policy: &dyn AcceptancePolicy, statements: &[Planned]) -> u32 {
        let held = self.slots.len();
        if statements.len() > held {
            tracing::warn!(
                held,
                moved = statements.len(),
                "claim.plan: statements the plan does not hold moved; they are ignored"
            );
        }
        let mut beyond: u32 = 0;
        for (slot, statement) in self.slots.iter_mut().zip(statements) {
            let at = settled(policy, statement.at);
            slot.class = class_at(slot.class, at);
            slot.ended = at == PlannedAt::Ended(None);
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
                beyond = beyond.saturating_add(now - slot.most);
                slot.most = now;
            }
        }
        self.release();
        beyond
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
