//! The teaching review after a claim repair or a user edit leaves the
//! sentences claims stand on as they are (claim sources design §10.3): a
//! finding about a protected sentence waits for the user, except one about the
//! sentence of a machine's claim with rewrites left, which is repaired. The
//! review's repair changes only the sentences its findings name, every other
//! one reverted and every protected one byte-identical. A finding that the
//! lesson lacks teaching is never held: its correction adds sentences beside
//! the protected ones, which stay as they are.

use crate::lessons::content_verification::{confine, unit_fields, Protected};
use crate::review_findings::{leaves, FindingId, FindingSite, ReviewFinding};
use lattice_core::shared::error::Result;
use serde_json::Value;
use std::collections::BTreeMap;

/// How many units the lesson has: blocks, then questions.
fn units_in(lesson: &Value) -> usize {
    let count = |name: &str| {
        lesson
            .get(name)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    count("blocks") + count("questions")
}

fn blocks_in(lesson: &Value) -> usize {
    lesson
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

/// The unit index of a part of the lesson candidate.
fn unit_of(candidate: &Value, site: FindingSite) -> Option<usize> {
    site.unit(blocks_in(candidate))
}

/// The section at unit `unit` in `candidate`.
fn section_at(candidate: &Value, unit: usize) -> Option<&Value> {
    candidate.pointer(&FindingSite::in_candidate(candidate, unit).pointer()?)
}

/// The protected items for a specific unit.
fn in_unit(protected: &[Protected], unit: usize) -> Vec<Protected> {
    protected
        .iter()
        .filter(|item| item.unit == unit)
        .cloned()
        .collect()
}

/// Whether a passage overlaps any protected quote in its unit.
fn overlapping_quote(passage: Option<&str>, guarded: &[Protected]) -> bool {
    passage.is_some_and(|passage| {
        let passage = passage.trim();
        !passage.is_empty()
            && guarded
                .iter()
                .any(|item| passage.contains(item.quote.as_str()) || item.quote.contains(passage))
    })
}

/// Whether `passage`, in the part at `site` of `candidate`, overlaps a
/// protected sentence.
pub(super) fn guards(
    candidate: &Value,
    site: FindingSite,
    passage: &str,
    protected: &[Protected],
) -> bool {
    unit_of(candidate, site)
        .is_some_and(|unit| overlapping_quote(Some(passage), &in_unit(protected, unit)))
}

/// The findings about a protected sentence: their passage holds one, or
/// lies inside one.
pub(super) fn on_protected(
    candidate: &Value,
    findings: &[ReviewFinding],
    protected: &[Protected],
) -> Vec<FindingId> {
    if protected.is_empty() {
        return Vec::new();
    }
    let mut held = Vec::new();
    for finding in findings.iter().filter(|finding| !finding.adds()) {
        if finding.site.unwrap_or(FindingSite::Lesson) == FindingSite::Lesson {
            held.push(finding.id());
            continue;
        }
        let Some(unit) = unit_of(candidate, finding.site.unwrap_or(FindingSite::Lesson)) else {
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
        let fixed: Vec<Protected> = guarded
            .iter()
            .filter(|item| !item.released)
            .cloned()
            .collect();
        let text_fields = unit_fields(section, block);
        let editable = targets_of(section, (unit, block), &[finding], &fixed)
            .into_iter()
            .any(|target| text_fields.contains(&target.field));
        if overlapping_quote(finding.passage.as_deref(), &fixed) || !editable {
            held.push(finding.id());
        }
    }
    held
}

/// Every sentence of the unit's teaching text that no protected sentence
/// overlaps.
fn unprotected_sentences(
    section: &Value,
    (unit, block): (usize, bool),
    protected: &[Protected],
    targets: &mut Vec<Protected>,
) {
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
                    && (sentence.contains(item.quote.as_str()) || item.quote.contains(sentence))
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

/// `value` and everything under it as targets, a string by its whole text.
fn whole_value(value: &Value, path: String, unit: usize, targets: &mut Vec<Protected>) {
    let child = |key: &str| format!("{path}/{key}");
    targets.push(Protected {
        unit,
        field: path.clone(),
        quote: value.as_str().unwrap_or_default().to_owned(),
        released: false,
    });
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                whole_value(value, child(key), unit, targets);
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                whole_value(value, child(&index.to_string()), unit, targets);
            }
        }
        _ => {}
    }
}

/// Whether `finding` is about the unit: its site is the unit, or it has none
/// and its passage lies in the unit.
fn about_whole_unit(finding: &ReviewFinding, fields: &[(String, &str)]) -> bool {
    match finding.site {
        Some(FindingSite::Section(_) | FindingSite::Question(_)) => true,
        Some(FindingSite::Lesson) | None => finding
            .passage
            .as_deref()
            .map(str::trim)
            .filter(|passage| !passage.is_empty())
            .is_some_and(|passage| fields.iter().any(|(_, text)| text.contains(passage))),
    }
}

/// The sentences `sent` names in `section` (`unit`, and whether it is a
/// block): each finding's passage where any field holds it; for a finding
/// with no passage, every sentence of the unit's teaching text that no
/// protected sentence overlaps (`protected` holds the sentences that are not
/// released); for a finding that adds teaching, every sentence of it, protected
/// or not, since its correction adds sentences and `confine` keeps the
/// protected ones as they are. A finding about the unit names all of it: its
/// sentences, its citations, and for a question its key.
fn targets_of(
    section: &Value,
    (unit, block): (usize, bool),
    sent: &[&ReviewFinding],
    protected: &[Protected],
) -> Vec<Protected> {
    let fields = leaves(section);
    let mut targets = Vec::new();
    for finding in sent {
        let open: &[Protected] = if finding.adds() { &[] } else { protected };
        if about_whole_unit(finding, &fields) {
            unprotected_sentences(section, (unit, block), open, &mut targets);
            if let (false, Some(key)) = (block, section.get("correctIndex")) {
                whole_value(key, "correctIndex".into(), unit, &mut targets);
            }
            match section.get("citations") {
                Some(citations) => whole_value(citations, "citations".into(), unit, &mut targets),
                None => targets.push(Protected {
                    unit,
                    field: "citations".into(),
                    quote: String::new(),
                    released: false,
                }),
            }
            continue;
        }
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
            _ => unprotected_sentences(section, (unit, block), open, &mut targets),
        }
    }
    targets
}

/// `repaired` when no change of it falls outside what its findings may
/// change, else one reason for each change that does. `touched` gives, for
/// each part (`/blocks/N`, `/questions/N`), the findings whose changes touched
/// it.
pub(super) fn confine_review(
    previous: &Value,
    repaired: &Value,
    touched: &BTreeMap<String, Vec<&ReviewFinding>>,
    protected: &[Protected],
) -> Result<std::result::Result<Value, Vec<String>>> {
    let mut lesson = repaired.clone();
    let mut reasons: Vec<String> = Vec::new();
    let blocks = blocks_in(previous);
    for unit in 0..units_in(previous) {
        let Some(pointer) = FindingSite::of_unit(unit, blocks).pointer() else {
            continue;
        };
        let (Some(before), Some(after)) =
            (previous.pointer(&pointer), lesson.pointer_mut(&pointer))
        else {
            continue;
        };
        let here: &[&ReviewFinding] = touched.get(&pointer).map(Vec::as_slice).unwrap_or_default();
        let guarded = in_unit(protected, unit);
        let fixed: Vec<Protected> = guarded
            .iter()
            .filter(|item| !item.released)
            .cloned()
            .collect();
        let targets = targets_of(before, (unit, unit < blocks), here, &fixed);
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
        let confined = confine(before, after, &targets, &kept)?;
        for reason in confined.reasons() {
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        *after = confined.section;
    }
    Ok(if reasons.is_empty() {
        Ok(lesson)
    } else {
        Err(reasons)
    })
}

#[cfg(test)]
#[path = "protected_tests.rs"]
mod tests;
