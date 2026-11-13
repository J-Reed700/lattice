//! The questions of a lesson or other material, reviewed as the quiz items
//! they are: the same blind solve and judge read that bank items get
//! (`item_review`), so one policy decides whether a question is sound.
use super::item_review::{self, Context, Entry};
use crate::assessment_engine::LearningAssessmentCandidate;
use crate::dto::{LessonAnswerKey, LessonQuestion, LessonQuestionKind};
use crate::lessons::teaching::contract;
use crate::review_findings::ReviewFinding;
use lattice_core::shared::hash::sha256_hex;
use lattice_core::{application::ports::LLMPort, shared::error::Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// What the questions are for.
pub(crate) struct Outcome<'a> {
    pub title: &'a str,
    pub description: String,
}

impl<'a> Outcome<'a> {
    /// The lesson an authoring context describes: its title, and its
    /// objective followed by what the course plan gives it to teach; blank
    /// when the context has none.
    pub fn of_lesson(authoring: &'a Value) -> Self {
        let field = |name: &str| {
            authoring
                .get("lesson")
                .and_then(|lesson| lesson.get(name))
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        let plan = authoring
            .pointer("/courseSequence/coursePlan")
            .and_then(Value::as_array);
        let planned: Vec<String> = contract::at_position(plan, "this")
            .next()
            .map(contract::items_of)
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.text)
            .collect();
        let objective = field("objective");
        let description = if planned.is_empty() {
            objective.to_owned()
        } else {
            format!("{objective} The lesson teaches: {}.", planned.join("; "))
                .trim_start()
                .to_owned()
        };
        Self {
            title: field("title"),
            description,
        }
    }
}

/// A question as the lesson's writer drafted it; fields beyond these are not
/// part of the item.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Drafted {
    kind: LessonQuestionKind,
    prompt: String,
    options: Vec<String>,
    correct_index: usize,
    explanation: String,
    #[serde(default)]
    rationales: Vec<String>,
    quote: Option<String>,
}

impl Drafted {
    fn item(&self, index: usize) -> LearningAssessmentCandidate {
        let id = format!("question-{index}");
        crate::quiz_bank_jobs::lesson_question_item(
            &LessonQuestion {
                id: id.clone(),
                kind: self.kind.clone(),
                prompt: self.prompt.clone(),
                options: self.options.clone(),
                source_ids: vec![],
            },
            &LessonAnswerKey {
                question_id: id,
                correct_index: self.correct_index,
                explanation: self.explanation.clone(),
                rationales: self.rationales.clone(),
            },
            vec![],
        )
    }
}

/// What review made of one question, saved under the question's own key.
/// `others` is the digest of the other questions' prompts, which a saved
/// repeat depends on.
#[derive(Serialize, Deserialize)]
struct Verdict {
    dispute: Option<String>,
    defects: Vec<String>,
    repeats: Vec<String>,
    others: String,
}

impl Verdict {
    fn settled(&self, others: &str) -> bool {
        self.repeats.is_empty() || self.others == others
    }
}

/// What this review decides, in the words the evidence gate is given.
pub(crate) const DECIDES: &str = "whether the key of a question is right";

/// One finding per question that review faults, placed at that question. A
/// question is reviewed once: its verdict is saved by what it says, and a
/// repeat is reviewed again when the questions around it change. A question
/// that cannot be read as an item is left to the structure checks.
pub(crate) async fn check(
    llm: &dyn LLMPort,
    candidate: &Value,
    outcome: Outcome<'_>,
) -> Result<Vec<ReviewFinding>> {
    let drafted: Vec<(usize, Drafted)> = candidate
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, q)| Some((index, serde_json::from_value(q.clone()).ok()?)))
        .collect();
    let keys: Vec<String> = drafted
        .iter()
        .map(|(_, question)| {
            format!(
                "question-review-v5:{}",
                sha256_hex(
                    json!({
                        "model": llm.model_name(),
                        "context": llm.max_context_tokens(),
                        "outcome": {"title": outcome.title, "description": outcome.description},
                        "question": question,
                    })
                    .to_string()
                )
            )
        })
        .collect();
    let others: Vec<String> = (0..drafted.len())
        .map(|at| {
            let prompts: Vec<&str> = drafted
                .iter()
                .enumerate()
                .filter(|(n, _)| *n != at)
                .map(|(_, (_, question))| question.prompt.as_str())
                .collect();
            sha256_hex(json!(prompts).to_string())
        })
        .collect();
    let mut verdicts: Vec<Option<Verdict>> = Vec::with_capacity(drafted.len());
    for (key, others) in keys.iter().zip(&others) {
        let saved = crate::lesson_drafts::checkpoint(key)
            .await?
            .and_then(|v| serde_json::from_value::<Verdict>(v).ok())
            .filter(|verdict| verdict.settled(others));
        verdicts.push(saved);
    }
    let open: Vec<usize> = (0..drafted.len())
        .filter(|at| verdicts.get(*at).is_some_and(Option::is_none))
        .collect();
    if open.is_empty() {
        if !drafted.is_empty() {
            crate::lesson_progress::stage(
                "Reusing completed question review for unchanged questions",
            );
        }
    } else {
        let learner_has: Vec<String> = drafted
            .iter()
            .zip(&verdicts)
            .filter(|(_, verdict)| verdict.is_some())
            .map(|((_, question), _)| question.prompt.clone())
            .collect();
        let fresh = review(llm, &drafted, &open, &outcome, &learner_has).await?;
        for (at, found) in open.iter().zip(fresh) {
            let verdict = Verdict {
                dispute: found.dispute,
                defects: found.defects,
                repeats: found.repeats.into_iter().map(|r| r.text).collect(),
                others: others.get(*at).cloned().unwrap_or_default(),
            };
            if let Some(key) = keys.get(*at) {
                crate::lesson_drafts::record_checkpoint(key, json!(verdict)).await?;
            }
            if let Some(slot) = verdicts.get_mut(*at) {
                *slot = Some(verdict);
            }
        }
    }
    Ok(drafted
        .iter()
        .zip(&verdicts)
        .filter_map(|((index, question), verdict)| {
            let verdict = verdict.as_ref()?;
            finding(
                *index,
                &question.prompt,
                verdict.dispute.as_deref(),
                &verdict.defects,
                &verdict.repeats,
            )
        })
        .collect())
}

/// The open questions reviewed as items, the settled ones standing as the
/// questions the learner already has: a finding list per open question.
async fn review(
    llm: &dyn LLMPort,
    drafted: &[(usize, Drafted)],
    open: &[usize],
    outcome: &Outcome<'_>,
    learner_has: &[String],
) -> Result<Vec<item_review::Findings>> {
    let asked: Vec<&(usize, Drafted)> = open.iter().filter_map(|at| drafted.get(*at)).collect();
    let items: Vec<LearningAssessmentCandidate> = asked
        .iter()
        .map(|(index, question)| question.item(*index))
        .collect();
    let entries: Vec<Entry<'_>> = items
        .iter()
        .zip(&asked)
        .map(|(item, (_, question))| Entry {
            item,
            quote: question.quote.as_deref().unwrap_or_default(),
        })
        .collect();
    let context = Context {
        outcome: outcome.title,
        outcome_description: &outcome.description,
        learner_has,
    };
    item_review::review(llm, &entries, &context).await
}

/// The finding for question `index`, placed at it; none when review found
/// nothing. A repeat is a finding on the question, not a defect of it.
fn finding(
    index: usize,
    prompt: &str,
    dispute: Option<&str>,
    defects: &[String],
    repeats: &[String],
) -> Option<ReviewFinding> {
    let texts: Vec<&str> = dispute
        .into_iter()
        .chain(defects.iter().map(String::as_str))
        .chain(repeats.iter().map(String::as_str))
        .collect();
    if texts.is_empty() {
        return None;
    }
    let summary = if dispute.is_some() && defects.is_empty() && repeats.is_empty() {
        "A second, independent solution disagreed with the answer key."
    } else {
        "The question review found a problem with this question."
    };
    let note = format!("Question {}: {}", index + 1, texts.join(" "));
    let mut finding = ReviewFinding::answer_key(index, note).with_summary(summary);
    finding.passage = Some(prompt.to_owned());
    Some(finding)
}
