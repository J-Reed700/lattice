//! What the session shows beside the scores, counted rather than judged.

use super::Facts;
use crate::practice::session::kind::Kind;
use crate::practice::session::record::{Counted, EventKind, SessionEvent};
use lattice_core::domain::learning::practice::work::{CheckScope, Rung, Stage};

fn add_time(spent: &mut Vec<(Stage, u64)>, stage: Stage, ms: u64) {
    match spent.iter_mut().find(|(known, _)| *known == stage) {
        Some((_, total)) => *total = total.saturating_add(ms),
        None => spent.push((stage, ms)),
    }
}

fn span(since: i64, until: i64) -> u64 {
    u64::try_from(until.saturating_sub(since)).unwrap_or(0)
}

/// The active time of each stage the learner spent time in, in the form's order. A span runs from an opened event
/// to the next closed event; one the app never closed ends at the last event before the next opened, or at the
/// last event of the session. A stage move splits the span it falls in.
fn stage_times(order: &[Stage], events: &[SessionEvent]) -> Vec<(Stage, u64)> {
    let mut events: Vec<&SessionEvent> = events.iter().collect();
    events.sort_by_key(|event| event.ordinal);
    let mut spent: Vec<(Stage, u64)> = Vec::new();
    let mut stage = order.first().copied();
    let mut open_since: Option<i64> = None;
    let mut last_at: Option<i64> = None;
    for event in events {
        match event.kind {
            EventKind::Opened => {
                if let (Some(since), Some(stage)) = (open_since, stage) {
                    add_time(&mut spent, stage, span(since, last_at.unwrap_or(since)));
                }
                open_since = Some(event.at);
            }
            EventKind::Closed => {
                if let (Some(since), Some(stage)) = (open_since.take(), stage) {
                    add_time(&mut spent, stage, span(since, event.at));
                }
            }
            EventKind::Moved(to) => {
                if let (Some(since), Some(stage)) = (open_since, stage) {
                    add_time(&mut spent, stage, span(since, event.at));
                    open_since = Some(event.at);
                }
                if to.is_some() {
                    stage = to;
                }
            }
            EventKind::Other => {}
        }
        last_at = Some(event.at);
    }
    if let (Some(since), Some(stage)) = (open_since, stage) {
        add_time(&mut spent, stage, span(since, last_at.unwrap_or(since)));
    }
    order
        .iter()
        .filter_map(|stage| {
            let (_, active_ms) = spent.iter().find(|(known, _)| known == stage)?;
            (*active_ms > 0).then_some((*stage, *active_ms))
        })
        .collect()
}

fn count(items: usize) -> u32 {
    u32::try_from(items).unwrap_or(u32::MAX)
}

/// What the session shows beside the scores.
pub fn facts(kind: &Kind, counted: &Counted<'_>) -> Facts {
    let order = counted.form.stages(kind.learner_cases.is_some());
    let hints = [Rung::Nudge, Rung::Idea, Rung::Plan, Rung::Point]
        .into_iter()
        .filter_map(|rung| {
            let given = counted
                .hints
                .iter()
                .filter(|hint| hint.rung == rung)
                .count();
            (given > 0).then(|| (rung, count(given)))
        })
        .collect();
    Facts {
        stage_ms: stage_times(&order, counted.events),
        checks: count(counted.scopes.len()),
        submits: count(
            counted
                .scopes
                .iter()
                .filter(|scope| matches!(scope, CheckScope::All))
                .count(),
        ),
        hints,
        settings: counted.settings.to_vec(),
    }
}
