//! The review of generated material, one loop over a subject: fit, review,
//! decide, repair, confine, save.
use std::collections::VecDeque;

use super::repair;
use super::subject::{Decision, Pass, Settled, Step, Subject};
use super::{json_or_text, ReviewSchema};
use crate::lessons::structured_call::refusal;
use crate::lessons::trace::{self, Cause, Stage};
use crate::model_call::ErrorClass;
use crate::outline_progress::OutlineStage;
use crate::review_findings::{ReviewFinding, SetAside};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::Value;

/// The reviewed material, and what the repairs could not settle: a lesson
/// publishes with these waiting for the user.
pub(super) struct Reviewed {
    pub candidate: String,
    pub set_aside: Vec<SetAside>,
}

/// A second model call critiques the actual candidate, and one targeted
/// repair is allowed; what the subject does with the rest is its own (a
/// lesson sends every open finding to a repair, has the reviewer judge again
/// only what a repair changed, and publishes only from a pass whose content was reviewed, with
/// what is left waiting; other material fails closed).
/// This is an AI review, not a claim of independent expert verification.
pub(super) async fn review_material(
    llm: &dyn LLMPort,
    system: &str,
    prompt: &str,
    schema: ReviewSchema<'_>,
    mut candidate: String,
    output_tokens: usize,
    subject: &mut dyn Subject,
) -> Result<Reviewed> {
    subject.resume(&candidate).await?;
    let mut authoring = json_or_text(prompt);
    if let ReviewSchema::Lesson(shape) = schema {
        shape.state_in(&mut authoring);
    }
    subject.given(llm, &authoring);
    subject.save(&candidate).await?;
    for attempt in 0usize.. {
        if let Some(progress) = subject.progress() {
            progress.stage(if attempt == 0 {
                OutlineStage::Reviewing
            } else {
                OutlineStage::CheckingRepair
            });
        }
        // fit
        let mut value = json_or_text(&candidate);
        if subject.settle_quotes(&mut value, &authoring, attempt) {
            let before = candidate.clone();
            candidate = serde_json::to_string(&value)?;
            trace::changed(Stage::TeachingReview, &Cause::none(), &before, &candidate);
            subject.save(&candidate).await?;
        }
        subject.select_sources(llm, &mut authoring, &value).await?;
        let quote_checks = subject.quote_checks(&value, &authoring);
        // review
        let pass = Pass {
            candidate: &candidate,
            value: &value,
            authoring: &authoring,
            quote_checks: &quote_checks,
            schema,
        };
        let found = subject.find(llm, &pass).await?;
        for finding in &found.findings {
            trace::decided_finding(
                Stage::TeachingReview,
                finding,
                "raised",
                &format!("by review pass {attempt}"),
            );
        }
        let edits = found.edits;
        // decide, and repair until the material changes
        loop {
            let send = match subject.decide(attempt, &value, found.findings.clone())? {
                Decision::Publish(set_aside) => {
                    for aside in &set_aside {
                        trace::decided_finding(
                            Stage::TeachingReview,
                            &aside.finding,
                            "waits for the user",
                            &format!("{} repairs were tried and did not settle it", aside.repairs),
                        );
                    }
                    subject.save(&candidate).await?;
                    return Ok(Reviewed {
                        candidate,
                        set_aside,
                    });
                }
                Decision::Repair(send) => send,
                Decision::Review => break,
            };
            for finding in &send {
                trace::decided_finding(
                    Stage::TeachingRepair,
                    finding,
                    "sent to repair",
                    &format!("after review pass {attempt}"),
                );
            }
            if let Some(progress) = subject.progress() {
                progress.stage(OutlineStage::Repairing);
            }
            subject.step(Step::Repairing);
            let repairs = Repairs {
                llm,
                system,
                schema,
                authoring: &authoring,
                edits,
                output_tokens,
            };
            if repairs.make(subject, &mut candidate, send).await? {
                break;
            }
        }
    }
    Err(AppError::InternalError(
        "Teaching review did not complete".into(),
    ))
}

/// The repairs of one pass: what each needs besides the findings it is sent.
struct Repairs<'a> {
    llm: &'a dyn LLMPort,
    system: &'a str,
    schema: ReviewSchema<'a>,
    authoring: &'a Value,
    edits: bool,
    output_tokens: usize,
}

impl Repairs<'_> {
    /// Repairs `send` in groups: a group whose repair cannot be made is
    /// repaired as its two halves, the second against what the first made.
    /// Returns whether the candidate changed.
    async fn make(
        &self,
        subject: &mut dyn Subject,
        candidate: &mut String,
        send: Vec<ReviewFinding>,
    ) -> Result<bool> {
        let mut changed = false;
        let mut groups = VecDeque::from([send]);
        while let Some(group) = groups.pop_front() {
            let previous = json_or_text(candidate);
            let quote_checks = subject.quote_checks(&previous, self.authoring);
            let request = repair::Request {
                system: self.system,
                schema: self.schema.value(),
                authoring: self.authoring,
                candidate: &previous,
                quote_checks: &quote_checks,
                issues: group
                    .iter()
                    .map(|finding| repair::Issue {
                        finding,
                        last_attempt: subject.last_attempt(finding),
                    })
                    .collect(),
                edits: self.edits,
                wording: subject.wording(),
                output_tokens: self.output_tokens,
            };
            let made = repair::ask(self.llm, &request, subject.progress())
                .await
                .and_then(|repaired| subject.settle_repair(&previous, &group, repaired));
            let settled = match made {
                Ok(settled) => settled,
                Err(error) => match ErrorClass::of(&error) {
                    ErrorClass::Outage | ErrorClass::Unavailable | ErrorClass::Fault => {
                        return Err(error)
                    }
                    _ if self.edits && group.len() > 1 => {
                        let (first, second) = group.split_at(group.len() / 2);
                        tracing::info!(%error, findings = group.len(), "The teaching repair could not be made for these findings together; each half is repaired on its own");
                        trace::decided(
                            Stage::TeachingRepair,
                            &trace::group(&group),
                            "split",
                            &format!("the repair could not be made together; each half is repaired on its own: {error}"),
                        );
                        Settled::Split(first.to_vec(), second.to_vec())
                    }
                    class => {
                        let reason = refusal(&error);
                        trace::refused(Stage::TeachingRepair, &trace::group(&group), &reason, None);
                        subject.unmade(&group, class, reason)?
                    }
                },
            };
            match settled {
                Settled::Changed(text) => {
                    trace::changed(
                        Stage::TeachingRepair,
                        &Cause::none().findings(&group),
                        candidate.as_str(),
                        &text,
                    );
                    *candidate = text;
                    changed = true;
                    subject.save(candidate).await?;
                }
                Settled::Unchanged => {
                    trace::decided(
                        Stage::TeachingRepair,
                        &trace::group(&group),
                        "left the material unchanged",
                        "the repair's edits changed nothing that could stay",
                    );
                    subject.save(candidate).await?
                }
                Settled::Split(first, second) => {
                    groups.push_front(second);
                    groups.push_front(first);
                }
            }
        }
        Ok(changed)
    }
}
