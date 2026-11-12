//! Learning's structured model calls: one schema-constrained request, or, for
//! authoring work, a prose draft followed by a schema-constrained answer built
//! from it, since asking for a format while thinking costs small models their
//! structured-output accuracy. The answer step continues the draft's
//! conversation, so it reuses the prompt the server just read.

use lattice_core::shared::hash::sha256_hex;
use serde_json::{json, Value};

use crate::model_call::ErrorClass;
use crate::outline_progress::OutlineProgress;
use crate::{lesson_drafts, lesson_progress, model_call};
use lattice_core::application::ports::llm_port::{CompletionResponse, SamplingOverride};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::{
    self, GroundedOutput, GroundedRequest, HistoryItem, Streaming,
};
use lattice_core::application::services::model_contract::{
    answer, ContractCall, Transport, CORRECTION_LEAD,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::protocol::Protocol;

pub(crate) use lattice_core::application::services::model_contract::refusal;

/// Room kept beside the draft for the answer step's own request and the
/// framing of the two extra messages.
const ANSWER_STEP_OVERHEAD_TOKENS: usize = 256;

/// Names a saved draft's checkpoint; a change to the drafting protocol must
/// not resume a draft written under the old one.
const DRAFT_CHECKPOINT_POLICY: Protocol = lattice_core::protocol!("reason-first-draft", 1);

/// Opens the paragraph a two-step call adds to its system message. The
/// paragraph stays in the answer step too, so both requests share their
/// opening and the answer step reads as the promised second step.
const TWO_STEPS: &str = "Answer in two steps. Step 1, now: work through the task in prose.";

/// Closes that paragraph, after the call's own brief.
const DRAFT_CLOSE: &str = "Write out everything the final answer needs, in full. Step 2 is requested separately and turns this draft into the final answer.";

/// The answer step's request. It names no format: the schema travels with the
/// request and the server holds the answer to it.
const ANSWER_STEP: &str = "Step 2: give the final answer from your draft. Keep its content, wording, numbers, citations and answers. Copy quotations and identifiers exactly as the task supplies them. Where the draft left something out or stopped early, complete it from the task.";

/// Closes a task sent again without the answer it refused, for a window too
/// full to carry that answer. Like [`ANSWER_STEP`] it names no format.
const RESTATE: &str = "An earlier answer to this task could not be used, so avoid this:";

/// What a learning call's window refusal says does not fit.
pub(crate) const MATERIAL_DOES_NOT_FIT: &str = "The requested learning material does not fit";

/// Which knob in [`model_call`] decides whether a call reasons first.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CallKind {
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
pub(crate) struct ReasonFirst<'a> {
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
    call: &ContractCall,
    system: &str,
    prompt: String,
    schema: Value,
    output_tokens: usize,
) -> Result<String> {
    complete_json_with_progress(llm, call, system, prompt, schema, output_tokens, None).await
}

pub(crate) async fn complete_json_with_progress(
    llm: &dyn LLMPort,
    call: &ContractCall,
    system: &str,
    prompt: String,
    schema: Value,
    output_tokens: usize,
    progress: Option<&OutlineProgress>,
) -> Result<String> {
    complete_checked(
        llm,
        call,
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
pub(crate) async fn complete_reasoned_json(
    llm: &dyn LLMPort,
    call: &ContractCall,
    system: &str,
    prompt: String,
    schema: Value,
    output_tokens: usize,
    progress: Option<&OutlineProgress>,
    reasoning: ReasonFirst<'_>,
) -> Result<String> {
    complete_checked(
        llm,
        call,
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
pub(crate) async fn complete_checked<T>(
    llm: &dyn LLMPort,
    call: &ContractCall,
    system: &str,
    prompt: String,
    schema: Value,
    output_tokens: usize,
    progress: Option<&OutlineProgress>,
    reasoning: Option<ReasonFirst<'_>>,
    accept: impl Fn(&str) -> Result<T> + Send + Sync,
) -> Result<T> {
    complete_checked_request(
        llm,
        call,
        model_call::structured(system, prompt, schema, output_tokens),
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
pub(crate) async fn complete_checked_request<T>(
    llm: &dyn LLMPort,
    call: &ContractCall,
    request: GroundedRequest,
    progress: Option<&OutlineProgress>,
    reasoning: Option<ReasonFirst<'_>>,
    accept: impl Fn(&str) -> Result<T> + Send + Sync,
) -> Result<T> {
    let output_tokens = request.output_tokens;
    let one_step = || async {
        let direct = Learning {
            llm,
            call,
            progress,
            step: Step::Direct,
        };
        answer(llm, call, &direct, request.clone(), &accept).await
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
    let draft_step = || {
        let mut draft = request.clone();
        draft.instructions = two_step_system.clone();
        draft.call.json_schema = None;
        draft
    };
    // A draft cut to fit a small window helps less than it costs, so such a
    // window gets the one-step call instead.
    let room = model_call::source_room(llm, &draft_step(), usize::MAX);
    if room < output_tokens + ANSWER_STEP_OVERHEAD_TOKENS {
        tracing::info!(
            subject,
            room,
            output_tokens,
            "Window cannot hold a draft beside the answer; answering in one step"
        );
        return one_step().await;
    }

    let key = DRAFT_CHECKPOINT_POLICY.key(sha256_hex(
        json!({"model":llm.model_name(),"system":two_step_system,"prompt":prompt}).to_string(),
    ));
    let saved = lesson_drafts::checkpoint(&key).await?.and_then(|value| {
        value
            .get("draft")
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let draft_reused = saved.is_some();
    let draft = match saved {
        Some(draft) => {
            lesson_progress::stage(format!(
                "Reusing the saved draft of {subject} (step 1 of 2)"
            ));
            draft
        }
        None => {
            lesson_progress::stage(format!("Working through {subject} in prose (step 1 of 2)"));
            let draft_call = call.draft();
            let writing = Learning {
                llm,
                call: &draft_call,
                progress,
                step: Step::Draft,
            };
            let drafted = answer(llm, &draft_call, &writing, draft_step(), |text| {
                Ok(text.to_owned())
            })
            .await;
            // A declined or cut-off draft is no draft: the one-step request
            // asks for no prose of its own and may still be answered.
            match drafted {
                Ok(text) if !text.trim().is_empty() => {
                    lesson_drafts::record_checkpoint(&key, json!({ "draft": text })).await?;
                    text
                }
                Ok(_) => {
                    tracing::warn!(
                        subject,
                        "Draft step returned no text; answering in one step"
                    );
                    return one_step().await;
                }
                Err(error) if ErrorClass::of(&error) == ErrorClass::Unusable => {
                    tracing::warn!(subject, %error, "Draft step was unusable; answering in one step");
                    return one_step().await;
                }
                Err(error) => return Err(error),
            }
        }
    };

    let draft_tokens = llm.count_tokens(&draft);
    let answer_step = || {
        let mut request = request.clone();
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
    if !model_call::fits(llm, &answer_step()).await {
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
    let answering = Learning {
        llm,
        call,
        progress,
        step: Step::Answer,
    };
    let value = match answer(llm, call, &answering, answer_step(), &accept).await {
        Ok(value) => value,
        Err(error) => {
            // The answer step is deterministic, so a retry on this draft fails the same way.
            if ErrorClass::of(&error) == ErrorClass::Unusable {
                lesson_drafts::record_checkpoint(&key, Value::Null).await?;
            }
            return Err(error);
        }
    };
    tracing::info!(
        subject,
        draft_reused,
        draft_tokens,
        "Learning two-step call completed"
    );
    Ok(value)
}

/// The learning side of one model call: sends a grounded request, streams its
/// text to the running progress, and builds the request that asks again.
struct Learning<'a> {
    llm: &'a dyn LLMPort,
    call: &'a ContractCall,
    progress: Option<&'a OutlineProgress>,
    step: Step,
}

impl Learning<'_> {
    /// Whether the model's window holds `request`, by the planner's estimate.
    fn holds(&self, request: &GroundedRequest) -> bool {
        grounded_generation::estimate(self.llm, request).is_ok_and(|measured| measured.fits())
    }

    /// The refusal for a correction `restated` that the window cannot hold: for
    /// a packer's call, with the size the planner estimated; otherwise the
    /// answer's own refusal.
    fn too_large(&self, restated: &GroundedRequest, reason: &str) -> AppError {
        let size = match grounded_generation::estimate(self.llm, restated) {
            Ok(measured) => Some(measured.prompt_tokens),
            Err(error) => error.overflow().map(|overflow| overflow.required),
        };
        let window = self.llm.max_context_tokens();
        tracing::warn!(
            step = self.step.name(),
            reason,
            prompt_tokens = size,
            window,
            "A refused answer cannot be asked for again within the window"
        );
        let Some(prompt_tokens) = size.filter(|_| model_call::sent_by_packer()) else {
            return AppError::ModelAnswerUnusable(reason.to_owned());
        };
        AppError::PromptExceedsWindow {
            prompt_tokens,
            window,
            counted_exactly: false,
            message: format!(
                "The model's answer could not be used: {reason} Asking again with that reason needs a prompt of {prompt_tokens} tokens (estimated), which does not fit the configured context window of {window}."
            ),
        }
    }
}

#[async_trait::async_trait]
impl Transport for Learning<'_> {
    type Request = GroundedRequest;

    async fn send(
        &self,
        llm: &dyn LLMPort,
        request: &GroundedRequest,
    ) -> Result<CompletionResponse> {
        let output = send(llm, self.call, request.clone(), self.progress, self.step).await?;
        Ok(CompletionResponse {
            text: output.text,
            input_tokens: output.input_tokens,
            output_tokens: output.output_tokens,
            finish_reason: output.finish_reason,
            ..Default::default()
        })
    }

    /// The refused answer continues the request, so the server reads the
    /// opening it has just read; a task that nearly fills the window is sent
    /// again with the reason instead.
    fn correction(
        &self,
        request: &GroundedRequest,
        previous: &CompletionResponse,
        refusal: &str,
    ) -> Result<GroundedRequest> {
        let continued = reask(request.clone(), &previous.text, refusal);
        let restated = restate(request.clone(), refusal);
        let again = if self.holds(&continued) {
            continued
        } else if self.holds(&restated) {
            restated
        } else {
            return Err(self.too_large(&restated, refusal));
        };
        tracing::warn!(
            step = self.step.name(),
            refusal,
            "The model's answer could not be used; asking once more"
        );
        lesson_progress::stage("The model's answer could not be used; asking once more.");
        Ok(again)
    }

    /// An answer cut off at its output reservation is asked for once more with
    /// the reservation doubled, unless it already had the whole window or the
    /// doubled request does not fit.
    fn larger(&self, request: &GroundedRequest) -> Option<GroundedRequest> {
        if fills_window(self.progress, self.step) {
            return None;
        }
        let mut larger = request.clone();
        larger.output_tokens = request.output_tokens.saturating_mul(2);
        if !self.holds(&larger) {
            return None;
        }
        tracing::warn!(
            step = self.step.name(),
            output_tokens = request.output_tokens,
            "An answer reached its output limit; asking once more with twice the room"
        );
        Some(larger)
    }
}

/// `request` continued with the answer it drew and a request to answer again.
/// Learning requests carry their whole user message in `task`, so moving it
/// into the history keeps every message the first request sent.
pub(crate) fn reask(mut request: GroundedRequest, rejected: &str, reason: &str) -> GroundedRequest {
    let task = std::mem::replace(
        &mut request.task,
        format!("{CORRECTION_LEAD} {reason} Answer the same task again, fixing that."),
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

/// Whether a step's answer may take the whole window the prompt leaves: a
/// streamed step under a lesson job or an outline run, other than the draft.
fn fills_window(progress: Option<&OutlineProgress>, step: Step) -> bool {
    (progress.is_some() || lesson_progress::active()) && !matches!(step, Step::Draft)
}

/// Send one learning request, streaming its text to whichever progress the
/// call runs under.
async fn send(
    llm: &dyn LLMPort,
    call: &ContractCall,
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
    let _model_call = lesson_progress.then(lesson_progress::request);
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
        call,
        request,
        unbounded.then_some(&streaming),
        model_call::TooLarge::Window(MATERIAL_DOES_NOT_FIT),
    )
    .await?;
    tracing::info!(
        stage = ?progress.map(|value| value.snapshot().stage),
        lesson = ?lesson_drafts::lesson_id(),
        work = ?lesson_progress::current_stage(),
        step = step.name(),
        input_tokens = response.input_tokens,
        output_tokens = response.output_tokens,
        finish_reason = %response.finish_reason,
        "Learning model request completed"
    );
    Ok(response)
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
        .find(|content| *content != ANSWER_STEP && !content.starts_with(CORRECTION_LEAD));
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
