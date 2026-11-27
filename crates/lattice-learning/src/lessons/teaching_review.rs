//! Bind review findings to app-owned passages, not model-transcribed quotations.
use crate::dto::LearningBlockKind;
use crate::lessons::structured_call::{Asking, CallKind, ReasonFirst};
use crate::lessons::trace::{self, Stage};
use crate::lessons::writer_excerpts::WriterExcerpts;
use crate::reference_collection::item_tokens;
use crate::review_findings::{FindingId, FindingSite, ReviewFinding};
use lattice_core::{
    application::ports::LLMPort,
    application::services::model_contract::{
        ask::ask_new,
        packed::Packed,
        shown::{self, Told},
    },
    protocol,
    shared::error::{AppError, Result},
    shared::hash::sha256_hex,
    shared::protocol::Protocol,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub(crate) use concerns::LessonConcern;
mod contract;
#[cfg(test)]
use contract::answer_form;
pub(crate) use contract::{
    Anything, Concerns, Entry, Input, Item, Lens, OpenFinding, Passage, TeachingReview,
};
#[cfg(test)]
pub(super) use contract::{ANSWER_FORM_HEAD, CONCERN_FORM_HEAD};

pub(super) const POLICY: Protocol = protocol!("section-review", 8);

/// The saved answers of the section review, by what it was given.
const FINDINGS_KEYS: Protocol = protocol!("teaching-review-findings", 8);

/// What the reviewer works through in prose before its findings are written
/// to the response schema, where each section's verdict would otherwise be
/// chosen as it is written.
const REVIEW_DRAFT_BRIEF: &str = "Inspect the candidate part by part: for a lesson, every section whose inReview is true and every question; for an outline, every module. Solve each exercise and question yourself and check each worked example against its stated premises and result. Record only actual material defects (with their passage id or part, concern, and why) — skip parts that pass cleanly instead of writing confirmations for them. If no material defects exist, state that clearly.";

/// The sentence the prose step is given when open findings are asked about.
const OPEN_FINDINGS_BRIEF: &str = " For each entry of openFindings, read the part it names as the candidate now has it, whether or not that part is in review. Find the words of that part that still show the defect, or, for something left out, the words it should follow. When no words of the part show it any more, the finding is resolved. An open finding's id is never used for a different defect: report that as a new issue.";

/// An open finding shown to the reviewer.
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
fn review_candidate(candidate: &Value, sections: &[usize]) -> (Value, Vec<Passage>) {
    let all = passages_of(candidate);
    let mut candidate = candidate.clone();
    if let Some(blocks) = candidate.get_mut("blocks").and_then(Value::as_array_mut) {
        for (index, block) in blocks.iter_mut().enumerate() {
            let passages: Vec<Value> = all
                .get(index)
                .into_iter()
                .flatten()
                .map(|(_, text)| json!({"text":text}))
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
    let pointable = all
        .into_iter()
        .enumerate()
        .filter(|(section, _)| sections.contains(section))
        .flat_map(|(section, passages)| {
            passages
                .into_iter()
                .enumerate()
                .map(move |(number, (id, _))| Passage {
                    id,
                    section,
                    number,
                })
        })
        .collect();
    (candidate, pointable)
}

/// What a review's verdicts stand under, besides its text: the policy, the
/// model and its window.
fn run_inputs(llm: &dyn LLMPort) -> Map<String, Value> {
    let mut inputs = Map::new();
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

/// The kind of section `index` of the candidate, when it can be read.
fn section_kind(candidate: &Value, index: usize) -> Option<LearningBlockKind> {
    candidate
        .get("blocks")?
        .get(index)?
        .get("kind")?
        .as_str()
        .and_then(LearningBlockKind::parse)
}

/// Whether a lesson's defect in section `index` names a concern its kind
/// takes; what it names beyond that, or does not name, is traced.
fn names_a_concern_taken(named: &[LessonConcern], index: usize, candidate: &Value) -> bool {
    let site = FindingSite::Section(index).label();
    let kind = section_kind(candidate, index);
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
    for message in &messages {
        trace::refused(
            Stage::TeachingReview,
            &format!("{site} defect"),
            message,
            None,
        );
    }
    kept > 0
}

/// What the answer's entries make of the review: the defects they report and
/// a verdict on each open finding. An open finding no entry resolves is
/// present only when the answer was read to its end.
fn judged(
    input: &Input,
    candidate: &Value,
    remit: Remit,
    entries: Vec<Entry>,
    complete: bool,
) -> Judged {
    let all = passages_of(candidate);
    let mut reported: Vec<Reported> = Vec::new();
    let mut resolved: Vec<FindingId> = Vec::new();
    for entry in entries {
        let found = match entry {
            Entry::Resolved(finding) => {
                if !resolved.contains(&finding.reference) {
                    resolved.push(finding.reference);
                }
                continue;
            }
            Entry::Issue { note, continues } => {
                if note.trim().is_empty() {
                    trace::refused(
                        Stage::TeachingReview,
                        "an issue",
                        "An issue must not be blank.",
                        None,
                    );
                    continue;
                }
                let summary = note.trim().to_owned();
                Reported {
                    site: None,
                    note,
                    summary,
                    passage: None,
                    passage_id: None,
                    continues: continues.map(|picked| picked.reference),
                }
            }
            Entry::Defect {
                at,
                finding,
                concerns,
                continues,
            } => {
                let Some(placed) = input
                    .passages
                    .iter()
                    .find(|passage| passage.id == at.reference)
                else {
                    continue;
                };
                let site = FindingSite::Section(placed.section);
                if finding.trim().is_empty() {
                    trace::refused(
                        Stage::TeachingReview,
                        &format!("{} defect", site.label()),
                        &format!("{}: finding must not be blank.", site.label()),
                        None,
                    );
                    continue;
                }
                if remit == Remit::Lesson
                    && !names_a_concern_taken(&concerns, placed.section, candidate)
                {
                    continue;
                }
                Reported {
                    site: Some(site),
                    note: format!("{}: {}", site.note_label(), finding),
                    summary: finding.trim().to_owned(),
                    passage: all
                        .get(placed.section)
                        .and_then(|passages| passages.get(placed.number))
                        .map(|(_, text)| text.clone()),
                    passage_id: Some(placed.id.clone()),
                    continues: continues.map(|picked| picked.reference),
                }
            }
        };
        if !reported.iter().any(|known| known.note == found.note) {
            reported.push(found);
        }
    }
    let verdicts = input
        .open
        .iter()
        .filter_map(|finding| {
            let present = if resolved.contains(&finding.id) {
                Some(false)
            } else {
                complete.then_some(true)
            };
            present.map(|present| Verdict {
                id: finding.id.clone(),
                present,
            })
        })
        .collect();
    Judged { verdicts, reported }
}

/// What a cut-off answer's standing entries already hold, in words.
fn already_reported<L: Lens>(standing: &[Item<L>]) -> Vec<String> {
    standing
        .iter()
        .filter_map(|item| match &item.0 {
            Entry::Issue { note, .. } => Some(note.clone()),
            Entry::Defect { finding, .. } => Some(finding.clone()),
            Entry::Resolved(_) => None,
        })
        .collect()
}

/// The entries the reviewer wrote, and whether its answer was read to its end.
async fn ask_entries<L: Lens>(llm: &dyn LLMPort, input: &Input) -> Result<(Vec<Entry>, bool)> {
    let brief = if input.open.is_empty() {
        REVIEW_DRAFT_BRIEF.to_owned()
    } else {
        format!("{REVIEW_DRAFT_BRIEF}{OPEN_FINDINGS_BRIEF}")
    };
    let found = ask_new::<TeachingReview<L>>(
        llm,
        &Asking {
            progress: None,
            reasoning: Some(ReasonFirst {
                kind: CallKind::Authoring,
                subject: "the teaching review",
                brief: &brief,
            }),
        },
        &mut shown::Scope::for_question(),
        input.clone(),
        |standing: &[Item<L>]| {
            let mut next = input.clone();
            next.reported.0.extend(already_reported(standing));
            let resolved_ids: std::collections::HashSet<&str> = standing
                .iter()
                .filter_map(|item| match &item.0 {
                    Entry::Resolved(id) => Some(id.reference.as_str()),
                    _ => None,
                })
                .collect();
            if !resolved_ids.is_empty() {
                next.open
                    .retain(|open| !resolved_ids.contains(open.id.as_str()));
            }
            Some(next)
        },
        &crate::model_call::answer_options(),
    )
    .await?;
    let complete = found.rest.is_none();
    Ok((
        found.items.into_iter().map(|item| item.0).collect(),
        complete,
    ))
}

/// What the reviewer found: its verdict on each finding in `scope.asked`,
/// and the defects it reports beyond those. A section defect is placed in
/// the section its passage is in, and a section with no entry has none; a
/// reviewer's issue is placed later, by the evidence check. An open finding
/// the reviewer does not resolve stays open, and is judged present only in
/// an answer read to its end; an answer that cannot be read at all fails
/// the review.
pub async fn review(
    llm: &dyn LLMPort,
    system: &str,
    prompt: Value,
    candidate: &Value,
    scope: Scope<'_>,
    remit: Remit,
) -> Result<Judged> {
    let Value::Object(mut prompt) = prompt else {
        return Err(AppError::InternalError(
            "Teaching review requires an object context.".into(),
        ));
    };
    prompt.shift_remove("candidate");
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
    let sources = match remit {
        Remit::Open => {
            let excerpts = prompt
                .get_mut("authoringContext")
                .and_then(|context| WriterExcerpts::take(llm, context, candidate));
            Packed::new(
                excerpts
                    .map(|excerpts| excerpts.ranked())
                    .unwrap_or_default()
                    .into_iter()
                    .map(Told)
                    .collect(),
                |source: &Told<Value>| item_tokens(llm, &source.0),
            )
        }
        Remit::Lesson => {
            if let Some(Value::Object(context)) = prompt.get_mut("authoringContext") {
                context.remove("sources");
                context.remove("referenceCatalog");
            }
            Packed::new(Vec::new(), |source: &Told<Value>| {
                item_tokens(llm, &source.0)
            })
        }
    };
    let open: Vec<OpenFinding> = scope
        .asked
        .into_iter()
        .flatten()
        .map(|asked| OpenFinding {
            id: asked.id.clone(),
            part: Told(asked.site.label()),
            passage: asked
                .passage_id
                .clone()
                .filter(|id| passages.iter().any(|passage| &passage.id == id)),
            note: Told(asked.note.clone()),
        })
        .collect();
    let input = Input {
        system: Told(system.to_owned()),
        prompt: Told(Value::Object(prompt)),
        candidate: Told(reviewed),
        passages,
        sources,
        open,
        reported: Told(Vec::new()),
    };
    // Save findings, including defects, before the later grounding/repair step.
    // Reusing a completed review never approves the lesson or skips grounding.
    let mut inputs = run_inputs(llm);
    inputs.insert("input".into(), json!(input));
    let key = FINDINGS_KEYS.key(sha256_hex(Value::Object(inputs).to_string()));
    if let Some(saved) = crate::lesson_drafts::saved::<Judged>(&key).await? {
        crate::lesson_progress::stage(
            "Reusing completed teaching review; continuing evidence checks",
        );
        return Ok(saved);
    }
    let (entries, complete) = match remit {
        Remit::Open => ask_entries::<Anything>(llm, &input).await?,
        Remit::Lesson => ask_entries::<Concerns>(llm, &input).await?,
    };
    let found = judged(&input, candidate, remit, entries, complete);
    if complete {
        crate::lesson_drafts::record_checkpoint(&key, json!(found)).await?;
    }
    Ok(found)
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
pub(crate) mod scripted;
#[cfg(test)]
mod tests;
