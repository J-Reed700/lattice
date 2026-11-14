//! `author_items` reviewing each written question as a question: a scripted
//! writer, a blind solver and an item judge, and the requests between them.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::assessment_engine::LearningBlueprintRequirement;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
    /// A window that holds one judged item but not two.
    chunked: bool,
    /// The kind whose first request for two or more items is cut off.
    cut: Option<&'static str>,
    cut_done: AtomicBool,
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
        let held = match kind {
            "solve" => context["questions"].as_array().map_or(0, Vec::len),
            "judge" => context["items"].as_array().map_or(0, Vec::len),
            _ => 0,
        };
        if self.cut == Some(kind) && held > 1 && !self.cut_done.swap(true, Ordering::SeqCst) {
            return Ok(CompletionResponse {
                finish_reason: "length".into(),
                ..CompletionResponse::from_text("{\"answers\":[{\"index\":0")
            });
        }
        let reply = match kind {
            "write" => {
                let next = self
                    .writes
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or_else(|| json!({"items": []}));
                if next.get("fail").is_some() {
                    return Err(AppError::ServiceNotAvailable("the model is gone".into()));
                }
                next
            }
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
    fn count_tokens(&self, text: &str) -> usize {
        if self.chunked && text.matches("\"independentSolution\":").count() > 1 {
            return 10_000_000;
        }
        text.len() / 4
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
            chunked: false,
            cut: None,
            cut_done: AtomicBool::new(false),
        }
    }

    fn chunked(mut self) -> Self {
        self.chunked = true;
        self
    }

    fn cut_off(mut self, kind: &'static str) -> Self {
        self.cut = Some(kind);
        self
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
    need_for(format, count, existing, BankPass::Retakes)
}

fn need_for(
    format: LearningItemFormat,
    count: usize,
    existing: &[&str],
    pass: BankPass,
) -> AuthoringNeed {
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
        pass,
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
        vec![
            written(&[FIRST, SECOND, "A third case."]),
            written(&[SECOND_FIXED]),
        ],
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

#[tokio::test]
async fn items_in_different_chunks_see_each_other_and_a_repeat_across_them_is_found() -> Result<()>
{
    let llm = Script::new(
        vec![written(&[FIRST, SECOND]), written(&[SECOND_FIXED])],
        agrees(),
        finds(|item| {
            if item["prompt"] == SECOND {
                (String::new(), vec!["new-0".into()])
            } else {
                (String::new(), vec![])
            }
        }),
    )
    .chunked();

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await;

    llm.no_lesson_review();
    let first = llm.request("judge", 0);
    let second = llm.request("judge", 1);
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["id"], "new-1");
    assert_eq!(
        second["otherItems"],
        json!([{"id": "new-0", "prompt": FIRST}])
    );
    let repair = llm.request("write", 1);
    assert_eq!(repair["repair"]["question"], SECOND);
    assert_eq!(
        repair["repair"]["findings"],
        json!([format!(
            "It asks for the same decision as another new question: “{FIRST}”."
        )])
    );
    assert_eq!(prompts(&result?), [FIRST, SECOND_FIXED]);
    let alone = llm.request("judge", 2);
    assert!(alone.get("otherItems").is_none(), "{alone}");
    Ok(())
}

#[tokio::test]
async fn a_cut_off_judge_answer_for_two_items_is_asked_again_as_two_single_item_requests(
) -> Result<()> {
    let llm = Script::new(vec![written(&[FIRST, SECOND])], agrees(), sound()).cut_off("judge");

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await;

    llm.no_lesson_review();
    assert_eq!(llm.kinds(), ["write", "solve", "judge", "judge", "judge"]);
    assert_eq!(
        llm.request("judge", 1)["items"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        llm.request("judge", 2)["items"].as_array().unwrap().len(),
        1
    );
    assert_eq!(prompts(&result?), [FIRST, SECOND]);
    Ok(())
}

#[tokio::test]
async fn a_cut_off_solver_answer_for_two_items_is_asked_again_as_two_single_item_requests(
) -> Result<()> {
    let llm = Script::new(vec![written(&[FIRST, SECOND])], agrees(), sound()).cut_off("solve");

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await;

    llm.no_lesson_review();
    assert_eq!(llm.kinds(), ["write", "solve", "solve", "solve", "judge"]);
    assert_eq!(
        llm.request("solve", 1)["questions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        llm.request("solve", 2)["questions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(prompts(&result?), [FIRST, SECOND]);
    Ok(())
}

#[tokio::test]
async fn a_repair_write_failing_for_another_reason_fails_the_call() {
    let llm = Script::new(
        vec![written(&[FIRST]), json!({"fail": true})],
        agrees(),
        finds(|_| ("The question needs a value.".into(), vec![])),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 1, &[])).await;

    assert!(
        matches!(result, Err(AppError::ServiceNotAvailable(_))),
        "{result:?}"
    );
}

const HAD: &str = "Where does a torn green stamp go first?";
const THIRD: &str = "A torn green stamp arrives at the desk. Which place takes it first?";

fn writes(llm: &Script) -> usize {
    llm.kinds().iter().filter(|k| **k == "write").count()
}

#[tokio::test]
async fn an_item_dropped_as_a_repeat_is_not_written_again() -> Result<()> {
    let llm = Script::new(
        vec![
            written(&[FIRST, SECOND]),
            written(&[SECOND_FIXED]),
            written(&[THIRD]),
        ],
        agrees(),
        finds(|item| match item["prompt"].as_str() {
            Some(SECOND | SECOND_FIXED) => (String::new(), vec!["have-0".into()]),
            _ => (String::new(), vec![]),
        }),
    );

    let result = author_items(
        &llm,
        &need_for(
            LearningItemFormat::SingleChoice,
            2,
            &[HAD],
            BankPass::Retakes,
        ),
    )
    .await;

    llm.no_lesson_review();
    assert_eq!(writes(&llm), 2, "{:?}", llm.kinds());
    assert_eq!(prompts(&result?), [FIRST]);
    Ok(())
}

#[tokio::test]
async fn a_first_form_item_dropped_as_a_repeat_is_written_again() -> Result<()> {
    let llm = Script::new(
        vec![
            written(&[FIRST, SECOND]),
            written(&[SECOND_FIXED]),
            written(&[THIRD]),
        ],
        agrees(),
        finds(|item| match item["prompt"].as_str() {
            Some(SECOND | SECOND_FIXED) => (String::new(), vec!["have-0".into()]),
            _ => (String::new(), vec![]),
        }),
    );

    let result = author_items(
        &llm,
        &need_for(
            LearningItemFormat::SingleChoice,
            2,
            &[HAD],
            BankPass::FirstForm,
        ),
    )
    .await;

    llm.no_lesson_review();
    assert_eq!(writes(&llm), 3, "{:?}", llm.kinds());
    assert_eq!(prompts(&result?), [FIRST, THIRD]);
    Ok(())
}

#[tokio::test]
async fn a_round_after_a_drop_tells_the_writer_why_the_item_was_dropped() -> Result<()> {
    let llm = Script::new(
        vec![
            written(&[FIRST, SECOND]),
            written(&[SECOND_FIXED]),
            written(&[THIRD]),
        ],
        agrees(),
        finds(|item| match item["prompt"].as_str() {
            Some(SECOND | SECOND_FIXED) => ("The question needs a value.".into(), vec![]),
            _ => (String::new(), vec![]),
        }),
    );

    author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await?;

    assert_eq!(writes(&llm), 3, "{:?}", llm.kinds());
    let avoid = llm.request("write", 2)["problemsToAvoid"].to_string();
    assert!(
        avoid.contains("The question needs a value."),
        "the round's writer was not told why: {avoid}"
    );
    Ok(())
}

/// Passes before and after the change: a defect the rewrite keeps is not a repeat.
#[tokio::test]
async fn an_item_dropped_for_a_defect_is_written_again() -> Result<()> {
    let llm = Script::new(
        vec![
            written(&[FIRST, SECOND]),
            written(&[SECOND_FIXED]),
            written(&[THIRD]),
        ],
        agrees(),
        finds(|item| match item["prompt"].as_str() {
            Some(SECOND | SECOND_FIXED) => ("The question needs a value.".into(), vec![]),
            _ => (String::new(), vec![]),
        }),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[])).await;

    llm.no_lesson_review();
    assert_eq!(writes(&llm), 3, "{:?}", llm.kinds());
    assert_eq!(prompts(&result?), [FIRST, THIRD]);
    Ok(())
}

/// Passes before and after the change: a defect beside the repeat keeps it a defect drop.
#[tokio::test]
async fn an_item_dropped_for_a_defect_and_a_repeat_is_written_again() -> Result<()> {
    let llm = Script::new(
        vec![
            written(&[FIRST, SECOND]),
            written(&[SECOND_FIXED]),
            written(&[THIRD]),
        ],
        agrees(),
        finds(|item| match item["prompt"].as_str() {
            Some(SECOND) => (String::new(), vec!["have-0".into()]),
            Some(SECOND_FIXED) => ("The question needs a value.".into(), vec!["have-0".into()]),
            _ => (String::new(), vec![]),
        }),
    );

    let result = author_items(&llm, &need(LearningItemFormat::SingleChoice, 2, &[HAD])).await;

    llm.no_lesson_review();
    assert_eq!(writes(&llm), 3, "{:?}", llm.kinds());
    assert_eq!(prompts(&result?), [FIRST, THIRD]);
    Ok(())
}

#[tokio::test]
async fn review_reports_a_repeat_apart_from_the_defects() -> Result<()> {
    let need = need(LearningItemFormat::SingleChoice, 1, &[HAD]);
    let raw: RawItem = serde_json::from_value(choice(FIRST)).unwrap();
    let passage = &need.material[0];
    let authored = to_candidate(raw, &need, &[passage]).unwrap();
    let candidate: crate::assessment_engine::LearningAssessmentCandidate =
        authored.candidate.clone().into();
    let entries = [Entry {
        item: &candidate,
        quote: &authored.source_quote,
    }];
    let context = Context {
        outcome: &need.outcome_title,
        outcome_description: &need.outcome_description,
        learner_has: &need.existing_prompts,
    };
    let llm = Script::new(
        vec![],
        agrees(),
        finds(|_| (String::new(), vec!["have-0".into()])),
    );

    let found = item_review::review(&llm, &entries, &context).await?;

    assert_eq!(found.len(), 1);
    assert!(found[0].defects.is_empty(), "{:?}", found[0]);
    assert_eq!(found[0].repeats.len(), 1, "{:?}", found[0]);
    assert_eq!(
        found[0].repeats[0].of,
        item_review::RepeatOf::Held,
        "{:?}",
        found[0]
    );
    Ok(())
}

const THIRD_FIXED: &str = "A torn green stamp arrives at the desk with its colour still readable. Which place takes it first?";
const FOURTH: &str = "A torn yellow stamp arrives at the desk. Which place takes it first?";
const FIFTH: &str = "A torn white stamp arrives at the desk. Which place takes it first?";

/// Round one: the first item is sound, the second and third each have a
/// defect. The reply to the rewrites is chosen by the id each is reviewed under.
fn two_rewrites(
    second_review: impl Fn(&str) -> (String, Vec<String>) + Send + Sync + 'static,
    existing: &[&str],
    next: Value,
    count: usize,
) -> (Script, AuthoringNeed) {
    let llm = Script::new(
        vec![
            written(&[FIRST, SECOND, THIRD]),
            written(&[SECOND_FIXED]),
            written(&[THIRD_FIXED]),
            next,
        ],
        agrees(),
        finds(move |item| match item["prompt"].as_str() {
            Some(SECOND | THIRD) => ("The question needs a value.".into(), vec![]),
            Some(SECOND_FIXED | THIRD_FIXED) => second_review(item["id"].as_str().unwrap_or("")),
            _ => (String::new(), vec![]),
        }),
    );
    (llm, need(LearningItemFormat::SingleChoice, count, existing))
}

#[tokio::test]
async fn a_rewrite_that_repeats_only_a_rewrite_dropped_for_a_defect_is_written_again() -> Result<()>
{
    let (llm, need) = two_rewrites(
        |id| match id {
            "new-0" => ("The question needs a value.".into(), vec![]),
            _ => (String::new(), vec!["new-0".into()]),
        },
        &[],
        written(&[FOURTH, FIFTH]),
        3,
    );

    let result = author_items(&llm, &need).await;

    llm.no_lesson_review();
    assert_eq!(writes(&llm), 4, "{:?}", llm.kinds());
    assert_eq!(llm.request("write", 3)["returnCount"], 2);
    assert_eq!(prompts(&result?), [FIRST, FOURTH, FIFTH]);
    Ok(())
}

/// Passes before the change as well: both rewrites have no defect.
#[tokio::test]
async fn a_rewrite_that_repeats_a_rewrite_dropped_as_a_repeat_is_not_written_again() -> Result<()> {
    let (llm, need) = two_rewrites(
        |id| match id {
            "new-0" => (String::new(), vec!["have-0".into()]),
            _ => (String::new(), vec!["new-0".into()]),
        },
        &[HAD],
        written(&[FOURTH]),
        3,
    );

    let result = author_items(&llm, &need).await;

    llm.no_lesson_review();
    assert_eq!(writes(&llm), 3, "{:?}", llm.kinds());
    assert_eq!(prompts(&result?), [FIRST]);
    Ok(())
}

#[test]
fn the_item_review_contract_reads_what_it_renders() {
    let checks: Vec<item_review::Check> = serde_json::from_value(json!([
        {"id":"i1","finding":"","hasDefect":false,"repeats":[]},
        {"id":"i2","finding":"No answer is defensible.","hasDefect":true,"repeats":["i1"]}
    ]))
    .unwrap();
    lattice_core::application::services::model_contract::test_support::assert_round_trip::<
        item_review::QuizItemReview,
    >(&checks);
}
