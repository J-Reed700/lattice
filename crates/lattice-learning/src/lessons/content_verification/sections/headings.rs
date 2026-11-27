//! Markdown heading checks and empty-heading cleanup around section cuts.

use lattice_core::shared::text::segmentation::code_fences;

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

/// Removes Markdown headings left without body content after cuts (at EOF or
/// immediately before another heading of the same or higher level), outside
/// fenced code blocks. Preserves the check-reveal delimiter heading.
pub(in crate::lessons::content_verification) fn prune_empty_headings(text: &str) -> String {
    let fences = code_fences(text);
    let in_fence = |offset: usize| fences.iter().any(|fence| fence.whole().contains(&offset));
    let mut lines: Vec<(&str, bool)> = Vec::new();
    let mut offset = 0;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        lines.push((line, in_fence(offset)));
        offset += raw.len() + 1;
    }

    let mut drop = vec![false; lines.len()];
    loop {
        let mut changed = false;
        for index in 0..lines.len() {
            if drop[index] || lines[index].1 || is_reveal_heading(lines[index].0) {
                continue;
            }
            let Some(level) = heading_level(lines[index].0) else {
                continue;
            };
            let next = (index + 1..lines.len())
                .find(|&later| !drop[later] && !lines[later].0.trim().is_empty());
            let empty = match next {
                None => true,
                Some(later) => {
                    !lines[later].1
                        && heading_level(lines[later].0)
                            .is_some_and(|next_level| next_level <= level)
                }
            };
            if empty {
                drop[index] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    if !drop.iter().any(|&removed| removed) {
        return text.to_owned();
    }

    let mut kept: Vec<&str> = Vec::new();
    for (index, &(line, fenced)) in lines.iter().enumerate() {
        if drop[index] {
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
