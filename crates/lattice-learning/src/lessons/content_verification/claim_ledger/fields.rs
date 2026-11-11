//! Where text is in a lesson draft: a unit's fields and their text, and
//! the place of a claim's quote among them (claim sources design §18.3).

use lattice_core::domain::learning::claim_review::{ClaimLocation, UnitKind};
use lattice_core::shared::text::{claim_sentence, sentences};
use serde_json::Value;

/// Where a lesson field is in the lesson, as a JSON pointer: `body`,
/// `rubric/2/description`, `options/1` of a unit.
pub(crate) fn field_pointer(candidate: &Value, unit: usize, field: &str) -> Option<String> {
    let blocks = candidate.get("blocks")?.as_array()?.len();
    Some(if unit < blocks {
        format!("/blocks/{unit}/{field}")
    } else {
        format!("/questions/{}/{field}", unit - blocks)
    })
}

/// The text of a lesson field: `body`, `rubric/2/description`, `options/1`.
pub(crate) fn field_text(candidate: &Value, unit: usize, field: &str) -> Option<String> {
    candidate
        .pointer(&field_pointer(candidate, unit, field)?)?
        .as_str()
        .map(str::to_owned)
}

/// The fields of a unit a claim can come from, in the order searched.
pub(crate) fn unit_fields(item: &Value, block: bool) -> Vec<String> {
    let mut fields: Vec<String> = Vec::new();
    let names: &[&str] = if block {
        &["body", "title", "rubric"]
    } else {
        &["prompt", "options", "explanation", "rationales"]
    };
    for name in names {
        match item.get(*name) {
            Some(Value::String(_)) => fields.push((*name).to_owned()),
            Some(Value::Array(items)) => {
                for (index, entry) in items.iter().enumerate() {
                    match entry {
                        Value::String(_) => fields.push(format!("{name}/{index}")),
                        Value::Object(object) => fields.extend(
                            object
                                .iter()
                                .filter(|(_, value)| value.is_string())
                                .map(|(key, _)| format!("{name}/{index}/{key}")),
                        ),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    fields
}

/// Each unit of a lesson in the report's order (sections, then questions)
/// as `(title, text)`: a section's title or a question's prompt, and every
/// other field a claim can come from, joined. What the Verifier reads.
pub(crate) fn unit_texts(candidate: &Value) -> Vec<(String, String)> {
    let mut units = Vec::new();
    for (list, title_field, block) in [("blocks", "title", true), ("questions", "prompt", false)] {
        let items = candidate.get(list).and_then(Value::as_array);
        for item in items.into_iter().flatten() {
            let title = item
                .get(title_field)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let text = unit_fields(item, block)
                .iter()
                .filter(|field| field.as_str() != title_field)
                .filter_map(|field| item.pointer(&format!("/{field}")).and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n\n");
            units.push((title, text));
        }
    }
    units
}

/// Where `quote` is in unit `unit`: the first field holding it, at the
/// earliest place it stands there. A quote of one sentence is the claim's
/// sentence as it was named and stays. With the claim's `statement`, a quote
/// of several sentences (a claim an older pass stored) narrows to the one
/// sentence the statement restates (claim sources design §18.3), its span the
/// sentence's own place; a statement the locator cannot place, and a code
/// example (`None`, its quote is the code), keep the quote.
pub(crate) fn locate(
    candidate: &Value,
    unit: usize,
    quote: &str,
    statement: Option<&str>,
) -> ClaimLocation {
    let blocks = candidate
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let block = unit < blocks;
    let item = if block {
        candidate.pointer(&format!("/blocks/{unit}"))
    } else {
        candidate.pointer(&format!("/questions/{}", unit - blocks))
    };
    let fields = item
        .map(|item| unit_fields(item, block))
        .unwrap_or_default();
    let found = fields.iter().find_map(|field| {
        field_text(candidate, unit, field)
            .filter(|text| text.contains(quote))
            .map(|text| (field.clone(), text))
    });
    let (field, text) = found.unwrap_or_else(|| {
        let field = if block { "body" } else { "prompt" };
        (
            field.to_owned(),
            field_text(candidate, unit, field).unwrap_or_default(),
        )
    });
    let kind = match (block, field.as_str()) {
        (true, _) => UnitKind::Block,
        (false, field) if field == "explanation" || field.starts_with("rationales/") => {
            UnitKind::AnswerKey
        }
        (false, _) => UnitKind::Question,
    };
    let sentence = statement
        .filter(|_| sentences(quote).len() > 1)
        .and_then(|statement| {
            let within = text
                .find(quote)
                .map_or((0, text.len()), |at| (at, at + quote.len()));
            let (start, end) = claim_sentence(&text, within, statement)?;
            Some((text.get(start..end)?.to_owned(), (start, end)))
        });
    match sentence {
        Some((sentence, span)) => ClaimLocation {
            unit_index: unit,
            unit_kind: kind,
            field,
            quote: sentence,
            span: Some(span),
        },
        None => ClaimLocation {
            span: text
                .find(quote)
                .filter(|_| !quote.is_empty())
                .map(|at| (at, at + quote.len())),
            unit_index: unit,
            unit_kind: kind,
            field,
            quote: quote.to_owned(),
        },
    }
}
