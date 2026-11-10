//! `author_items` reviewing each written question as a question: a scripted
//! writer, a blind solver and an item judge, and the requests between them.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::assessment_engine::LearningBlueprintRequirement;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

const PASSAGE: &str =
    "A stamp goes into the box that matches its colour, and a torn stamp goes into the repair tray.";
const QUOTE: &str = "goes into the box that matches its colour";
const RIGHT: &str = "The repair tray";
const FIRST: &str = "A torn blue stamp arrives at the desk. Which place takes it first?";
const SECOND: &str = "A torn red stamp arrives at the desk. Which place takes it first?";
const FIRST_FIXED: &str = "A torn blue stamp arrives at the desk with its colour still readable. Which place takes it first?";
const SECOND_FIXED: &str = "A torn red stamp arrives at the desk with its colour still readable. Which place takes it first?";

type Reply = Box<dyn Fn(&Value) -> Value + Send + Sync>;

/// Writes from a queue (an empty queue writes no items), solves and judges
/// with the closures it is given, and keeps every request in order.
struct Script {
    writes: Mutex<VecDeque<Value>>,
    solve: Reply,
    judge: Reply,
    seen: Mutex<Vec<(&'static str, String)>>,
}

#[async_trait::async_trait]
impl LLMPort for Script {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (system, user) = crate::lessons::structured_call::task_messages(request);
        let kind = if system.starts_with("Write quiz questions") {
            "write"
        } else if system.starts_with("Solve quiz questions") {
            "solve"
        } else if system.starts_with("Review quiz questions") {
            "judge"
        } else {
            "other"
        };
        self.seen.lock().unwrap().push((kind, user.to_owned()));
        let context: Value = serde_json::from_str(user).unwrap_or(Value::Null);
        let reply = match kind {
            "write" => self
                .writes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| json!({"items": []})),
            "solve" => (self.solve)(&context),
            "judge" => (self.judge)(&context),
            _ => return Err(AppError::InvalidInput("an unscripted request".into())),
        };
        Ok(CompletionResponse::from_text(reply.to_string()))
    }
    fn model_name(&self) -> &str {
        "item-review-script"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

impl Script {
    fn new(writes: Vec<Value>, solve: Reply, judge: Reply) -> Self {
        Self {
            writes: Mutex::new(writes.into()),
            solve,
            judge,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn kinds(&self) -> Vec<&'static str> {
        self.seen.lock().unwrap().iter().map(|(k, _)| *k).collect()
    }

    fn raw(&self, kind: &str, nth: usize) -> String {
        let seen = self.seen.lock().unwrap();
        seen.iter()
            .filter(|(k, _)| *k == kind)
            .nth(nth)
            .map(|(_, user)| user.clone())
            .unwrap()
    }

    fn request(&self, kind: &str, nth: usize) -> Value {
        serde_json::from_str(&self.raw(kind, nth)).unwrap()
    }

    fn no_lesson_review(&self) {
        assert!(
            !self.kinds().contains(&"other"),
            "a request other than write, solve and judge was made: {:?}",
            self.kinds()
        );
    }
}

fn need(format: LearningItemFormat, count: usize, existing: &[&str]) -> AuthoringNeed {
    AuthoringNeed {
        requirement: LearningBlueprintRequirement {
            outcome_id: uuid::Uuid::new_v4().to_string(),
            format,
            count,
            difficulty_min: 2,
            difficulty_max: 3,
        },
        outcome_title: "Sorting stamps".into(),
        outcome_description: "Say where a stamp goes by its colour and condition.".into(),
        lesson_id: Some("lesson".into()),
        count,
        material: vec![MaterialPassage {
            lesson_title: "Stamps".into(),
            title: "The boxes".into(),
            text: PASSAGE.into(),
            source_version_ids: vec![],
        }],
        existing_prompts: existing.iter().map(|p| (*p).to_owned()).collect(),
    }
}

fn choice(prompt: &str) -> Value {
    json!({
        "prompt": prompt,
        "options": [RIGHT, "The green box", "The red box", "The blue box"],
        "distractorRationales": [
            "",
            "A green box takes green stamps only.",
            "The red box takes whole red stamps only.",
            "The blue box takes whole blue stamps only."
        ],
        "answerIndex": 0,
        "explanation": "A torn stamp is set aside in the tray whatever its colour.",
        "blockIndex": 0,
        "quote": QUOTE,
        "difficulty": 3
    })
}

fn written(prompts: &[&str]) -> Value {
    json!({"items": prompts.iter().map(|p| choice(p)).collect::<Vec<_>>()})
}

fn right_option(question: &Value) -> Vec<usize> {
    question["options"]
        .as_array()
        .into_iter()
        .flatten()
        .position(|option| option == RIGHT)
        .into_iter()
        .collect()
}

fn solved(context: &Value, pick: impl Fn(&Value) -> Vec<usize>) -> Value {
    let answers: Vec<Value> = context["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|question| {
            json!({
                "index": question["index"],
                "reason": "Torn stamps are set aside.",
                "correctIndices": pick(question),
                "answer": "",
                "blankAnswers": [],
                "matches": [],
                "orderedValues": []
            })
        })
        .collect();
    json!({"answers": answers})
}

fn judged(context: &Value, tell: impl Fn(&Value) -> (String, Vec<String>)) -> Value {
    let checks: Vec<Value> = context["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let (finding, repeats) = tell(item);
            json!({
                "id": item["id"],
                "hasDefect": !finding.is_empty(),
                "finding": finding,
                "repeats": repeats
            })
        })
        .collect();
    json!({"checks": checks})
}

fn agrees() -> Reply {
    Box::new(|context| solved(context, right_option))
}

fn finds(tell: impl Fn(&Value) -> (String, Vec<String>) + Send + Sync + 'static) -> Reply {
    Box::new(move |context| judged(context, &tell))
}

fn sound() -> Reply {
    finds(|_| (String::new(), vec![]))
}

fn prompts(authored: &Authored) -> Vec<&str> {
    authored
        .items
        .iter()
        .map(|item| item.candidate.prompt.as_str())
        .collect()
}

#[tokio::test]
async fn a_flawed_item_is_repaired_alone_and_the_others_are_kept_as_written() -> Result<()> {
    const FINDING: &str = "The question depends on a value it never gives.";
    let llm = Script::new(
        vec![written(&[FIRST, SECOND, "A third case."]), written(&[SECOND_FIXED])],
        agrees(),
        finds(|item| {
            if item["prompt"] == SECOND {
                (FINDING.into(), vec![])
            } else {
                (String::new(), vec![])
            }
        }),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 3, &[])).await;

    llm.no_lesson_review();
    assert_eq!(
        llm.kinds(),
        ["write", "solve", "judge", "write", "solve", "judge"]
    );
    let repair = llm.request("write", 1);
    assert_eq!(repair["returnCount"], 1);
    assert_eq!(repair["repair"]["question"], SECOND);
    assert_eq!(repair["repair"]["findings"], json!([FINDING]));
    let authored = result?;
    assert_eq!(prompts(&authored), [FIRST, "A third case.", SECOND_FIXED]);
    assert!(authored.dropped.is_empty(), "{:?}", authored.dropped);
    Ok(())
}

#[tokio::test]
async fn a_repaired_item_that_is_still_flawed_is_dropped_with_its_findings() -> Result<()> {
    let llm = Script::new(
        vec![written(&[FIRST, SECOND]), written(&[SECOND_FIXED])],
        agrees(),
        finds(|item| match item["prompt"].as_str() {
            Some(SECOND) => ("The question needs a value.".into(), vec![]),
            Some(SECOND_FIXED) => ("The reason is about another case.".into(), vec![]),
            _ => (String::new(), vec![]),
        }),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await;

    llm.no_lesson_review();
    let authored = result?;
    assert_eq!(prompts(&authored), [FIRST]);
    assert!(
        authored
            .dropped
            .iter()
            .any(|reason| reason.contains("The reason is about another case.")),
        "{:?}",
        authored.dropped
    );
    Ok(())
}

#[tokio::test]
async fn an_explanation_item_is_solved_blind_and_the_solution_reaches_the_judge() -> Result<()> {
    const ANSWER: &str = "The torn stamp goes into the repair tray.";
    let written = json!({"items": [{
        "prompt": "A torn blue stamp arrives at the desk. Explain where it goes and why.",
        "rubric": [
            {"title": "CRIT_TITLE_ONE", "description": "Says where the torn stamp goes.", "dimension": "application"},
            {"title": "CRIT_TITLE_TWO", "description": "Says why it goes there.", "dimension": "explanation"}
        ],
        "explanation": "SECRET_EXPLANATION the tray takes every torn stamp.",
        "blockIndex": 0,
        "quote": QUOTE,
        "difficulty": 3
    }]});
    let llm = Script::new(
        vec![written],
        Box::new(|context| {
            let mut reply = solved(context, |_| vec![]);
            reply["answers"][0]["answer"] = json!(ANSWER);
            reply
        }),
        sound(),
    );

    let result = author_items(&llm, &need(LearningItemFormat::Explanation, 1, &[])).await;

    llm.no_lesson_review();
    assert_eq!(llm.kinds(), ["write", "solve", "judge"]);
    let solve = llm.raw("solve", 0);
    assert!(solve.contains("Explain where it goes and why."), "{solve}");
    assert!(!solve.contains("SECRET_EXPLANATION"), "{solve}");
    assert!(!solve.contains("CRIT_TITLE"), "{solve}");
    let judge = llm.request("judge", 0);
    assert_eq!(judge["items"][0]["independentSolution"]["answer"], ANSWER);
    assert_eq!(
        judge["items"][0]["independentSolution"]["working"],
        "Torn stamps are set aside."
    );
    assert_eq!(result?.items.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_solver_naming_two_answers_disputes_the_item_and_the_repair_is_kept_when_it_agrees(
) -> Result<()> {
    let solves = Arc::new(AtomicUsize::new(0));
    let counted = solves.clone();
    let llm = Script::new(
        vec![written(&[FIRST]), written(&[FIRST_FIXED])],
        Box::new(move |context| {
            let first = counted.fetch_add(1, Ordering::SeqCst) == 0;
            solved(context, |question| {
                let mut picked = right_option(question);
                if first {
                    picked.push((picked[0] + 1) % 4);
                }
                picked
            })
        }),
        sound(),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 1, &[])).await;

    llm.no_lesson_review();
    assert_eq!(
        llm.kinds(),
        ["write", "solve", "judge", "write", "solve", "judge"]
    );
    let repair = llm.request("write", 1);
    let finding = repair["repair"]["findings"][0].as_str().unwrap();
    assert!(
        finding.starts_with("An independent solution answered"),
        "{finding}"
    );
    let authored = result?;
    assert_eq!(prompts(&authored), [FIRST_FIXED]);
    assert!(authored.dropped.is_empty(), "{:?}", authored.dropped);
    Ok(())
}

#[tokio::test]
async fn an_item_the_judge_says_the_learner_already_has_is_repaired_with_that_question_quoted(
) -> Result<()> {
    const HAD: &str = "Where does a torn green stamp go first?";
    let llm = Script::new(
        vec![written(&[FIRST]), written(&[FIRST_FIXED])],
        agrees(),
        finds(|item| {
            if item["prompt"] == FIRST {
                (String::new(), vec!["have-0".into()])
            } else {
                (String::new(), vec![])
            }
        }),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 1, &[HAD])).await;

    llm.no_lesson_review();
    let judge = llm.request("judge", 0);
    assert_eq!(judge["learnerHas"][0]["id"], "have-0");
    assert_eq!(judge["learnerHas"][0]["prompt"], HAD);
    let repair = llm.request("write", 1);
    assert_eq!(
        repair["repair"]["findings"],
        json!([format!(
            "It asks for the same decision as a question the learner already has: “{HAD}”."
        )])
    );
    assert_eq!(prompts(&result?), [FIRST_FIXED]);
    Ok(())
}

#[tokio::test]
async fn of_two_new_items_naming_each_other_only_the_later_is_repaired() -> Result<()> {
    let llm = Script::new(
        vec![written(&[FIRST, SECOND]), written(&[SECOND_FIXED])],
        agrees(),
        finds(|item| match item["prompt"].as_str() {
            Some(FIRST) => (String::new(), vec!["new-1".into()]),
            Some(SECOND) => (String::new(), vec!["new-0".into()]),
            _ => (String::new(), vec![]),
        }),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await;

    llm.no_lesson_review();
    let repair = llm.request("write", 1);
    assert_eq!(repair["repair"]["question"], SECOND);
    assert_eq!(
        repair["repair"]["findings"],
        json!([format!(
            "It asks for the same decision as another new question: “{FIRST}”."
        )])
    );
    assert_eq!(prompts(&result?), [FIRST, SECOND_FIXED]);
    Ok(())
}

#[tokio::test]
async fn prompts_sharing_most_of_their_words_are_both_kept_when_the_judge_calls_no_repeat(
) -> Result<()> {
    let longer = format!("{FIRST} Reply now.");
    let llm = Script::new(vec![written(&[FIRST, &longer])], agrees(), sound());

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await;

    llm.no_lesson_review();
    let authored = result?;
    assert_eq!(prompts(&authored), [FIRST, longer.as_str()]);
    assert_eq!(llm.kinds(), ["write", "solve", "judge"]);
    Ok(())
}

#[tokio::test]
async fn items_whose_review_cannot_be_read_twice_are_dropped_as_not_reviewed() -> Result<()> {
    let llm = Script::new(
        vec![written(&[FIRST, SECOND])],
        agrees(),
        Box::new(|_| json!({"checks": []})),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await;

    llm.no_lesson_review();
    let kinds = llm.kinds();
    assert_eq!(kinds.iter().filter(|k| **k == "judge").count(), 2);
    let authored = result?;
    assert!(authored.items.is_empty(), "{:?}", prompts(&authored));
    assert_eq!(authored.dropped.len(), 2, "{:?}", authored.dropped);
    assert!(authored
        .dropped
        .iter()
        .all(|reason| reason.starts_with("The question could not be reviewed")));
    Ok(())
}
