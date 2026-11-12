//! Why a lesson stops: a statement the policy cannot walk, a checker whose
//! answers cannot be read, a pass that moved nothing. Each stop says in words
//! what is wrong and what to do (acceptance design §2.10).

use super::super::research::Unresearched;
use super::super::*;
use super::{ClaimLedger, Waiting};
use crate::persistence::now;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Step, Walked};
use lattice_core::domain::learning::claim_review::{
    Actor, ClaimEvent, ClaimLedgerEntry, ClaimState, LessonReviewState, StopCode, StopReason,
};

/// What a statement waits for at a node of `step`, in words.
fn describe(step: Step) -> &'static str {
    match step {
        Step::Read(_) => "to be read",
        Step::Research => "for research",
        Step::Recite => "for its citation to move",
        Step::Uncite => "for its citation to go",
        Step::Rewrite(_) => "for a rewrite",
        Step::Cut => "to be cut",
    }
}

impl ClaimLedger {
    /// The claims the machine ended whose sentences `candidate` still holds. A
    /// code example's claim ends with its run, and the claims of a section that
    /// goes end with it.
    pub(super) fn standing_ended(&self, candidate: &Value) -> Vec<&ClaimLedgerEntry> {
        self.entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.state(),
                    ClaimState::Removed {
                        by: Actor::Machine,
                        ..
                    }
                ) && self.stands(candidate, entry.claim_id())
            })
            .collect()
    }

    /// The stop for a lesson whose statements the policy cannot walk: one
    /// decided under another policy than the current, or one whose record
    /// does not fit its graph; and for one whose pass left out a statement
    /// while its sentence stands.
    pub(in crate::lessons::content_verification) fn fault(
        &self,
        policy: &dyn AcceptancePolicy,
    ) -> Option<StopReason> {
        if let Some(entry) = self.unplaced.iter().find_map(|id| self.entry(id)) {
            return Some(self.stop_reason(
                StopCode::ClaimLeftOut,
                format!(
                    "The pass named no statement for \"{}\", though the lesson still holds its sentence, \"{}\". The statement is kept as it was, not ended.",
                    entry.statement(),
                    entry.location().quote
                ),
                "The draft is saved. Resume this lesson to read it again. If it stops here again, report this lesson's statement review so its extraction can be repaired.",
            ));
        }
        for id in &self.findings {
            let Some(entry) = self.entry(id).filter(|entry| !entry.state().is_gone()) else {
                continue;
            };
            let other = entry
                .events()
                .iter()
                .find_map(|record| match &record.event {
                    ClaimEvent::Stepped { policy: held, .. } if held != policy.version() => {
                        Some(held.as_str())
                    }
                    _ => None,
                });
            if let Some(held) = other {
                return Some(self.stop_reason(
                    StopCode::PolicyChanged,
                    format!(
                        "The checks on this lesson were decided under earlier checking rules ({held}), and this version of the app checks under {}. \"{}\" cannot go on under them.",
                        policy.version(),
                        entry.statement()
                    ),
                    "The saved checks are kept. Resume this lesson with a version of the app that continues checks made under those rules.",
                ));
            }
            if let Some(Walked::Diverged { at, taken }) = self.position(policy, id) {
                let node = policy
                    .graph(at.class)
                    .node(at.node)
                    .map_or("?", |node| node.name);
                return Some(self.stop_reason(
                    StopCode::PolicyRecordFault,
                    format!(
                        "The record of \"{}\" does not fit the checking rules: at {node}, {:?} came out {:?}, which the rules do not allow there.",
                        entry.statement(),
                        taken.step,
                        taken.outcome
                    ),
                    "The draft is saved. Report this lesson's statement review so its record can be repaired.",
                ));
            }
        }
        None
    }

    /// The stop for a lesson that would publish a sentence a claim the machine
    /// ended stands on, and how many such claims there are: a claim ends only
    /// with its sentence (acceptance design §2.9). A code example's claim ends
    /// with its run, and the claims of a section that goes end with it.
    pub(in crate::lessons::content_verification) fn ended_but_standing(
        &self,
        candidate: &Value,
    ) -> Option<(StopReason, usize)> {
        let standing = self.standing_ended(candidate);
        let first = standing.first()?;
        let more = match standing.len() {
            1 => String::new(),
            others => format!(
                " ({} more statements the machine ended have their sentences in it too)",
                others - 1
            ),
        };
        let reason = self.stop_reason(
            StopCode::EndedClaimStands,
            format!(
                "The lesson still holds \"{}\", which the statement \"{}\" stood on until the machine ended it{more}. A lesson is not published with a sentence no statement answers for.",
                first.location().quote,
                first.statement()
            ),
            "Open the lesson's statement review: undo the removal to check the sentence, or take the sentence out of the draft, then resume this lesson.",
        );
        Some((reason, standing.len()))
    }

    /// A stop with `code`, in the checker's name.
    pub(super) fn stop_reason(&self, code: StopCode, message: String, fix: &str) -> StopReason {
        StopReason {
            code,
            message,
            fix: fix.into(),
            at: now(),
            request: None,
        }
    }

    /// The stop for `claims` statements whose checker gave no answer the
    /// checks can use, asked twice.
    pub(super) fn unreadable(&self, claims: usize) -> StopReason {
        self.stop_reason(
            StopCode::CheckerUnreadable,
            format!(
                "The checking model's answers could not be read for {claims} statement{}: {} gave no answer the checks can use, asked twice.",
                if claims == 1 { "" } else { "s" },
                self.checker_model
            ),
            "Try another verifier model, or a larger context window, in Settings, then resume this lesson.",
        )
    }

    /// The lesson stops for `reason`: the review row says so, and the draft is
    /// kept as it is. No statement has been rewritten or cut for it (claim
    /// classes design §3.2).
    pub(in crate::lessons::content_verification) async fn stop(
        &mut self,
        raw: &str,
        reason: StopReason,
    ) -> Result<AppError> {
        self.write_review(LessonReviewState::Stopped, raw, Some(reason.clone()))
            .await?;
        Ok(AppError::LessonHeld(reason.message))
    }

    /// The lesson stops: a whole pass moved no statement and changed nothing,
    /// so going on would repeat it. The stop names each statement that waits and
    /// says in words what it waits for; the nodes go to the log. When the
    /// round's research could not run, which is the usual cause, it says why.
    pub(in crate::lessons::content_verification) async fn stop_no_progress(
        &mut self,
        raw: &str,
        waiting: &[Waiting],
        unresearched: Option<Unresearched>,
    ) -> Result<AppError> {
        let statement = |waits: &Waiting| {
            self.entry(&waits.claim)
                .map_or("a statement", ClaimLedgerEntry::statement)
        };
        for waits in waiting {
            tracing::info!(
                claim = %waits.claim.0,
                node = waits.node,
                step = ?waits.step,
                "claim.state: a pass moved nothing and this statement waits"
            );
        }
        let researching: Vec<&Waiting> = waiting
            .iter()
            .filter(|waits| waits.step == Step::Research)
            .collect();
        let reason = if let (Some(cause), false) = (unresearched, researching.is_empty()) {
            let quoted: Vec<String> = researching
                .iter()
                .map(|waits| format!("\"{}\"", statement(waits)))
                .collect();
            let count = quoted.len();
            let plural = if count == 1 { "" } else { "s" };
            let wait = if count == 1 { "waits" } else { "wait" };
            let quoted = quoted.join("; ");
            let (text, fix) = match cause {
                Unresearched::Unreached => (
                    format!("Research could not reach any search engine or web page, so {count} statement{plural} still {wait} for a source: {quoted}."),
                    "Check your internet connection and the research settings, then resume this lesson.",
                ),
                Unresearched::Unplanned => (
                    format!("Research could not plan its searches: the Learning model gave no plan Lattice could use, so {count} statement{plural} still {wait} for a source: {quoted}."),
                    "Resume this lesson to try again. If it stops here again, choose another Learning model in Settings → AI.",
                ),
            };
            self.stop_reason(StopCode::NoProgress, text, fix)
        } else {
            let waits: Vec<String> = waiting
                .iter()
                .map(|waits| format!("\"{}\" waits {}", statement(waits), describe(waits.step)))
                .collect();
            self.stop_reason(
                StopCode::NoProgress,
                format!(
                    "The checks stopped because a whole pass changed nothing: {}.",
                    waits.join("; ")
                ),
                "Resume this lesson to try again. If it stops here again, report this lesson's statement review.",
            )
        };
        self.stop(raw, reason).await
    }
}
