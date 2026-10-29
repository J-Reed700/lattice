//! A strict check's answer as fields a schema enforces, in the order the
//! decision needs: what the claim says, what the passages say, the difference
//! between them, the passage, then the verdict. The fields are read back into
//! the three labelled lines, so every check after the answer stays the same.
use super::*;
use serde_json::{json, Value};

/// How a strict located check writes its answer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VerdictForm {
    /// `Reason:`, `Source passage:` and `Verdict:` lines.
    #[default]
    Lines,
    /// The comparison split into fields, the difference stated before the
    /// verdict.
    Fields,
}

/// The format sentence of a located system prompt, as the lines form has it.
pub(super) const LINES_FORMAT: &str = "Start with exactly one label: supported, contradicted, or unsupported. Then write two short labeled lines: Reason: a factual comparison using only the passages; Source passage: the application-assigned passage-N identifier that supports that comparison, or none.";

/// What the fields form says in its place.
pub(super) const FIELDS_FORMAT: &str = "Answer in the response fields, in order: claimSays, what the claim asserts; passageSays, what the passages state about it; difference, every way the claim goes beyond, narrows or conflicts with the passages, or none; sourcePassage, the application-assigned passage-N identifier that supports the comparison, or none; verdict, supported, contradicted or unsupported, decided from difference.";

/// What a reasoned located check is asked, in place of its three lines.
pub(super) const FIELDS_ASK: &str = "Compare every factual part with the passages before deciding, in the response fields in order: claimSays, passageSays, difference, sourcePassage (one existing passage-N identifier, or none), verdict (supported, contradicted, or unsupported). The verdict follows from difference: a difference that names a missing required fact is unsupported, one that names an explicitly incompatible fact is contradicted, and only none is supported. Do not copy a quotation.";

/// How a batch's entries are described, in place of the three-line response.
pub(super) const FIELDS_BATCH: &str = "Return JSON with checks containing exactly one entry per supplied claim ID. Fill in each entry's fields in order: id; claimSays, what the claim asserts; passageSays, what its permitted passages state about it; difference, every way the claim goes beyond, narrows or conflicts with them, or none; sourcePassage, a permitted passage-N identifier, or none; verdict, supported, contradicted or unsupported, decided from difference: a difference that names a missing or incompatible fact is never supported. Do not combine decisions, omit claims, invent IDs or copy quotations.";

const VERDICTS: [&str; 3] = ["supported", "contradicted", "unsupported"];

/// The properties of one answer, `sources` naming the passages it may cite.
fn properties(sources: Vec<String>) -> Value {
    json!({
        "claimSays": {"type": "string", "minLength": 1},
        "passageSays": {"type": "string", "minLength": 1},
        "difference": {"type": "string", "minLength": 1},
        "sourcePassage": {"type": "string", "enum": sources},
        "verdict": {"type": "string", "enum": VERDICTS},
    })
}

fn source_choices(passages: usize) -> Vec<String> {
    std::iter::once("none".to_owned())
        .chain((0..passages).map(|index| format!("passage-{index}")))
        .collect()
}

const FIELD_NAMES: [&str; 5] = ["claimSays", "passageSays", "difference", "sourcePassage", "verdict"];

/// The schema of a single check's answer over `passages` passages.
pub(super) fn single_schema(passages: usize) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": FIELD_NAMES,
        "properties": properties(source_choices(passages)),
    })
}

/// The schema of a batch's answer: one entry per claim in `ids`, citing any of
/// the bank's `passages`; which of them a claim may use is checked after.
pub(super) fn batch_schema(ids: &[String], passages: usize) -> Value {
    let mut entry = properties(source_choices(passages));
    let mut ordered = serde_json::Map::new();
    ordered.insert("id".into(), json!({"type": "string", "enum": ids}));
    if let Some(fields) = entry.as_object_mut() {
        ordered.append(fields);
    }
    let required: Vec<&str> = std::iter::once("id").chain(FIELD_NAMES).collect();
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["checks"],
        "properties": {"checks": {
            "type": "array",
            "minItems": ids.len(),
            "maxItems": ids.len(),
            "items": {
                "type": "object",
                "additionalProperties": false,
                "required": required,
                "properties": Value::Object(ordered),
            },
        }},
    })
}

/// One answer's fields as the three labelled lines the checks read.
pub(super) fn as_lines(answer: &Value) -> Option<String> {
    let field = |name: &str| {
        answer
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let reason = format!(
        "The claim says {} The passages say {} Difference: {}",
        sentence(field("claimSays")?),
        sentence(field("passageSays")?),
        field("difference")?
    );
    Some(format!(
        "Reason: {}\nSource passage: {}\nVerdict: {}",
        reason.replace('\n', " "),
        field("sourcePassage")?,
        field("verdict")?
    ))
}

/// A single check's reply read as lines, when it is the fields' JSON.
pub(super) fn reply_as_lines(text: &str) -> Option<String> {
    as_lines(&serde_json::from_str(text.trim()).ok()?)
}

fn sentence(text: &str) -> String {
    if text.ends_with(['.', '!', '?']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_s_fields_read_as_its_three_lines() {
        let reply = r#"{"claimSays":"the SQL sent is redacted","passageSays":"EF10 sends the same SQL and logs a redacted form.","difference":"the passages redact only the log","sourcePassage":"passage-0","verdict":"contradicted"}"#;
        let lines = reply_as_lines(reply).unwrap();
        assert_eq!(final_verdict(&lines), Some(ClaimVerdict::Contradicted));
        assert_eq!(
            labeled_value(&lines, &["Reason"]).as_deref(),
            Some("The claim says the SQL sent is redacted. The passages say EF10 sends the same SQL and logs a redacted form. Difference: the passages redact only the log")
        );
        assert_eq!(
            located_quote(&lines, &["EF10 sends the same SQL.".to_string()]).as_deref(),
            Some("EF10 sends the same SQL.")
        );
    }

    #[test]
    fn a_reply_that_is_not_the_fields_is_left_to_the_lines_parser() {
        assert!(reply_as_lines("Reason: x\nSource passage: none\nVerdict: unsupported").is_none());
        assert!(reply_as_lines(r#"{"verdict":"supported"}"#).is_none());
    }

    #[test]
    fn the_schemas_ask_for_the_difference_before_the_verdict() {
        let single = single_schema(2);
        let keys: Vec<&str> = single["properties"].as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, FIELD_NAMES);
        assert_eq!(
            single["properties"]["sourcePassage"]["enum"],
            json!(["none", "passage-0", "passage-1"])
        );
        let batch = batch_schema(&["claim-0".into()], 1);
        let entry = &batch["properties"]["checks"]["items"]["properties"];
        let keys: Vec<&str> = entry.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys[0], "id");
        assert_eq!(keys[1..], FIELD_NAMES);
    }

    /// The fields form replaces sentences of the lines form's prompts; a
    /// reworded prompt must not leave the replacement doing nothing.
    #[test]
    fn the_sentences_the_fields_form_replaces_are_in_the_prompts() {
        let located = CLAIM_CHECK_SYSTEM.replace(
            "Source quote: an exact contiguous quote from a passage that supports that comparison, or none.",
            "Source passage: the application-assigned passage-N identifier that supports that comparison, or none.",
        );
        assert!(located.contains(LINES_FORMAT));
    }
}
