//! How much of the window the loop has left, in characters: what the prompt
//! already carries, what one more result may take, and when there is no room
//! for another.

use lattice_core::application::ports::llm_port::{
    chars_within_tokens, CompletionInput, DEFAULT_CHARS_PER_TOKEN,
};
use std::collections::HashSet;

/// The characters the prompt may fill this turn: the plan's input budget, at
/// a rate of characters per token this turn's own prompts correct.
///
/// The rate starts as the backend's calibrated average, which lags behind
/// the content and is floored at two characters a token. Chinese or Japanese
/// pages run nearer one, so a window judged by the average was overrun before
/// condensing began. Every count of this turn's prompt replaces the rate.
#[derive(Debug, Clone, Copy)]
pub(super) struct Window {
    input_budget: usize,
    chars_per_token: f64,
}

impl Window {
    pub(super) fn new(input_budget: usize, chars_per_token: f64) -> Self {
        Self {
            input_budget,
            chars_per_token,
        }
    }

    /// What the prompt may fill, erring short (see [`window_chars`]). Zero
    /// when the provider reports no window.
    pub(super) fn chars(&self) -> usize {
        window_chars(self.input_budget, self.chars_per_token)
    }

    /// The tokens the prompt may fill.
    pub(super) fn tokens(&self) -> usize {
        self.input_budget
    }

    /// About how many characters `tokens` of this turn's prompt take.
    pub(super) fn chars_in(&self, tokens: usize) -> usize {
        (tokens as f64 * self.chars_per_token).ceil() as usize
    }

    /// Fold in a count of this turn's prompt: `chars` characters, as
    /// [`chars_in_flight`] measures them, that the backend counted as
    /// `tokens`. The count includes the template and the tool schemas as the
    /// server renders them, so the rate errs short.
    pub(super) fn measured(&mut self, chars: usize, tokens: usize) {
        if chars > 0 && tokens > 0 {
            self.chars_per_token = chars as f64 / tokens as f64;
        }
    }
}

/// Whether another tool result could still fit in the window, at the size
/// below which a result says too little to be worth a round. A provider that
/// reports no window is never treated as full.
pub(super) fn window_has_room_for_a_result(window_chars: usize, chars_in_flight: usize) -> bool {
    if window_chars == 0 {
        return true;
    }
    window_chars.saturating_sub(chars_in_flight) >= MIN_TOOL_RESULT_CHARS
}

/// The characters the prompt may fill: the plan's input budget, converted at
/// the backend's calibrated characters per token and erring short, so dense
/// text under-fills rather than overruns.
pub(super) fn window_chars(input_budget: usize, chars_per_token: f64) -> usize {
    chars_within_tokens(input_budget, chars_per_token)
}

/// A result cut shorter than this says too little to have been worth the round
/// that asked for it, so a nearly full window still gets this much.
pub(super) const MIN_TOOL_RESULT_CHARS: usize = 1_500;

/// Roughly how many characters the next generation already has to read. A
/// picture counts as the characters its declared tokens stand for, never as
/// its bytes.
pub(super) fn chars_in_flight(input: &[CompletionInput]) -> usize {
    input
        .iter()
        .map(|item| match item {
            CompletionInput::Message { content, .. } => content.chars().count(),
            CompletionInput::Multimodal { .. } => {
                item.text().chars().count()
                    + (item.image_tokens() as f64 * DEFAULT_CHARS_PER_TOKEN) as usize
            }
            CompletionInput::ToolResult { output, .. } => output.chars().count(),
            CompletionInput::ToolCall {
                name, arguments, ..
            } => name.len() + arguments.to_string().len(),
            CompletionInput::Native { value } => value.to_string().len(),
        })
        .sum()
}

/// How many characters one tool result may take, given the model's window,
/// what is already in it, and how many results this round has still to fit.
///
/// The window alone decides: there is no configured ceiling on top of it, so a
/// large window carries a long result whole. A result longer than its share is
/// shown in part and says how to read on (see `format_tool_result`).
///
/// One share of what is left is always kept back for the round after this
/// one. A lone page read that took the whole window left the next round
/// nothing to read with: it had to condense or stop, so one long page ended
/// the research. The rest of a long page is a further call by offset.
pub(super) fn tool_result_allowance(
    window_chars: usize,
    chars_in_flight: usize,
    calls_left: usize,
) -> usize {
    let room = window_chars.saturating_sub(chars_in_flight);
    let share = room / calls_left.saturating_add(1).max(2);
    // The floor is only kept while the window can still take it. Past that, a
    // result is given its share of what is left and no more: overrunning the
    // window fails the next round outright, which is worse than a short result.
    if room < MIN_TOOL_RESULT_CHARS {
        return share;
    }
    share.max(MIN_TOOL_RESULT_CHARS)
}

/// Said in place of a result taken out of a prompt that no longer fit.
pub(super) const SHED_RESULT: &str = "This result was taken out: with it, the prompt no longer fit in the context window. Answer from the results that are still here, and say so if the answer needed this one.";

/// Take the newest tool results after `loop_start` out of `input`, newest
/// first, until about `excess` characters are freed, leaving [`SHED_RESULT`]
/// in each one's place so every call still has its answer.
///
/// The ids of the results shed, empty when nothing could be. Each call frees
/// what it can or nothing, so a caller that sheds until the prompt fits stops:
/// once every result is shed, the prompt is the planned request, which was
/// planned to fit.
pub(super) fn shed_newest_results(
    input: &mut [CompletionInput],
    loop_start: usize,
    excess: usize,
) -> HashSet<String> {
    let note = SHED_RESULT.chars().count();
    let mut freed = 0;
    let mut shed = HashSet::new();
    for item in input.iter_mut().skip(loop_start).rev() {
        if !shed.is_empty() && freed >= excess {
            break;
        }
        if let CompletionInput::ToolResult { id, output } = item {
            let chars = output.chars().count();
            if chars > note {
                freed += chars - note;
                *output = SHED_RESULT.to_string();
                shed.insert(id.clone());
            }
        }
    }
    shed
}
