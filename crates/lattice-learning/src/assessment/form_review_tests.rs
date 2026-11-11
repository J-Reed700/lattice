//! The form read beside the blind solver and the key-aware judge: what the
//! form read is shown, which questions it is asked about, and what a named
//! option makes of a question's findings.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::item_review::{self, Context, Entry, ITEM_REVIEW_SYSTEM};
use super::question_review::Outcome;
use crate::assessment_engine::{
    LearningAssessmentCandidate as Item, LearningAssessmentKey as Key, LearningItemFormat,
};
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use serde_json::{json, Value};
use std::sync::Mutex;

const EXPLANATION: &str = "Alpha holds the line, so it is the one.";
const RATIONALE: &str = "Bravo gives way at the line.";
const WHY: &str = "Alpha alone repeats the question's words.";

type Forms = Box<dyn Fn(&Value) -> Option<usize> + Send + Sync>;

/// Agrees with every key, finds nothing in any item, and answers the form
/// read with the option its closure picks (or none); keeps every request.
struct Script {
    forms: Forms,
    seen: Mutex<Vec<(&'static str, String)>>,
}

#[async_trait::async_trait]
impl LLMPort for Script {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let kind = if system.starts_with("Solve quiz questions") {
            "solve"
        } else if system.starts_with("You are shown multiple-choice questions") {
            "form"
        } else if system.starts_with("Review quiz questions") {
            "judge"
        } else {
            "other"
        };
        self.seen.lock().unwrap().push((kind, user.to_owned()));
        let context: Value = serde_json::from_str(user).unwrap_or(Value::Null);
        let reply = match kind {
            "solve" => {
                json!({"answers": context["questions"].as_array().into_iter().flatten().map(|q| json!({
                "index": q["index"], "reason": "The case decides it.", "correctIndices": [0],
                "answer": "", "blankAnswers": [], "matches": [], "orderedValues": []
            })).collect::<Vec<_>>()})
            }
            "form" => {
                json!({"answers": context["questions"].as_array().into_iter().flatten().map(|q| {
                let apart = (self.forms)(q);
                let why = if apart.is_some() { WHY } else { "" };
                json!({"index": q["index"], "standsApart": apart, "why": why})
            }).collect::<Vec<_>>()})
            }
            "judge" => {
                json!({"checks": context["items"].as_array().into_iter().flatten().map(|item| json!({
                "id": item["id"], "hasDefect": false, "finding": "", "repeats": []
            })).collect::<Vec<_>>()})
            }
            _ => json!({}),
        };
        Ok(CompletionResponse::from_text(reply.to_string()))
    }
    fn model_name(&self) -> &str {
        "form-review-script"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

impl Script {
    fn new(forms: Forms) -> Self {
        Self {
            forms,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self, kind: &str) -> Vec<(String, Value)> {
        let seen = self.seen.lock().unwrap();
        seen.iter()
            .filter(|(k, _)| *k == kind)
            .map(|(_, user)| (user.clone(), serde_json::from_str(user).unwrap()))
            .collect()
    }
}

fn item(prompt: &str, format: LearningItemFormat, key: Key) -> Item {
    Item {
        id: uuid::Uuid::new_v4().to_string(),
        outcome_ids: vec![],
        format,
        difficulty: 3,
        prompt: prompt.into(),
        options: vec![
            "Alpha".into(),
            "Bravo".into(),
            "Charlie".into(),
            "Delta".into(),
        ],
        left: vec![],
        source_version_ids: vec![],
        rubric: vec![],
        key,
        explanation: EXPLANATION.into(),
        distractor_rationales: vec![
            String::new(),
            RATIONALE.into(),
            "Charlie gives way at the line.".into(),
            "Delta gives way at the line.".into(),
        ],
    }
}

fn single(prompt: &str) -> Item {
    item(prompt, LearningItemFormat::SingleChoice, Key::Choice(0))
}

async fn review(llm: &Script, items: &[Item]) -> Result<Vec<item_review::Findings>> {
    let entries: Vec<Entry<'_>> = items
        .iter()
        .map(|item| Entry {
            item,
            quote: "A passage the question rests on.",
        })
        .collect();
    let context = Context {
        outcome: "Holding a line",
        outcome_description: "Say which one holds the line.",
        learner_has: &[],
    };
    item_review::review(llm, &entries, &context).await
}

#[tokio::test]
async fn a_form_read_that_names_the_right_option_is_a_defect_carrying_its_reason() -> Result<()> {
    let llm = Script::new(Box::new(|_| Some(0)));
    let found = review(&llm, &[single("Which one holds the line?")]).await?;

    assert_eq!(llm.requests("form").len(), 1);
    assert_eq!(found[0].defects.len(), 1, "{:?}", found[0]);
    assert!(found[0].defects[0].contains(WHY), "{}", found[0].defects[0]);
    Ok(())
}

#[tokio::test]
async fn a_form_read_that_names_a_wrong_option_is_no_defect() -> Result<()> {
    let llm = Script::new(Box::new(|_| Some(2)));
    let found = review(&llm, &[single("Which one holds the line?")]).await?;

    assert_eq!(llm.requests("form").len(), 1);
    assert!(found[0].defects.is_empty(), "{:?}", found[0]);
    Ok(())
}

#[tokio::test]
async fn a_form_read_that_names_no_option_is_no_defect() -> Result<()> {
    let llm = Script::new(Box::new(|_| None));
    let found = review(&llm, &[single("Which one holds the line?")]).await?;

    assert_eq!(llm.requests("form").len(), 1);
    assert!(found[0].defects.is_empty(), "{:?}", found[0]);
    Ok(())
}

#[tokio::test]
async fn the_form_read_is_shown_each_question_as_index_prompt_and_options_only() -> Result<()> {
    let llm = Script::new(Box::new(|_| None));
    review(
        &llm,
        &[
            single("Which one holds the line?"),
            single("Which one yields?"),
        ],
    )
    .await?;

    let requests = llm.requests("form");
    assert_eq!(requests.len(), 1);
    let (raw, parsed) = &requests[0];
    let top: Vec<&str> = parsed
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(top, ["questions"]);
    let questions = parsed["questions"].as_array().unwrap();
    assert_eq!(questions.len(), 2);
    for question in questions {
        let mut keys: Vec<&str> = question
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort();
        assert_eq!(keys, ["index", "options", "prompt"]);
    }
    assert!(!raw.contains(EXPLANATION), "{raw}");
    assert!(!raw.contains(RATIONALE), "{raw}");
    assert!(!raw.contains("A passage the question rests on."), "{raw}");
    assert!(!raw.contains("The case decides it."), "{raw}");
    Ok(())
}

#[tokio::test]
async fn only_single_choice_questions_with_one_right_option_are_sent_to_the_form_read() -> Result<()>
{
    let llm = Script::new(Box::new(|_| None));
    let items = [
        item(
            "Which of these hold the line?",
            LearningItemFormat::MultiSelect,
            Key::Choices([0, 1].into()),
        ),
        single("Which one holds the line?"),
        item(
            "Explain why the line holds.",
            LearningItemFormat::Explanation,
            Key::Rubric,
        ),
    ];
    review(&llm, &items).await?;

    let requests = llm.requests("form");
    assert_eq!(requests.len(), 1);
    let questions = requests[0].1["questions"].as_array().unwrap();
    assert_eq!(questions.len(), 1, "{}", requests[0].0);
    assert_eq!(questions[0]["prompt"], "Which one holds the line?");

    let none_sent = Script::new(Box::new(|_| None));
    review(&none_sent, &items[2..]).await?;
    assert!(none_sent.requests("form").is_empty());
    Ok(())
}

#[tokio::test]
async fn an_answer_naming_no_option_of_the_question_is_asked_for_again_then_fails() {
    let llm = Script::new(Box::new(|_| Some(9)));
    let found = review(&llm, &[single("Which one holds the line?")]).await;

    assert_eq!(llm.requests("form").len(), 2);
    assert!(found.is_err());
}

#[test]
fn the_quiz_bank_writer_is_asked_for_choices_alike_in_form() {
    let system = super::assessment_generation::SYSTEM;
    assert!(system.contains("Write all the choices of a question alike in form: close in length and detail, the same grammatical shape and opening, none alone hedged or absolute, none alone carrying a reason or an example, and none repeating the question's distinctive words, so that the right one cannot be told from the others without knowing the material."));
    assert!(!system.contains("distractors of similar length"));
}

#[test]
fn the_key_aware_judge_is_not_asked_whether_an_option_stands_apart() {
    assert!(!ITEM_REVIEW_SYSTEM.contains("in length, detail or wording"));
    assert!(ITEM_REVIEW_SYSTEM.contains("do not report the form, length or wording"));
}

#[test]
fn a_lessons_outcome_includes_what_the_course_plan_gives_it_to_teach() {
    let authoring = json!({
        "lesson": {"title": "Holding a line", "objective": "Say which one holds the line."},
        "courseSequence": {"coursePlan": [
            {"position": "earlier", "ideas": ["Charlie came before"]},
            {"position": "this", "ideas": ["Alpha holds the line"], "terms": ["Bravo gives way"], "actions": []},
            {"position": "later", "ideas": ["Delta comes after"]}
        ]}
    });

    let outcome = Outcome::of_lesson(&authoring);

    assert_eq!(outcome.title, "Holding a line");
    assert!(outcome
        .description
        .contains("Say which one holds the line."));
    assert!(outcome.description.contains("Alpha holds the line"));
    assert!(outcome.description.contains("Bravo gives way"));
    assert!(!outcome.description.contains("Charlie came before"));
    assert!(!outcome.description.contains("Delta comes after"));
}
