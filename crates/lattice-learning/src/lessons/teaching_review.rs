//! Bind review findings to app-owned passages, not model-transcribed quotations.
use crate::lessons::structured_call::{CallKind, ReasonFirst};
use crate::lessons::writer_excerpts::{self, WriterExcerpts};
use crate::review_findings::{FindingId, FindingSite, ReviewFinding};
use crate::{generation, outline_progress::OutlineProgress};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

pub(super) const POLICY: &str = "section-review-v5";

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

/// The sentence the prose step is given when open findings are asked about.
const OPEN_FINDINGS_BRIEF: &str = " For each entry of openFindings, say whether the defect it describes is still in the candidate.";

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
    #[serde(default, rename = "openFindings")]
    open_findings: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Judgement {
    Present,
    Resolved,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerdictEntry {
    id: String,
    verdict: Judgement,
    note: String,
}

/// One open finding shown to the reviewer.
#[derive(Clone, Debug)]
pub struct Asked {
    pub id: FindingId,
    pub site: FindingSite,
    /// The passage it is about now, when one still holds its words.
    pub passage_id: Option<String>,
    pub note: String,
}

/// What one review covers. The default is a first review of everything.
#[derive(Clone, Copy, Default)]
pub struct Scope<'a> {
    /// The sections to check; none is every section.
    pub sections: Option<&'a [usize]>,
    /// The open findings to judge again; none on a first review.
    pub asked: Option<&'a [Asked]>,
    /// Open findings in parts that have not changed: shown, not judged.
    pub known: &'a [Asked],
}

/// The reviewer's judgement of one finding it was asked about.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Verdict {
    pub id: FindingId,
    pub present: bool,
    pub note: String,
}

/// A defect the reviewer reported, before it has a number.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Reported {
    pub site: Option<FindingSite>,
    pub note: String,
    pub summary: String,
    pub passage: Option<String>,
    /// The app-owned id of the passage a section check chose.
    pub passage_id: Option<String>,
}

impl Reported {
    pub fn finding(self, number: u32) -> ReviewFinding {
        let mut finding =
            ReviewFinding::reviewer(number, self.site, self.note).with_summary(self.summary);
        finding.passage = self.passage;
        finding
    }
}

/// What a review found: a verdict on each finding it was asked about, and
/// the defects it reports beyond those.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Judged {
    pub verdicts: Vec<Verdict>,
    pub reported: Vec<Reported>,
}

/// How the reviewer is shown a part: "Section 3", "Question 2" or "The whole
/// lesson".
pub(crate) fn label(site: FindingSite) -> String {
    match site {
        FindingSite::Lesson => "The whole lesson".to_owned(),
        FindingSite::Section(index) => format!("Section {}", index + 1),
        FindingSite::Question(index) => format!("Question {}", index + 1),
    }
}

/// Each section's passages as `(id, text)`: the ids are the application's.
pub(crate) fn passages_of(candidate: &Value) -> Vec<Vec<(String, String)>> {
    candidate
        .get("blocks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, block)| {
            let body = block
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default();
            passages::split(body)
                .into_iter()
                .enumerate()
                .map(|(n, text)| (format!("section-{index}-passage-{n}"), text.to_owned()))
                .collect()
        })
        .collect()
}

// Preserve every byte, including code indentation, newlines and Unicode. IDs
// belong only to this candidate; nothing is normalized or fuzzy-matched.
fn review_candidate(candidate: &Value) -> (Value, Vec<Vec<String>>) {
    let all = passages_of(candidate);
    let mut candidate = candidate.clone();
    if let Some(blocks) = candidate.get_mut("blocks").and_then(Value::as_array_mut) {
        for (index, block) in blocks.iter_mut().enumerate() {
            let passages: Vec<Value> = all
                .get(index)
                .into_iter()
                .flatten()
                .map(|(id, text)| json!({"id":id,"text":text}))
                .collect();
            if let Some(block) = block.as_object_mut() {
                block.remove("body");
                block.insert("index".into(), json!(index));
                block.insert("label".into(), json!(label(FindingSite::Section(index))));
                block.insert("bodyPassages".into(), json!(passages));
            }
        }
    }
    if let Some(questions) = candidate.get_mut("questions").and_then(Value::as_array_mut) {
        for (index, question) in questions.iter_mut().enumerate() {
            if let Some(question) = question.as_object_mut() {
                question.insert("label".into(), json!(label(FindingSite::Question(index))));
            }
        }
    }
    let ids = all
        .into_iter()
        .map(|passages| passages.into_iter().map(|(id, _)| id).collect())
        .collect();
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

fn schema(indices: &[usize], passages: &[Vec<String>], verdict_ids: &[&str]) -> Value {
    let mut required = Vec::new();
    let mut properties = serde_json::Map::new();
    if !verdict_ids.is_empty() {
        let count = verdict_ids.len();
        required.push("openFindings");
        properties.insert("openFindings".into(), json!({"type":"array","minItems":count,"maxItems":count,"items":{"type":"object","additionalProperties":false,"required":["id","verdict","note"],"properties":{"id":{"type":"string","enum":verdict_ids},"verdict":{"type":"string","enum":["present","resolved"]},"note":{"type":"string"}}}}));
    }
    required.push("issues");
    properties.insert(
        "issues".into(),
        json!({"type":"array","items":{"type":"string","minLength":1}}),
    );
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

/// What a review's verdicts stand under, besides its text: the policy, the
/// model and its window.
fn run_inputs(llm: &dyn LLMPort) -> serde_json::Map<String, Value> {
    let mut inputs = serde_json::Map::new();
    inputs.insert("policy".into(), json!(POLICY));
    inputs.insert("model".into(), json!(llm.model_name()));
    inputs.insert("context".into(), json!(llm.max_context_tokens()));
    inputs
}

/// The digest of what a review's reviewer is given besides the material:
/// `run_inputs`, the subject's first-review `system` and evidence `scope`, and
/// the review's `inputs`.
pub(crate) fn basis(llm: &dyn LLMPort, system: &str, scope: &str, inputs: &Value) -> String {
    let mut given = run_inputs(llm);
    for (name, value) in [
        ("system", json!(system)),
        ("scope", json!(scope)),
        ("inputs", inputs.clone()),
    ] {
        given.insert(name.into(), value);
    }
    crate::content_verification::digest(&Value::Object(given).to_string())
}

/// The entries of `openFindings`: the findings to be judged.
fn open_entries<'a>(asked: impl IntoIterator<Item = &'a Asked>) -> Value {
    let entries: Vec<Value> = asked
        .into_iter()
        .map(|finding| {
            let mut entry = serde_json::Map::new();
            entry.insert("id".into(), json!(finding.id.as_str()));
            entry.insert("part".into(), json!(label(finding.site)));
            if let Some(passage_id) = &finding.passage_id {
                entry.insert("passageId".into(), json!(passage_id));
            }
            entry.insert("note".into(), json!(finding.note));
            Value::Object(entry)
        })
        .collect();
    json!(entries)
}

/// A finding shown by the part it is about (none: the material as a whole)
/// and its note.
fn part_note(site: Option<FindingSite>, note: &str) -> Value {
    json!({"part": site.map(label), "note": note})
}

/// The note of a defect a section's check reports.
fn section_note(index: usize, finding: &str) -> String {
    format!(
        "Section {} (candidate blocks-{index}): {finding}",
        index + 1
    )
}

/// What the reviewer found: its verdict on each finding in `scope.asked`,
/// and the defects it reports beyond those. A defect of a section's check is
/// placed in that section; a reviewer's issue is placed later, by the
/// evidence check. Malformed output is corrected in the same call without
/// spending the author's repair pass, and an unavailable provider propagates.
pub async fn review(
    llm: &dyn LLMPort,
    system: &str,
    prompt: Value,
    candidate: &Value,
    scope: Scope<'_>,
    progress: Option<&OutlineProgress>,
    lengths: LengthRule,
) -> Result<Judged> {
    let Value::Object(mut prompt) = prompt else {
        return Err(AppError::InternalError(
            "Teaching review requires an object context.".into(),
        ));
    };
    if let Some(asked) = scope.asked {
        prompt.insert("openFindings".into(), open_entries(asked));
    }
    if !scope.known.is_empty() {
        let entries: Vec<Value> = scope
            .known
            .iter()
            .map(|finding| part_note(Some(finding.site), &finding.note))
            .collect();
        prompt.insert("knownFindings".into(), json!(entries));
    }
    // Save findings, including defects, before the later grounding/repair step.
    // Reusing a completed review never approves the lesson or skips grounding.
    let mut inputs = run_inputs(llm);
    for (name, value) in [
        ("system", json!(system)),
        ("prompt", json!(prompt)),
        ("candidate", json!(candidate)),
        ("sections", json!(scope.sections)),
    ] {
        inputs.insert(name.into(), value);
    }
    let key = format!(
        "teaching-review-findings-v4:{}",
        crate::content_verification::digest(&Value::Object(inputs).to_string())
    );
    if let Some(saved) = crate::lesson_drafts::checkpoint(&key)
        .await?
        .and_then(|v| serde_json::from_value::<Judged>(v).ok())
    {
        crate::lesson_progress::stage(
            "Reusing completed teaching review; continuing evidence checks",
        );
        return Ok(saved);
    }
    let judged = review_sections(llm, system, prompt, candidate, scope, progress, lengths).await?;
    crate::lesson_drafts::record_checkpoint(&key, json!(judged)).await?;
    Ok(judged)
}

async fn review_sections(
    llm: &dyn LLMPort,
    system: &str,
    mut prompt: serde_json::Map<String, Value>,
    candidate: &Value,
    scope: Scope<'_>,
    progress: Option<&OutlineProgress>,
    lengths: LengthRule,
) -> Result<Judged> {
    let (reviewed, passages) = review_candidate(candidate);
    let shown = reviewed.clone();
    prompt.insert("candidate".into(), reviewed);
    let excerpts = prompt
        .get_mut("authoringContext")
        .and_then(|context| WriterExcerpts::take(llm, context, candidate));
    let wanted: Vec<usize> = scope.sections.map_or_else(
        || (0..passages.len()).collect(),
        |sections| {
            sections
                .iter()
                .copied()
                .filter(|index| *index < passages.len())
                .collect()
        },
    );
    let brief = if scope.asked.is_some_and(|asked| !asked.is_empty()) {
        format!("{REVIEW_DRAFT_BRIEF}{OPEN_FINDINGS_BRIEF}")
    } else {
        REVIEW_DRAFT_BRIEF.to_owned()
    };
    let mut accepted = BTreeMap::<usize, BlockCheck>::new();
    let mut verdicts = BTreeMap::<FindingId, Verdict>::new();
    let mut reported: Vec<Reported> = Vec::new();
    let mut problems = Vec::new();
    // The first correction is always asked for; later ones while each answer
    // validates another section.
    for attempt in 0usize.. {
        // After the correction, a blank finding no longer fails a lesson's review.
        let settle = lengths == LengthRule::DropBlank && attempt > 0;
        let checked = accepted.len() + verdicts.len();
        let indices: Vec<_> = wanted
            .iter()
            .copied()
            .filter(|i| !accepted.contains_key(i))
            .collect();
        let remaining: Vec<&Asked> = scope
            .asked
            .into_iter()
            .flatten()
            .filter(|finding| !verdicts.contains_key(&finding.id))
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
            prompt.insert(
                "retainedIssues".into(),
                json!(reported
                    .iter()
                    .map(|report| part_note(report.site, &report.note))
                    .collect::<Vec<_>>()),
            );
            prompt.insert(
                "acceptedBlockChecks".into(),
                json!(accepted.values().collect::<Vec<_>>()),
            );
            if scope.asked.is_some() {
                prompt.insert(
                    "openFindings".into(),
                    open_entries(remaining.iter().copied()),
                );
                prompt.insert(
                    "acceptedVerdicts".into(),
                    json!(verdicts.values().collect::<Vec<_>>()),
                );
            }
            prompt.insert("responseCorrection".into(), json!("Correct the review response, not the lesson. Return checks only for sectionIndicesToReview. Read all passages in each requested section; select a passageId from that same section. Keep every established defect. Do not regenerate the lesson or treat a response-format error as a content defect."));
        }
        let request = writer_excerpts::Request {
            subject: "the teaching review",
            system,
            prompt: &prompt,
            context: "authoringContext",
            schema: schema(
                &indices,
                &passages,
                &remaining
                    .iter()
                    .map(|finding| finding.id.as_str())
                    .collect::<Vec<_>>(),
            ),
            output_tokens: REVIEW_ANSWER_TOKENS,
        };
        // A correction repairs the response, not the judgement, so only the
        // first review reasons in prose before answering.
        let reasoning = (attempt == 0).then_some(ReasonFirst {
            kind: CallKind::Authoring,
            subject: "the teaching review",
            brief: &brief,
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
                    } else if !blank && !reported.iter().any(|known| known.note == issue) {
                        let summary = issue.trim().to_owned();
                        reported.push(Reported {
                            site: None,
                            note: issue,
                            summary,
                            passage: None,
                            passage_id: None,
                        });
                    }
                }
                let mut entries = Vec::new();
                for entry in review.open_findings {
                    match serde_json::from_value::<VerdictEntry>(entry) {
                        Ok(entry) => entries.push(entry),
                        Err(_) => problems.push("A verdict has missing, unexpected or incorrectly typed fields.".into()),
                    }
                }
                for entry in &entries {
                    if !remaining.iter().any(|finding| finding.id.as_str() == entry.id) {
                        let message = format!("{} was not asked about; give a verdict only for the ids in openFindings.", entry.id);
                        if !problems.contains(&message) {
                            problems.push(message);
                        }
                    }
                }
                for finding in &remaining {
                    let mut own = entries.iter().filter(|entry| entry.id == finding.id.as_str());
                    match (own.next(), own.next()) {
                        (Some(entry), None) => {
                            verdicts.insert(finding.id.clone(), Verdict {
                                id: finding.id.clone(),
                                present: matches!(entry.verdict, Judgement::Present),
                                note: entry.note.trim().to_owned(),
                            });
                        }
                        _ => problems.push(format!("{} needs one verdict.", finding.id.as_str())),
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
                            let note = section_note(index, finding);
                            if !reported.iter().any(|known| known.note == note) {
                                let chosen = value.get("passageId").and_then(Value::as_str);
                                reported.push(Reported {
                                    site: Some(FindingSite::Section(index)),
                                    note,
                                    summary: finding.trim().to_owned(),
                                    passage: chosen.and_then(|id| passage_text(&shown, index, id)),
                                    passage_id: chosen.map(str::to_owned),
                                });
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
                        if check.has_defect {
                            let note = section_note(check.index, &check.finding);
                            let site = Some(FindingSite::Section(check.index));
                            if let Some(report) = reported
                                .iter_mut()
                                .find(|known| known.site == site && known.note == note)
                            {
                                report.passage = passage_text(&shown, check.index, &check.passage_id);
                                report.passage_id = Some(check.passage_id.clone());
                            }
                        }
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
            let verdicts = scope
                .asked
                .into_iter()
                .flatten()
                .filter_map(|finding| verdicts.remove(&finding.id))
                .collect();
            return Ok(Judged { verdicts, reported });
        }
        tracing::warn!(attempt = attempt + 1, accepted_sections = accepted.len(), total_sections = passages.len(), problems = ?problems, "Teaching reviewer response failed validation");
        if attempt > 0 && accepted.len() + verdicts.len() == checked {
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
