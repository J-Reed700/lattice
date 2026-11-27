use super::{FindingKind, FindingSite, ReviewFinding, Unit};
use lattice_core::domain::learning::claim_review::text_sha256;
use lattice_core::shared::text::same_text;
use lattice_core::shared::text::segmentation::sentences;
use serde_json::Value;
use std::collections::BTreeSet;

/// The part of the candidate a site names.
pub(crate) fn part_at(candidate: &Value, site: FindingSite) -> Option<&Value> {
    match site.pointer() {
        None => Some(candidate),
        Some(pointer) => candidate.pointer(&pointer),
    }
}

/// The digest of the text at `site`, which a "keep as written" is bound to.
pub(crate) fn site_sha256(candidate: &Value, site: FindingSite) -> String {
    text_sha256(
        &part_at(candidate, site)
            .map(Value::to_string)
            .unwrap_or_default(),
    )
}

/// Every string of `value` at any depth, with its path relative to `value`
/// ("body", "rubric/0/description"; "" for a string `value`).
pub(crate) fn leaves(value: &Value) -> Vec<(String, &str)> {
    fn walk<'a>(value: &'a Value, path: &str, out: &mut Vec<(String, &'a str)>) {
        let child = |name: &str| {
            if path.is_empty() {
                name.to_owned()
            } else {
                format!("{path}/{name}")
            }
        };
        match value {
            Value::String(text) => out.push((path.to_owned(), text)),
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &child(&index.to_string()), out);
                }
            }
            Value::Object(fields) => {
                for (name, field) in fields {
                    walk(field, &child(name), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, "", &mut out);
    out
}

/// The whole sentences of the finding's part that its passage or sentence
/// lies in, wherever they occur there.
pub(crate) fn sentences_of(candidate: &Value, finding: &ReviewFinding) -> Vec<String> {
    let Some(part) = part_at(candidate, finding.site.unwrap_or(FindingSite::Lesson)) else {
        return Vec::new();
    };
    let needles = [finding.passage.as_deref(), finding.sentence.as_deref()];
    let mut found: Vec<String> = Vec::new();
    for (_, leaf) in leaves(part) {
        for needle in needles.into_iter().flatten() {
            for span in same_text::find_all(leaf, needle) {
                for (from, to) in same_text::sentences_over(leaf, span) {
                    if let Some(sentence) = leaf.get(from..to) {
                        if !found.iter().any(|known| known == sentence) {
                            found.push(sentence.to_owned());
                        }
                    }
                }
            }
        }
    }
    found
}

/// The lesson bytes of the first string of the part `site` names (the whole
/// candidate for the lesson) that holds `quote` as a copy.
#[cfg(test)]
pub(crate) fn part_quote<'c>(
    candidate: &'c Value,
    site: FindingSite,
    quote: &str,
) -> Option<&'c str> {
    leaves(part_at(candidate, site)?)
        .into_iter()
        .find_map(|(_, leaf)| {
            let (start, end) = same_text::find(leaf, quote)?;
            leaf.get(start..end)
        })
}

/// The sentences of every string of `value` that are not blank once folded.
pub(crate) fn sentences_in(value: &Value) -> Vec<String> {
    leaves(value)
        .into_iter()
        .flat_map(|(_, leaf)| {
            sentences(leaf)
                .into_iter()
                .filter_map(move |(from, to)| leaf.get(from..to))
        })
        .filter(|sentence| !same_text::fold(sentence).is_empty())
        .map(str::to_owned)
        .collect()
}

/// What a lesson holds outside its sections and questions.
fn lesson_rest(lesson: &Value) -> Value {
    let mut rest = lesson.clone();
    if let Some(fields) = rest.as_object_mut() {
        fields.remove("blocks");
        fields.remove("questions");
    }
    rest
}

/// For each part a repair changed, the sentences it holds that the lesson did
/// not have before, anywhere: a sentence that only moved is not new.
pub(crate) fn new_sentences(previous: &Value, repaired: &Value) -> Vec<(FindingSite, Vec<String>)> {
    let known: BTreeSet<String> = sentences_in(previous)
        .iter()
        .map(|sentence| super::subject_of(sentence))
        .collect();
    let mut changed = Vec::new();
    let mut visit = |site: FindingSite, before: Option<&Value>, after: &Value| {
        if before == Some(after) {
            return;
        }
        let fresh: Vec<String> = sentences_in(after)
            .into_iter()
            .filter(|sentence| !known.contains(&super::subject_of(sentence)))
            .collect();
        if !fresh.is_empty() {
            changed.push((site, fresh));
        }
    };
    let items = |field: &str| {
        repaired
            .get(field)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    for (index, block) in items("blocks").iter().enumerate() {
        let site = FindingSite::Section(index);
        visit(site, part_at(previous, site), block);
    }
    for (index, question) in items("questions").iter().enumerate() {
        let site = FindingSite::Question(index);
        visit(site, part_at(previous, site), question);
    }
    visit(
        FindingSite::Lesson,
        Some(&lesson_rest(previous)),
        &lesson_rest(repaired),
    );
    changed
}

pub(crate) fn unit(candidate: &Value, finding: &ReviewFinding) -> Unit {
    let text = |value: Option<&Value>, field: &str| {
        value
            .and_then(|value| value.get(field))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
    };
    let site = finding.site.unwrap_or(FindingSite::Lesson);
    let part = part_at(candidate, site);
    let title = match site {
        FindingSite::Section(_) => site.titled(&text(part, "title").unwrap_or_default()),
        FindingSite::Lesson | FindingSite::Question(_) => site.label(),
    };
    let passage = match site {
        FindingSite::Question(_) if finding.kind != FindingKind::Quotation => text(part, "prompt"),
        _ => None,
    };
    Unit {
        title,
        sha256: site_sha256(candidate, site),
        passage,
    }
}
