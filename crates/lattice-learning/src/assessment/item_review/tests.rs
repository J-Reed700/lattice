#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::scripted::{judged, sound};
use super::*;
use crate::assessment_engine::LearningItemFormat;
use crate::key_review::scripted::{answering, solved};
use crate::key_review::SolvePicks;
use crate::lessons::structured_call::task_messages;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::ScriptedModel;

const FIRST: &str = "A torn blue stamp arrives at the desk. Which place takes it first?";
const SECOND: &str = "A torn red stamp arrives at the desk. Which place takes it first?";
const HELD: &str = "A smudged stamp arrives at the desk. Which place takes it first?";
const NEEDS_A_VALUE: &str = "The question needs a value.";

fn item(prompt: &str) -> Item {
    Item {
        id: "item".into(),
        outcome_ids: vec![],
        format: LearningItemFormat::SingleChoice,
        difficulty: 3,
        prompt: prompt.into(),
        options: vec!["Alpha".into(), "Bravo".into(), "Charlie".into()],
        left: vec![],
        source_version_ids: vec![],
        rubric: vec![],
        key: Key::Choice(0),
        explanation: "Alpha follows.".into(),
        distractor_rationales: vec![],
    }
}

fn context(learner_has: &[String]) -> Context<'_> {
    Context {
        outcome: "Sorting stamps",
        outcome_description: "Say where a stamp goes by its colour and condition.",
        learner_has,
    }
}

/// A model whose solver finds every key right and whose judge answers as `judge` does.
fn model(judge: impl Fn(&Value, usize) -> Value + Send + Sync + 'static) -> ScriptedModel {
    let model = answering::<SolvePicks>(ScriptedModel::new(), |task, _| solved(task, |_| vec![0]));
    answering::<QuizItemReview>(model, judge)
}

async fn review_of(
    model: &ScriptedModel,
    prompts: &[&str],
    learner_has: &[String],
) -> Vec<Findings> {
    let items: Vec<Item> = prompts.iter().map(|prompt| item(prompt)).collect();
    let entries: Vec<Entry<'_>> = items
        .iter()
        .map(|item| Entry {
            item,
            quote: "Reference quote.",
        })
        .collect();
    review(model, &entries, &context(learner_has))
        .await
        .unwrap()
}

/// The reply with the entry named `of` left out.
fn without(mut reply: Value, of: &str) -> Value {
    if let Some(entries) = reply["entries"].as_array_mut() {
        entries.retain(|entry| entry["of"] != of);
    }
    reply
}

#[test]
fn the_item_review_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        outcome: marks.told("Sorting stamps by colour".to_owned()),
        outcome_description: marks.told("Say where a stamp goes by its colour.".to_owned()),
        held: vec![Held {
            at: marks.reference(100_001_usize),
            prompt: marks.told("A smudged stamp arrives at the desk.".to_owned()),
        }],
        items: vec![Reviewed {
            at: marks.reference(100_002_usize),
            content: marks.told(json!({
                "prompt": "A torn blue stamp arrives at the desk.",
                "explanation": "A torn stamp is set aside in the tray."
            })),
        }],
        siblings: vec![
            Sibling {
                at: 100_002,
                prompt: Told("A torn blue stamp arrives at the desk.".to_owned()),
            },
            Sibling {
                at: marks.reference(100_003_usize),
                prompt: marks.told("A torn red stamp arrives at the desk.".to_owned()),
            },
        ],
    };

    holds::<QuizItemReview>(&input, &marks);
}

#[tokio::test]
async fn a_complete_review_is_read() {
    let model = model(|task, _| {
        judged(task, |item| match item["prompt"].as_str() {
            Some(SECOND) => (NEEDS_A_VALUE.into(), vec![]),
            _ => (String::new(), vec![]),
        })
    });

    let findings = review_of(&model, &[FIRST, SECOND], &[]).await;

    assert!(findings[0].is_sound());
    assert_eq!(findings[1].defects, [NEEDS_A_VALUE]);
    assert!(!findings[1].unreviewed);
}

#[tokio::test]
async fn a_missing_item_is_asked_for_again_alone() {
    let model = model(|task, nth| {
        let reply = sound(task);
        if nth == 0 {
            without(reply, "q2")
        } else {
            reply
        }
    });

    let findings = review_of(&model, &[FIRST, SECOND], &[]).await;

    assert!(findings.iter().all(Findings::is_sound));
    let asked: Vec<_> = model
        .sent()
        .into_iter()
        .filter(|sent| sent.call.id == QuizItemReview::ID && sent.request.schema().is_some())
        .collect();
    assert_eq!(asked.len(), 2);
    let second: Input = serde_json::from_value(asked[1].call.input.clone()).unwrap();
    assert_eq!(second.items.len(), 1);
}

#[tokio::test]
async fn an_empty_defect_is_corrected() {
    let model = model(|task, nth| {
        if nth == 0 {
            judged(task, |_| (" ".into(), vec![]))
        } else {
            judged(task, |_| (NEEDS_A_VALUE.into(), vec![]))
        }
    });

    let findings = review_of(&model, &[FIRST], &[]).await;

    assert_eq!(findings[0].defects, [NEEDS_A_VALUE]);
}

#[tokio::test]
async fn an_item_the_judge_never_answers_is_unreviewed_and_the_rest_stand() {
    let model = model(|task, _| without(sound(task), "q2"));

    let findings = review_of(&model, &[FIRST, SECOND], &[]).await;

    assert!(findings[0].is_sound());
    assert!(findings[1].unreviewed);
    assert!(!findings[1].is_sound());
    assert!(findings[1].texts().contains(&UNREVIEWED.to_owned()));
}

#[tokio::test]
async fn an_item_that_could_not_be_solved_is_not_sent_to_the_judge() {
    let solver = answering::<SolvePicks>(ScriptedModel::new(), |task, _| {
        without(solved(task, |_| vec![0]), "q2")
    });
    let model = answering::<QuizItemReview>(solver, |task, _| sound(task));

    let findings = review_of(&model, &[FIRST, SECOND], &[]).await;

    assert!(findings[0].is_sound());
    assert!(findings[1].unreviewed);
    let asked: Vec<_> = model
        .sent()
        .into_iter()
        .filter(|sent| sent.call.id == QuizItemReview::ID && sent.request.schema().is_some())
        .collect();
    assert_eq!(asked.len(), 1);
    let first: Input = serde_json::from_value(asked[0].call.input.clone()).unwrap();
    assert_eq!(first.items.len(), 1);
}

#[tokio::test]
async fn an_item_is_offered_the_other_items_and_never_itself() {
    let model = model(|task, _| sound(task));

    review_of(&model, &[FIRST, SECOND], &[]).await;

    let sent = model
        .sent()
        .into_iter()
        .find(|sent| sent.call.id == QuizItemReview::ID)
        .unwrap();
    let task: Value = serde_json::from_str(task_messages(&sent.request).1).unwrap();
    assert_eq!(task["items"][0]["id"], "q1");
    assert_eq!(
        task["items"][0]["otherItems"],
        json!([{"id": "n1", "prompt": SECOND}])
    );
    assert_eq!(
        task["items"][1]["otherItems"],
        json!([{"id": "n1", "prompt": FIRST}])
    );
    assert!(task.get("otherItems").is_none());
}

#[tokio::test]
async fn a_lone_item_has_no_others_to_repeat() {
    let model = model(|task, _| {
        let mut reply = sound(task);
        reply["entries"][0]["repeatsNew"] = json!(["n1"]);
        reply
    });

    let findings = review_of(&model, &[FIRST], &[]).await;

    assert!(findings[0].is_sound());
}

#[tokio::test]
async fn items_that_repeat_each_other_flag_the_later_one() {
    let model = model(|task, _| {
        judged(task, |item| match item["prompt"].as_str() {
            Some(FIRST) => (String::new(), vec!["new-1".into()]),
            _ => (String::new(), vec!["new-0".into()]),
        })
    });

    let findings = review_of(&model, &[FIRST, SECOND], &[]).await;

    assert!(findings[0].is_sound());
    assert_eq!(findings[1].repeats.len(), 1);
    assert_eq!(findings[1].repeats[0].of, RepeatOf::New(0));
}

#[tokio::test]
async fn a_repeat_of_a_question_the_learner_has_is_a_finding_on_the_item() {
    let model = model(|task, _| {
        judged(task, |item| match item["prompt"].as_str() {
            Some(SECOND) => (String::new(), vec!["have-0".into()]),
            _ => (String::new(), vec![]),
        })
    });
    let learner_has = vec![HELD.to_owned()];

    let findings = review_of(&model, &[FIRST, SECOND], &learner_has).await;

    assert!(findings[0].is_sound());
    assert_eq!(findings[1].repeats[0].of, RepeatOf::Held);
    assert!(findings[1].repeats[0].text.contains(HELD));
    assert!(!findings[1].has_defect());
}

#[tokio::test]
async fn the_held_questions_are_listed_once_for_the_whole_request() {
    let model = model(|task, _| sound(task));
    let learner_has = vec![HELD.to_owned()];

    review_of(&model, &[FIRST, SECOND], &learner_has).await;

    let sent = model
        .sent()
        .into_iter()
        .find(|sent| sent.call.id == QuizItemReview::ID)
        .unwrap();
    let task: Value = serde_json::from_str(task_messages(&sent.request).1).unwrap();
    assert_eq!(task["learnerHas"], json!([{"id": "h1", "prompt": HELD}]));
}
