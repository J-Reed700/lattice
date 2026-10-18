//! Where the verbatim window opens, and the refold it asks for when it has to
//! move.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §24.8.

use super::{ContextAssembler, ContextRequest};
use crate::application::ports::llm_port::CompletionInput;
use crate::domain::conversation::memory::SummaryRefold;

/// The working summary as the assembler may carry it: where it ends (when that
/// can be placed), the rendered message, and its cost.
pub(super) type SummaryCandidate = (Option<i64>, CompletionInput, usize);

/// The recent turns chosen for a request, oldest first.
pub(super) struct RecentWindow {
    pub messages: Vec<(i64, CompletionInput)>,
    pub tokens: usize,
    /// `i64::MAX` when nothing was kept.
    pub oldest_kept_sequence: i64,
    /// The summary's end when it anchors the window, otherwise zero.
    pub anchored_after: i64,
}

impl ContextAssembler {
    /// The newest whole turns that fit, never reaching back past the summary
    /// that anchors them.
    ///
    /// A transcript that fits `recent_pool` whole is carried whole and the
    /// summary anchors nothing. Otherwise a summary that fits `summary_pool`
    /// and knows where it ends anchors the window right after that message.
    pub(super) fn recent_window(
        &self,
        request: &ContextRequest<'_>,
        summary: Option<&SummaryCandidate>,
        recent_pool: usize,
        remaining: usize,
        summary_pool: usize,
    ) -> RecentWindow {
        let candidates: Vec<(i64, CompletionInput, usize)> = request
            .recent
            .iter()
            .map(|message| {
                let rendered = super::render::render_recent(message);
                let cost = self.count_input(&rendered);
                (message.sequence, rendered, cost)
            })
            .collect();
        let whole_transcript_fits = request.recent_reaches_start
            && candidates.iter().map(|(_, _, cost)| cost).sum::<usize>() <= recent_pool;
        let anchored_after = summary
            .filter(|(_, _, cost)| !whole_transcript_fits && *cost <= summary_pool)
            .and_then(|(through, _, _)| *through)
            .unwrap_or(0);
        let mut messages: Vec<(i64, CompletionInput)> = Vec::new();
        let mut tokens = 0usize;
        let mut oldest_kept_sequence = i64::MAX;
        for (sequence, rendered, cost) in candidates
            .into_iter()
            .rev()
            .take_while(|(sequence, _, _)| *sequence > anchored_after)
        {
            if tokens + cost > recent_pool && !messages.is_empty() {
                break;
            }
            if tokens + cost > remaining && !messages.is_empty() {
                break;
            }
            tokens += cost;
            oldest_kept_sequence = oldest_kept_sequence.min(sequence);
            messages.push((sequence, rendered));
        }
        messages.reverse();
        RecentWindow {
            messages,
            tokens,
            oldest_kept_sequence,
            anchored_after,
        }
    }

    /// The refold this request asks for, when its window had to drop turns
    /// the summary does not cover.
    ///
    /// `covered` is where the window was anchored: the summary's end, or zero
    /// when no summary anchors it. An uncovered turn was dropped when a
    /// candidate between the two was left out, or when the candidates begin
    /// past it because older ones were never read.
    ///
    /// The summary is asked to end inside the window rather than just before
    /// it. The newest half of what the window carried stays verbatim, opening
    /// on a user turn, and the rest is folded, so the next window starts with
    /// half its room free and the turns that follow extend an unchanged prefix
    /// until it fills. Half is counted in tokens, and also in messages when it
    /// was the candidate list rather than the budget that ended the window.
    pub(super) fn summary_refold(
        &self,
        request: &ContextRequest<'_>,
        window: &[(i64, CompletionInput)],
        covered: i64,
    ) -> Option<SummaryRefold> {
        let window_start = window.first().map(|(sequence, _)| *sequence)?;
        let first_candidate = request.recent.first().map(|message| message.sequence);
        let unread_before_candidates = !request.recent_reaches_start
            && first_candidate.is_some_and(|first| first > covered.saturating_add(1));
        let dropped = unread_before_candidates
            || request
                .recent
                .iter()
                .any(|message| message.sequence > covered && message.sequence < window_start);
        if !dropped {
            return None;
        }

        let costs: Vec<usize> = window
            .iter()
            .map(|(_, message)| self.count_input(message))
            .collect();
        let token_room = costs.iter().sum::<usize>() / 2;
        let message_room = if first_candidate == Some(window_start) {
            window.len() / 2
        } else {
            window.len()
        };
        let mut kept_tokens = 0usize;
        let mut kept = 0usize;
        for cost in costs.iter().rev() {
            if kept >= message_room || kept_tokens.saturating_add(*cost) > token_room {
                break;
            }
            kept += 1;
            kept_tokens += cost;
        }
        let through_sequence = match window
            .iter()
            .skip(window.len().saturating_sub(kept))
            .find(|(_, message)| is_user_turn(message))
        {
            Some((next_start, _)) => next_start.saturating_sub(1),
            // Nothing small enough stays: the whole window is folded.
            None => window.last().map(|(sequence, _)| *sequence)?,
        };
        Some(SummaryRefold {
            window_start_sequence: window_start,
            through_sequence,
        })
    }
}

/// Whether a rendered recent message is one of the user's own turns.
fn is_user_turn(message: &CompletionInput) -> bool {
    message.is_user()
}
