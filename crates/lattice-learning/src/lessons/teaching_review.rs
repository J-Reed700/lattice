//! Bind review findings to app-owned passages, not model-transcribed quotations.
use crate::lessons::structured_call::{CallKind, ReasonFirst};
use crate::lessons::writer_excerpts::{self, WriterExcerpts};
use crate::review_findings::{FindingKind, FindingSite, ReviewFinding};
use crate::{generation, outline_progress::OutlineProgress};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
    shared::hash::sha256_hex,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

pub(super) const POLICY: &str = "section-review-v3";

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
            let mut passages = Vec::new();
            let mut start = 0;
            let mut length = 0;
            for (offset, character) in body.char_indices() {
                length += 1;
                if (length >= 240 && character == '\n') || length >= 480 {
                    let end = offset + character.len_utf8();
                    passages.push(json!({"id":format!("section-{index}-passage-{}", passages.len()),"text":body.get(start..end).unwrap_or_default()}));
                    start = end;
                    length = 0;
                }
            }
            if start < body.len() {
                passages.push(json!({"id":format!("section-{index}-passage-{}", passages.len()),"text":body.get(start..).unwrap_or_default()}));
            }
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
    let key = format!("teaching-review-findings-v2:{}", sha256_hex(
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
                            ReviewFinding::new(FindingKind::Teaching, None, issue)
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
                                let mut issue = ReviewFinding::new(FindingKind::Teaching, Some(FindingSite::Section(index)), note)
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
mod section_list_tests;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::test_support::candidate;
    use super::*;
    use crate::lessons::course_generation_tests::{ScriptedModel, Windowed};

    fn check(index: usize) -> Value {
        json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":"The explanation correctly distinguishes the stated inputs and resulting behavior.","hasDefect":false})
    }
    fn model(replies: Vec<Value>) -> ScriptedModel {
        ScriptedModel {
            outputs: std::sync::Mutex::new(replies.into_iter().map(|v| v.to_string()).collect()),
            prompts: Default::default(),
        }
    }

    #[tokio::test]
    async fn retains_more_than_six_findings_without_rejecting_a_complete_review() -> Result<()> {
        let findings: Vec<_> = (0..12)
            .map(|i| format!("Finding {i}: this assessment has multiple valid answers."))
            .collect();
        let response = json!({"issues":findings,"blockChecks":fixture_checks(vec![check(0),check(1),check(2)])});
        let (_, passages) = review_candidate(&candidate());
        assert!(
            jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages))
                .unwrap()
                .is_valid(&response)
        );
        let model = model(vec![response]);
        let actual = review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail,
        )
        .await?;
        assert_eq!(
            actual.iter().map(|f| f.note.clone()).collect::<Vec<_>>(),
            findings
        );
        assert!(actual
            .iter()
            .all(|f| f.kind == FindingKind::Teaching && f.site.is_none()));
        // The reviewer's own statement of the problem is what the user reads.
        assert_eq!(
            actual.iter().map(|f| f.summary.clone()).collect::<Vec<_>>(),
            findings
        );
        assert_eq!(model.prompts.lock().unwrap().len(), 1);
        Ok(())
    }

    #[test]
    fn response_slots_cannot_renumber_sections_or_cite_unknown_passages() {
        let (_, passages) = review_candidate(&candidate());
        let compiled = jsonschema::JSONSchema::compile(&schema(&[0, 1, 2], &passages)).unwrap();
        let mut response =
            json!({"issues":[],"blockChecks":fixture_checks(vec![check(0),check(1),check(2)])});
        assert!(compiled.is_valid(&response));
        response["blockChecks"][0]["passageId"] = json!("invented-passage");
        assert!(!compiled.is_valid(&response));
        response["blockChecks"][0]["passageId"] = json!("section-0-passage-0");
        response["blockChecks"][0]["index"] = json!(1);
        assert!(!compiled.is_valid(&response));
        response["blockChecks"][0]
            .as_object_mut()
            .unwrap()
            .remove("index");
        response["blockChecks"][0]["sectionId"] = json!("section-7");
        assert!(!compiled.is_valid(&response));
    }

    #[test]
    fn each_reviewed_part_carries_the_label_the_user_sees_beside_its_key() {
        let original = json!({
            "blocks":[{"body":"one","title":"First"},{"body":"two","title":"Second"}],
            "questions":[{"prompt":"ask"},{"prompt":"again"}]
        });
        let (reviewed, ids) = review_candidate(&original);
        assert_eq!(reviewed["blocks"][0]["label"], "Section 1");
        assert_eq!(reviewed["blocks"][1]["label"], "Section 2");
        assert_eq!(reviewed["questions"][0]["label"], "Question 1");
        assert_eq!(reviewed["questions"][1]["label"], "Question 2");
        assert_eq!(reviewed["blocks"][1]["index"], 1);
        assert_eq!(ids[1], ["section-1-passage-0"]);
        assert_eq!(reviewed["blocks"][1]["title"], "Second");
        assert_eq!(original["blocks"][1].get("label"), None);
    }

    #[test]
    fn passages_preserve_every_byte_and_ids_are_scoped_to_the_section() {
        let body = "🦀 Rust’s ‘book’\n```rust\n    println!(\"a  b\");\n```\r\n".repeat(40);
        let original = json!({"blocks":[{"body":body,"title":"Unicode and code"}]});
        let (reviewed, ids) = review_candidate(&original);
        let rejoined: String = reviewed["blocks"][0]["bodyPassages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["text"].as_str().unwrap())
            .collect();
        assert_eq!(rejoined, body);
        assert!(ids[0].len() > 1);
        assert_eq!(original["blocks"][0]["body"], body);
        assert_eq!(reviewed["blocks"][0]["title"], "Unicode and code");
        for indices in [vec![0], vec![]] {
            jsonschema::JSONSchema::compile(&schema(&indices, &ids)).unwrap();
        }
    }

    #[tokio::test]
    async fn malformed_section_recovers_without_rewriting_content_or_rechecking_valid_sections(
    ) -> Result<()> {
        let mut wrong_source = check(1);
        wrong_source["passageId"] = json!("section-0-passage-0");
        wrong_source["hasDefect"] = json!(true);
        wrong_source["finding"] =
            json!("The claimed membrane behavior contradicts the stated boundary condition.");
        let model = model(vec![
            json!({"issues":["An assessment has two defensible correct choices."],"blockChecks":crate::teaching_review::fixture_checks(vec![check(0),wrong_source,check(2)])}),
            json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(1)])}),
        ]);
        let issues = review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail,
        )
        .await?;
        assert_eq!(issues.len(), 2);
        let defect = issues
            .iter()
            .find(|i| i.note.contains("membrane behavior"))
            .unwrap();
        assert_eq!(defect.site, Some(FindingSite::Section(1)));
        assert_eq!(
            defect.summary,
            "The claimed membrane behavior contradicts the stated boundary condition."
        );
        let prompts = model.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 2);
        let retry: Value = serde_json::from_str(prompts[1].rsplit("\n\n").next().unwrap())?;
        assert_eq!(retry["sectionIndicesToReview"], json!([1]));
        assert_eq!(retry["acceptedBlockChecks"].as_array().unwrap().len(), 2);
        assert!(retry["responseCorrection"]
            .as_str()
            .unwrap()
            .contains("not the lesson"));
        Ok(())
    }

    #[tokio::test]
    async fn the_review_is_corrected_while_each_answer_checks_another_section() -> Result<()> {
        let only = |index| json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(index)])});
        let model = model(vec![only(0), only(1), only(2)]);
        review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail,
        )
        .await?;
        assert_eq!(model.prompts.lock().unwrap().len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn missing_duplicate_unknown_and_invented_checks_never_approve_unchecked_content() {
        for checks in [
            vec![],
            vec![check(0), check(0), check(2)],
            vec![check(1), check(2), check(3)],
            vec![
                json!({"index":0,"passageId":"invented","finding":"An invented passage cannot establish a complete review.","hasDefect":false}),
                check(1),
                check(2),
            ],
        ] {
            let bad =
                json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(checks)});
            let model = model(vec![bad.clone(), bad]);
            let error = review(
                &model,
                "Review instructional quality.",
                json!({}),
                &candidate(),
                None,
                LengthRule::Fail,
            )
            .await
            .unwrap_err();
            assert!(matches!(error, AppError::ServiceNotAvailable(_)));
            assert!(error.to_string().contains("Nothing was published"));
            assert_eq!(model.prompts.lock().unwrap().len(), 2);
        }
    }

    #[tokio::test]
    async fn invalid_global_findings_can_be_corrected_without_losing_section_checks() -> Result<()>
    {
        let model = model(vec![
            json!({"issues":["  "],"blockChecks":crate::teaching_review::fixture_checks(vec![check(0),check(1),check(2)])}),
            json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![])}),
        ]);
        assert!(review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail
        )
        .await?
        .is_empty());
        assert!(model.prompts.lock().unwrap()[1].contains("\"sectionIndicesToReview\":[]"));
        Ok(())
    }

    #[tokio::test]
    async fn a_lesson_s_corrected_review_drops_blank_findings_and_keeps_the_rest() -> Result<()> {
        let long = format!(
            "The worked example's stated result is wrong: {}",
            "x".repeat(720)
        );
        let mut blank = check(1);
        blank["hasDefect"] = json!(true);
        blank["finding"] = json!("  ");
        let first = json!({"issues":["  "],"blockChecks":fixture_checks(vec![check(0),blank.clone(),check(2)])});
        let second = json!({"issues":[" ",long],"blockChecks":fixture_checks(vec![blank])});
        let lesson = model(vec![first.clone(), second.clone()]);
        let issues = review(
            &lesson,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::DropBlank,
        )
        .await?;
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].note, long);
        let strict = model(vec![first, second]);
        assert!(review(
            &strict,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail,
        )
        .await
        .is_err());
        Ok(())
    }

    #[tokio::test]
    async fn a_section_finding_carries_the_passage_the_reviewer_chose() -> Result<()> {
        let mut defect = check(2);
        defect["hasDefect"] = json!(true);
        defect["finding"] = json!("The rest is placed after rolling, contradicting the recipe.");
        let model = model(vec![
            json!({"issues":[],"blockChecks":fixture_checks(vec![check(0),check(1),defect])}),
        ]);
        let issues = review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::DropBlank,
        )
        .await?;
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].site, Some(FindingSite::Section(2)));
        assert_eq!(
            issues[0].passage.as_deref(),
            candidate()["blocks"][2]["body"].as_str()
        );
        Ok(())
    }

    #[tokio::test]
    async fn malformed_json_is_corrected_as_a_review_not_as_lesson_content() -> Result<()> {
        let model = ScriptedModel {
            outputs: std::sync::Mutex::new(
                vec![
                    "not json".into(),
                    json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(0),check(1),check(2)])}).to_string(),
                ]
                .into(),
            ),
            prompts: Default::default(),
        };
        assert!(review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail
        )
        .await?
        .is_empty());
        assert_eq!(model.prompts.lock().unwrap().len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn missing_evidence_field_cannot_erase_a_reported_defect() -> Result<()> {
        let mut incomplete = check(1);
        incomplete.as_object_mut().unwrap().remove("passageId");
        incomplete["hasDefect"] = json!(true);
        let model = model(vec![
            json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(0),incomplete,check(2)])}),
            json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![check(1)])}),
        ]);
        let issues = review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail,
        )
        .await?;
        assert_eq!(issues.len(), 1);
        assert!(issues[0]
            .note
            .starts_with("Section 2 (candidate blocks-1):"));
        assert_eq!(issues[0].site, Some(FindingSite::Section(1)));
        Ok(())
    }

    #[tokio::test]
    async fn a_long_issue_from_the_teaching_review_is_read_not_refused() -> Result<()> {
        let long = format!(
            "The worked example's stated result is wrong: {}",
            "it contradicts the premise it was given. ".repeat(25)
        );
        assert!(long.chars().count() > 700);
        let response =
            json!({"issues":[long],"blockChecks":fixture_checks(vec![check(0),check(1),check(2)])});
        for lengths in [LengthRule::Fail, LengthRule::DropBlank] {
            let model = model(vec![response.clone()]);
            let issues = review(
                &model,
                "Review instructional quality.",
                json!({}),
                &candidate(),
                None,
                lengths,
            )
            .await?;
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0].note, long);
            assert_eq!(model.prompts.lock().unwrap().len(), 1);
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_section_finding_of_any_length_is_read_not_refused() -> Result<()> {
        let long = format!(
            "The stated boundary condition is contradicted: {}",
            "the membrane passes what it should hold back. ".repeat(20)
        );
        assert!(long.chars().count() > 700);
        let mut terse = check(1);
        terse["hasDefect"] = json!(true);
        terse["finding"] = json!("Wrong.");
        let mut lengthy = check(2);
        lengthy["hasDefect"] = json!(true);
        lengthy["finding"] = json!(long);
        let model = model(vec![
            json!({"issues":[],"blockChecks":fixture_checks(vec![check(0),terse,lengthy])}),
        ]);
        let issues = review(
            &model,
            "Review instructional quality.",
            json!({}),
            &candidate(),
            None,
            LengthRule::Fail,
        )
        .await?;
        let notes: Vec<_> = issues.iter().map(|issue| issue.note.clone()).collect();
        assert_eq!(
            notes,
            vec![
                "Section 2 (candidate blocks-1): Wrong.".to_owned(),
                format!("Section 3 (candidate blocks-2): {long}"),
            ]
        );
        assert_eq!(model.prompts.lock().unwrap().len(), 1);
        Ok(())
    }

    /// The review carries the lesson beside the writer's prompt, whose
    /// excerpts filled the room the writer had: on a 9B's 32k window the
    /// review of a PostgreSQL lesson did not fit, and the lesson failed after
    /// an hour of checked work (2026-11-06). Six excerpts of 2,000 tokens
    /// here, the lesson citing the fifth, in a window that holds some.
    #[tokio::test]
    async fn a_review_too_big_for_its_window_keeps_the_excerpts_that_fit_the_cited_first(
    ) -> Result<()> {
        let sources: Vec<_> = (0..6)
            .map(|index| json!({"sourceIndex":index,"title":format!("Source {index}"),"url":null,"excerpt":"x ".repeat(4000)}))
            .collect();
        let mut lesson = candidate();
        lesson["blocks"][1]["sourceIndex"] = json!(4);
        let scripted = model(vec![
            json!({"issues":[],"blockChecks":fixture_checks(vec![check(0),check(1),check(2)])}),
        ]);
        let window = Windowed(&scripted, 13_000);

        review(
            &window,
            "Review instructional quality.",
            json!({"authoringContext":{"goal":"Read query plans.","sources":sources}}),
            &lesson,
            None,
            LengthRule::Fail,
        )
        .await?;

        let sent = scripted.prompts.lock().unwrap()[0].clone();
        assert!(sent.contains("Source 4"));
        assert!(!sent.contains("Source 5"));
        Ok(())
    }

    #[test]
    fn the_review_schema_bounds_no_text_but_a_blank() {
        let (_, passages) = review_candidate(&candidate());
        let bounds =
            crate::lessons::course_generation_tests::length_bounds(&schema(&[0, 1, 2], &passages));
        assert!(bounds.is_empty(), "{bounds:?}");
    }
}
