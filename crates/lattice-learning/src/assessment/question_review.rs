//! The questions of a lesson or other material, reviewed as the quiz items
//! they are: the same blind solve and judge read that bank items get
//! (`item_review`), so one policy decides whether a question is sound.
use super::item_review::{self, Context, Entry};
use crate::assessment_engine::LearningAssessmentCandidate;
use crate::dto::{LessonAnswerKey, LessonQuestion, LessonQuestionKind};
use crate::review_findings::ReviewFinding;
use lattice_core::{application::ports::LLMPort, shared::error::Result};
use serde::Deserialize;
use serde_json::{json, Value};

/// What the questions are for.
pub(crate) struct Outcome<'a> {
    pub title: &'a str,
    pub description: &'a str,
}

impl<'a> Outcome<'a> {
    /// The lesson an authoring context describes: its title and objective,
    /// blank when the context has none.
    pub fn of_lesson(authoring: &'a Value) -> Self {
        let field = |name: &str| {
            authoring
                .get("lesson")
                .and_then(|lesson| lesson.get(name))
                .and_then(Value::as_str)
                .unwrap_or_default()
        };
        Self {
            title: field("title"),
            description: field("objective"),
        }
    }
}

/// A question as the lesson's writer drafted it; fields beyond these are not
/// part of the item.
#[derive(Deserialize)]
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

/// One finding per question that review faults, placed at that question. A
/// question that cannot be read as an item is left to the structure checks.
pub(crate) async fn check(
    llm: &dyn LLMPort,
    candidate: &Value,
    outcome: Outcome<'_>,
) -> Result<Vec<ReviewFinding>> {
    let key = format!(
        "question-review-v1:{}",
        crate::content_verification::digest(
            &json!({
                "model": llm.model_name(),
                "context": llm.max_context_tokens(),
                "questions": candidate.get("questions"),
                "outcome": {"title": outcome.title, "description": outcome.description},
            })
            .to_string()
        )
    );
    if let Some(saved) = crate::lesson_drafts::checkpoint(&key)
        .await?
        .and_then(|v| serde_json::from_value::<Vec<ReviewFinding>>(v).ok())
    {
        crate::lesson_progress::stage("Reusing completed question review for unchanged questions");
        return Ok(saved);
    }
    let issues = review(llm, candidate, &outcome).await?;
    crate::lesson_drafts::record_checkpoint(&key, json!(issues)).await?;
    Ok(issues)
}

async fn review(
    llm: &dyn LLMPort,
    candidate: &Value,
    outcome: &Outcome<'_>,
) -> Result<Vec<ReviewFinding>> {
    let drafted: Vec<(usize, Drafted)> = candidate
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, q)| Some((index, serde_json::from_value(q.clone()).ok()?)))
        .collect();
    if drafted.is_empty() {
        return Ok(vec![]);
    }
    let items: Vec<LearningAssessmentCandidate> = drafted
        .iter()
        .map(|(index, question)| question.item(*index))
        .collect();
    let entries: Vec<Entry<'_>> = items
        .iter()
        .zip(&drafted)
        .map(|(item, (_, question))| Entry {
            item,
            quote: question.quote.as_deref().unwrap_or_default(),
        })
        .collect();
    let context = Context {
        outcome: outcome.title,
        outcome_description: outcome.description,
        learner_has: &[],
    };
    let findings = item_review::review(llm, &entries, &context).await?;
    Ok(drafted
        .iter()
        .zip(findings)
        .filter(|(_, found)| !found.is_sound())
        .map(|((index, question), found)| {
            let note = format!("Question {}: {}", index + 1, found.texts().join(" "));
            let summary = if found.dispute.is_some()
                && found.defects.is_empty()
                && found.repeats.is_empty()
            {
                "A second, independent solution disagreed with the answer key."
            } else {
                "The question review found a defect in this question."
            };
            let mut finding = ReviewFinding::answer_key(*index, note).with_summary(summary);
            finding.passage = Some(question.prompt.clone());
            finding
        })
        .collect())
}
