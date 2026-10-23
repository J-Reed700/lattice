//! Learning's structured model calls.
//!
//! Every generator — lessons, outlines, assessments, practice, practical
//! activities and study decks — sends its strict-JSON
//! request through the grounded-generation service, which divides the model's
//! window and refuses a request that does not fit. A generator states what it
//! needs (the answer's reservation, its priority and schema); it never does
//! window arithmetic of its own.
//!
//! Material is fitted, not refused: a site that packs material builds its
//! request at a room, [`fit_room`] measures it exactly where the backend can
//! count and shrinks the room until it fits, and [`send_fitted`] also shrinks
//! on a size refusal the measurement could not foresee. Only instructions
//! that do not fit without any material are a window too small.
//!
//! No learning call has a wall-clock limit. A slow model is not a failed
//! one: the adapter's stall detection ends a call whose server has gone
//! quiet, the answer's output reservation ends a runaway one, and dropping
//! the future (a cancelled job, a closed window) ends one nobody wants.

use std::future::Future;

use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::{
    self, CallOptions, GroundedGenerationError, GroundedOutput, GroundedRequest, Measured,
    Streaming,
};
use lattice_core::shared::error::{AppError, Result};

/// Whether authoring calls — outline and lesson drafts and the reviews that
/// judge them — work through the task in prose before the schema-constrained
/// answer. Asking for the format in the same call that does the reasoning
/// costs small open models most of their structured-output accuracy, while
/// the grammar itself is neutral at 4B and above, so these calls reason in
/// one request and are held to the schema in a second.
pub(in crate::features::learning) const REASON_FIRST_AUTHORING: bool = true;

/// The same for short checks that choose among fixed verdicts. Their answer
/// is a label and a sentence, so a prose step would roughly double their
/// latency for little gain.
pub(in crate::features::learning) const REASON_FIRST_CLASSIFIERS: bool = false;

/// A strict-JSON request: the system instructions, a prompt the model reads
/// whole, and the schema its answer must match. `output_tokens` is the
/// answer's reservation; the planner clamps it to what the model can give.
pub(in crate::features::learning) fn structured(
    system: &str,
    prompt: String,
    schema: serde_json::Value,
    output_tokens: usize,
    reasoning_effort: &str,
) -> GroundedRequest {
    let mut request = GroundedRequest::new(system, prompt);
    request.output_tokens = output_tokens;
    request.call = CallOptions {
        json_schema: Some(schema),
        reasoning_effort: Some(reasoning_effort.to_owned()),
        ..Default::default()
    };
    request
}

/// What a request the model's window cannot hold fails with.
pub(in crate::features::learning) enum TooLarge<'a> {
    /// Input the learner can shorten: this message, as invalid input.
    Input(&'a str),
    /// Material the learner cannot trim: [`window_too_small`], saying what
    /// does not fit, with the Learning model, its window and the fix.
    Window(&'a str),
}

impl<'a> From<&'a str> for TooLarge<'a> {
    fn from(message: &'a str) -> Self {
        Self::Input(message)
    }
}

/// Send `request` with no elapsed-time limit. A request the model's window
/// cannot hold fails as `too_large` says and is never sent.
pub(in crate::features::learning) async fn send<'a>(
    llm: &dyn LLMPort,
    mut request: GroundedRequest,
    streaming: Option<&Streaming<'_>>,
    too_large: impl Into<TooLarge<'a>>,
) -> Result<GroundedOutput> {
    request.call.time_budget = None;
    request.call.no_time_limit = true;
    let result = match streaming {
        Some(streaming) => grounded_generation::generate_streaming(llm, request, streaming).await,
        None => grounded_generation::generate(llm, request).await,
    };
    result.map_err(|error| match error {
        GroundedGenerationError::Budget(_) => match too_large.into() {
            TooLarge::Input(message) => AppError::InvalidInput(message.into()),
            TooLarge::Window(what) => window_too_small(llm, what),
        },
        GroundedGenerationError::Model(error) => error,
    })
}

/// The refusal for material that does not fit the Learning model's window:
/// `what` says what does not fit, and the message names the model, its
/// window and the fix. A configuration error, because the window is a
/// setting, not something the learner typed.
pub(in crate::features::learning) fn window_too_small(llm: &dyn LLMPort, what: &str) -> AppError {
    AppError::InvalidConfig(format!(
        "{what} in {}'s {}-token context window. Raise your Learning model's context length in Settings → Models, or choose a Learning model with a larger window",
        llm.model_name(),
        llm.max_context_tokens()
    ))
}

/// Whether the model's window holds `request` as it stands: counted exactly
/// where the backend can count, by the planner's estimate where it cannot.
pub(in crate::features::learning) async fn fits(
    llm: &dyn LLMPort,
    request: &GroundedRequest,
) -> bool {
    grounded_generation::measure(llm, request)
        .await
        .is_ok_and(|measured| measured.fits())
}

/// The room the planner leaves for source material once `request` — the
/// prompt without its sources — and the answer's reservation are paid for,
/// at most `ceiling`. Zero when the request does not fit at all.
pub(in crate::features::learning) fn source_room(
    llm: &dyn LLMPort,
    request: &GroundedRequest,
    ceiling: usize,
) -> usize {
    grounded_generation::evidence_room(llm, request).map_or(0, |room| room.min(ceiling))
}

/// The room [`fit_room`] settled on and the request built at it. Room zero
/// means the request carries no material: none fits beside the skeleton, or
/// the skeleton itself does not fit.
pub(in crate::features::learning) struct Fit {
    pub room: usize,
    pub request: GroundedRequest,
    /// How `request` measured; `None` when the planner refuses even the
    /// skeleton, so nothing was measured.
    pub measured: Option<Measured>,
}

impl Fit {
    /// Whether `request` fits the window as measured.
    pub fn fits(&self) -> bool {
        self.measured.is_some_and(|measured| measured.fits())
    }
}

/// Build the request at `room`, measure it once, and while it is over the
/// window shrink the room in proportion and build again. One count when it
/// fits; two or three on material denser than the estimate. A backend that
/// cannot count is measured by the estimate, which `room` already fits.
///
/// `room` is in the planner's estimate tokens, as [`source_room`] gives it,
/// and `build` packs material up to that many; `build(0)` is the request
/// with no material, the skeleton.
pub(in crate::features::learning) async fn fit_room(
    llm: &dyn LLMPort,
    room: usize,
    build: impl FnMut(usize) -> GroundedRequest,
) -> Fit {
    settle(llm, room, build).await.0
}

/// [`fit_room`], with the skeleton's estimate the shrinking measured from.
async fn settle(
    llm: &dyn LLMPort,
    first: usize,
    mut build: impl FnMut(usize) -> GroundedRequest,
) -> (Fit, usize) {
    let skeleton = build(0);
    // The skeleton's estimate needs no round trip. Material can only be added
    // to a skeleton the estimate leaves room beside.
    let skeleton_tokens = match grounded_generation::estimate(llm, &skeleton) {
        Ok(measured) if measured.estimated < measured.allowed => measured.estimated,
        measured => {
            let fit = Fit {
                room: 0,
                request: skeleton,
                measured: measured.ok(),
            };
            log_fit(first, &fit, 0);
            return (fit, 0);
        }
    };
    let mut room = first;
    let mut counts = 0;
    loop {
        let request = build(room);
        let measured = match grounded_generation::measure(llm, &request).await {
            Ok(measured) => Some(measured),
            Err(error) => {
                tracing::debug!(%error, room, "The planner refuses the request at this room");
                None
            }
        };
        counts += usize::from(measured.is_some_and(|measured| measured.exact));
        let next = match measured {
            Some(measured) if measured.fits() => None,
            Some(measured) => Some(shrunk(room, skeleton_tokens, &measured)),
            None => Some(room / 2),
        };
        // A request whose estimate is the skeleton's carries no material.
        let empty = measured.is_some_and(|measured| measured.estimated <= skeleton_tokens);
        match next {
            Some(next) if room > 0 => room = next,
            _ => {
                let fit = Fit {
                    room: if empty { 0 } else { room },
                    request,
                    measured,
                };
                log_fit(first, &fit, counts);
                return (fit, skeleton_tokens);
            }
        }
    }
}

/// The room to try after a request built at `room` measured over: the
/// material it carried, scaled by the room the skeleton leaves over what the
/// prompt beside that skeleton measured. Every token of the overflow is
/// blamed on the material, which shrinks too far rather than too little.
/// Always below `room`; halved when the measurement says nothing better.
fn shrunk(room: usize, skeleton_tokens: usize, measured: &Measured) -> usize {
    let material = measured.estimated.saturating_sub(skeleton_tokens);
    let beside = measured.allowed.saturating_sub(skeleton_tokens);
    let next = material
        .saturating_mul(beside)
        .checked_div(measured.prompt_tokens.saturating_sub(skeleton_tokens))
        .unwrap_or(0);
    if next < room && measured.prompt_tokens > measured.allowed {
        next
    } else {
        room / 2
    }
}

fn log_fit(first: usize, fit: &Fit, counts: usize) {
    tracing::info!(
        room = first,
        shrunk_to = fit.room,
        fits = fit.fits(),
        exact = fit.measured.is_some_and(|measured| measured.exact),
        prompt_tokens = fit.measured.map(|measured| measured.prompt_tokens),
        allowed = fit.measured.map(|measured| measured.allowed),
        counts,
        "learning.fit"
    );
}

/// [`fit_room`], then `call` with the request built; a size refusal from any
/// gate or server shrinks the room and calls again. A refusal that counted
/// the prompt exactly is the measurement for the next room; one that
/// estimated it says only "too big", so the room is halved.
///
/// `Ok(None)` when no material fits: the skeleton alone does not, or the
/// room shrank to nothing. The site then degrades; it never fails for size.
///
/// # Errors
///
/// Whatever `call` fails with, other than a size refusal.
pub(in crate::features::learning) async fn send_fitted<T, F>(
    llm: &dyn LLMPort,
    room: usize,
    mut build: impl FnMut(usize) -> GroundedRequest,
    mut call: impl FnMut(GroundedRequest) -> F,
) -> Result<Option<T>>
where
    F: Future<Output = Result<T>>,
{
    let mut room = room;
    loop {
        let (fit, skeleton_tokens) = settle(llm, room, &mut build).await;
        if fit.room == 0 || !fit.fits() {
            return Ok(None);
        }
        let sent = fit.room;
        let measured = fit.measured;
        room = match call(fit.request).await {
            Err(AppError::PromptExceedsWindow {
                prompt_tokens,
                counted_exactly,
                ..
            }) => match measured.filter(|_| counted_exactly) {
                Some(measured) => shrunk(
                    sent,
                    skeleton_tokens,
                    &Measured {
                        prompt_tokens,
                        exact: true,
                        ..measured
                    },
                ),
                None => sent / 2,
            },
            result => return result.map(Some),
        };
        tracing::info!(
            room = sent,
            shrunk_to = room,
            counted_exactly = measured.is_some_and(|measured| measured.exact),
            "learning.fit_refused"
        );
        if room == 0 {
            return Ok(None);
        }
    }
}

#[cfg(test)]
#[path = "model_call_tests.rs"]
mod tests;
