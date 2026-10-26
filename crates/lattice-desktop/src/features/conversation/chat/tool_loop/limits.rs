//! A round cut short by one of the turn's limits, and the round that answers
//! in its place.
//!
//! Every error used to end the turn and mark the question failed, sources and
//! results with it: two hours of deep research came to nothing but an error
//! when the deadline passed during its last page read. A round that fails at
//! the deadline, or with a prompt the window cannot hold, is asked again as
//! the answering round instead, from what the turn has gathered.

use super::calls::ToolCalls;
use super::clock::TurnClock;
use super::final_round::{self, LastRound};
use super::room::{self, Window};
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest};
use lattice_core::shared::error::{AppError, Result};
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// Whether a call that failed did so at the turn's deadline: the deadline has
/// passed, and Stop was not pressed.
pub(super) fn ran_out(clock: &TurnClock, cancel: &CancellationToken) -> bool {
    clock.expired() && !cancel.is_cancelled()
}

/// What a round that failed with `error` is asked again as, once the prompt
/// is made to fit if it did not; `error` back when no round would fare
/// better.
///
/// Past the deadline, the round answers with no deadline of its own. A prompt
/// too long for the window sheds its newest results and answers; a round
/// already past the deadline stays that round. Shedding frees something or
/// fails, so a round asked again for its size is asked a bounded number of
/// times.
pub(super) fn recover(
    error: AppError,
    last_round: Option<LastRound>,
    request: &mut CompletionRequest,
    loop_start: usize,
    sent_chars: usize,
    window: &mut Window,
    calls: &mut ToolCalls<'_>,
    clock: &TurnClock,
    cancel: &CancellationToken,
) -> Result<LastRound> {
    let out_of_time = last_round == Some(LastRound::OutOfTime);
    // The round asked again says only what holds for it. Its instruction can
    // sit at the end of the newest result, which shedding replaces.
    if let Some(last_round) = last_round {
        final_round::take_back(request, last_round);
    }
    let next = match &error {
        _ if !out_of_time && ran_out(clock, cancel) => Some(LastRound::OutOfTime),
        AppError::PromptExceedsWindow {
            prompt_tokens,
            counted_exactly,
            ..
        } => {
            let excess = if *counted_exactly {
                window.measured(sent_chars, *prompt_tokens);
                calls.set_window_chars(window.chars());
                window.chars_in(prompt_tokens.saturating_sub(window.tokens()))
            } else {
                sent_chars.saturating_sub(window.chars())
            };
            shed(&mut request.input, loop_start, excess, calls).then_some(if out_of_time {
                LastRound::OutOfTime
            } else {
                LastRound::WindowFull
            })
        }
        _ => None,
    };
    let Some(next) = next else {
        return Err(error);
    };
    warn!(%error, ?next, "A round failed at a limit; asking for the answer instead");
    Ok(next)
}

/// Shed the newest results to free about `excess` characters, and forget the
/// pages they carried: a repeat request for one is read again, not told it is
/// above. Whether anything was shed.
pub(super) fn shed(
    input: &mut [CompletionInput],
    loop_start: usize,
    excess: usize,
    calls: &mut ToolCalls<'_>,
) -> bool {
    let shed = room::shed_newest_results(input, loop_start, excess);
    if shed.is_empty() {
        return false;
    }
    warn!(
        results = shed.len(),
        excess, "The prompt no longer fit the window; shed its newest results to answer"
    );
    calls.results_condensed(&shed);
    true
}
