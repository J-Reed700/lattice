//! Learning's structured model calls as the generators make them: one
//! schema-constrained request, or, for authoring work, a prose draft followed
//! by a schema-constrained answer built from it.
//!
//! The two-step form exists because asking for a format in the same request
//! that does the thinking is what costs small open models most of their
//! structured-output accuracy; reasoning first and formatting second recovers
//! most of it. The grammar itself is not the cost at 4B and above, so the
//! second step keeps it. That step continues the first as a conversation (the
//! task, the model's own draft, then a request for the final answer), so it
//! opens with exactly the prompt the server has just read and can reuse.

use std::time::Duration;

use serde_json::{json, Value};

use crate::application::ports::llm_port::SamplingOverride;
use crate::application::ports::LLMPort;
use crate::application::services::grounded_generation::{
    GroundedOutput, GroundedRequest, HistoryItem, Streaming,
};
use crate::features::learning::outline_progress::OutlineProgress;
use crate::features::learning::{lesson_drafts, lesson_progress, model_call};
use crate::shared::error::{AppError, Result};

/// The wall-clock limit on one material call that has no interactive progress
/// attached. Each step of a two-step call has its own: the draft and the
/// answer are separate generations, and either alone may need all of it.
const MATERIAL_CALL_BUDGET: Duration = Duration::from_secs(300);

/// Room kept beside the draft for the answer step's own request and the
/// framing of the two extra messages.
const ANSWER_STEP_OVERHEAD_TOKENS: usize = 256;

/// Names a saved draft's checkpoint; a change to the drafting protocol must
/// not resume a draft written under the old one.
const DRAFT_CHECKPOINT_POLICY: &str = "reason-first-draft-v1";

/// Opens the paragraph a two-step call adds to its system message. The
/// paragraph stays in the answer step too, so both requests share their
/// opening and the answer step reads as the promised second step.
const TWO_STEPS: &str = "Answer in two steps. Step 1, now: work through the task in prose.";

/// Closes that paragraph, after the call's own brief.
const DRAFT_CLOSE: &str = "Write out everything the final answer needs, in full. Step 2 is requested separately and turns this draft into the final answer.";

/// The answer step's request. It names no format: the schema travels with the
/// request and the server holds the answer to it.
const ANSWER_STEP: &str = "Step 2: give the final answer from your draft. Keep its content, wording, numbers, citations and answers. Copy quotations and identifiers exactly as the task supplies them. Where the draft left something out or stopped early, complete it from the task.";

/// Which knob in [`model_call`] decides whether a call reasons first.
#[derive(Debug, Clone, Copy)]
pub(in crate::features::learning) enum CallKind {
    /// Outlines, lessons and the reviews that judge them.
    Authoring,
    /// Checks that choose among fixed verdicts.
    Classifier,
}

impl CallKind {
    fn reasons_first(self) -> bool {
        match self {
            Self::Authoring => model_call::REASON_FIRST_AUTHORING,
            Self::Classifier => model_call::REASON_FIRST_CLASSIFIERS,
        }
    }
}

/// How a two-step call describes its work.
pub(in crate::features::learning) struct ReasonFirst<'a> {
    pub kind: CallKind,
    /// What the call produces, for progress text and logs: "the lesson".
    pub subject: &'a str,
    /// What the prose step works through, in whole sentences. It names no
    /// output format.
    pub brief: &'a str,
}

#[derive(Debug, Clone, Copy)]
enum Step {
    /// The only request of a one-step call.
    Direct,
    /// The prose step of a two-step call.
    Draft,
    /// The schema-constrained step that answers from the draft.
    Answer,
}

impl Step {
    fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Draft => "draft",
            Self::Answer => "answer",
        }
    }
}

pub(crate) async fn complete_json(
    llm: &dyn LLMPort,
    system: &str,
    prompt: String,
    schema: Value,
    output_tokens: usize,
) -> Result<String> {
    complete_json_with_progress(llm, system, prompt, schema, output_tokens, None).await
}

pub(crate) async fn complete_json_with_progress(
    llm: &dyn LLMPort,
    system: &str,
    prompt: String,
    schema: Value,
    output_tokens: usize,
    progress: Option<&OutlineProgress>,
) -> Result<String> {
    let request = model_call::structured(system, prompt, schema, output_tokens, "low");
    let response = send(llm, request, progress, Step::Direct).await?;
    reject_incomplete_finish_reason(&response.finish_reason)?;
    Ok(response.text)
}

/// Work through the task in prose, then answer it under `schema` from that
/// draft. A call whose knob is off, or whose window cannot hold the prompt, a
/// draft as long as the answer and the answer itself, is made in one step.
///
/// Inside a lesson job the draft is checkpointed, so a job interrupted
/// between the steps resumes at the answer.
pub(in crate::features::learning) async fn complete_reasoned_json(
    llm: &dyn LLMPort,
    system: &str,
    prompt: String,
    schema: Value,
    output_tokens: usize,
    progress: Option<&OutlineProgress>,
    reasoning: ReasonFirst<'_>,
) -> Result<String> {
    if !reasoning.kind.reasons_first() {
        return complete_json_with_progress(llm, system, prompt, schema, output_tokens, progress)
            .await;
    }
    let subject = reasoning.subject;
    let two_step_system = format!("{system}\n\n{TWO_STEPS} {} {DRAFT_CLOSE}", reasoning.brief);
    // A draft cut to fit a small window helps less than it costs, so such a
    // window gets the one-step call instead.
    let room = model_call::source_room(
        llm,
        &model_call::prose(&two_step_system, prompt.clone(), output_tokens, "low"),
        usize::MAX,
    );
    if room < output_tokens + ANSWER_STEP_OVERHEAD_TOKENS {
        tracing::info!(
            subject,
            room,
            output_tokens,
            "Window cannot hold a draft beside the answer; answering in one step"
        );
        return complete_json_with_progress(llm, system, prompt, schema, output_tokens, progress)
            .await;
    }

    let key = format!(
        "{DRAFT_CHECKPOINT_POLICY}:{}",
        crate::features::learning::content_verification::digest(
            &json!({"model":llm.model_name(),"system":two_step_system,"prompt":prompt}).to_string()
        )
    );
    let saved = lesson_drafts::checkpoint(&key).await?.and_then(|value| {
        value
            .get("draft")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let draft_reused = saved.is_some();
    let (draft, draft_input_tokens, draft_output_tokens, draft_truncated) = match saved {
        Some(draft) => {
            lesson_progress::stage(format!(
                "Reusing the saved draft of {subject} (step 1 of 2)"
            ));
            (draft, 0, 0, false)
        }
        None => {
            lesson_progress::stage(format!("Working through {subject} in prose (step 1 of 2)"));
            let output = send(
                llm,
                model_call::prose(&two_step_system, prompt.clone(), output_tokens, "low"),
                progress,
                Step::Draft,
            )
            .await?;
            // A draft that ran out of room is still worth answering from: the
            // answer step has the whole task and completes what is missing.
            let truncated = cut_off(&output.finish_reason);
            if !truncated {
                reject_incomplete_finish_reason(&output.finish_reason)?;
            }
            if output.text.trim().is_empty() {
                tracing::warn!(
                    subject,
                    "Draft step returned no text; answering in one step"
                );
                return complete_json_with_progress(
                    llm,
                    system,
                    prompt,
                    schema,
                    output_tokens,
                    progress,
                )
                .await;
            }
            lesson_drafts::record_checkpoint(&key, json!({ "draft": output.text })).await?;
            (
                output.text,
                output.input_tokens,
                output.output_tokens,
                truncated,
            )
        }
    };

    let draft_tokens = llm.count_tokens(&draft);
    let mut request = model_call::structured(
        &two_step_system,
        ANSWER_STEP.to_owned(),
        schema.clone(),
        output_tokens,
        "low",
    );
    request.history = vec![
        HistoryItem {
            role: "user".into(),
            content: prompt.clone(),
        },
        HistoryItem {
            role: "assistant".into(),
            content: draft,
        },
    ];
    // The answer transcribes a draft; sampling it at the chat temperature
    // would only add variation the draft does not contain.
    request.call.sampling = Some(SamplingOverride::deterministic());
    if !model_call::fits(llm, &request) {
        // The draft was capped in the model's own tokens, but the planner
        // counts it with an estimate that can come out larger. The one-step
        // call needs no room for a draft.
        tracing::warn!(
            subject,
            draft_tokens,
            "Draft does not fit beside the answer; answering in one step"
        );
        return complete_json_with_progress(llm, system, prompt, schema, output_tokens, progress)
            .await;
    }
    lesson_progress::stage(format!(
        "Structuring {subject} from a {draft_tokens}-token draft (step 2 of 2)"
    ));
    let output = send(llm, request, progress, Step::Answer).await?;
    reject_incomplete_finish_reason(&output.finish_reason)?;
    tracing::info!(
        subject,
        draft_reused,
        draft_truncated,
        draft_tokens,
        draft_input_tokens,
        draft_output_tokens,
        answer_input_tokens = output.input_tokens,
        answer_output_tokens = output.output_tokens,
        total_output_tokens = draft_output_tokens + output.output_tokens,
        "Learning two-step call completed"
    );
    Ok(output.text)
}

/// Send one learning request, streaming its text to whichever progress the
/// call runs under.
async fn send(
    llm: &dyn LLMPort,
    mut request: GroundedRequest,
    progress: Option<&OutlineProgress>,
    step: Step,
) -> Result<GroundedOutput> {
    // The output estimate reserves space when selecting sources; it is not a
    // generation ceiling for streamed work. Reasoning models spend output
    // tokens before emitting JSON, so those calls let the answer take the
    // window the prompt leaves, still clamped to the user's configured output
    // limit and its provider-specific budget. A draft keeps its reservation so
    // that the answer step carrying it still fits.
    let lesson_progress = lesson_progress::active();
    let unbounded = progress.is_some() || lesson_progress;
    request.output_fills_window = unbounded && !matches!(step, Step::Draft);
    request.call.time_budget = (!unbounded).then_some(MATERIAL_CALL_BUDGET);
    request.call.no_time_limit = unbounded;
    request.call.cache_key = lesson_progress::cache_key();
    let _model_call = lesson_progress.then(lesson_progress::model_call);
    let receive = |text: String| {
        if let Some(progress) = progress {
            progress.received(&text);
        }
        if lesson_progress {
            lesson_progress::received(&text);
        }
        Ok(())
    };
    // Partial structured output is not published or parsed. The adapter can
    // discard it and retry a broken connection, while the progress counter
    // resets for the replacement response.
    let retry = |attempt: usize| {
        lesson_progress::model_retry(attempt);
        Ok(())
    };
    let streaming = Streaming {
        on_text: &receive,
        on_retry: if lesson_progress { Some(&retry) } else { None },
    };
    // Outline and lesson requests are interactive and cancellable. A slow
    // model is not a failed model; no elapsed-time deadline applies to them.
    let response = model_call::send(
        llm,
        request,
        unbounded.then_some(&streaming),
        (!unbounded).then_some(model_call::Deadline {
            after: MATERIAL_CALL_BUDGET,
            message: "Learning material generation timed out. Try again.",
        }),
        "The requested learning material does not fit in this model's context window. Choose fewer or shorter sources.",
    )
    .await?;
    tracing::info!(
        stage = ?progress.map(|value| value.snapshot().stage),
        step = step.name(),
        input_tokens = response.input_tokens,
        output_tokens = response.output_tokens,
        finish_reason = %response.finish_reason,
        "Learning model request completed"
    );
    Ok(response)
}

/// Whether the model stopped because it ran out of output room.
fn cut_off(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    ["length", "max_tokens", "max_output_tokens", "incomplete"]
        .iter()
        .any(|needle| reason.contains(needle))
}

pub(in crate::features::learning) fn reject_incomplete_finish_reason(reason: &str) -> Result<()> {
    if cut_off(reason) || reason.to_ascii_lowercase().contains("content_filter") {
        return Err(AppError::InvalidInput("The model response was cut off before the learning material was complete. Try again with a smaller outline or lesson.".into()));
    }
    Ok(())
}

/// What the test fixtures answer a draft step with.
#[cfg(test)]
pub(crate) const FIXTURE_DRAFT: &str = "Fixture draft: the task worked through in prose.";

/// Whether `request` is the prose step of a two-step call.
#[cfg(test)]
pub(crate) fn is_draft_request(
    request: &crate::application::ports::llm_port::CompletionRequest,
) -> bool {
    request.json_schema.is_none()
        && messages(request, "system").any(|content| content.contains(TWO_STEPS))
}

/// The system and user text of the task behind `request`: for either step of
/// a two-step call, what the one-step call would have sent. Fixtures that
/// answer a task by its prompt then answer both forms alike.
#[cfg(test)]
pub(crate) fn task_messages(
    request: &crate::application::ports::llm_port::CompletionRequest,
) -> (&str, &str) {
    let system = messages(request, "system").next().unwrap_or("");
    let system = system
        .split_once(&format!("\n\n{TWO_STEPS}"))
        .map_or(system, |(task, _)| task);
    let mut users = messages(request, "user");
    let user = match users.next() {
        Some(ANSWER_STEP) => users.next(),
        last => last,
    };
    (system, user.unwrap_or(""))
}

/// Contents of `role`'s messages, last first.
#[cfg(test)]
fn messages<'a>(
    request: &'a crate::application::ports::llm_port::CompletionRequest,
    role: &'a str,
) -> impl Iterator<Item = &'a str> + 'a {
    request
        .input
        .iter()
        .rev()
        .filter_map(move |item| match item {
            crate::application::ports::llm_port::CompletionInput::Message {
                role: candidate,
                content,
            } if candidate == role => Some(content.as_str()),
            _ => None,
        })
}

#[cfg(test)]
#[path = "structured_call_tests.rs"]
mod tests;
