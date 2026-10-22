//! The teaching review after a claim repair or a user edit leaves the
//! sentences claims stand on as they are (claim sources design §10.3): a
//! finding about a protected sentence waits for the user, and the review's
//! repair changes only the sentences its findings name, every other one
//! reverted and every protected one byte-identical.

use crate::features::learning::lessons::content_verification::{confine, Protected};
use crate::features::learning::review_findings::{FindingKey, FindingSite, ReviewFinding};
use lattice_core::shared::error::Result;
use serde_json::Value;

/// The findings about a protected sentence: their passage holds one, or
/// lies inside one.
pub(super) fn on_protected(findings: &[ReviewFinding], protected: &[Protected]) -> Vec<FindingKey> {
    findings
        .iter()
        .filter(|finding| {
            finding.passage.as_deref().is_some_and(|passage| {
                let passage = passage.trim();
                !passage.is_empty()
                    && protected.iter().any(|item| {
                        passage.contains(item.quote.as_str()) || item.quote.contains(passage)
                    })
            })
        })
        .map(ReviewFinding::key)
        .collect()
}

/// The lesson's sections in unit order: blocks, then questions.
fn sections(lesson: &Value) -> Vec<(&'static str, usize)> {
    let count = |name: &str| {
        lesson
            .get(name)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    (0..count("blocks"))
        .map(|index| ("blocks", index))
        .chain((0..count("questions")).map(|index| ("questions", index)))
        .collect()
}

/// Every string field of `value` with its path, as claims name fields.
fn leaves<'v>(value: &'v Value, path: String, out: &mut Vec<(String, &'v str)>) {
    let child = |key: &str| {
        if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}/{key}")
        }
    };
    match value {
        Value::String(text) => out.push((path, text)),
        Value::Object(fields) => {
            for (key, value) in fields {
                leaves(value, child(key), out);
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                leaves(value, child(&index.to_string()), out);
            }
        }
        _ => {}
    }
}

/// The sentences `sent` names in `section` (unit `unit`): each finding's
/// passage where a field holds it; for a finding with no passage, every
/// sentence of the section that no protected sentence overlaps.
fn targets_of(
    section: &Value,
    unit: usize,
    sent: &[&ReviewFinding],
    protected: &[Protected],
) -> Vec<Protected> {
    let mut fields = Vec::new();
    leaves(section, String::new(), &mut fields);
    let mut targets = Vec::new();
    for finding in sent {
        match finding.passage.as_deref().map(str::trim) {
            Some(passage) if !passage.is_empty() => {
                for (field, text) in &fields {
                    if text.contains(passage) {
                        targets.push(Protected {
                            unit,
                            field: field.clone(),
                            quote: passage.to_owned(),
                        });
                    }
                }
            }
            _ => {
                for (field, text) in &fields {
                    for (start, end) in lattice_core::shared::text::sentences(text) {
                        let sentence = &text[start..end];
                        let guarded = protected.iter().any(|item| {
                            item.field == *field
                                && (sentence.contains(item.quote.as_str())
                                    || item.quote.contains(sentence))
                        });
                        if !guarded {
                            targets.push(Protected {
                                unit,
                                field: field.clone(),
                                quote: sentence.to_owned(),
                            });
                        }
                    }
                }
            }
        }
    }
    targets
}

/// `repaired`, the review's repair of `previous`, confined section by
/// section to the sentences `sent` names, `protected` kept byte-identical.
/// `None` when the confined repair changes nothing: it fixed nothing the
/// findings name, which waits for the user like an unusable answer.
pub(super) fn confine_review(
    previous: &Value,
    repaired: &str,
    sent: &[ReviewFinding],
    protected: &[Protected],
) -> Result<Option<String>> {
    let mut lesson: Value = crate::features::learning::generation::parse_json(repaired)?;
    let blocks = previous
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    for (unit, (field, index)) in sections(previous).into_iter().enumerate() {
        let pointer = format!("/{field}/{index}");
        let (Some(before), Some(after)) =
            (previous.pointer(&pointer), lesson.pointer_mut(&pointer))
        else {
            continue;
        };
        let names = |finding: &&ReviewFinding| match finding.site {
            Some(FindingSite::Section(section)) => section == unit,
            Some(FindingSite::Question(question)) => blocks + question == unit,
            Some(FindingSite::Lesson) | None => true,
        };
        let here: Vec<&ReviewFinding> = sent.iter().filter(names).collect();
        let guarded: Vec<Protected> = protected
            .iter()
            .filter(|item| item.unit == unit)
            .cloned()
            .collect();
        let targets = targets_of(before, unit, &here, &guarded);
        *after = confine(before, after, &targets, &guarded)?;
    }
    Ok((lesson != *previous).then(|| lesson.to_string()))
}
