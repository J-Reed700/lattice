//! A lesson's questions reviewed as quiz items: a scripted blind solver and
//! item judge, and the requests between them.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::question_review::{check, Outcome};
use crate::review_findings::FindingSite;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use serde_json::{json, Value};
use std::sync::Mutex;

type Solves = Box<dyn Fn(usize) -> usize + Send + Sync>;
type Judges = Box<dyn Fn(&Value) -> (String, Vec<String>) + Send + Sync>;

/// Solves and judges with the closures it is given and keeps every request;
/// any other request fails the test.
struct Script {
    solves: Solves,
    judges: Judges,
    seen: Mutex<Vec<(&'static str, String)>>,
    schemas: Mutex<Vec<Value>>,
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
        self.schemas
            .lock()
            .unwrap()
            .extend(request.json_schema.clone());
        let context: Value = serde_json::from_str(user).unwrap_or(Value::Null);
        let reply = match kind {
            "solve" => {
                json!({"answers": context["questions"].as_array().into_iter().flatten().map(|q| json!({
                "index": q["index"],
                "reason": "The case decides it.",
                "correctIndices": [(self.solves)(q["index"].as_u64().unwrap_or(0) as usize)],
                "answer": "", "blankAnswers": [], "matches": [], "orderedValues": []
            })).collect::<Vec<_>>()})
            }
            "form" => {
                json!({"answers": context["questions"].as_array().into_iter().flatten().map(|q| json!({
                "index": q["index"], "standsApart": null, "why": ""
            })).collect::<Vec<_>>()})
            }
            "judge" => {
                json!({"checks": context["items"].as_array().into_iter().flatten().map(|item| {
                let (finding, repeats) = (self.judges)(item);
                json!({"id": item["id"], "hasDefect": !finding.is_empty(), "finding": finding, "repeats": repeats})
            }).collect::<Vec<_>>()})
            }
            _ => return Err(AppError::InvalidInput("an unscripted request".into())),
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
            schemas: Mutex::new(Vec::new()),
        }
    }

    fn agreeing() -> Solves {
        Box::new(|_| 0)
    }

    fn sound() -> Judges {
        Box::new(|_| (String::new(), vec![]))
    }

    /// The kinds of request in order, the form read left out: it runs beside the solver.
    fn kinds(&self) -> Vec<&'static str> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| *k != "form")
            .collect()
    }

    fn requests(&self, kind: &str) -> Vec<Value> {
        let seen = self.seen.lock().unwrap();
        seen.iter()
            .filter(|(k, _)| *k == kind)
            .filter_map(|(_, user)| serde_json::from_str(user).ok())
            .collect()
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
        description: "Say which one takes a case.".into(),
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

/// A solver that disagrees with the key is a finding at that question.
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

#[tokio::test]
async fn the_question_review_asks_for_no_text_length() -> Result<()> {
    use crate::practice::practice_tests::text_length_rules;
    let llm = Script::new(Script::agreeing(), Script::sound());

    check(&llm, &lesson(), outcome()).await?;

    let schemas = llm.schemas.lock().unwrap().clone();
    assert!(schemas.len() >= 2);
    for schema in &schemas {
        let rules = text_length_rules(schema);
        assert!(rules.is_empty(), "{rules:?}");
    }
    Ok(())
}

fn lesson_of(prompts: [&str; 3]) -> Value {
    json!({"questions": prompts.iter().map(|p| question(p)).collect::<Vec<_>>()})
}

const FIRST: &str = "Which one takes the first case?";
const SECOND: &str = "Which one takes the second case?";
const THIRD: &str = "Which one takes the third case?";
const CHANGED: &str = "Which one takes the changed case?";

async fn in_a_task<T>(body: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    let pool = crate::test_support::pool().await?;
    let (repo, job, lesson_id) = crate::content_verification::test_support::setup(&pool).await?;
    crate::lesson_drafts::run(&Database::from(pool.clone()), &repo, &job, &lesson_id, body).await
}

fn prompts_of(request: &Value, field: &str) -> Vec<String> {
    request[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["prompt"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_question_that_passed_is_not_reviewed_again_when_another_changes() -> Result<()> {
    let llm = Script::new(Script::agreeing(), Script::sound());

    in_a_task(async {
        assert!(check(&llm, &lesson_of([FIRST, SECOND, THIRD]), outcome())
            .await?
            .is_empty());
        assert!(check(&llm, &lesson_of([FIRST, CHANGED, THIRD]), outcome())
            .await?
            .is_empty());
        Ok(())
    })
    .await?;

    let solves = llm.requests("solve");
    let judges = llm.requests("judge");
    assert_eq!(solves.len(), 2);
    assert_eq!(solves[1]["questions"].as_array().unwrap().len(), 1);
    assert_eq!(judges[1]["items"].as_array().unwrap().len(), 1);
    assert_eq!(prompts_of(&judges[1], "learnerHas"), [FIRST, THIRD]);
    Ok(())
}

#[tokio::test]
async fn a_changed_question_that_repeats_an_unchanged_one_is_a_finding_on_the_changed_one(
) -> Result<()> {
    let llm = Script::new(
        Script::agreeing(),
        Box::new(|item| {
            if item["prompt"] == CHANGED {
                (String::new(), vec!["have-1".into()])
            } else {
                (String::new(), vec![])
            }
        }),
    );

    let found = in_a_task(async {
        assert!(check(&llm, &lesson_of([FIRST, SECOND, THIRD]), outcome())
            .await?
            .is_empty());
        check(&llm, &lesson_of([CHANGED, SECOND, THIRD]), outcome()).await
    })
    .await?;

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].site, Some(FindingSite::Question(0)));
    assert!(found[0].note.contains(THIRD), "{}", found[0].note);
    Ok(())
}

/// Passes before the change as well: a set reviewed as it stands is not reviewed again.
#[tokio::test]
async fn a_faulted_question_keeps_its_finding_without_another_review() -> Result<()> {
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

    let (first, second) = in_a_task(async {
        let first = check(&llm, &lesson(), outcome()).await?;
        let sent = llm.kinds().len();
        let second = check(&llm, &lesson(), outcome()).await?;
        assert_eq!(llm.kinds().len(), sent);
        Ok((first, second))
    })
    .await?;

    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(second.len(), 1, "{second:?}");
    assert_eq!(second[0].site, first[0].site);
    assert_eq!(second[0].note, first[0].note);
    Ok(())
}

#[tokio::test]
async fn a_repeat_is_reviewed_again_when_the_question_it_repeats_changes() -> Result<()> {
    let repeating = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let judging = repeating.clone();
    let llm = Script::new(
        Script::agreeing(),
        Box::new(move |item| {
            let repeats = judging.load(std::sync::atomic::Ordering::SeqCst);
            if repeats && item["prompt"] == THIRD {
                (String::new(), vec!["new-0".into()])
            } else {
                (String::new(), vec![])
            }
        }),
    );

    let (first, second) = in_a_task(async {
        let first = check(&llm, &lesson_of([FIRST, SECOND, THIRD]), outcome()).await?;
        repeating.store(false, std::sync::atomic::Ordering::SeqCst);
        let second = check(&llm, &lesson_of([CHANGED, SECOND, THIRD]), outcome()).await?;
        Ok((first, second))
    })
    .await?;

    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].site, Some(FindingSite::Question(2)));
    let solves = llm.requests("solve");
    assert_eq!(solves[1]["questions"].as_array().unwrap().len(), 2);
    assert!(second.is_empty(), "{second:?}");
    Ok(())
}
