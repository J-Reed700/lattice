//! The most calls a lesson's checks can make (claim classes design §3.11,
//! §3.12). The panel shows the calls made so far and this ceiling, never an
//! expected count: each term of the design's count starts at the most it can
//! be and falls to what the first checks find, so the ceiling only comes down.
use lattice_core::application::services::claim_verification::{ClaimVerdict, KNOWLEDGE_SAMPLES};
use lattice_core::domain::learning::claim_review::Class;

/// The calls of one knowledge check: its samples.
pub(crate) const SAMPLES: u32 = KNOWLEDGE_SAMPLES as u32;
/// The calls a failed Sourced statement leads to: the re-check against what
/// research found, the rewrite and its re-check.
const SOURCED_FOLLOW_UP: u32 = 3;
/// The calls a failed Reasoning statement leads to: the rewrite, the
/// consistency re-check, the veto's samples and the strict call on its
/// passages.
const REASONING_FOLLOW_UP: u32 = 3 + SAMPLES;

/// What a check made of a statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fate {
    Passed,
    /// A Reasoning statement its passages contradict, a Common one most
    /// samples call false, a statement its sources do not support.
    Failed,
    /// A Reasoning statement most samples call false: it passes when its
    /// passages state it and fails when they do not.
    Vetoed {
        stated: bool,
    },
    /// It goes on to the strict check, as Sourced.
    ToSource,
    /// The answer to its check could not be read.
    Unanswered,
}

impl Fate {
    /// What a strict check's verdict makes of a statement.
    pub(crate) fn of_strict(verdict: ClaimVerdict) -> Self {
        match verdict {
            ClaimVerdict::Supported => Self::Passed,
            ClaimVerdict::Unverified => Self::Unanswered,
            ClaimVerdict::Contradicted | ClaimVerdict::Unsupported => Self::Failed,
        }
    }
}

/// A statement the checks will make calls for.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Counted {
    pub(crate) class: Class,
    /// Its sentence carries another statement, which a rewrite of the
    /// sentence changes too (§3.2).
    pub(crate) shares: bool,
}

/// The most calls one check of a statement of `class` makes: the check of a
/// Sourced one, its cited passage read part by part or, if it cites nothing,
/// the course's page, never both; the consistency check, the veto's samples
/// and the strict call on its passages for a Reasoning one; the knowledge
/// samples and the strict call of a source check for a Common one.
fn most_calls(class: Class) -> u32 {
    match class {
        Class::Sourced => 1,
        Class::Reasoning => 1 + SAMPLES + 1,
        Class::Common => SAMPLES + 1,
        Class::Internal => 0,
    }
}

/// What a statement stops being able to cost once a check of it ends.
#[derive(Default)]
struct Released {
    failing: u32,
    reasoning_failing: u32,
    vetoes: u32,
    sent: u32,
    repeats: u32,
}

/// What a statement of `class` stops being able to cost when a check ends as
/// `fate`; `strict` for the strict check, which the statements sent to it
/// meet after their own class's check. A statement whose answer could not be
/// read stops costing nothing: it is asked once more.
fn released(class: Class, fate: Fate, strict: bool) -> Released {
    let (passed, vetoed) = match fate {
        Fate::Passed => (true, false),
        Fate::Failed => (false, false),
        Fate::Vetoed { stated } => (stated, true),
        Fate::ToSource | Fate::Unanswered => return Released::default(),
    };
    let passed = u32::from(passed);
    match class {
        Class::Sourced => Released {
            failing: passed,
            repeats: 1,
            ..Released::default()
        },
        Class::Reasoning => Released {
            reasoning_failing: passed,
            vetoes: u32::from(!vetoed),
            repeats: SAMPLES,
            ..Released::default()
        },
        // A Common statement called false is cut at once unless its
        // sentence is shared, so it keeps its share of a rewrite.
        Class::Common => Released {
            failing: passed,
            sent: u32::from(!strict),
            repeats: SAMPLES,
            ..Released::default()
        },
        Class::Internal => Released::default(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Sorted,
    LighterChecked,
    StrictChecked,
}

/// The terms of the design's count (§3.11) as the checks of one lesson leave
/// them: each at the most it can be until the first checks end, then at what
/// they found. Only the first pass's checks lower them; the calls of later
/// passes are the follow-ups already counted.
pub(crate) struct CallPlan {
    /// The sorting calls, made before the statements had classes.
    sorting: u32,
    sourced: u32,
    reasoning: u32,
    common: u32,
    /// F: statements checked as Sourced that can still fail, a Common one
    /// sent to its source check among them.
    failing: u32,
    /// G: Reasoning statements that can still fail.
    reasoning_failing: u32,
    /// K: sections that can still take cuts.
    flow_repairs: u32,
    /// V: Reasoning statements that can still be called false.
    vetoes: u32,
    /// D: Common statements that can still be sent to their source.
    sent: u32,
    /// M: the re-checks of statements that share a sentence.
    shared: u32,
    /// U: the calls to repeat for answers that could not be read.
    repeats: u32,
    stage: Stage,
}

impl CallPlan {
    /// The plan once `statements` are sorted, after `sorting` calls, in
    /// `sections` sections that could take cuts.
    pub(crate) fn new(sorting: u32, statements: &[Counted], sections: u32) -> Self {
        let count = |class: Class| {
            let of_class = statements
                .iter()
                .filter(|statement| statement.class == class);
            u32::try_from(of_class.count()).unwrap_or(u32::MAX)
        };
        let (sourced, reasoning, common) = (
            count(Class::Sourced),
            count(Class::Reasoning),
            count(Class::Common),
        );
        Self {
            sorting,
            sourced,
            reasoning,
            common,
            failing: sourced.saturating_add(common),
            reasoning_failing: reasoning,
            flow_repairs: sections,
            vetoes: reasoning,
            sent: common,
            shared: statements
                .iter()
                .filter(|statement| statement.shares)
                .map(|statement| most_calls(statement.class))
                .fold(0, u32::saturating_add),
            repeats: sourced
                .saturating_add(reasoning.saturating_mul(SAMPLES))
                .saturating_add(common.saturating_mul(SAMPLES)),
            stage: Stage::Sorted,
        }
    }

    /// The most calls the checks can make in all.
    pub(crate) fn ceiling(&self) -> u32 {
        [
            self.sorting,
            self.sourced,
            self.reasoning,
            self.reasoning.saturating_mul(SAMPLES),
            self.common.saturating_mul(SAMPLES),
            self.failing.saturating_mul(SOURCED_FOLLOW_UP),
            self.reasoning_failing.saturating_mul(REASONING_FOLLOW_UP),
            self.flow_repairs,
            self.vetoes,
            self.sent,
            self.shared,
            self.repeats,
        ]
        .into_iter()
        .fold(0, u32::saturating_add)
    }

    /// The cuts are known: only `sections` of the lesson's sections took one
    /// and are left, so only they can need a flow repair.
    pub(crate) fn cuts_known(&mut self, sections: u32) {
        self.flow_repairs = self.flow_repairs.min(sections);
    }

    /// Every Reasoning and Common statement has met the check its class
    /// names, and these are the classes and fates.
    pub(crate) fn lighter_checked(&mut self, fates: &[(Class, Fate)]) {
        if self.stage == Stage::Sorted {
            self.stage = Stage::LighterChecked;
            self.release(fates, false);
        }
    }

    /// Every statement sent to the strict check, whatever its class, has met
    /// it.
    pub(crate) fn strict_checked(&mut self, fates: &[(Class, Fate)]) {
        if self.stage == Stage::LighterChecked {
            self.stage = Stage::StrictChecked;
            self.release(fates, true);
        }
    }

    fn release(&mut self, fates: &[(Class, Fate)], strict: bool) {
        for (class, fate) in fates {
            let freed = released(*class, *fate, strict);
            self.failing = self.failing.saturating_sub(freed.failing);
            self.reasoning_failing = self
                .reasoning_failing
                .saturating_sub(freed.reasoning_failing);
            self.vetoes = self.vetoes.saturating_sub(freed.vetoes);
            self.sent = self.sent.saturating_sub(freed.sent);
            self.repeats = self.repeats.saturating_sub(freed.repeats);
        }
    }
}

#[cfg(test)]
#[path = "call_plan_tests.rs"]
mod tests;
