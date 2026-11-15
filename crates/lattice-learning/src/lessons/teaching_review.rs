//! Bind review findings to app-owned passages, not model-transcribed quotations.
use crate::dto::LearningBlockKind;
use crate::lessons::structured_call::{CallKind, ReasonFirst};
use crate::lessons::trace::{self, Stage};
use crate::lessons::writer_excerpts::{self, WriterExcerpts};
use crate::review_findings::{part_quote, FindingId, FindingSite, ReviewFinding};
use crate::{generation, outline_progress::OutlineProgress};
use lattice_core::{
    application::ports::LLMPort,
    application::services::model_contract::{Contract, ContractCall, ContractId, PromptInput},
    protocol,
    shared::error::{AppError, Result},
    shared::hash::sha256_hex,
    shared::protocol::Protocol,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) use concerns::LessonConcern;

pub(super) const POLICY: Protocol = protocol!("section-review", 7);

/// The saved answers of the section review, by what it was given.
const FINDINGS_KEYS: Protocol = protocol!("teaching-review-findings", 7);

/// The call that reviews a lesson's sections or an outline.
pub(crate) struct TeachingReview;

impl Contract for TeachingReview {
    const ID: ContractId = ContractId::new("teaching-review");
    type Input = PromptInput;
}

#[cfg(any(test, feature = "test-support"))]
impl lattice_core::application::services::model_contract::test_support::RenderAnswer
    for TeachingReview
{
    type Answer = Review;

    fn render(answer: &Review) -> String {
        json!(answer).to_string()
    }

    fn read(text: &str) -> Result<Review> {
        read_review(text)
    }
}

/// The room the review's answer is given.
const REVIEW_ANSWER_TOKENS: usize = 6000;

/// What the reviewer works through in prose before its findings are written
/// to the response schema, where each section's verdict would otherwise be
/// chosen as it is written.
const REVIEW_DRAFT_BRIEF: &str = "Go through the candidate part by part: for a lesson, every section whose inReview is true and every question; for an outline, every module. Solve each exercise and question yourself and check each worked example against its stated premises and result. For each part say what you checked, what you found and whether it has a material defect.";

/// The sentence the prose step is given when open findings are asked about.
const OPEN_FINDINGS_BRIEF: &str = " For each entry of openFindings, read the part it names as the candidate now has it, whether or not that part is in review. Quote the exact words of that part that still show the defect, or, for something left out, the words it should follow. When no words of the part show it any more, leave the quote empty and answer resolved. An open finding's id is never used for a different defect: report that as a new issue.";

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SectionDefect {
    passage_id: String,
    finding: String,
    #[serde(default)]
    concerns: Option<Vec<LessonConcern>>,
}

#[derive(Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(Serialize, Debug, PartialEq)
)]
#[serde(deny_unknown_fields)]
pub(crate) struct Review {
    issues: Vec<Issue>,
    #[serde(default, rename = "sectionDefects")]
    section_defects: Vec<Value>,
    #[serde(default, rename = "openFindings")]
    open_findings: Vec<Value>,
}

/// An issue as the reviewer gave it: plain text, or, when open findings were
/// asked about, text with the open finding it continues.
#[derive(Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(Serialize, Debug, PartialEq)
)]
#[serde(untagged)]
enum Issue {
    Plain(String),
    Continuing {
        note: String,
        #[serde(default)]
        continues: Option<String>,
        #[serde(default)]
        concern: Option<LessonConcern>,
    },
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
    quote: String,
    verdict: Judgement,
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

/// Which defects a review reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remit {
    /// Any material defect.
    Open,
    /// Only the concerns a lesson's reviewer owns, each in a section whose kind takes it.
    Lesson,
}

/// What one review covers. The default is a first review of everything.
#[derive(Clone, Copy, Default)]
pub struct Scope<'a> {
    /// The sections to check; none is every section.
    pub sections: Option<&'a [usize]>,
    /// The open findings to judge again; none on a first review.
    pub asked: Option<&'a [Asked]>,
}

/// The reviewer's judgement of one finding it was asked about.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Verdict {
    pub id: FindingId,
    pub present: bool,
    /// The words of its part that show the defect now; blank when resolved.
    pub quote: String,
}

/// A defect the reviewer reported, before it has a number.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Reported {
    pub site: Option<FindingSite>,
    pub note: String,
    pub summary: String,
    pub passage: Option<String>,
    /// The app-owned id of the passage a section defect names.
    pub passage_id: Option<String>,
    /// The open finding the reviewer says this defect is.
    #[serde(default)]
    pub continues: Option<FindingId>,
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
                .map(|(n, text)| {
                    let id = format!("{}-passage-{n}", FindingSite::Section(index).id_text());
                    (id, text.to_owned())
                })
                .collect()
        })
        .collect()
}

// Preserve every byte, including code indentation, newlines and Unicode. IDs
// belong only to this candidate; nothing is normalized or fuzzy-matched.
fn review_candidate(candidate: &Value, sections: &[usize]) -> (Value, Vec<Vec<String>>) {
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
                block.insert("inReview".into(), json!(sections.contains(&index)));
                block.insert("label".into(), json!(FindingSite::Section(index).label()));
                block.insert("bodyPassages".into(), json!(passages));
            }
        }
    }
    if let Some(questions) = candidate.get_mut("questions").and_then(Value::as_array_mut) {
        for (index, question) in questions.iter_mut().enumerate() {
            if let Some(question) = question.as_object_mut() {
                question.insert("label".into(), json!(FindingSite::Question(index).label()));
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
        .pointer(&FindingSite::Section(index).field_pointer("bodyPassages")?)?
        .as_array()?
        .iter()
        .find(|passage| passage.get("id").and_then(Value::as_str) == Some(id))?
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn schema(
    indices: &[usize],
    passages: &[Vec<String>],
    verdict_ids: &[&str],
    remit: Remit,
) -> Value {
    let mut required = Vec::new();
    let mut properties = serde_json::Map::new();
    if !verdict_ids.is_empty() {
        let count = verdict_ids.len();
        required.push("openFindings");
        properties.insert("openFindings".into(), json!({"type":"array","minItems":count,"maxItems":count,"items":{"type":"object","additionalProperties":false,"required":["id","quote","verdict"],"properties":{"id":{"type":"string","enum":verdict_ids},"quote":{"type":"string"},"verdict":{"type":"string","enum":["present","resolved"]}}}}));
    }
    required.push("issues");
    let continues = (!verdict_ids.is_empty()).then(|| {
        let choices: Vec<&str> = verdict_ids
            .iter()
            .copied()
            .chain(std::iter::once("none"))
            .collect();
        json!({"type":"string","enum":choices})
    });
    let issue = if remit == Remit::Lesson {
        let mut required = vec!["note", "concern"];
        let mut fields = json!({"note":{"type":"string","minLength":1},"concern":{"type":"string","enum":LessonConcern::ids_where(LessonConcern::in_issues)}});
        if let (Some(continues), Some(fields)) = (&continues, fields.as_object_mut()) {
            required.push("continues");
            fields.insert("continues".into(), continues.clone());
        }
        json!({"type":"object","additionalProperties":false,"required":required,"properties":fields})
    } else {
        match &continues {
            Some(continues) => {
                json!({"type":"object","additionalProperties":false,"required":["note","continues"],"properties":{"note":{"type":"string","minLength":1},"continues":continues}})
            }
            None => json!({"type":"string","minLength":1}),
        }
    };
    properties.insert("issues".into(), json!({"type":"array","items":issue}));
    if !passages.is_empty() {
        required.push("sectionDefects");
        let cited: Vec<&str> = indices
            .iter()
            .filter_map(|index| passages.get(*index))
            .flatten()
            .map(String::as_str)
            .collect();
        let passage_id = if cited.is_empty() {
            json!({"type":"string"})
        } else {
            json!({"type":"string","enum":cited})
        };
        let defects = if indices.is_empty() {
            json!({"type":"array","maxItems":0,"items":{"type":"string"}})
        } else {
            let mut required = vec!["passageId", "finding"];
            let mut fields =
                json!({"passageId":passage_id,"finding":{"type":"string","minLength":1}});
            if let (true, Some(fields)) = (remit == Remit::Lesson, fields.as_object_mut()) {
                required.push("concerns");
                fields.insert("concerns".into(), json!({"type":"array","items":{"type":"string","enum":LessonConcern::ids_where(LessonConcern::in_checks)}}));
            }
            if let (Some(continues), Some(fields)) = (&continues, fields.as_object_mut()) {
                required.push("continues");
                fields.insert("continues".into(), continues.clone());
            }
            json!({"type":"array","items":{"type":"object","additionalProperties":false,"required":required,"properties":fields}})
        };
        properties.insert("sectionDefects".into(), defects);
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
    sha256_hex(Value::Object(given).to_string())
}

/// The entries of `openFindings`: the findings to be judged.
fn open_entries<'a>(asked: impl IntoIterator<Item = &'a Asked>) -> Value {
    let entries: Vec<Value> = asked
        .into_iter()
        .map(|finding| {
            let mut entry = serde_json::Map::new();
            entry.insert("id".into(), json!(finding.id.as_str()));
            entry.insert("part".into(), json!(finding.site.label()));
            if let Some(passage_id) = &finding.passage_id {
                entry.insert("passageId".into(), json!(passage_id));
            }
            entry.insert("note".into(), json!(finding.note));
            Value::Object(entry)
        })
        .collect();
    json!(entries)
}

/// What a reply is read against: the reviewed candidate, the sections it
/// must check and the open findings it must judge.
struct Reading<'a> {
    candidate: &'a Value,
    shown: &'a Value,
    passages: &'a [Vec<String>],
    sections: &'a [usize],
    asked: &'a [&'a Asked],
    remit: Remit,
    /// The reply after a correction: a blank issue or finding is dropped and
    /// an open finding with no usable verdict stays as it stood, not refused.
    settle: bool,
}

/// Records `message` once.
fn note(problems: &mut Vec<String>, message: String) {
    if !problems.contains(&message) {
        problems.push(message);
    }
}

fn continuing(reading: &Reading<'_>, id: &str) -> Option<FindingId> {
    reading
        .asked
        .iter()
        .find(|asked| asked.id.as_str() == id)
        .map(|asked| asked.id.clone())
}

/// Refuses `message`: on the settled reply what it is about is dropped and traced.
fn refuse(reading: &Reading<'_>, problems: &mut Vec<String>, subject: &str, message: String) {
    if reading.settle {
        trace::refused(Stage::TeachingReview, subject, &message, None);
    } else {
        note(problems, message);
    }
}

/// Whether a lesson's issue may be raised: it holds a concern no section holds.
fn issue_concern_allowed(
    concern: Option<LessonConcern>,
    reading: &Reading<'_>,
    problems: &mut Vec<String>,
) -> bool {
    let allowed = LessonConcern::ids_where(LessonConcern::in_issues).join(", ");
    match concern {
        Some(concern) if concern.in_issues() => true,
        Some(concern) => {
            let message = format!("An issue's concern must be one of {allowed}; report {} in the sectionDefects entry of the section that holds it.", concern.id());
            refuse(reading, problems, "an issue", message);
            false
        }
        None => {
            let message = format!("An issue needs a concern, one of {allowed}.");
            refuse(reading, problems, "an issue", message);
            false
        }
    }
}

fn read_issues(
    issues: Vec<Issue>,
    reading: &Reading<'_>,
    reported: &mut Vec<Reported>,
    problems: &mut Vec<String>,
) {
    for issue in issues {
        let (issue, continued) = match (issue, reading.remit) {
            (Issue::Plain(_), Remit::Lesson) => {
                let message = "An issue is incorrectly typed: give it as an object with a note and a concern.".to_owned();
                refuse(reading, problems, "an issue", message);
                continue;
            }
            (Issue::Plain(note), Remit::Open) => (note, None),
            (
                Issue::Continuing {
                    concern: Some(_), ..
                },
                Remit::Open,
            ) => {
                note(
                    problems,
                    "An issue has missing, unexpected or incorrectly typed fields.".into(),
                );
                continue;
            }
            (
                Issue::Continuing {
                    note,
                    continues,
                    concern,
                },
                remit,
            ) => {
                if remit == Remit::Lesson && !issue_concern_allowed(concern, reading, problems) {
                    continue;
                }
                (note, continues)
            }
        };
        if issue.trim().is_empty() {
            if !reading.settle {
                note(problems, "An issue must not be blank.".into());
            }
        } else if !reported.iter().any(|known| known.note == issue) {
            let summary = issue.trim().to_owned();
            reported.push(Reported {
                site: None,
                note: issue,
                summary,
                passage: None,
                passage_id: None,
                continues: continued.and_then(|id| continuing(reading, &id)),
            });
        }
    }
}

fn read_verdicts(
    entries: Vec<Value>,
    reading: &Reading<'_>,
    problems: &mut Vec<String>,
) -> Vec<Verdict> {
    let mut refuse = |message: String| {
        if reading.settle {
            tracing::warn!(%message, "A verdict that could not be used leaves its finding open as it stood");
        } else {
            note(problems, message);
        }
    };
    let mut parsed = Vec::new();
    for entry in entries {
        match serde_json::from_value::<VerdictEntry>(entry) {
            Ok(entry) => parsed.push(entry),
            Err(_) => {
                refuse("A verdict has missing, unexpected or incorrectly typed fields.".into())
            }
        }
    }
    for entry in &parsed {
        if !reading
            .asked
            .iter()
            .any(|finding| finding.id.as_str() == entry.id)
        {
            refuse(format!(
                "{} was not asked about; give a verdict only for the ids in openFindings.",
                entry.id
            ));
        }
    }
    let mut verdicts = Vec::new();
    for finding in reading.asked {
        let mut own = parsed
            .iter()
            .filter(|entry| entry.id == finding.id.as_str());
        let (Some(entry), None) = (own.next(), own.next()) else {
            refuse(format!("{} needs one verdict.", finding.id.as_str()));
            continue;
        };
        let quote = entry.quote.trim();
        if !matches!(entry.verdict, Judgement::Present) {
            verdicts.push(Verdict {
                id: finding.id.clone(),
                present: false,
                quote: quote.to_owned(),
            });
        } else if let Some(held) = part_quote(reading.candidate, finding.site, quote) {
            verdicts.push(Verdict {
                id: finding.id.clone(),
                present: true,
                quote: held.to_owned(),
            });
        } else {
            refuse(format!("{}: the quote is not in {} as it now stands; quote the words there that still show the defect, or answer resolved.", finding.id.as_str(), finding.site.label()));
        }
    }
    verdicts
}

/// The kind of section `index` of the candidate, when it can be read.
fn section_kind(candidate: &Value, index: usize) -> Option<LearningBlockKind> {
    candidate
        .get("blocks")?
        .get(index)?
        .get("kind")?
        .as_str()
        .and_then(LearningBlockKind::parse)
}

/// Whether a lesson's defect in section `index` names a
/// concern its kind takes. A reply that is not settled is refused instead if
/// it names none, or one the kind does not take.
fn raises(
    named: &[LessonConcern],
    index: usize,
    reading: &Reading<'_>,
) -> std::result::Result<bool, String> {
    let site = FindingSite::Section(index).label();
    let kind = section_kind(reading.candidate, index);
    let takes: Vec<LessonConcern> = LessonConcern::ALL
        .into_iter()
        .filter(|concern| concern.takes(kind.as_ref()))
        .collect();
    let what = match &kind {
        Some(kind) => format!("of kind {}", kind.as_str()),
        None => "of unknown kind".to_owned(),
    };
    let mut once: Vec<LessonConcern> = Vec::new();
    for concern in named {
        if !once.contains(concern) {
            once.push(*concern);
        }
    }
    let mut messages = Vec::new();
    if once.is_empty() {
        messages.push(format!(
            "{site}: a defect needs the concerns the section fails, from those its kind takes: {}.",
            LessonConcern::list(takes.iter().copied())
        ));
    }
    let mut kept = 0;
    for concern in once {
        if takes.contains(&concern) {
            kept += 1;
        } else {
            messages.push(format!(
                "{site} is {what}, which takes only {}; {} does not apply to it.",
                LessonConcern::list(takes.iter().copied()),
                concern.id()
            ));
        }
    }
    if reading.settle {
        for message in &messages {
            trace::refused(
                Stage::TeachingReview,
                &format!("{site} defect"),
                message,
                None,
            );
        }
        Ok(kept > 0)
    } else if messages.is_empty() {
        Ok(true)
    } else {
        Err(messages.join(" "))
    }
}

/// The reviewed section whose passages include `passage_id`.
fn section_of(reading: &Reading<'_>, passage_id: &str) -> Option<usize> {
    reading.sections.iter().copied().find(|index| {
        reading
            .passages
            .get(*index)
            .is_some_and(|ids| ids.iter().any(|id| id == passage_id))
    })
}

const WRONG_DEFECT_FIELDS: &str =
    "A section defect has missing, unexpected or incorrectly typed fields.";

/// The defect a section-defect entry reports, if it is usable.
fn read_defect(
    mut value: Value,
    reading: &Reading<'_>,
    problems: &mut Vec<String>,
) -> Option<Reported> {
    let continues = value
        .as_object_mut()
        .and_then(|fields| fields.remove("continues"))
        .and_then(|id| id.as_str().and_then(|id| continuing(reading, id)));
    let Ok(defect) = serde_json::from_value::<SectionDefect>(value) else {
        refuse(
            reading,
            problems,
            "a section defect",
            WRONG_DEFECT_FIELDS.into(),
        );
        return None;
    };
    if reading.remit == Remit::Open && defect.concerns.is_some() {
        refuse(
            reading,
            problems,
            "a section defect",
            WRONG_DEFECT_FIELDS.into(),
        );
        return None;
    }
    let Some(index) = section_of(reading, &defect.passage_id) else {
        let message = format!(
            "passageId {} is not a passage of a section in review.",
            defect.passage_id
        );
        refuse(reading, problems, "a section defect", message);
        return None;
    };
    let site = FindingSite::Section(index);
    if defect.finding.trim().is_empty() {
        let message = format!("{}: finding must not be blank.", site.label());
        refuse(
            reading,
            problems,
            &format!("{} defect", site.label()),
            message,
        );
        return None;
    }
    if reading.remit == Remit::Lesson {
        let named = defect.concerns.clone().unwrap_or_default();
        match raises(&named, index, reading) {
            Ok(true) => {}
            Ok(false) => return None,
            Err(message) => {
                note(problems, message);
                return None;
            }
        }
    }
    Some(Reported {
        site: Some(site),
        note: format!("{}: {}", site.note_label(), defect.finding),
        summary: defect.finding.trim().to_owned(),
        passage: passage_text(reading.shown, index, &defect.passage_id),
        passage_id: Some(defect.passage_id.clone()),
        continues,
    })
}

fn read_defects(
    defects: Vec<Value>,
    reading: &Reading<'_>,
    reported: &mut Vec<Reported>,
    problems: &mut Vec<String>,
) {
    for value in defects {
        if let Some(report) = read_defect(value, reading, problems) {
            if !reported.iter().any(|known| known.note == report.note) {
                reported.push(report);
            }
        }
    }
}

/// The review a reply holds, before it is held to the sections asked about.
fn read_review(raw: &str) -> Result<Review> {
    generation::parse_json(raw).map_err(|_| {
        AppError::InvalidInput(
            "The review could not be read. Give issues and sectionDefects as the schema has them."
                .into(),
        )
    })
}

/// The reviewer's reply as findings; every problem it has, in one error.
fn read(raw: &str, reading: &Reading<'_>) -> Result<Judged> {
    let review = read_review(raw)?;
    let mut problems = Vec::new();
    let mut reported = Vec::new();
    read_issues(review.issues, reading, &mut reported, &mut problems);
    let verdicts = read_verdicts(review.open_findings, reading, &mut problems);
    read_defects(
        review.section_defects,
        reading,
        &mut reported,
        &mut problems,
    );
    if problems.is_empty() {
        Ok(Judged { verdicts, reported })
    } else {
        Err(AppError::InvalidInput(problems.join(" ")))
    }
}

/// What the reviewer found: its verdict on each finding in `scope.asked`,
/// and the defects it reports beyond those. A section defect is placed in
/// the section its passage is in, and a section with no entry has none; a
/// reviewer's issue is placed later, by the evidence check. A reply with a
/// problem is corrected once. In the second, an entry that cannot be used
/// is dropped and traced, and a verdict that cannot be used leaves its
/// finding as it stood, as an unjudged statement is left (claim classes
/// design §3.2); a reply that cannot be read at all fails the review.
pub async fn review(
    llm: &dyn LLMPort,
    system: &str,
    prompt: Value,
    candidate: &Value,
    scope: Scope<'_>,
    remit: Remit,
    progress: Option<&OutlineProgress>,
) -> Result<Judged> {
    let Value::Object(mut prompt) = prompt else {
        return Err(AppError::InternalError(
            "Teaching review requires an object context.".into(),
        ));
    };
    if let Some(asked) = scope.asked {
        prompt.insert("openFindings".into(), open_entries(asked));
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
    let key = FINDINGS_KEYS.key(sha256_hex(Value::Object(inputs).to_string()));
    let count = candidate
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let sections: Vec<usize> = scope.sections.map_or_else(
        || (0..count).collect(),
        |sections| {
            sections
                .iter()
                .copied()
                .filter(|index| *index < count)
                .collect()
        },
    );
    let (reviewed, passages) = review_candidate(candidate, &sections);
    prompt.insert("candidate".into(), reviewed.clone());
    let excerpts = prompt
        .get_mut("authoringContext")
        .and_then(|context| WriterExcerpts::take(llm, context, candidate));
    let asked: Vec<&Asked> = scope.asked.into_iter().flatten().collect();
    let reading = |settle: bool| Reading {
        candidate,
        shown: &reviewed,
        passages: &passages,
        sections: &sections,
        asked: &asked,
        remit,
        settle,
    };
    if let Some(saved) = crate::lesson_drafts::saved::<String>(&key)
        .await?
        .and_then(|raw| read(&raw, &reading(true)).ok())
    {
        crate::lesson_progress::stage(
            "Reusing completed teaching review; continuing evidence checks",
        );
        return Ok(saved);
    }
    let brief = if asked.is_empty() {
        REVIEW_DRAFT_BRIEF.to_owned()
    } else {
        format!("{REVIEW_DRAFT_BRIEF}{OPEN_FINDINGS_BRIEF}")
    };
    let request = writer_excerpts::Request {
        subject: "the teaching review",
        system,
        prompt: &prompt,
        context: "authoringContext",
        schema: schema(
            &sections,
            &passages,
            &asked
                .iter()
                .map(|finding| finding.id.as_str())
                .collect::<Vec<_>>(),
            remit,
        ),
        output_tokens: REVIEW_ANSWER_TOKENS,
    };
    let call = ContractCall::of::<TeachingReview>(&PromptInput {
        system: system.to_owned(),
        prompt: Value::Object(prompt.clone()).to_string(),
    })?;
    let replies = AtomicUsize::new(0);
    let (raw, judged) = writer_excerpts::complete(
        llm,
        &call,
        &request,
        excerpts.as_ref(),
        progress,
        Some(ReasonFirst {
            kind: CallKind::Authoring,
            subject: "the teaching review",
            brief: &brief,
        }),
        |text| {
            let settle = replies.fetch_add(1, Ordering::SeqCst) > 0;
            read(text, &reading(settle)).map(|judged| (text.to_owned(), judged))
        },
    )
    .await?;
    crate::lesson_drafts::record_checkpoint(&key, json!(raw)).await?;
    Ok(judged)
}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod concern_tests;
mod concerns;
#[cfg(test)]
mod passage_tests;
mod passages;
#[cfg(test)]
mod section_list_tests;

#[cfg(test)]
mod tests;
