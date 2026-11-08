//! The schema's citation apparatus is structured data. A writer that also
//! types it into the prose leaves a learner reading it, and verification
//! reading its quotations as the lesson's own statements.
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

fn pattern(source: &str) -> Regex {
    match Regex::new(source) {
        Ok(re) => re,
        Err(_) => match Regex::new("$^") {
            Ok(never) => never,
            Err(_) => unreachable!(),
        },
    }
}

fn mention() -> &'static Regex {
    static MENTION: LazyLock<Regex> = LazyLock::new(|| {
        pattern(r"(?i) ?[(\[][ \t]*source[ \t]?index[ \t]*:?[ \t]*\d+[ \t]*[)\]]")
    });
    &MENTION
}

fn list_marker() -> &'static Regex {
    static MARKER: LazyLock<Regex> = LazyLock::new(|| pattern(r"^\s*(?:[-*+]|\d+[.)])[ \t]+"));
    &MARKER
}

struct Line {
    body: String,
    ending: &'static str,
    blank: bool,
    item: bool,
    removed: bool,
    heading: bool,
    drop: bool,
}

fn backticks_at(bytes: &[u8], at: usize) -> usize {
    bytes.iter().skip(at).take_while(|b| **b == b'`').count()
}

/// Removes the mentions outside inline code spans; reports whether any went.
fn without_mentions(line: &str) -> (String, bool) {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut found = false;
    let mut plain = 0;
    let mut i = 0;
    let mut clean = |text: &str, out: &mut String| {
        let stripped = mention().replace_all(text, "");
        found |= stripped.len() != text.len();
        out.push_str(&stripped);
    };
    while let Some(&byte) = bytes.get(i) {
        if byte != b'`' {
            i += 1;
            continue;
        }
        let run = backticks_at(bytes, i);
        let mut j = i + run;
        let mut end = None;
        while let Some(&next) = bytes.get(j) {
            if next != b'`' {
                j += 1;
                continue;
            }
            let closing = backticks_at(bytes, j);
            if closing == run {
                end = Some(j + closing);
                break;
            }
            j += closing;
        }
        match end {
            Some(end) => {
                clean(line.get(plain..i).unwrap_or_default(), &mut out);
                out.push_str(line.get(i..end).unwrap_or_default());
                plain = end;
                i = end;
            }
            None => i += run,
        }
    }
    clean(line.get(plain..).unwrap_or_default(), &mut out);
    (out, found)
}

fn fence_of(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let mark = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = trimmed.chars().take_while(|c| *c == mark).count();
    let info = trimmed.trim_start_matches(mark);
    (run >= 3 && !(mark == '`' && info.contains('`'))).then_some((mark, run))
}

fn closes(line: &str, mark: char, run: usize) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= run && trimmed.chars().all(|c| c == mark)
}

fn is_heading(line: &str) -> bool {
    let label = line
        .trim()
        .trim_start_matches('#')
        .trim_matches(|c: char| c == '*' || c == '_' || c == ':' || c.is_whitespace())
        .to_lowercase();
    matches!(
        label.as_str(),
        "citations" | "citation" | "sources" | "source" | "references" | "reference"
    )
}

fn is_quotation(text: &str) -> bool {
    let text = text.trim();
    text.chars().count() >= 2
        && text.starts_with(['"', '\u{201c}'])
        && text.ends_with(['"', '\u{201d}'])
}

/// Takes the schema's citation apparatus out of a text the learner reads:
/// a source named by its index, the list items that held only a quotation
/// beside such a mention, and the heading left over a list that is gone.
/// Code is left alone, and so is everything else, byte for byte.
pub(in crate::features::learning) fn strip_citation_apparatus(text: &str) -> String {
    let mut lines = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    for raw in text.split_inclusive('\n') {
        let ending = if raw.ends_with("\r\n") {
            "\r\n"
        } else if raw.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        let content = raw.strip_suffix(ending).unwrap_or(raw);
        let in_code = match fence {
            Some((mark, run)) => {
                if closes(content, mark, run) {
                    fence = None;
                }
                true
            }
            None => {
                fence = fence_of(content);
                fence.is_some()
            }
        };
        let mut line = Line {
            body: content.to_owned(),
            ending,
            blank: false,
            item: false,
            removed: false,
            heading: false,
            drop: false,
        };
        if !in_code {
            let (body, mentioned) = without_mentions(content);
            line.blank = body.trim().is_empty();
            if let Some(marker) = list_marker().find(&body) {
                line.item = true;
                line.removed =
                    mentioned && is_quotation(body.get(marker.end()..).unwrap_or_default());
            }
            line.heading = !line.item && is_heading(&body);
            line.drop = line.removed;
            line.body = body;
        }
        lines.push(line);
    }
    for h in 0..lines.len() {
        if !lines.get(h).is_some_and(|line| line.heading && !line.drop) {
            continue;
        }
        let (mut seen, mut last, mut j) = (0, h, h + 1);
        let mut standing = false;
        while let Some(line) = lines.get(j) {
            if line.blank {
            } else if line.removed {
                seen += 1;
                last = j;
            } else {
                standing = line.item;
                break;
            }
            j += 1;
        }
        if seen == 0 || standing {
            continue;
        }
        let to = if j == lines.len() {
            lines.len().saturating_sub(1)
        } else {
            last
        };
        for line in lines.iter_mut().take(to + 1).skip(h) {
            line.drop = true;
        }
        let mut k = h;
        while k > 0
            && lines
                .get(k - 1)
                .is_some_and(|line| line.blank && !line.drop)
        {
            k -= 1;
            if let Some(line) = lines.get_mut(k) {
                line.drop = true;
            }
        }
    }
    let mut out: String = lines
        .iter()
        .filter(|line| !line.drop)
        .flat_map(|line| [line.body.as_str(), line.ending])
        .collect();
    if lines
        .last()
        .is_some_and(|line| line.drop && line.ending.is_empty())
    {
        if out.ends_with("\r\n") {
            out.truncate(out.len() - 2);
        } else if out.ends_with('\n') {
            out.truncate(out.len() - 1);
        }
    }
    if out.trim().is_empty() {
        text.to_owned()
    } else {
        out
    }
}

/// A draft's block bodies and question explanations without the apparatus.
/// A draft that held none comes back as the writer sent it.
pub(in crate::features::learning) fn strip_draft_apparatus(raw: String) -> String {
    let Ok(mut draft) = serde_json::from_str::<Value>(&raw) else {
        return raw;
    };
    let mut changed = 0;
    for (list, field) in [("blocks", "body"), ("questions", "explanation")] {
        let items = draft.get_mut(list).and_then(Value::as_array_mut);
        for item in items.into_iter().flatten() {
            let Some(slot) = item.get_mut(field) else {
                continue;
            };
            let Some(text) = slot.as_str() else {
                continue;
            };
            let stripped = strip_citation_apparatus(text);
            if stripped != text {
                *slot = Value::String(stripped);
                changed += 1;
            }
        }
    }
    if changed == 0 {
        return raw;
    }
    tracing::info!(
        fields = changed,
        "took the citation apparatus out of a lesson draft"
    );
    serde_json::to_string(&draft).unwrap_or(raw)
}
