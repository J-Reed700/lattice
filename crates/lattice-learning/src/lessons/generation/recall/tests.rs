#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::dto::{LearningBlockDto, LearningProgramDto};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::shape::Pointers;
use lattice_core::application::services::model_contract::test_support::samples::schema_of;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::shared::error::AppError;

const GATE: &str = "The harbour gate opens at first light.";

const TABLE: &str = "The tide table lists each crossing.";

const KILN: &str = "The kiln fires every bowl once the damper is shut.";

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn cut_off(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    let mut response = CompletionResponse::from_text(text);
    response.finish_reason = "length".into();
    Box::pin(std::future::ready(Ok(response)))
}

/// An active program whose first lesson is ready; its blocks cite the first source when `sourced`.
fn prepared(sourced: bool) -> (LearningProgramDto, String) {
    let mut program = crate::tests::fixture();
    program.summary.status = LearningProgramStatus::Active;
    program.sources[0].excerpt = format!("{GATE} {TABLE} {KILN}");
    let cited = if sourced {
        vec![program.sources[0].id.clone()]
    } else {
        Vec::new()
    };
    let lesson = &mut program.modules[0].lessons[0];
    lesson.preparation = LearningPreparation::Ready;
    lesson.blocks = vec![
        LearningBlockDto::new(
            LearningBlockKind::Explanation,
            "Gate times".into(),
            format!("{GATE} {TABLE}"),
            cited.clone(),
            vec![],
        ),
        LearningBlockDto::new(
            LearningBlockKind::Recap,
            "Recap".into(),
            KILN.into(),
            cited,
            vec![],
        ),
    ];
    let id = lesson.id.clone();
    (program, id)
}

/// The task a request carries.
fn task(sent: &Sent) -> Value {
    sent.request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => serde_json::from_str::<Value>(content).ok(),
            _ => None,
        })
        .find(|task| task.get("requirements").is_some())
        .unwrap()
}

/// The names the request shows beside sentences, in the order shown.
fn names(sent: &Sent) -> Vec<String> {
    let task = task(sent);
    let mut rows: Vec<String> = task["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["excerpt"].as_str().unwrap().to_owned())
        .collect();
    if rows.is_empty() {
        rows = task["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|block| block["body"].as_str().unwrap().to_owned())
            .collect();
    }
    rows.iter()
        .flat_map(|text| text.lines())
        .filter_map(|line| {
            let (name, _) = line.strip_prefix('[')?.split_once("] ")?;
            Some(name.to_owned())
        })
        .collect()
}

fn card(question: &str, cites: &[impl AsRef<str>]) -> Value {
    let cites: Vec<&str> = cites.iter().map(AsRef::as_ref).collect();
    json!({
        "question":question,
        "answer":"At first light.",
        "explanation":"The gate opens when the day begins.",
        "cites":cites
    })
}

/// Every message of the request, one after another.
fn prompt_of(sent: &Sent) -> String {
    sent.request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(
            "

",
        )
}

fn answer(cards: Vec<Value>) -> String {
    json!({"items":cards}).to_string()
}

/// The drafts a model answering with `cards` (given the names shown) makes of the lesson.
async fn generated(
    sourced: bool,
    count: usize,
    cards: impl Fn(&[String]) -> Vec<Value> + Send + Sync + 'static,
) -> (ScriptedModel, Result<Vec<GeneratedLearningCardDraft>>) {
    let (program, lesson_id) = prepared(sourced);
    let model = ScriptedModel::new()
        .on_request::<RecallDrafts>(move |sent, _| reply(answer(cards(&names(sent)))));
    let result = generate_recall_drafts(&model, &program, &lesson_id, count).await;
    (model, result)
}

#[tokio::test]
async fn a_complete_answer_is_read_and_each_card_cites_its_source() {
    let (program, lesson_id) = prepared(true);
    let model = ScriptedModel::new().on_request::<RecallDrafts>(|sent, _| {
        let shown = names(sent);
        reply(answer(vec![
            card("What opens at first light?", &[&shown[0]]),
            card("What does the table list?", &[&shown[1], &shown[2]]),
        ]))
    });
    let drafts = generate_recall_drafts(&model, &program, &lesson_id, 2)
        .await
        .unwrap();
    assert_eq!(drafts.len(), 2);
    assert_eq!(drafts[0].question, "What opens at first light?");
    assert_eq!(drafts[0].source_ids, vec![program.sources[0].id.clone()]);
    assert_eq!(drafts[1].source_ids, vec![program.sources[0].id.clone()]);
    let sent = model.sent();
    assert_eq!(sent.len(), 1);
    let task = task(&sent[0]);
    assert_eq!(task["count"], json!(2));
    assert!(task.get("alreadyWritten").is_none());
    let prompt = prompt_of(&sent[0]);
    assert!(!prompt.contains(&program.sources[0].id), "{prompt}");
    assert_eq!(names(&sent[0]), ["u1", "u2", "u3"]);
}

#[tokio::test]
async fn one_card_that_cites_nothing_shown_is_left_out_and_the_others_stand() {
    let (model, result) = generated(true, 2, |shown| {
        vec![
            card("What opens at first light?", &[&shown[0]]),
            card("What does the table list?", &["u99"]),
        ]
    })
    .await;
    let drafts = result.unwrap();
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].question, "What opens at first light?");
    assert_eq!(model.sent().len(), 1);
}

#[tokio::test]
async fn a_card_with_a_blank_field_is_left_out_alone() {
    let (_, result) = generated(true, 2, |shown| {
        let mut blank = card("What does the table list?", &[&shown[1]]);
        blank["answer"] = json!("  ");
        vec![card("What opens at first light?", &[&shown[0]]), blank]
    })
    .await;
    assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn fewer_cards_than_asked_stand_when_the_answer_ended() {
    let (model, result) = generated(true, 4, |shown| {
        vec![card("What opens at first light?", &[&shown[0]])]
    })
    .await;
    assert_eq!(result.unwrap().len(), 1);
    assert_eq!(model.sent().len(), 1);
}

#[tokio::test]
async fn a_repeated_question_and_cards_past_the_count_are_dropped() {
    let (_, result) = generated(true, 2, |shown| {
        vec![
            card("What opens at first light?", &[&shown[0]]),
            card("  What opens   at first light? ", &[&shown[0]]),
            card("What does the table list?", &[&shown[1]]),
            card("What shuts the damper?", &[&shown[2]]),
        ]
    })
    .await;
    let questions: Vec<String> = result
        .unwrap()
        .into_iter()
        .map(|draft| draft.question)
        .collect();
    assert_eq!(
        questions,
        ["What opens at first light?", "What does the table list?"]
    );
}

#[tokio::test]
async fn an_answer_cut_off_after_a_card_keeps_it_and_asks_only_for_the_rest() {
    let (program, lesson_id) = prepared(true);
    let model = ScriptedModel::new().on_request::<RecallDrafts>(|sent, nth| {
        let shown = names(sent);
        if nth == 0 {
            let closed = card("What opens at first light?", &[&shown[0]]);
            cut_off(format!("{{\"items\":[{closed},{{\"question\":\"Half"))
        } else {
            reply(answer(vec![card(
                "What does the table list?",
                &[&shown[1]],
            )]))
        }
    });
    let drafts = generate_recall_drafts(&model, &program, &lesson_id, 2)
        .await
        .unwrap();
    assert_eq!(drafts.len(), 2);
    let sent = model.sent();
    let rest = sent.last().map(task).unwrap();
    assert_eq!(rest["count"], json!(1));
    assert_eq!(
        rest["alreadyWritten"],
        json!(["What opens at first light?"])
    );
}

#[tokio::test]
async fn a_lesson_without_sources_is_cited_by_its_own_sentences() {
    let (model, result) = generated(false, 2, |shown| {
        vec![
            card("What opens at first light?", &[&shown[0]]),
            card("What fires every bowl?", &[&shown[2]]),
        ]
    })
    .await;
    let drafts = result.unwrap();
    assert_eq!(drafts.len(), 2);
    assert!(drafts.iter().all(|draft| draft.source_ids.is_empty()));
    let task = task(&model.sent()[0]);
    assert_eq!(task["sources"], json!([]));
    assert_eq!(task["blocks"][1]["body"], json!(format!("[u3] {KILN}")));
}

#[tokio::test]
async fn an_answer_with_no_card_fails_the_job() {
    let (_, none) = generated(true, 2, |_| Vec::new()).await;
    assert!(matches!(none, Err(AppError::InvalidInput(_))));
    let (program, lesson_id) = prepared(true);
    let model =
        ScriptedModel::new().on_request::<RecallDrafts>(|_, _| reply("not an answer".into()));
    let unreadable = generate_recall_drafts(&model, &program, &lesson_id, 2).await;
    assert!(matches!(unreadable, Err(AppError::ModelAnswerUnusable(_))));
}

#[test]
fn the_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        count: Told(2),
        written: vec![marks.told("What opens the gate?".to_owned())],
        blocks: vec![Block {
            kind: marks.told(LearningBlockKind::Explanation),
            title: marks.told("Gate times".to_owned()),
            body: marks.told(GATE.to_owned()),
        }],
        sourced: marks.internal(true),
        passages: Packed::new(
            vec![Passage {
                source: marks.internal("source-quay-7731".to_owned()),
                rank: marks.internal(900_001_usize),
                start: marks.internal(900_002_usize),
                tokens: marks.internal(900_003_usize),
                title: marks.told("Harbour almanac".to_owned()),
                text: marks.told(TABLE.to_owned()),
            }],
            |_| 1,
        ),
    };
    holds::<RecallDrafts>(&input, &marks);
    let mut marks = Marks::new();
    let unsourced = Input {
        count: Told(2),
        written: Vec::new(),
        blocks: vec![Block {
            kind: marks.told(LearningBlockKind::Reflection),
            title: marks.told("Recap of the kiln".to_owned()),
            body: marks.told(KILN.to_owned()),
        }],
        sourced: marks.internal(false),
        passages: Packed::default(),
    };
    holds::<RecallDrafts>(&unsourced, &marks);
}

fn two_sources() -> Input {
    let passage = |source: &str, rank: usize, start: usize, text: &str| Passage {
        source: Internal::of(source.to_owned()),
        rank: Internal::of(rank),
        start: Internal::of(start),
        tokens: Internal::of(1),
        title: Told(format!("Reference {rank}")),
        text: Told(text.to_owned()),
    };
    Input {
        count: Told(2),
        written: Vec::new(),
        blocks: Vec::new(),
        sourced: Internal::of(true),
        passages: Packed::new(
            vec![
                passage("b", 1, 0, KILN),
                passage("a", 0, 80, TABLE),
                passage("a", 0, 0, GATE),
            ],
            |_| 1,
        ),
    }
}

#[test]
fn sources_are_shown_in_the_lessons_order_and_each_in_document_order() {
    let mut scope = Scope::for_question();
    let says = RecallDrafts::says(&two_sources(), &mut scope.request()).unwrap();
    let task: Value = serde_json::from_str(&says.task).unwrap();
    assert_eq!(
        task["sources"],
        json!([
            {"title":"Reference 0","excerpt":format!("[u1] {GATE}\n\n[u2] {TABLE}")},
            {"title":"Reference 1","excerpt":format!("[u3] {KILN}")},
        ])
    );
}

#[test]
fn a_cut_of_the_passages_keeps_the_best_first() {
    let cut = RecallDrafts::within(&two_sources(), 2).unwrap();
    assert_eq!(cut.passages.len(), 2);
    assert_eq!(
        RecallDrafts::within(&two_sources(), 0)
            .unwrap()
            .passages
            .len(),
        0
    );
}

#[test]
fn the_answer_points_at_what_is_shown_and_bounds_no_text() {
    let schema = schema_of::<RecallDrafts>(&two_sources(), Pointers::Enum)
        .unwrap()
        .as_json()
        .to_string();
    for named in ["\"items\"", "\"cites\"", "\"u1\"", "\"u2\"", "\"u3\""] {
        assert!(schema.contains(named), "{named}: {schema}");
    }
    for absent in [
        "\"u4\"",
        "sourceIds",
        "quote",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
    ] {
        assert!(!schema.contains(absent), "{absent}: {schema}");
    }
}

#[test]
fn the_cards_are_written_for_a_learner_who_sees_only_the_lesson() {
    let mut scope = Scope::for_question();
    let says = RecallDrafts::says(&two_sources(), &mut scope.request()).unwrap();
    for expected in [
        "The learner sees a card and the lesson, never the source excerpts.",
        "never point at an excerpt, passage or tutorial the learner cannot see",
    ] {
        assert!(says.task.contains(expected), "missing {expected:?}");
    }
}
