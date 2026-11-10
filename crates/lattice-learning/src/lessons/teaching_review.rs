//! Bind review findings to app-owned passages, not model-transcribed quotations.
use crate::lessons::structured_call::{CallKind, ReasonFirst};
use crate::lessons::writer_excerpts::{self, WriterExcerpts};
use crate::review_findings::{FindingSite, ReviewFinding};
use crate::{generation, outline_progress::OutlineProgress};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

pub(super) const POLICY: &str = "section-review-v4";

/// The room the review's answer is given.
const REVIEW_ANSWER_TOKENS: usize = 6000;

/// What the review does with a blank finding after the response correction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LengthRule {
    /// The review fails.
    Fail,
    /// A blank finding is dropped: no teaching-review finding fails a lesson
    /// job.
    DropBlank,
}

/// What the reviewer works through in prose before its findings are written
/// to the response schema, where each section's verdict would otherwise be
/// chosen as it is written.
const REVIEW_DRAFT_BRIEF: &str = "Go through the candidate part by part: for a lesson, every section listed in sectionIndicesToReview and every question; for an outline, every module. Solve each exercise and question yourself and check each worked example against its stated premises and result. For each part say what you checked, what you found and whether it has a material defect.";

#[cfg(test)]
pub(super) fn fixture_checks(checks: Vec<Value>) -> Value {
    Value::Array(
        checks
            .into_iter()
            .map(|mut check| {
                if let Some(fields) = check.as_object_mut() {
                    let index = fields.remove("index").unwrap_or(Value::Null);
                    fields.insert("sectionId".into(), json!(format!("section-{index}")));
                }
                check
            })
            .collect(),
    )
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BlockCheck {
    index: usize,
    passage_id: String,
    finding: String,
    has_defect: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Review {
    issues: Vec<String>,
    #[serde(default, rename = "blockChecks")]
    block_checks: Vec<Value>,
}

// Preserve every byte, including code indentation, newlines and Unicode. IDs
// belong only to this candidate; nothing is normalized or fuzzy-matched.
fn review_candidate(candidate: &Value) -> (Value, Vec<Vec<String>>) {
    let mut candidate = candidate.clone();
    let mut ids = Vec::new();
    if let Some(blocks) = candidate.get_mut("blocks").and_then(Value::as_array_mut) {
        for (index, block) in blocks.iter_mut().enumerate() {
            let body = block
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let passages: Vec<Value> = passages::split(body)
                .into_iter()
                .enumerate()
                .map(|(n, text)| json!({"id":format!("section-{index}-passage-{n}"),"text":text}))
                .collect();
            ids.push(
                passages
                    .iter()
                    .filter_map(|p| p.get("id").and_then(Value::as_str).map(str::to_owned))
                    .collect(),
            );
            if let Some(block) = block.as_object_mut() {
                block.remove("body");
                block.insert("index".into(), json!(index));
                block.insert("label".into(), json!(format!("Section {}", index + 1)));
                block.insert("bodyPassages".into(), json!(passages));
            }
        }
    }
    if let Some(questions) = candidate.get_mut("questions").and_then(Value::as_array_mut) {
        for (index, question) in questions.iter_mut().enumerate() {
            if let Some(question) = question.as_object_mut() {
                question.insert("label".into(), json!(format!("Question {}", index + 1)));
            }
        }
    }
    (candidate, ids)
}

/// The text of passage `id` in section `index` of the reviewed candidate.
fn passage_text(reviewed: &Value, index: usize, id: &str) -> Option<String> {
    reviewed
        .pointer(&format!("/blocks/{index}/bodyPassages"))?
        .as_array()?
        .iter()
        .find(|passage| passage.get("id").and_then(Value::as_str) == Some(id))?
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn schema(indices: &[usize], passages: &[Vec<String>]) -> Value {
    let mut required = vec!["issues"];
    let mut properties = serde_json::Map::from_iter([(
        "issues".into(),
        json!({"type":"array","items":{"type":"string","minLength":1}}),
    )]);
    if !passages.is_empty() {
        required.push("blockChecks");
        // Each check names an app-owned section ID and one of those
        // sections' passages; the review holds the passage to its section.
        let sections: Vec<String> = indices.iter().map(|i| format!("section-{i}")).collect();
        let cited: Vec<&str> = indices
            .iter()
            .filter_map(|index| passages.get(*index))
            .flatten()
            .map(String::as_str)
            .collect();
        let section_id = json!({"type":"string","enum":sections});
        let passage_id = if cited.is_empty() {
            json!({"type":"string"})
        } else {
            json!({"type":"string","enum":cited})
        };
        let checks = if sections.is_empty() {
            json!({"type":"array","maxItems":0,"items":{"type":"string"}})
        } else {
            json!({"type":"array","items":{"type":"object","additionalProperties":false,"required":["sectionId","passageId","finding","hasDefect"],"properties":{"sectionId":section_id,"passageId":passage_id,"finding":{"type":"string","minLength":1},"hasDefect":{"type":"boolean"}}}})
        };
        properties.insert("blockChecks".into(), checks);
    }
    json!({"type":"object","additionalProperties":false,"required":required,"properties":properties})
}

/// Correct malformed reviewer output once, retaining valid checks and defects.
/// This does not spend the author's content-repair pass or rerun answer keys.
/// An unavailable provider propagates immediately; this is not a network retry.
/// A section's finding is placed in that section; a reviewer's issue is
/// placed later, by the evidence check.
pub async fn review(
    llm: &dyn LLMPort,
    system: &str,
    prompt: Value,
    candidate: &Value,
    progress: Option<&OutlineProgress>,
    lengths: LengthRule,
) -> Result<Vec<ReviewFinding>> {
    // Save findings, including defects, before the later grounding/repair step.
    // Reusing a completed review never approves the lesson or skips grounding.
    let key = format!("teaching-review-findings-v3:{}", crate::content_verification::digest(
        &json!({"policy":POLICY,"model":llm.model_name(),"context":llm.max_context_tokens(),"system":system,"prompt":prompt,"candidate":candidate}).to_string()
    ));
    if let Some(saved) = crate::lesson_drafts::checkpoint(&key)
        .await?
        .and_then(|v| serde_json::from_value::<Vec<ReviewFinding>>(v).ok())
    {
        crate::lesson_progress::stage(
            "Reusing completed teaching review; continuing evidence checks",
        );
        return Ok(saved);
    }
    let issues = review_sections(llm, system, prompt, candidate, progress, lengths).await?;
    crate::lesson_drafts::record_checkpoint(&key, json!(issues)).await?;
    Ok(issues)
}

async fn review_sections(
    llm: &dyn LLMPort,
    system: &str,
    prompt: Value,
    candidate: &Value,
    progress: Option<&OutlineProgress>,
    lengths: LengthRule,
) -> Result<Vec<ReviewFinding>> {
    let (reviewed, passages) = review_candidate(candidate);
    let shown = reviewed.clone();
    let Value::Object(mut prompt) = prompt else {
        return Err(AppError::InternalError(
            "Teaching review requires an object context.".into(),
        ));
    };
    prompt.insert("candidate".into(), reviewed);
    let excerpts = prompt
        .get_mut("authoringContext")
        .and_then(|context| WriterExcerpts::take(llm, context, candidate));
    let mut accepted = BTreeMap::<usize, BlockCheck>::new();
    let mut issues: Vec<ReviewFinding> = Vec::new();
    let mut problems = Vec::new();
    // The first correction is always asked for; later ones while each answer
    // validates another section.
    for attempt in 0usize.. {
        // After the correction, a blank finding no longer fails a lesson's review.
        let settle = lengths == LengthRule::DropBlank && attempt > 0;
        let checked = accepted.len();
        let indices: Vec<_> = (0..passages.len())
            .filter(|i| !accepted.contains_key(i))
            .collect();
        prompt.insert("sectionIndicesToReview".into(), json!(indices));
        if attempt > 0 {
            crate::lesson_progress::stage(if indices.is_empty() {
                "Correcting the review's findings".into()
            } else {
                format!(
                    "Correcting the review response ({} sections need checks)",
                    indices.len()
                )
            });
            prompt.insert("reviewResponseProblems".into(), json!(problems));
            prompt.insert("retainedIssues".into(), json!(issues));
            prompt.insert(
                "acceptedBlockChecks".into(),
                json!(accepted.values().collect::<Vec<_>>()),
            );
            prompt.insert("responseCorrection".into(), json!("Correct the review response, not the lesson. Return checks only for sectionIndicesToReview. Read all passages in each requested section; select a passageId from that same section. Keep every established defect. Do not regenerate the lesson or treat a response-format error as a content defect."));
        }
        let request = writer_excerpts::Request {
            subject: "the teaching review",
            system,
            prompt: &prompt,
            context: "authoringContext",
            schema: schema(&indices, &passages),
            output_tokens: REVIEW_ANSWER_TOKENS,
        };
        // A correction repairs the response, not the judgement, so only the
        // first review reasons in prose before answering.
        let reasoning = (attempt == 0).then_some(ReasonFirst {
            kind: CallKind::Authoring,
            subject: "the teaching review",
            brief: REVIEW_DRAFT_BRIEF,
        });
        let raw = writer_excerpts::complete(
            llm,
            &request,
            excerpts.as_ref(),
            progress,
            reasoning,
            |text| Ok(text.to_owned()),
        )
        .await?;
        problems.clear();
        match generation::parse_json::<Review>(&raw) {
            Err(_) => problems.push("The previous review was incomplete. Give the issues and a check for every requested section.".into()),
            Ok(review) => {
                for issue in review.issues {
                    let blank = issue.trim().is_empty();
                    if blank && !settle {
                        problems.push("An issue must not be blank.".into());
                    } else if !blank && !issues.iter().any(|known| known.note == issue) {
                        let summary = issue.trim().to_owned();
                        issues.push(
                            ReviewFinding::reviewer(None, issue)
                                .with_summary(summary),
                        );
                    }
                }
                let mut named = Vec::new();
                for mut value in review.block_checks {
                    let section = value
                        .as_object_mut()
                        .and_then(|fields| fields.remove("sectionId"))
                        .and_then(|id| id.as_str().map(str::to_owned));
                    match section {
                        Some(section) => named.push((section, value)),
                        None => problems.push("A block check needs the sectionId of the section it checks.".into()),
                    }
                }
                let mut seen = HashSet::new();
                let repeated: HashSet<String> = named
                    .iter()
                    .filter(|(section, _)| !seen.insert(section.clone()))
                    .map(|(section, _)| section.clone())
                    .collect();
                for (section, mut value) in named {
                    let Some(index) = indices.iter().copied().find(|index| section == format!("section-{index}")) else {
                        problems.push(format!("Section ID {section} was not requested."));
                        continue;
                    };
                    if repeated.contains(&section) {
                        let message = format!("{section} was checked more than once; give each section one check.");
                        if !problems.contains(&message) {
                            problems.push(message);
                        }
                        continue;
                    }
                    // Preserve actionable defects even when another field is
                    // missing and the complete check cannot deserialize.
                    if let (Some(finding), Some(true)) = (
                        value.get("finding").and_then(Value::as_str), value.get("hasDefect").and_then(Value::as_bool),
                    ) {
                        if indices.contains(&index) && !finding.trim().is_empty() {
                            let note = format!("Section {} (candidate blocks-{index}): {}", index + 1, finding);
                            if !issues.iter().any(|known| known.note == note) {
                                let mut issue = ReviewFinding::reviewer(Some(FindingSite::Section(index)), note)
                                    .with_summary(finding.trim());
                                issue.passage = value.get("passageId").and_then(Value::as_str).and_then(|id| passage_text(&shown, index, id));
                                issues.push(issue);
                            }
                        }
                    }
                    let Some(fields) = value.as_object_mut() else {
                        problems.push(format!("Section {index} needs an object check."));
                        continue;
                    };
                    if fields.insert("index".into(), json!(index)).is_some() {
                        problems.push(format!("Section {index}: the sectionId names the section; do not supply an index field."));
                        continue;
                    }
                    let check: BlockCheck = match serde_json::from_value(value) {
                        Ok(check) => check,
                        Err(_) => { problems.push("A block check has missing, unexpected or incorrectly typed fields.".into()); continue; }
                    };
                    if !indices.contains(&check.index) {
                        problems.push(format!("Section index {} was not requested. Use the explicit zero-based index from the candidate.", check.index));
                        continue;
                    }
                    if passages.get(check.index).is_none_or(|ids| !ids.contains(&check.passage_id)) {
                        problems.push(format!("Section {}: passageId must be one of that section's bodyPassages IDs.", check.index));
                    } else if check.finding.trim().is_empty() && !settle {
                        problems.push(format!("Section {}: finding must not be blank.", check.index));
                    } else {
                        accepted.insert(check.index, check);
                    }
                }
                for index in indices {
                    if !accepted.contains_key(&index) {
                        problems.push(format!("Section {index} still needs a valid check."));
                    }
                }
            }
        }
        if problems.is_empty() {
            return Ok(issues);
        }
        tracing::warn!(attempt = attempt + 1, accepted_sections = accepted.len(), total_sections = passages.len(), problems = ?problems, "Teaching reviewer response failed validation");
        if attempt > 0 && accepted.len() == checked {
            break;
        }
    }
    let recovery = if crate::lesson_drafts::active() {
        "The lesson draft is saved. Retry will reuse it and run the checks again."
    } else {
        "Retry the review."
    };
    Err(AppError::ServiceNotAvailable(format!("The model could not return a complete teaching review after response correction: {} {recovery} Nothing was published.", problems.join(" "))))
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod passage_tests;
mod passages;
#[cfg(test)]
mod section_list_tests;

#[cfg(test)]
mod tests;
