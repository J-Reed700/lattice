//! Why a lesson stops: a statement the policy cannot walk, a checker whose
//! answers cannot be read, a pass that moved nothing. Each stop says in words
//! what is wrong and what to do (acceptance design §2.10).

use super::super::*;
use super::{is_gone, ClaimLedger, Waiting};
use crate::features::learning::persistence::now;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Step, Walked};
use lattice_core::domain::learning::claim_review::{
    ClaimEvent, ClaimLedgerEntry, LessonReviewState, StopReason,
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
    /// The stop for a lesson whose statements the policy cannot walk: one
    /// decided under another policy than the current, or one whose record
    /// does not fit its graph; and for one whose pass left out a statement
    /// while its sentence stands.
    pub(in crate::features::learning::lessons::content_verification) fn fault(
        &self,
        policy: &dyn AcceptancePolicy,
    ) -> Option<StopReason> {
        if let Some(entry) = self.unplaced.iter().find_map(|id| self.entry(id)) {
            return Some(self.stop_reason(
                "claim_left_out",
                format!(
                    "The pass named no statement for \"{}\", though the lesson still holds its sentence, \"{}\". The claim is kept as it was, not ended.",
                    entry.statement(),
                    entry.location().quote
                ),
                "The draft is saved. Resume this lesson to read it again. If it stops here again, report this lesson's claim review so its extraction can be repaired.",
            ));
        }
        for id in &self.findings {
            let Some(entry) = self.entry(id).filter(|entry| !is_gone(entry.state())) else {
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
                    "policy_changed",
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
                    "policy_record_fault",
                    format!(
                        "The record of \"{}\" does not fit the checking rules: at {node}, {:?} came out {:?}, which the rules do not allow there.",
                        entry.statement(),
                        taken.step,
                        taken.outcome
                    ),
                    "The draft is saved. Report this lesson's claim review so its record can be repaired.",
                ));
            }
        }
        None
    }

    /// A stop with `code`, in the checker's name.
    pub(super) fn stop_reason(&self, code: &str, message: String, fix: &str) -> StopReason {
        StopReason {
            code: code.into(),
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
            "checker_unreadable",
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
    pub(in crate::features::learning::lessons::content_verification) async fn stop(
        &mut self,
        raw: &str,
        reason: StopReason,
    ) -> Result<AppError> {
        self.write_review(LessonReviewState::Stopped, raw, Some(reason.clone()))
            .await?;
        Ok(AppError::Other(reason.message))
    }

    /// The lesson stops: a whole pass moved no statement and changed nothing,
    /// so going on would repeat it. The stop names each statement that waits and
    /// says in words what it waits for; the nodes go to the log. When the
    /// round's research reached nothing, which is the usual cause, it says so.
    pub(in crate::features::learning::lessons::content_verification) async fn stop_no_progress(
        &mut self,
        raw: &str,
        waiting: &[Waiting],
        unreached: bool,
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
        let reason = if unreached && !researching.is_empty() {
            let quoted: Vec<String> = researching
                .iter()
                .map(|waits| format!("\"{}\"", statement(waits)))
                .collect();
            let count = quoted.len();
            self.stop_reason(
                "no_progress",
                format!(
                    "Research could not reach any search engine or web page, so {count} statement{} still {} for a source: {}.",
                    if count == 1 { "" } else { "s" },
                    if count == 1 { "waits" } else { "wait" },
                    quoted.join("; ")
                ),
                "Check your internet connection and the research settings, then resume this lesson.",
            )
        } else {
            let waits: Vec<String> = waiting
                .iter()
                .map(|waits| format!("\"{}\" waits {}", statement(waits), describe(waits.step)))
                .collect();
            self.stop_reason(
                "no_progress",
                format!(
                    "The checks stopped because a whole pass changed nothing: {}.",
                    waits.join("; ")
                ),
                "Resume this lesson to try again. If it stops here again, report this lesson's claim review.",
            )
        };
        self.stop(raw, reason).await
    }
}
