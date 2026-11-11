//! A lesson's questions reviewed as quiz items: a scripted blind solver and
//! item judge, and the requests between them.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::question_review::{check, Outcome};
use crate::review_findings::FindingSite;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use serde_json::{json, Value};
use std::sync::Mutex;

type Solves = Box<dyn Fn(usize) -> usize + Send + Sync>;
type Judges = Box<dyn Fn(&Value) -> (String, Vec<String>) + Send + Sync>;

/// Solves and judges with the closures it is given and keeps every request.
/// A request of any other kind is answered as the earlier answer-key check
/// was, with the same picks as the solver.
struct Script {
    solves: Solves,
    judges: Judges,
    seen: Mutex<Vec<(&'static str, String)>>,
}

#[async_trait::async_trait]
impl LLMPort for Script {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let kind = if system.starts_with("Solve quiz questions") {
            "solve"
        } else if system.starts_with("Review quiz questions") {
            "judge"
        } else {
            "other"
        };
        self.seen.lock().unwrap().push((kind, user.to_owned()));
        let context: Value = serde_json::from_str(user)
            .or_else(|_| serde_json::from_str(user.rsplit("\n\n").next().unwrap_or("")))
            .unwrap_or(Value::Null);
        let reply = match kind {
            "solve" => {
                json!({"answers": context["questions"].as_array().into_iter().flatten().map(|q| json!({
                "index": q["index"],
                "reason": "The case decides it.",
                "correctIndices": [(self.solves)(q["index"].as_u64().unwrap_or(0) as usize)],
                "answer": "", "blankAnswers": [], "matches": [], "orderedValues": []
            })).collect::<Vec<_>>()})
            }
            "judge" => {
                json!({"checks": context["items"].as_array().into_iter().flatten().map(|item| {
                let (finding, repeats) = (self.judges)(item);
                json!({"id": item["id"], "hasDefect": !finding.is_empty(), "finding": finding, "repeats": repeats})
            }).collect::<Vec<_>>()})
            }
            _ => {
                json!({"answers": context["requestedIndices"].as_array().into_iter().flatten().map(|i| json!({
                "index": i,
                "correctIndices": [(self.solves)(i.as_u64().unwrap_or(0) as usize)],
                "reason": "The case decides it."
            })).collect::<Vec<_>>()})
            }
        };
        Ok(CompletionResponse::from_text(reply.to_string()))
    }
    fn model_name(&self) -> &str {
        "question-review-script"
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
    fn new(solves: Solves, judges: Judges) -> Self {
        Self {
            solves,
            judges,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn agreeing() -> Solves {
        Box::new(|_| 0)
    }

    fn sound() -> Judges {
        Box::new(|_| (String::new(), vec![]))
    }

    fn kinds(&self) -> Vec<&'static str> {
        self.seen.lock().unwrap().iter().map(|(k, _)| *k).collect()
    }

    fn request(&self, kind: &str) -> Option<Value> {
        let seen = self.seen.lock().unwrap();
        let (_, user) = seen.iter().find(|(k, _)| *k == kind)?;
        serde_json::from_str(user).ok()
    }
}

fn question(prompt: &str) -> Value {
    json!({
        "kind": "quiz",
        "prompt": prompt,
        "options": ["Alpha", "Bravo", "Charlie", "Delta"],
        "correctIndex": 0,
        "rationales": ["", "Bravo takes the other case.", "Charlie takes the other case.", "Delta takes the other case."],
        "explanation": "Alpha takes this case.",
        "quote": "A passage the question rests on."
    })
}

fn lesson() -> Value {
    json!({"questions": [
        question("Which one takes the first case?"),
        question("Which one takes the second case?"),
        question("Which one takes the third case?")
    ]})
}

fn outcome() -> Outcome<'static> {
    Outcome {
        title: "Sorting cases",
        description: "Say which one takes a case.",
    }
}

const STANDS_APART: &str = "One option stands apart from the others in length.";

#[tokio::test]
async fn a_question_whose_key_stands_apart_is_a_finding_at_that_question() -> Result<()> {
    let llm = Script::new(
        Script::agreeing(),
        Box::new(|item| {
            if item["id"] == "new-1" {
                (STANDS_APART.into(), vec![])
            } else {
                (String::new(), vec![])
            }
        }),
    );

    let found = check(&llm, &lesson(), outcome()).await?;

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].site, Some(FindingSite::Question(1)));
    assert!(
        found[0].note.starts_with("Question 2: "),
        "{}",
        found[0].note
    );
    assert!(found[0].note.contains(STANDS_APART), "{}", found[0].note);
    Ok(())
}

/// Passes before the change as well: the earlier check disputed a key the same way.
#[tokio::test]
async fn a_disputed_key_is_a_finding_at_its_question() -> Result<()> {
    let llm = Script::new(
        Box::new(|index| if index == 2 { 1 } else { 0 }),
        Script::sound(),
    );

    let found = check(&llm, &lesson(), outcome()).await?;

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].site, Some(FindingSite::Question(2)));
    assert_eq!(
        found[0].summary,
        "A second, independent solution disagreed with the answer key."
    );
    Ok(())
}

#[tokio::test]
async fn a_question_that_repeats_an_earlier_one_is_a_finding_on_the_later_one() -> Result<()> {
    let llm = Script::new(
        Script::agreeing(),
        Box::new(|item| {
            if item["id"] == "new-2" {
                (String::new(), vec!["new-0".into()])
            } else {
                (String::new(), vec![])
            }
        }),
    );

    let found = check(&llm, &lesson(), outcome()).await?;

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].site, Some(FindingSite::Question(2)));
    assert!(found[0].note.contains("Which one takes the first case?"));
    Ok(())
}

#[tokio::test]
async fn sound_questions_have_no_finding_and_unchanged_questions_are_not_reviewed_again(
) -> Result<()> {
    let pool = crate::test_support::pool().await?;
    let (repo, job, lesson_id) = crate::content_verification::test_support::setup(&pool).await?;
    let llm = Script::new(Script::agreeing(), Script::sound());

    crate::lesson_drafts::run(
        &Database::from(pool.clone()),
        &repo,
        &job,
        &lesson_id,
        async {
            assert!(check(&llm, &lesson(), outcome()).await?.is_empty());
            assert_eq!(llm.kinds(), ["solve", "judge"]);
            assert!(check(&llm, &lesson(), outcome()).await?.is_empty());
            assert_eq!(llm.kinds(), ["solve", "judge"]);
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn the_review_reads_a_lesson_question_as_the_item_the_bank_seeds() -> Result<()> {
    use crate::dto::{LessonAnswerKey, LessonQuestion, LessonQuestionKind};
    let drafted = question("Which one takes the first case?");
    let options: Vec<String> = serde_json::from_value(drafted["options"].clone())?;
    let rationales: Vec<String> = serde_json::from_value(drafted["rationales"].clone())?;
    let seeded = crate::quiz_bank_jobs::lesson_seed_items(
        "lesson",
        "outcome",
        &[LessonQuestion {
            id: "question-0".into(),
            kind: LessonQuestionKind::Quiz,
            prompt: "Which one takes the first case?".into(),
            options,
            source_ids: vec![],
        }],
        &[LessonAnswerKey {
            question_id: "question-0".into(),
            correct_index: 0,
            explanation: "Alpha takes this case.".into(),
            rationales,
        }],
    );
    let llm = Script::new(Script::agreeing(), Script::sound());

    check(&llm, &json!({"questions": [drafted]}), outcome())
        .await
        .ok();

    let judged = llm.request("judge");
    assert!(judged.is_some(), "{:?}", llm.kinds());
    let shown = &judged.unwrap()["items"][0];
    let item = &seeded[0].item;
    assert_eq!(shown["prompt"], item.prompt);
    assert_eq!(shown["options"], json!(item.options));
    assert_eq!(shown["key"], item.options[0]);
    assert_eq!(shown["explanation"], item.explanation);
    let reasons: Vec<Value> = item
        .options
        .iter()
        .zip(&item.distractor_rationales)
        .filter(|(_, reason)| !reason.is_empty())
        .map(|(option, reason)| json!({"option": option, "reason": reason}))
        .collect();
    assert_eq!(shown["reasons"], json!(reasons));
    Ok(())
}
