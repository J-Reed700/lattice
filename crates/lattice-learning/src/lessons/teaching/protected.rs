//! The teaching review after a claim repair or a user edit leaves the
//! sentences claims stand on as they are (claim sources design §10.3): a
//! finding about a protected sentence waits for the user, except one about the
//! sentence of a machine's claim with rewrites left, which is repaired. The
//! review's repair changes only the sentences its findings name, every other
//! one reverted and every protected one byte-identical.

use crate::lessons::content_verification::{confine, unit_fields, Protected};
use crate::review_findings::{FindingKey, FindingSite, ReviewFinding};
use lattice_core::shared::error::Result;
use serde_json::Value;

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

fn blocks_in(lesson: &Value) -> usize {
    lesson
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

/// The unit index of a finding in the lesson candidate.
fn unit_of(candidate: &Value, finding: &ReviewFinding) -> Option<usize> {
    let blocks = blocks_in(candidate);
    match finding.key().site {
        FindingSite::Section(section) => Some(section),
        FindingSite::Question(question) => Some(blocks + question),
        FindingSite::Lesson => None,
    }
}

/// The section at unit `unit` in `candidate`.
fn section_at(candidate: &Value, unit: usize) -> Option<&Value> {
    let (field, index) = sections(candidate).get(unit).copied()?;
    candidate.pointer(&format!("/{field}/{index}"))
}

/// The protected items for a specific unit.
fn in_unit(protected: &[Protected], unit: usize) -> Vec<Protected> {
    protected
        .iter()
        .filter(|item| item.unit == unit)
        .cloned()
        .collect()
}

/// Whether a finding's passage overlaps any protected quote in its unit.
fn overlapping_quote(finding: &ReviewFinding, guarded: &[Protected]) -> bool {
    finding.passage.as_deref().is_some_and(|passage| {
        let passage = passage.trim();
        !passage.is_empty()
            && guarded
                .iter()
                .any(|item| passage.contains(item.quote.as_str()) || item.quote.contains(passage))
    })
}

/// The findings about a protected sentence: their passage holds one, or
/// lies inside one.
pub(super) fn on_protected(
    candidate: &Value,
    findings: &[ReviewFinding],
    protected: &[Protected],
) -> Vec<FindingKey> {
    if protected.is_empty() {
        return Vec::new();
    }
    let mut held = Vec::new();
    for finding in findings {
        if finding.key().site == FindingSite::Lesson {
            held.push(finding.key());
            continue;
        }
        let Some(unit) = unit_of(candidate, finding) else {
            continue;
        };
        let Some(section) = section_at(candidate, unit) else {
            continue;
        };
        let guarded = in_unit(protected, unit);
        if guarded.is_empty() {
            continue;
        }
        let block = unit < blocks_in(candidate);
        let editable = targets_of(section, (unit, block), &[finding], &guarded);
        let fixed: Vec<Protected> = guarded
            .iter()
            .filter(|item| !item.released)
            .cloned()
            .collect();
        if overlapping_quote(finding, &fixed) || editable.is_empty() {
            held.push(finding.key());
        }
    }
    held
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

/// The sentences `sent` names in `section` (`unit`, and whether it is a
/// block): each finding's passage where any field holds it; for a finding
/// with no passage, every sentence of the unit's teaching text that no
/// protected sentence overlaps.
fn targets_of(
    section: &Value,
    (unit, block): (usize, bool),
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
                            released: false,
                        });
                    }
                }
            }
            _ => {
                for field in unit_fields(section, block) {
                    let Some(text) = section
                        .pointer(&format!("/{field}"))
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    for (start, end) in lattice_core::shared::text::sentences(text) {
                        let sentence = &text[start..end];
                        let guarded = protected.iter().any(|item| {
                            item.field == field
                                && (sentence.contains(item.quote.as_str())
                                    || item.quote.contains(sentence))
                        });
                        if !guarded {
                            targets.push(Protected {
                                unit,
                                field: field.clone(),
                                quote: sentence.to_owned(),
                                released: false,
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
/// `(None, unchanged)` when the confined repair changes nothing: it fixed nothing the
/// findings name, which waits for the user like an unusable answer.
pub(super) fn confine_review(
    previous: &Value,
    repaired: &str,
    sent: &[ReviewFinding],
    protected: &[Protected],
) -> Result<(Option<String>, Vec<FindingKey>)> {
    let mut lesson: Value = crate::generation::parse_json(repaired)?;
    let blocks = blocks_in(previous);
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
        let guarded = in_unit(protected, unit);
        let targets = targets_of(before, (unit, unit < blocks), &here, &guarded);
        let named = |item: &Protected| {
            item.released
                && targets.iter().any(|target| {
                    target.field == item.field
                        && (target.quote.contains(item.quote.as_str())
                            || item.quote.contains(target.quote.as_str()))
                })
        };
        let kept: Vec<Protected> = guarded
            .iter()
            .filter(|item| !named(item))
            .cloned()
            .collect();
        *after = confine(before, after, &targets, &kept)?.section;
    }
    let unchanged: Vec<FindingKey> = sent
        .iter()
        .filter(|finding| match unit_of(previous, finding) {
            Some(unit) => section_at(previous, unit) == section_at(&lesson, unit),
            None => true,
        })
        .map(ReviewFinding::key)
        .collect();
    if lesson == *previous {
        Ok((None, sent.iter().map(ReviewFinding::key).collect()))
    } else {
        Ok((Some(lesson.to_string()), unchanged))
    }
}
