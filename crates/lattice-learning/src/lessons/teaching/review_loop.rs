//! The review of generated material, one loop over a subject: fit, review,
//! decide, repair, confine, save.
use super::repair;
use super::subject::{Decision, Pass, Settled, Step, Subject};
use super::{json_or_text, ReviewSchema};
use crate::outline_progress::OutlineStage;
use crate::review_findings::SetAside;
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::json;

/// The reviewed material, and what the repairs could not settle: a lesson
/// publishes with these waiting for the user.
pub(super) struct Reviewed {
    pub candidate: String,
    pub set_aside: Vec<SetAside>,
}

/// A second model call critiques the actual candidate, and one targeted
/// repair is allowed; what the subject does with the rest is its own (a
/// lesson repairs while each pass fixes something and publishes with what is
/// left waiting; other material fails closed).
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
    subject.save(&candidate).await?;
    let mut authoring = json_or_text(prompt);
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
            candidate = serde_json::to_string(&value)?;
            subject.save(&candidate).await?;
        }
        subject.select_sources(llm, &mut authoring, &value).await?;
        let quote_checks = subject.quote_checks(&value, &authoring, attempt)?;
        // review
        let pass = Pass {
            attempt,
            candidate: &candidate,
            value: &value,
            authoring: &authoring,
            quote_checks: &quote_checks,
            schema,
        };
        let found = subject.find(llm, &pass).await?;
        let edits = found.edits;
        // decide
        let send = match subject.decide(attempt, &value, found.findings)? {
            Decision::Publish(set_aside) => {
                return Ok(Reviewed {
                    candidate,
                    set_aside,
                })
            }
            Decision::Repair(send) => send,
        };
        // repair
        if let Some(progress) = subject.progress() {
            progress.stage(OutlineStage::Repairing);
        }
        subject.step(Step::Repairing);
        let request = repair::Request {
            system,
            schema: schema.value(),
            authoring: &authoring,
            candidate: &value,
            quote_checks: &quote_checks,
            issues: json!(send
                .iter()
                .map(|finding| finding.note.as_str())
                .collect::<Vec<_>>()),
            edits,
            wording: subject.wording(),
            output_tokens,
        };
        let result = repair::ask(llm, &request, subject.progress()).await;
        // confine → save
        match subject.settle_repair(&value, &send, result)? {
            Settled::Changed(text) => {
                candidate = text;
                subject.save(&candidate).await?;
            }
            Settled::Unchanged => {}
            Settled::Publish(set_aside) => {
                return Ok(Reviewed {
                    candidate,
                    set_aside,
                })
            }
        }
    }
    Err(AppError::InternalError(
        "Teaching review did not complete".into(),
    ))
}
