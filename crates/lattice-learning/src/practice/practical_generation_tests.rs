#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::practical_dto::{
    LearningSimulationSessionDto, LearningSimulationSpeaker, LearningSimulationStatus,
    LearningSimulationTurnDto,
};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use serde_json::json;

const GATE: &str = "The harbour gate opens at first light.";

const KILN: &str = "The kiln fires every bowl once the damper is shut.";

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn prompt_of(sent: &Sent) -> String {
    sent.request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The handle, as written, that the prompt shows before `sentence`.
fn handle_of(prompt: &str, sentence: &str) -> String {
    let row = prompt.find(&format!("] {sentence}")).unwrap();
    let open = prompt[..row].rfind('[').unwrap();
    prompt[open + 1..row].to_owned()
}

fn excerpt(text: &str) -> Excerpt {
    Excerpt {
        source: Internal::of("logical-source".to_owned()),
        version: Internal::of("version-1".to_owned()),
        title: Told("Harbour almanac".to_owned()),
        text: Told(text.to_owned()),
    }
}

fn session_of(turns: &[(LearningSimulationSpeaker, &str)]) -> LearningSimulationSessionDto {
    LearningSimulationSessionDto {
        id: uuid::Uuid::new_v4().to_string(),
        program_id: uuid::Uuid::new_v4().to_string(),
        activity_id: uuid::Uuid::new_v4().to_string(),
        activity_revision: 0,
        practice_session_id: None,
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        status: LearningSimulationStatus::Active,
        revision: 0,
        turns: turns
            .iter()
            .enumerate()
            .map(|(ordinal, (speaker, content))| LearningSimulationTurnDto {
                id: uuid::Uuid::new_v4().to_string(),
                ordinal: ordinal as i64,
                speaker: *speaker,
                content: (*content).to_owned(),
                citations: vec![],
                model_name: None,
                created_at: 0,
            })
            .collect(),
        created_at: 0,
        updated_at: 0,
        submitted_at: None,
    }
}

fn turn_input(passages: Packed<Excerpt>) -> TurnInput {
    let said = "I would check the tide tables first.";
    TurnInput {
        system: Told("Play the counterpart of the harbour review.".to_owned()),
        context: Told(
            json!({"scenario": {"title": "Harbour review", "brief": "Check the tide tables."}}),
        ),
        earlier: Packed::default(),
        earlier_total: Internal::of(0),
        latest: Told(vec![json!({"speaker": "learner", "content": said})]),
        latest_tokens: Internal::of(8),
        passages,
        session: Internal::of(session_of(&[(LearningSimulationSpeaker::Learner, said)])),
        plan: Internal::of(None),
        finish: Internal::of(false),
    }
}

/// A model that cites each sentence named in `cited` by the label the request shows before it.
fn turn_model(cited: serde_json::Value) -> ScriptedModel {
    ScriptedModel::new().on_request::<SimulationTurn>(move |sent, _| {
        let prompt = prompt_of(sent);
        let citations: Vec<serde_json::Value> = cited
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| match entry.as_str() {
                Some(sentence) if sentence.starts_with("The ") => {
                    json!(handle_of(&prompt, sentence))
                }
                _ => entry.clone(),
            })
            .collect();
        reply(
            json!({"content": "Which observation would help the harbour crew?", "citations": citations})
                .to_string(),
        )
    })
}

async fn asked_turn(model: &ScriptedModel, input: &TurnInput) -> Settled<Turn> {
    ask::<SimulationTurn>(
        model,
        &Asking {
            progress: None,
            reasoning: None,
        },
        &mut Scope::for_question(),
        input,
        &crate::model_call::answer_options(),
    )
    .await
    .unwrap()
}

fn activity_input(reserved: &[&str]) -> ActivityInput {
    ActivityInput {
        context: Told(json!({
            "request": "Practise reading tide tables.",
            "executionEnvironment": {"reservedPaths": reserved},
        })),
        passages: Packed::new(vec![excerpt(GATE)], |_| 5),
    }
}

/// A model that answers the nth ask with the nth of `replies`, and every later one with the last.
fn activity_model(replies: Vec<serde_json::Value>) -> ScriptedModel {
    ScriptedModel::new().on_request::<PracticalActivity>(move |_, nth| {
        reply(replies[nth.min(replies.len() - 1)].to_string())
    })
}

fn activity(sources: serde_json::Value, starter: serde_json::Value) -> serde_json::Value {
    json!({
        "title": "Tide tables",
        "brief": "Read the tide table and plan the crane lifts.",
        "sources": sources,
        "rubric": [{"title": "Reading", "description": "Reads the table.", "maxPoints": 4}],
        "starterFiles": starter,
        "referenceFiles": [],
    })
}

async fn asked_activity(model: &ScriptedModel, input: &ActivityInput) -> Settled<PracticalDraft> {
    ask::<PracticalActivity>(
        model,
        &Asking {
            progress: None,
            reasoning: None,
        },
        &mut Scope::for_question(),
        input,
        &crate::model_call::answer_options(),
    )
    .await
    .unwrap()
}

#[test]
fn the_turn_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let mut input = turn_input(Packed::new(
        vec![Excerpt {
            source: marks.internal("logical-source-77".to_owned()),
            version: marks.internal("version-77-a".to_owned()),
            title: marks.told("Harbour almanac".to_owned()),
            text: marks.told(GATE.to_owned()),
        }],
        |_| 5,
    ));
    input.system = marks.told("Play the counterpart of the harbour review.".to_owned());
    input.context = marks.told(
        json!({"scenario": {"title": "Harbour review", "brief": "Check the tide tables."}}),
    );
    input.earlier = Packed::new(
        vec![Said {
            turn: marks
                .told(json!({"speaker": "counterpart", "content": "Which gate opens first?"})),
            tokens: Internal::of(8),
        }],
        |_| 8,
    );
    input.earlier_total = Internal::of(1);
    holds::<SimulationTurn>(&input, &marks);
}

#[test]
fn the_activity_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = ActivityInput {
        context: marks.told(
            json!({"request": "Practise reading tide tables.", "lesson": {"title": "Harbour tides"}}),
        ),
        passages: Packed::new(
            vec![Excerpt {
                source: marks.internal("logical-source-77".to_owned()),
                version: marks.internal("version-77-a".to_owned()),
                title: marks.told("Harbour almanac".to_owned()),
                text: marks.told(GATE.to_owned()),
            }],
            |_| 5,
        ),
    };
    holds::<PracticalActivity>(&input, &marks);
}

#[tokio::test]
async fn a_turn_cites_a_sentence_by_its_label_and_the_app_copies_it() {
    let input = turn_input(Packed::new(
        vec![excerpt(&format!("{GATE} {KILN}"))],
        |_| 10,
    ));
    let model = turn_model(json!([KILN]));
    let Settled::Answered(turn) = asked_turn(&model, &input).await else {
        panic!("the turn was not answered");
    };
    let citations = citations_of(&input.passages, &turn.cited);
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].source_id, "logical-source");
    assert_eq!(citations[0].version_id, "version-1");
    assert_eq!(citations[0].quote, KILN);
}

#[tokio::test]
async fn a_cite_that_names_nothing_shown_costs_only_that_cite() {
    let input = turn_input(Packed::new(vec![excerpt(GATE)], |_| 5));
    let model = turn_model(json!([GATE, "u99"]));
    let Settled::Answered(turn) = asked_turn(&model, &input).await else {
        panic!("the turn was not answered");
    };
    assert_eq!(citations_of(&input.passages, &turn.cited).len(), 1);
    assert_eq!(model.sent().len(), 1, "no correction was asked for");
}

#[test]
fn older_turns_are_cut_before_the_newest_and_the_passages_get_the_rest() {
    let said = |text: &str| Said {
        turn: Told(json!({"speaker": "counterpart", "content": text})),
        tokens: Internal::of(10),
    };
    let mut input = turn_input(Packed::new(
        vec![excerpt(GATE), excerpt(KILN), excerpt(GATE)],
        |_| 5,
    ));
    input.earlier = Packed::new(
        vec![said("The newest earlier turn."), said("The oldest earlier turn.")],
        |_| 10,
    );
    input.earlier_total = Internal::of(2);
    let cut = SimulationTurn::within(&input, 30).unwrap();
    let kept: Vec<_> = cut.earlier.iter().collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].turn.0["content"], "The newest earlier turn.");
    assert_eq!(cut.passages.len(), 3);
}

#[tokio::test]
async fn an_activity_names_its_sources_by_label_and_the_app_reads_the_version() {
    let model = activity_model(vec![activity(json!(["s1", "s9"]), json!([]))]);
    let Settled::Answered(draft) = asked_activity(&model, &activity_input(&[])).await else {
        panic!("the activity was not answered");
    };
    assert!(prompt_of(&model.sent()[0]).contains("\"source\":\"s1\""));
    assert_eq!(draft.into_json().source_version_ids, vec!["version-1"]);
    assert_eq!(
        model.sent().len(),
        1,
        "an unknown source label is no reason to ask again"
    );
}

#[tokio::test]
async fn an_activity_with_an_evaluator_file_is_refused_once_and_then_corrected() {
    let with_checks = activity(
        json!([]),
        json!([{"path": "checks.py", "content": "assert True\n"}]),
    );
    let clean = activity(
        json!([]),
        json!([{"path": "notes.md", "content": "Tide notes"}]),
    );
    let model = activity_model(vec![with_checks, clean]);
    let Settled::Answered(draft) = asked_activity(&model, &activity_input(&["checks.py"])).await
    else {
        panic!("the activity was not answered");
    };
    assert_eq!(model.sent().len(), 2);
    let json = draft.into_json();
    assert!(json.starter_files.iter().all(|file| file.path != "checks.py"));
}

#[tokio::test]
async fn an_unreadable_activity_stays_unanswered() {
    let model = ScriptedModel::new()
        .on_request::<PracticalActivity>(|_, _| reply("The tide is in.".to_owned()));
    assert!(matches!(
        asked_activity(&model, &activity_input(&[])).await,
        Settled::Unanswered(_)
    ));
}
