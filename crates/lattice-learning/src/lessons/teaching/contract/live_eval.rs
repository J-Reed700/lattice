//! Live evaluation entry points for `contract::rule_on_reports` and
//! `contract::confirm`'s three confirming reads (`ConfirmPlanned`, `ConfirmUse`,
//! `Confirm<Conflict>`). Compiled only for tests and `test-support`.
#![allow(clippy::indexing_slicing)]

use super::confirm::{self, Read, Verdicts};
use super::reading::{Clash, Hit};
use super::sentence::{self, Lesson};
use super::{parts_of, rule_on_reports, Item, Located, Part};
use crate::review_findings::{FindingSite, ReviewFinding};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::shown::Kind;
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

fn find_sentence_hit<'a>(parts: &'a [Part], part_index: usize, quote: &str) -> Result<Hit<'a>> {
    let lesson = Lesson::of(parts, parts.len(), |_| false);
    let texts = lesson.texts();
    let needle = quote.trim();
    for (whose, units) in texts.iter() {
        let sentence::Whose::Lesson { part, text } = whose else {
            continue;
        };
        if *part != part_index {
            continue;
        }
        for unit in units.units() {
            let words = unit.text();
            if words.contains(needle) || needle.contains(words.trim()) {
                let span = sentence::span_of(&texts, unit.reference())
                    .ok_or_else(|| invalid(format!("Missing span for quote {needle:?}")))?;
                return Ok(Hit {
                    order: *part,
                    part: parts
                        .get(*part)
                        .ok_or_else(|| invalid(format!("Unknown part index {part_index}")))?,
                    found: Located {
                        text: *text,
                        start: span.start,
                        end: span.end,
                        exact: words.to_owned(),
                    },
                });
            }
        }
    }
    Err(invalid(format!(
        "Could not locate quote {needle:?} in part index {part_index}"
    )))
}

fn parse_site(raw: Option<&Value>) -> Result<Option<FindingSite>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    let text = raw
        .as_str()
        .ok_or_else(|| invalid(format!("Invalid finding site {raw}")))?;
    if text == "lesson" {
        return Ok(Some(FindingSite::Lesson));
    }
    if let Some(index) = text.strip_prefix("section-") {
        let index: usize = index
            .parse()
            .map_err(|_| invalid(format!("Invalid section site {text}")))?;
        return Ok(Some(FindingSite::Section(index)));
    }
    if let Some(index) = text.strip_prefix("question-") {
        let index: usize = index
            .parse()
            .map_err(|_| invalid(format!("Invalid question site {text}")))?;
        return Ok(Some(FindingSite::Question(index)));
    }
    Err(invalid(format!("Unknown finding site {text}")))
}

/// Runs `contract::rule_on_reports` on a live eval fixture case.
pub async fn live_rule_on_reports(llm: &dyn LLMPort, case: &Value) -> Result<Value> {
    let authoring = case
        .get("authoringContext")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let candidate = case
        .get("candidate")
        .ok_or_else(|| invalid("Missing candidate in contract reports case"))?;
    let reported_raw = case
        .get("reported")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Missing reported array in contract reports case"))?;
    let mut findings = Vec::with_capacity(reported_raw.len());
    for (index, item) in reported_raw.iter().enumerate() {
        let site = parse_site(item.get("site"))?;
        let note = item
            .get("note")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Missing note in reported finding"))?;
        let mut finding = ReviewFinding::reviewer(
            u32::try_from(index).unwrap_or(u32::MAX),
            site,
            note.to_owned(),
        );
        finding.passage = item
            .get("passage")
            .and_then(Value::as_str)
            .map(str::to_owned);
        findings.push(finding);
    }
    let refs: Vec<&ReviewFinding> = findings.iter().collect();
    let ruled = rule_on_reports(llm, &authoring, candidate, &[], &refs).await?;
    Ok(json!({
        "outside": ruled.outside,
        "findings": ruled.findings.iter().map(|finding| json!({
            "kind": format!("{:?}", finding.kind),
            "site": finding.site.map(|site| site.id_text()),
            "passage": finding.passage,
            "note": finding.note,
        })).collect::<Vec<_>>(),
    }))
}

/// Runs `confirm::read_planned` directly on a single planned-item eval case.
pub async fn live_confirm_planned(llm: &dyn LLMPort, case: &Value) -> Result<Value> {
    let candidate = case
        .get("candidate")
        .ok_or_else(|| invalid("Missing candidate in confirm planned case"))?;
    let parts = parts_of(candidate);
    let kind: &'static str = match case.get("kind").and_then(Value::as_str) {
        Some("idea") => "idea",
        Some("term") => "term",
        Some("action") => "action",
        Some("contrast") => "contrast",
        other => return Err(invalid(format!("Unknown planned item kind {other:?}"))),
    };
    let text = case
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing text in confirm planned case"))?;
    let item = Item {
        id: format!("{kind}-0"),
        kind,
        text: text.to_owned(),
    };
    let verdict = confirm::read_planned(llm, &item, &parts).await?;
    Ok(match verdict {
        Verdicts::Taught(hits) => json!({
            "taught": true,
            "hitCount": hits.len(),
            "belongs": Value::Null,
        }),
        Verdicts::Belongs(part) => json!({
            "taught": false,
            "hitCount": 0,
            "belongs": part.map(|part| part.id.as_str()),
        }),
    })
}

/// Runs `confirm::read_use` directly on a single used-before-taught eval case.
pub async fn live_confirm_use(llm: &dyn LLMPort, case: &Value) -> Result<Value> {
    let candidate = case
        .get("candidate")
        .ok_or_else(|| invalid("Missing candidate in confirm use case"))?;
    let parts = parts_of(candidate);
    let what = case
        .get("what")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing what in confirm use case"))?;
    let part_index = case
        .get("partIndex")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| invalid("Missing partIndex in confirm use case"))?;
    let quote = case
        .get("quote")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing quote in confirm use case"))?;
    let hit = find_sentence_hit(&parts, part_index, quote)?;
    let prior = case.get("priorKnowledge").filter(|value| !value.is_null());
    let earlier: Vec<Value> = case
        .get("earlierLessons")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let outcome = confirm::read_use(llm, what, &hit, &parts, prior, &earlier).await?;
    let label = match outcome {
        Read::NotAsked => "not_asked",
        Read::Taught => "taught",
        Read::Stands => "stands",
    };
    Ok(json!({ "outcome": label }))
}

/// Runs `confirm::read_clash` directly on a single contradiction eval case.
pub async fn live_confirm_clash(llm: &dyn LLMPort, case: &Value) -> Result<Value> {
    let candidate = case
        .get("candidate")
        .ok_or_else(|| invalid("Missing candidate in confirm clash case"))?;
    let parts = parts_of(candidate);
    let first_part = case
        .get("firstPartIndex")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| invalid("Missing firstPartIndex in confirm clash case"))?;
    let first_quote = case
        .get("firstQuote")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing firstQuote in confirm clash case"))?;
    let second_part = case
        .get("secondPartIndex")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| invalid("Missing secondPartIndex in confirm clash case"))?;
    let second_quote = case
        .get("secondQuote")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Missing secondQuote in confirm clash case"))?;
    let clash = Clash {
        first: find_sentence_hit(&parts, first_part, first_quote)?,
        second: find_sentence_hit(&parts, second_part, second_quote)?,
    };
    let upheld = confirm::read_clash(llm, clash, &parts).await?;
    Ok(match upheld {
        Some(upheld) => json!({ "contradict": true, "case": upheld.case }),
        None => json!({ "contradict": false, "case": "" }),
    })
}

/// Offline sanity check that a `uses` or `clashes` case's quotes locate inside `candidate`.
pub fn validate_confirm_quotes(suite: &Value) -> Result<()> {
    for case in suite["uses"].as_array().into_iter().flatten() {
        let parts = parts_of(&case["candidate"]);
        let part_index = case["partIndex"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| invalid("Missing partIndex"))?;
        let quote = case["quote"]
            .as_str()
            .ok_or_else(|| invalid("Missing quote"))?;
        find_sentence_hit(&parts, part_index, quote)?;
    }
    for case in suite["clashes"].as_array().into_iter().flatten() {
        let parts = parts_of(&case["candidate"]);
        let first_part = case["firstPartIndex"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| invalid("Missing firstPartIndex"))?;
        let first_quote = case["firstQuote"]
            .as_str()
            .ok_or_else(|| invalid("Missing firstQuote"))?;
        let second_part = case["secondPartIndex"]
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| invalid("Missing secondPartIndex"))?;
        let second_quote = case["secondQuote"]
            .as_str()
            .ok_or_else(|| invalid("Missing secondQuote"))?;
        find_sentence_hit(&parts, first_part, first_quote)?;
        find_sentence_hit(&parts, second_part, second_quote)?;
    }
    Ok(())
}
