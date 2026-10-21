//! Every source's items made into one list: which index holds behind which,
//! what each waiting item is queued behind, and what each holds, in words.
//!
//! Nothing here reads state or decides an order of work. The turn order
//! already decided who embeds first and each job kind who runs next; this
//! only says so, naming the other item by what it works on.

use super::dto::{ActivityEntryDto, ActivitySnapshotDto};
use crate::infrastructure::passage_index::Standing;
use lattice_core::application::ports::{ActivityItem, ActivityResource, ActivityState};
use std::collections::HashMap;
use std::path::PathBuf;

/// The snapshot of `items`, given the index turn order's `standing`.
pub fn assemble(mut items: Vec<ActivityItem>, standing: &[Standing]) -> ActivitySnapshotDto {
    pause_behind_turns(&mut items, standing);
    queue_behind_runners(&mut items);
    let queued = queued_requests(&items);
    items.sort_by(|a, b| {
        rank(a.state)
            .cmp(&rank(b.state))
            .then_with(|| oldest_first(a.since, b.since))
            .then_with(|| a.title.cmp(&b.title))
    });
    let running = items
        .iter()
        .filter(|item| item.state == ActivityState::Running)
        .count();
    ActivitySnapshotDto {
        running: u32::try_from(running).unwrap_or(u32::MAX),
        items: items.into_iter().map(|item| entry(item, &queued)).collect(),
    }
}

fn rank(state: ActivityState) -> u8 {
    match state {
        ActivityState::Running => 0,
        ActivityState::Paused => 1,
        ActivityState::Waiting => 2,
    }
}

/// Known times first, oldest first.
fn oldest_first(a: Option<i64>, b: Option<i64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// "A", "A and B", "A, B and C".
fn joined(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A running item whose index waits for one ranked above it is paused, and
/// says which goes first. While it holds it has no pace.
fn pause_behind_turns(items: &mut [ActivityItem], standing: &[Standing]) {
    let ahead_of: HashMap<&PathBuf, &Vec<PathBuf>> = standing
        .iter()
        .filter(|standing| !standing.ahead.is_empty())
        .map(|standing| (&standing.dir, &standing.ahead))
        .collect();
    if ahead_of.is_empty() {
        return;
    }
    let names: HashMap<PathBuf, String> = items
        .iter()
        .filter_map(|item| {
            let dir = item.index_dir.clone()?;
            Some((dir, item.subject_name().to_string()))
        })
        .collect();
    for item in items.iter_mut() {
        if item.state != ActivityState::Running {
            continue;
        }
        let Some(ahead) = item.index_dir.as_ref().and_then(|dir| ahead_of.get(dir)) else {
            continue;
        };
        let mut first: Vec<String> = Vec::new();
        for name in ahead.iter().filter_map(|dir| names.get(dir)) {
            if !first.contains(name) {
                first.push(name.clone());
            }
        }
        item.state = ActivityState::Paused;
        item.per_second = None;
        item.seconds_left = None;
        item.detail = Some(if first.is_empty() {
            "Paused while another index embeds first".to_string()
        } else {
            format!("Paused while {} embeds first", joined(&first))
        });
    }
}

/// A waiting item in a line says what is ahead of it: the running item,
/// then the waiting ones that came first. One put off by an outage keeps
/// its own words; the renderer adds when it tries again.
fn queue_behind_runners(items: &mut [ActivityItem]) {
    let mut ahead: Vec<(usize, String)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let (Some(queue), ActivityState::Waiting) = (&item.queue, item.state) else {
            continue;
        };
        if item.retry_at.is_some() {
            continue;
        }
        let mut before: Vec<&ActivityItem> = items
            .iter()
            .enumerate()
            .filter(|(other_index, other)| {
                *other_index != index
                    && other.queue.as_ref() == Some(queue)
                    && (other.state != ActivityState::Waiting
                        || (other.retry_at.is_none()
                            && oldest_first(other.since, item.since).is_lt()))
            })
            .map(|(_, other)| other)
            .collect();
        before.sort_by(|a, b| {
            rank(a.state)
                .cmp(&rank(b.state))
                .then_with(|| oldest_first(a.since, b.since))
        });
        let Some(first) = before.first().map(|first| first.subject_name()) else {
            continue;
        };
        let detail = match before.len() {
            1 => format!("Queued behind {first}"),
            more => format!("Queued behind {first} and {} more", more - 1),
        };
        ahead.push((index, detail));
    }
    for (index, detail) in ahead {
        if let Some(item) = items.get_mut(index) {
            item.detail = Some(detail);
        }
    }
}

/// Requests waiting on each model, from the items that stand for a model's
/// backend.
fn queued_requests(items: &[ActivityItem]) -> HashMap<String, usize> {
    items
        .iter()
        .filter_map(|item| Some((item.subject_name().to_string(), item.queued?)))
        .collect()
}

/// What an item holds, in words; nothing while it is paused or waiting.
fn holding(item: &ActivityItem, queued: &HashMap<String, usize>) -> Option<String> {
    if item.state != ActivityState::Running {
        return None;
    }
    let words: Vec<String> = item
        .uses
        .iter()
        .map(|resource| match resource {
            ActivityResource::EmbeddingModel => "Using the embedding model".to_string(),
            ActivityResource::Model { name } => match queued.get(name).copied().unwrap_or(0) {
                0 => format!("Using {name}"),
                1 => format!("Using {name} (1 request queued)"),
                more => format!("Using {name} ({more} requests queued)"),
            },
        })
        .collect();
    (!words.is_empty()).then(|| words.join("; "))
}

fn entry(item: ActivityItem, queued: &HashMap<String, usize>) -> ActivityEntryDto {
    let holding = holding(&item, queued);
    ActivityEntryDto {
        id: item.id,
        kind: item.kind,
        title: item.title,
        state: item.state,
        detail: item.detail,
        progress: item.progress,
        per_second: item.per_second,
        seconds_left: item.seconds_left,
        holding,
        target: item.target,
        since: item.since,
        retry_at: item.retry_at,
    }
}
