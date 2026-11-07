//! The schema's citation apparatus is structured data. A writer that also
//! types it into the prose leaves a learner reading it, and verification
//! reading its quotations as the lesson's own statements.
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

fn mention() -> &'static Regex {
    static MENTION: OnceLock<Regex> = OnceLock::new();
    MENTION.get_or_init(|| {
        Regex::new(r"(?i) ?[(\[][ \t]*source[ \t]?index[ \t]*:?[ \t]*\d+[ \t]*[)\]]")
            .expect("mention pattern")
    })
}

fn list_marker() -> &'static Regex {
    static MARKER: OnceLock<Regex> = OnceLock::new();
    MARKER.get_or_init(|| Regex::new(r"^\s*(?:[-*+]|\d+[.)])[ \t]+").expect("marker pattern"))
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
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
        let mut j = i + run;
        let mut end = None;
        while j < bytes.len() {
            if bytes[j] != b'`' {
                j += 1;
                continue;
            }
            let closing = bytes[j..].iter().take_while(|b| **b == b'`').count();
            if closing == run {
                end = Some(j + closing);
                break;
            }
            j += closing;
        }
        match end {
            Some(end) => {
                clean(&line[plain..i], &mut out);
                out.push_str(&line[i..end]);
                plain = end;
                i = end;
            }
            None => i += run,
        }
    }
    clean(&line[plain..], &mut out);
    (out, found)
}

fn fence_of(line: &str) -> Option<(char, usize)> {
    let trimmed = line.trim_start();
    let mark = trimmed.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = trimmed.chars().take_while(|c| *c == mark).count();
    let info = &trimmed[run..];
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
        let content = &raw[..raw.len() - ending.len()];
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
                line.removed = mentioned && is_quotation(&body[marker.end()..]);
            }
            line.heading = !line.item && is_heading(&body);
            line.drop = line.removed;
            line.body = body;
        }
        lines.push(line);
    }
    for h in 0..lines.len() {
        if !lines[h].heading || lines[h].drop {
            continue;
        }
        let (mut seen, mut last, mut j) = (0, h, h + 1);
        let mut standing = false;
        while j < lines.len() {
            if lines[j].blank {
            } else if lines[j].removed {
                seen += 1;
                last = j;
            } else {
                standing = lines[j].item;
                break;
            }
            j += 1;
        }
        if seen == 0 || standing {
            continue;
        }
        let to = if j == lines.len() { j - 1 } else { last };
        for line in &mut lines[h..=to] {
            line.drop = true;
        }
        let mut k = h;
        while k > 0 && lines[k - 1].blank && !lines[k - 1].drop {
            k -= 1;
            lines[k].drop = true;
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
        for item in draft[list].as_array_mut().into_iter().flatten() {
            let Some(text) = item[field].as_str() else {
                continue;
            };
            let stripped = strip_citation_apparatus(text);
            if stripped != text {
                item[field] = Value::String(stripped);
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
