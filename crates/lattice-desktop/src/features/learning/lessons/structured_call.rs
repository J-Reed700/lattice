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

use serde_json::{json, Value};

use crate::features::learning::outline_progress::OutlineProgress;
use crate::features::learning::{lesson_drafts, lesson_progress, model_call};
use lattice_core::application::ports::llm_port::SamplingOverride;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::{
    GroundedOutput, GroundedRequest, HistoryItem, Streaming,
};
use lattice_core::shared::error::{AppError, Result};

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

/// Opens the request that asks again for an answer the caller could not use.
/// Like [`ANSWER_STEP`] it names no format.
const REASK: &str = "That answer could not be used:";

/// Closes a task sent again without the answer it refused, for a window too
/// full to carry that answer. Like [`ANSWER_STEP`] it names no format.
const RESTATE: &str = "An earlier answer to this task could not be used, so avoid this:";

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
#[derive(Clone, Copy)]
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
    complete_checked(
        llm,
        system,
        prompt,
        schema,
        output_tokens,
        progress,
        None,
        |text| Ok(text.to_owned()),
    )
    .await
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
    complete_checked(
        llm,
        system,
        prompt,
        schema,
        output_tokens,
        progress,
        Some(reasoning),
        |text| Ok(text.to_owned()),
    )
    .await
}

/// Make a learning call, in one step or, with `reasoning`, in two as
/// [`complete_reasoned_json`] describes, and turn its answer into data with
/// `accept`. An answer `accept` refuses (see [`refusal`]) is asked for once
/// more in the same conversation, saying what was wrong; only the answer step
/// is asked again, so a two-step call keeps its draft. A second refusal fails
/// the call with [`AppError::ModelAnswerUnusable`] and the second reason.
pub(in crate::features::learning) async fn complete_checked<T>(
    llm: &dyn LLMPort,
    system: &str,
    prompt: String,
    schema: Value,
    output_tokens: usize,
    progress: Option<&OutlineProgress>,
    reasoning: Option<ReasonFirst<'_>>,
    accept: impl Fn(&str) -> Result<T>,
) -> Result<T> {
    complete_checked_request(
        llm,
        model_call::structured(system, prompt, schema, output_tokens, "low"),
        progress,
        reasoning,
        accept,
    )
    .await
}

/// [`complete_checked`] for a request built whole, as a site that packs its
/// own material builds it at the room [`model_call::send_fitted`] settled.
/// The answer's reservation is the request's `output_tokens`.
///
/// The two-step form continues the request's user message, which a learning
/// request carries in its task. One that also carries evidence passages,
/// which the planner selects, is made in one step.
pub(in crate::features::learning) async fn complete_checked_request<T>(
    llm: &dyn LLMPort,
    request: GroundedRequest,
    progress: Option<&OutlineProgress>,
    reasoning: Option<ReasonFirst<'_>>,
    accept: impl Fn(&str) -> Result<T>,
) -> Result<T> {
    let output_tokens = request.output_tokens;
    let at = |output_tokens: usize| {
        let mut request = request.clone();
        request.output_tokens = output_tokens;
        request
    };
    let one_step = || {
        let (at, accept) = (&at, &accept);
        async move {
            answer(llm, at, output_tokens, progress, Step::Direct, accept)
                .await
                .map(|(value, _)| value)
        }
    };
    // A model that thinks before every answer reasons first already, and
    // Claude's reasoning-extraction classifier refuses the prose draft as a
    // request for its chain of thought: every draft came back empty, each
    // billed for its whole prompt.
    let Some(reasoning) = reasoning
        .filter(|reasoning| reasoning.kind.reasons_first())
        .filter(|_| request.evidence.is_empty())
        .filter(|_| !llm.thinks_before_answering())
    else {
        return one_step().await;
    };
    let subject = reasoning.subject;
    let two_step_system = format!(
        "{}\n\n{TWO_STEPS} {} {DRAFT_CLOSE}",
        request.instructions, reasoning.brief
    );
    // The user message the draft reads, as the request renders it.
    let prompt = [request.task.as_str(), request.closing.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let draft_step = |output_tokens: usize| {
        let mut draft = at(output_tokens);
        draft.instructions = two_step_system.clone();
        draft.call.json_schema = None;
        draft
    };
    // A draft cut to fit a small window helps less than it costs, so such a
    // window gets the one-step call instead.
    let room = model_call::source_room(llm, &draft_step(output_tokens), usize::MAX);
    if room < output_tokens + ANSWER_STEP_OVERHEAD_TOKENS {
        tracing::info!(
            subject,
            room,
            output_tokens,
            "Window cannot hold a draft beside the answer; answering in one step"
        );
        return one_step().await;
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
            let output = send(llm, draft_step(output_tokens), progress, Step::Draft).await?;
            // A draft that ran out of room is still worth answering from: the
            // answer step has the whole task and completes what is missing.
            let truncated = cut_off(&output.finish_reason);
            // A declined draft is no draft: the one-step request asks for no
            // prose of its own and may still be answered.
            let declined = output.finish_reason.eq_ignore_ascii_case("refusal");
            if !truncated && !declined {
                reject_incomplete_finish_reason(&output.finish_reason)?;
            }
            if declined || output.text.trim().is_empty() {
                tracing::warn!(
                    subject,
                    finish_reason = %output.finish_reason,
                    "Draft step returned no text; answering in one step"
                );
                return one_step().await;
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
    let answer_step = |output_tokens: usize| {
        let mut request = at(output_tokens);
        request.instructions = two_step_system.clone();
        request.task = ANSWER_STEP.to_owned();
        request.closing.clear();
        request.history.extend([
            HistoryItem {
                role: "user".into(),
                content: prompt.clone(),
            },
            HistoryItem {
                role: "assistant".into(),
                content: draft.clone(),
            },
        ]);
        // The answer transcribes a draft; sampling it at the chat temperature
        // would only add variation the draft does not contain.
        request.call.sampling = Some(SamplingOverride::deterministic());
        request
    };
    if !model_call::fits(llm, &answer_step(output_tokens)).await {
        // The draft was capped in the model's own tokens, but it can still
        // measure over beside the task, by the backend's count or, where it
        // cannot count, by an estimate that can come out larger. The one-step
        // call needs no room for a draft.
        tracing::warn!(
            subject,
            draft_tokens,
            "Draft does not fit beside the answer; answering in one step"
        );
        return one_step().await;
    }
    lesson_progress::stage(format!(
        "Structuring {subject} from a {draft_tokens}-token draft (step 2 of 2)"
    ));
    let (value, output) = match answer(
        llm,
        answer_step,
        output_tokens,
        progress,
        Step::Answer,
        &accept,
    )
    .await
    {
        Ok(answered) => answered,
        Err(error) => {
            // The answer step is deterministic, so a retry on this draft fails the same way.
            if matches!(
                error,
                AppError::ModelAnswerUnusable(_)
                    | AppError::InvalidInput(_)
                    | AppError::InvalidData(_)
                    | AppError::ValidationFailed(_)
                    | AppError::Json { .. }
            ) {
                lesson_drafts::record_checkpoint(&key, Value::Null).await?;
            }
            return Err(error);
        }
    };
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
    Ok(value)
}

/// Send the answer step `request` builds and check what comes back with
/// `accept`. A refused answer is asked for once more: the request continues
/// with the refused answer and the reason, so the server reads the opening it
/// has just read. When the window cannot also hold the refused answer, the
/// task is sent again with the reason added instead. Returns the accepted
/// value and the response it came from.
async fn answer<T>(
    llm: &dyn LLMPort,
    request: impl Fn(usize) -> GroundedRequest,
    output_tokens: usize,
    progress: Option<&OutlineProgress>,
    step: Step,
    accept: &impl Fn(&str) -> Result<T>,
) -> Result<(T, GroundedOutput)> {
    let output = send_completing(llm, &request, output_tokens, progress, step).await?;
    reject_incomplete_finish_reason(&output.finish_reason)?;
    let error = match accept(&output.text) {
        Ok(value) => return Ok((value, output)),
        Err(error) => error,
    };
    let Some(reason) = refusal(&error) else {
        return Err(error);
    };
    // A task that nearly fills the window has no room left for the refused
    // answer, so it is sent again with the reason instead of continued.
    let continues =
        model_call::fits(llm, &reask(request(output_tokens), &output.text, reason)).await;
    let again = |output_tokens| {
        let request = request(output_tokens);
        if continues {
            reask(request, &output.text, reason)
        } else {
            restate(request, reason)
        }
    };
    if !continues && !model_call::fits(llm, &again(output_tokens)).await {
        tracing::warn!(
            step = step.name(),
            reason,
            "A refused answer cannot be asked for again within the window"
        );
        return Err(AppError::ModelAnswerUnusable(reason.to_owned()));
    }
    tracing::warn!(
        step = step.name(),
        reason,
        "The model's answer could not be used; asking once more"
    );
    lesson_progress::stage(format!(
        "The model's answer could not be used; asking once more. {reason}"
    ));
    let second = send_completing(llm, again, output_tokens, progress, step).await?;
    reject_incomplete_finish_reason(&second.finish_reason)?;
    match accept(&second.text) {
        Ok(value) => Ok((value, second)),
        Err(error) => Err(refusal(&error)
            .map(|reason| AppError::ModelAnswerUnusable(reason.to_owned()))
            .unwrap_or(error)),
    }
}

/// What an `accept` said was wrong with an answer, for the errors that refuse
/// the answer itself: it did not parse, or it broke a rule the caller checks.
/// Anything else (transport, configuration, cancellation, the window, a
/// budget) says nothing about the answer and is never asked again.
fn refusal(error: &AppError) -> Option<&str> {
    match error {
        AppError::InvalidInput(reason)
        | AppError::InvalidData(reason)
        | AppError::ValidationFailed(reason)
        | AppError::Json {
            message: reason, ..
        } => Some(reason.as_str()),
        _ => None,
    }
}

/// `request` continued with the answer it drew and a request to answer again.
/// Learning requests carry their whole user message in `task`, so moving it
/// into the history keeps every message the first request sent.
fn reask(mut request: GroundedRequest, rejected: &str, reason: &str) -> GroundedRequest {
    let task = std::mem::replace(
        &mut request.task,
        format!("{REASK} {reason} Answer the same task again, fixing that."),
    );
    request.history.extend([
        HistoryItem {
            role: "user".into(),
            content: task,
        },
        HistoryItem {
            role: "assistant".into(),
            content: rejected.to_owned(),
        },
    ]);
    request
}

/// `request` sent again with `reason` added to its task, for a window too
/// full to carry the refused answer as well.
fn restate(mut request: GroundedRequest, reason: &str) -> GroundedRequest {
    request.task = format!("{}\n\n{RESTATE} {reason}", request.task);
    request
}

/// Send the request `request` builds for `output_tokens`. An answer cut off
/// at its output reservation is asked for once more with the reservation
/// doubled; a doubled request the window cannot hold keeps the first answer.
/// So does an answer that already had the whole window: doubling its
/// reservation gives it no more room, and decoding is deterministic, so the
/// same answer would be cut off at the same place after the same wait.
async fn send_completing(
    llm: &dyn LLMPort,
    request: impl Fn(usize) -> GroundedRequest,
    output_tokens: usize,
    progress: Option<&OutlineProgress>,
    step: Step,
) -> Result<GroundedOutput> {
    let output = send(llm, request(output_tokens), progress, step).await?;
    if !cut_off(&output.finish_reason) || fills_window(progress, step) {
        return Ok(output);
    }
    let larger = request(output_tokens.saturating_mul(2));
    if !model_call::fits(llm, &larger).await {
        return Ok(output);
    }
    tracing::warn!(
        step = step.name(),
        output_tokens,
        "An answer reached its output limit; asking once more with twice the room"
    );
    send(llm, larger, progress, step).await
}

/// Whether a step's answer may take the whole window the prompt leaves: a
/// streamed step under a lesson job or an outline run, other than the draft.
fn fills_window(progress: Option<&OutlineProgress>, step: Step) -> bool {
    (progress.is_some() || lesson_progress::active()) && !matches!(step, Step::Draft)
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
    request.output_fills_window = fills_window(progress, step);
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
    // A slow model is not a failed model: no learning call has an
    // elapsed-time deadline (see `model_call`).
    let response = model_call::send(
        llm,
        request,
        unbounded.then_some(&streaming),
        model_call::TooLarge::Window("The requested learning material does not fit"),
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
pub(in crate::features::learning) fn cut_off(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    ["length", "max_tokens", "max_output_tokens", "incomplete"]
        .iter()
        .any(|needle| reason.contains(needle))
}

pub(in crate::features::learning) fn reject_incomplete_finish_reason(reason: &str) -> Result<()> {
    if cut_off(reason) || reason.to_ascii_lowercase().contains("content_filter") {
        // An unusable answer, not bad input: a lesson's passes and the job's
        // recovery treat it as a refusal they can work around, where an
        // invalid input failed the whole job.
        return Err(AppError::ModelAnswerUnusable("The model response was cut off before the learning material was complete. Try again with a smaller outline or lesson.".into()));
    }
    // A safety classifier's decline comes back as a finished reply with no
    // text, which would otherwise be parsed and fail as malformed.
    if reason.eq_ignore_ascii_case("refusal") {
        return Err(AppError::ModelAnswerUnusable(
            "The model declined to write this learning material. Try again, or choose another \
             model in Settings → AI."
                .into(),
        ));
    }
    Ok(())
}

/// What the test fixtures answer a draft step with.
#[cfg(test)]
pub(crate) const FIXTURE_DRAFT: &str = "Fixture draft: the task worked through in prose.";

/// Whether `request` is the prose step of a two-step call.
#[cfg(test)]
pub(crate) fn is_draft_request(
    request: &lattice_core::application::ports::llm_port::CompletionRequest,
) -> bool {
    request.json_schema.is_none()
        && messages(request, "system").any(|content| content.contains(TWO_STEPS))
}

/// The system and user text of the task behind `request`: for either step of
/// a two-step call, or a request asking again, what the one-step call would
/// have sent. Fixtures that
/// answer a task by its prompt then answer both forms alike.
#[cfg(test)]
pub(crate) fn task_messages(
    request: &lattice_core::application::ports::llm_port::CompletionRequest,
) -> (&str, &str) {
    let system = messages(request, "system").next().unwrap_or("");
    let system = system
        .split_once(&format!("\n\n{TWO_STEPS}"))
        .map_or(system, |(task, _)| task);
    let user = messages(request, "user")
        .find(|content| *content != ANSWER_STEP && !content.starts_with(REASK));
    (system, user.unwrap_or(""))
}

/// Contents of `role`'s messages, last first.
#[cfg(test)]
fn messages<'a>(
    request: &'a lattice_core::application::ports::llm_port::CompletionRequest,
    role: &'a str,
) -> impl Iterator<Item = &'a str> + 'a {
    request
        .input
        .iter()
        .rev()
        .filter_map(move |item| match item {
            lattice_core::application::ports::llm_port::CompletionInput::Message {
                role: candidate,
                content,
            } if candidate == role => Some(content.as_str()),
            _ => None,
        })
}

#[cfg(test)]
#[path = "structured_call_tests.rs"]
mod tests;
