//! Markdown heading checks and empty-heading cleanup around section cuts.

use lattice_core::shared::text::segmentation::code_fences;
use std::collections::HashMap;

const REVEAL_HEADING: &str = "### check your thinking";

/// ATX heading level (`1..=6`) when `line` opens a Markdown heading.
pub(in crate::lessons::content_verification) fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    ((1..=6).contains(&hashes) && trimmed.as_bytes().get(hashes) == Some(&b' ')).then_some(hashes)
}

fn is_reveal_heading(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case(REVEAL_HEADING)
}

fn split_marked_lines(text: &str) -> Vec<(&str, bool)> {
    let fences = code_fences(text);
    let in_fence = |offset: usize| fences.iter().any(|fence| fence.whole().contains(&offset));
    let mut lines = Vec::new();
    let mut offset = 0;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        lines.push((line, in_fence(offset)));
        offset += raw.len() + 1;
    }
    lines
}

fn empty_heading_drops(lines: &[(&str, bool)], preserve: &HashMap<&str, usize>) -> Vec<bool> {
    let mut reserved = preserve.clone();
    let mut kept_empty = vec![false; lines.len()];
    let mut drop = vec![false; lines.len()];
    loop {
        let mut changed = false;
        for index in 0..lines.len() {
            let Some(&(line, fenced)) = lines.get(index) else {
                continue;
            };
            if drop.get(index).copied().unwrap_or(false)
                || kept_empty.get(index).copied().unwrap_or(false)
                || fenced
                || is_reveal_heading(line)
            {
                continue;
            }
            let Some(level) = heading_level(line) else {
                continue;
            };
            let next = (index + 1..lines.len()).find(|&later| {
                !drop.get(later).copied().unwrap_or(false)
                    && lines
                        .get(later)
                        .is_some_and(|&(later_line, _)| !later_line.trim().is_empty())
            });
            let empty = match next {
                None => true,
                Some(later) => lines.get(later).is_some_and(|&(later_line, later_fenced)| {
                    !later_fenced
                        && heading_level(later_line).is_some_and(|next_level| next_level <= level)
                }),
            };
            if !empty {
                continue;
            }
            if let Some(left) = reserved.get_mut(line.trim()).filter(|left| **left > 0) {
                *left -= 1;
                if let Some(slot) = kept_empty.get_mut(index) {
                    *slot = true;
                }
                continue;
            }
            if let Some(slot) = drop.get_mut(index) {
                *slot = true;
            }
            changed = true;
        }
        if !changed {
            break;
        }
    }
    drop
}

fn prune_with_preserved(text: &str, preserve: &HashMap<&str, usize>) -> String {
    let lines = split_marked_lines(text);
    let drop = empty_heading_drops(&lines, preserve);
    if !drop.iter().any(|&removed| removed) {
        return text.to_owned();
    }

    let mut kept: Vec<&str> = Vec::new();
    for (index, &(line, fenced)) in lines.iter().enumerate() {
        if drop.get(index).copied().unwrap_or(false) {
            continue;
        }
        if !fenced
            && line.trim().is_empty()
            && (kept.is_empty() || kept.last().is_some_and(|prev| prev.trim().is_empty()))
        {
            continue;
        }
        kept.push(line);
    }
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    let joined = kept.join("\n");
    if joined.is_empty() {
        String::new()
    } else {
        let tail = text.get(text.trim_end().len()..).unwrap_or_default();
        format!("{joined}{tail}")
    }
}

/// Removes Markdown headings left without body content after cuts (at EOF or
/// immediately before another heading of the same or higher level), outside
/// fenced code blocks. Preserves the check-reveal delimiter heading.
pub(in crate::lessons::content_verification) fn prune_empty_headings(text: &str) -> String {
    prune_with_preserved(text, &HashMap::new())
}

/// Removes Markdown headings in `after` that became empty because of the edit
/// from `before` to `after`, preserving any headings that were already empty
/// in `before`.
pub(in crate::lessons::content_verification) fn prune_newly_empty_headings(
    before: &str,
    after: &str,
) -> String {
    let before_lines = split_marked_lines(before);
    let before_empty = empty_heading_drops(&before_lines, &HashMap::new());
    let mut preserve: HashMap<&str, usize> = HashMap::new();
    for (index, &is_empty) in before_empty.iter().enumerate() {
        if is_empty {
            if let Some(&(line, _)) = before_lines.get(index) {
                *preserve.entry(line.trim()).or_insert(0) += 1;
            }
        }
    }
    prune_with_preserved(after, &preserve)
}

/// Non-reveal Markdown heading lines lying inside the byte span between `before`
/// and `after` (outside fenced code blocks).
pub(in crate::lessons::content_verification) fn gap_headings(
    text: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> Vec<String> {
    let start = before.and_then(|sentence| text.find(sentence).map(|at| at + sentence.len()));
    let end = after.and_then(|sentence| {
        let from = start.unwrap_or(0);
        text.get(from..)?.find(sentence).map(|rel| from + rel)
    });
    let (span_start, span_end) = match (before, after) {
        (Some(_), Some(_)) => match start.zip(end) {
            Some(range) => range,
            None => return Vec::new(),
        },
        (Some(_), None) => match start {
            Some(from) => (from, text.len()),
            None => return Vec::new(),
        },
        (None, Some(_)) => match end {
            Some(until) => (0, until),
            None => return Vec::new(),
        },
        (None, None) => return Vec::new(),
    };

    let fences = code_fences(text);
    let in_fence = |offset: usize| fences.iter().any(|fence| fence.whole().contains(&offset));
    let mut found = Vec::new();
    let mut offset = 0;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let line_end = offset + line.len();
        if !in_fence(offset)
            && offset < span_end
            && line_end > span_start
            && heading_level(line).is_some()
            && !is_reveal_heading(line)
        {
            found.push(line.trim().to_owned());
        }
        offset += raw.len() + 1;
    }
    found
}
