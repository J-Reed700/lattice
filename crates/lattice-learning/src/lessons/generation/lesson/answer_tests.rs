#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::super::{requirements, Answering};
use super::*;
use crate::lessons::structured_call::{is_draft_request, task_messages};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::ScriptedModel;

const GATE: &str = "The harbour gate opens at first light.";

const CRANE: &str = "The crane lifts crate one before the tide turns.";

const KILN: &str = "The kiln fires every bowl once the damper is shut.";

fn source(id: &str, title: &str, excerpt: &str) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: title.into(),
        url: None,
        excerpt: excerpt.into(),
        acquired_at: 1,
    }
}

fn sources() -> Vec<LearningSourceDto> {
    vec![
        source("source-a", "Harbour almanac", &format!("{GATE} {CRANE}")),
        source("source-b", "Kiln manual", KILN),
    ]
}

/// The prompt the authoring stage saves, in the form later stages read.
fn stored_prompt() -> String {
    json!({
        "task": "Prepare one lesson",
        "programGoal": "Learn harbour tides.",
        "lesson": {"title": "Tides"},
        "sources": [
            {"sourceIndex": 0, "title": "Harbour almanac", "url": null, "excerpt": format!("{GATE} {CRANE}")},
            {"sourceIndex": 1, "title": "Kiln manual", "url": null, "excerpt": KILN}
        ],
        "evidenceByTopic": [
            {"topic": "Gate times", "coveringSourceIndexes": [0]},
            {"topic": "Kilns", "coveringSourceIndexes": [0, 1]}
        ],
        "referenceCatalog": [],
        "requirements": ["Old."]
    })
    .to_string()
}

fn request_of(prompt: &str) -> (String, Value) {
    let input = Input::of(prompt, &sources()).unwrap();
    let mut scope = Scope::for_question();
    let says = LessonWriter::says(&input, &mut scope.request()).unwrap();
    (says.instructions, serde_json::from_str(&says.task).unwrap())
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// A model that answers the writer with `answer(instructions, task)`, and its draft step with prose.
fn writing(answer: impl Fn(&str, &str) -> String + Send + Sync + 'static) -> ScriptedModel {
    ScriptedModel::new().on_request::<LessonWriter>(move |sent, _| {
        if is_draft_request(&sent.request) {
            return reply("The lesson is planned.".to_owned());
        }
        let (system, user) = task_messages(&sent.request);
        reply(answer(system, user))
    })
}

/// The handles of one source's sentences, as the task shows them.
fn handles(task: &Value, source: usize) -> Vec<String> {
    task["sources"][source]["excerpt"]
        .as_str()
        .unwrap()
        .lines()
        .map(|row| row.trim_start_matches('[').split_once(']').unwrap().0.to_owned())
        .collect()
}

fn part(title: &str) -> Value {
    json!({"title": title, "body": "Its body.", "cites": [], "citations": []})
}

fn practice(title: &str) -> Value {
    json!({"title": title, "body": "Its body.",
        "rubric": [{"title": "Clear", "description": "States it.", "dimension": "recall"}],
        "cites": [], "citations": []})
}

fn written(_: &str, user: &str) -> String {
    let task: Value = serde_json::from_str(user).unwrap();
    let gate = handles(&task, 0);
    let (first, second) = (gate[0].clone(), gate[1].clone());
    let kiln = handles(&task, 1)[0].clone();
    json!({
        "ideas": [
            {"sections": [
                {"kind": "explanation", "title": "Gates", "body": "The gate opens early.",
                 "cites": [first], "citations": [{"sentence": "The gate opens early.", "cites": [first, second]}]},
                {"kind": "worked_example", "title": "Case", "body": "Work a case.", "cites": [], "citations": []}
            ]},
            {"sections": [
                {"kind": "explanation", "title": "Kilns", "body": "Bowls fire.",
                 "cites": ["zz9", kiln], "citations": [{"sentence": "Bowls fire once shut.", "cites": ["u99"]}]}
            ]}
        ],
        "guidedPractice": practice("Try"),
        "independentPractice": practice("Make"),
        "reflection": part("Reflect"),
        "recap": part("Recap"),
        "questions": [{
            "kind": "quiz", "prompt": "When does the gate open?", "right": "Early",
            "wrong": [
                {"option": "Late", "reason": "It opens at first light."},
                {"option": "Never", "reason": "It does open."},
                {"option": "Noon", "reason": "Noon is long after."}
            ],
            "explanation": "It opens early.", "cites": [kiln], "citations": []
        }]
    })
    .to_string()
}

#[test]
fn the_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Input {
        before: marks.told(json!({"programGoal": "Learn harbour tides."})),
        sources: vec![Source {
            id: marks.reference("source-harbour-17".to_owned()),
            title: marks.told("Harbour almanac".to_owned()),
            url: Some(marks.told("https://example.test/almanac".to_owned())),
            excerpt: marks.told("The harbour gate opens at first light.".to_owned()),
        }],
        topics: vec![Topic {
            topic: marks.told("Gate times".to_owned()),
            covering: marks.internal(vec!["source-harbour-17".to_owned()]),
        }],
        after: marks.told(json!({"referenceCatalog": []})),
    };
    holds::<LessonWriter>(&input, &marks);
}

#[test]
fn the_request_names_sources_and_their_sentences_by_handle() {
    let (_, task) = request_of(&stored_prompt());
    let keys: Vec<&str> = task
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "task",
            "programGoal",
            "lesson",
            "sources",
            "evidenceByTopic",
            "referenceCatalog",
            "requirements"
        ]
    );
    assert_eq!(task["sources"][0]["id"], "s1");
    assert_eq!(task["sources"][1]["id"], "s2");
    assert_eq!(
        task["sources"][0]["excerpt"],
        format!("[u1] {GATE}\n[u2] {CRANE}")
    );
    assert!(task["sources"][1]["excerpt"]
        .as_str()
        .unwrap()
        .ends_with(KILN));
    assert_eq!(
        task["evidenceByTopic"][1]["coveringSources"],
        json!(["s1", "s2"])
    );
    assert!(task["sources"][0].get("sourceIndex").is_none());
}

#[test]
fn the_writers_text_never_speaks_of_numbers_positions_or_parallel_lists() {
    let (instructions, task) = request_of(&stored_prompt());
    let text = format!("{instructions}\n{task}");
    for old in [
        "sourceIndex",
        "correctIndex",
        "rationales",
        "coveringSourceIndexes",
        "copy the supporting words",
    ] {
        assert!(!text.contains(old), "{old} in {text}");
    }
}

#[test]
fn the_saved_prompt_keeps_asking_for_what_later_stages_read() {
    let stored = requirements(Answering::Stored { grounded: true }).to_string();
    for kept in [
        "sourceIndexes whose excerpts cover each",
        "rationales has one entry per choice",
        "All other blocks use an empty rubric array.",
    ] {
        assert!(stored.contains(kept), "{kept} missing from {stored}");
    }
}

#[tokio::test]
async fn the_answer_becomes_the_draft_later_stages_read() {
    let input = Input::of(&stored_prompt(), &sources()).unwrap();
    let model = writing(written);

    let stored: Value =
        serde_json::from_str(&write(&model, &input, "Write it.").await.unwrap()).unwrap();

    assert!(super::super::reads_as_lesson(&stored));
    let blocks = stored["blocks"].as_array().unwrap();
    let kinds: Vec<&str> = blocks.iter().map(|b| b["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "explanation",
            "worked_example",
            "explanation",
            "guided_practice",
            "independent_practice",
            "reflection",
            "recap"
        ]
    );
    assert_eq!(blocks[0]["sourceIndex"], 0);
    assert_eq!(blocks[0]["quote"], GATE);
    let cited = blocks[0]["citations"].as_array().unwrap();
    assert_eq!(cited.len(), 2);
    assert_eq!(
        cited[0],
        json!({"sentence": "The gate opens early.", "sourceIndex": 0, "quote": GATE})
    );
    assert_eq!(cited[1]["quote"], CRANE);
    assert_eq!(blocks[1]["sourceIndex"], Value::Null);
    assert_eq!(blocks[1]["quote"], "");
    assert_eq!(blocks[2]["sourceIndex"], 1);
    assert_eq!(blocks[2]["quote"], KILN);
    assert_eq!(blocks[2]["citations"], json!([]));
    assert_eq!(blocks[3]["rubric"][0]["dimension"], "recall");
    assert_eq!(blocks[0]["rubric"], json!([]));
    let question = &stored["questions"][0];
    let right = question["correctIndex"].as_u64().unwrap() as usize;
    assert_eq!(question["options"][right], "Early");
    assert_eq!(question["rationales"][right], "");
    for (option, reason) in [
        ("Late", "It opens at first light."),
        ("Never", "It does open."),
        ("Noon", "Noon is long after."),
    ] {
        let at = question["options"]
            .as_array()
            .unwrap()
            .iter()
            .position(|shown| shown == option)
            .unwrap();
        assert_eq!(question["rationales"][at], reason);
    }
}

#[test]
fn the_right_choice_is_not_always_laid_out_first() {
    let shown = |prompt: &str| {
        let wrong = |option: &str| Wrong {
            option: option.into(),
            reason: format!("{option} fails."),
        };
        let question = Question {
            kind: Asked::Quiz,
            prompt: prompt.into(),
            right: "Right".into(),
            wrong: vec![wrong("A"), wrong("B"), wrong("C")],
            explanation: "Why.".into(),
            cites: Vec::new(),
            cited: Vec::new(),
        };
        Excerpts(Vec::new()).question(&question)["correctIndex"].as_u64().unwrap()
    };
    let prompts = [
        "Which tide is highest?",
        "When does the gate open?",
        "What does the kiln need?",
        "Where does the crane stand?",
        "Why is the damper shut?",
        "How is the crate lifted?",
    ];
    let positions: Vec<u64> = prompts.iter().map(|prompt| shown(prompt)).collect();
    assert!(positions.iter().any(|at| *at != 0), "{positions:?}");
    assert!(positions.iter().any(|at| *at != positions[0]), "{positions:?}");
    let expected: Vec<u64> = prompts
        .iter()
        .map(|prompt| {
            crate::assessment::assessment_engine::option_order(
                crate::assessment::assessment_engine::LearningItemFormat::SingleChoice,
                4,
                prompt,
            )
            .iter()
            .position(|at| *at == 0)
            .unwrap() as u64
        })
        .collect();
    assert_eq!(positions, expected);
}

#[tokio::test]
async fn a_scripted_saved_lesson_is_written_as_the_same_lesson() {
    use crate::lessons::course_generation_tests::lesson_response;
    let mut old = lesson_response();
    for field in ["blocks", "questions"] {
        for item in old[field].as_array_mut().unwrap() {
            item["sourceIndex"] = json!(0);
            item["quote"] = json!(GATE);
        }
    }
    let input = Input::of(&stored_prompt(), &sources()).unwrap();
    let saved = old.to_string();
    let model =
        writing(move |system, user| scripted_lesson(&saved, &format!("{system}\n\n{user}")));

    let stored: Value =
        serde_json::from_str(&write(&model, &input, "Write it.").await.unwrap()).unwrap();

    assert_eq!(
        stored["blocks"].as_array().unwrap().len(),
        old["blocks"].as_array().unwrap().len()
    );
    assert_eq!(stored["blocks"][0]["quote"], GATE);
    let (old_question, question) = (&old["questions"][0], &stored["questions"][0]);
    assert_eq!(
        question["options"].as_array().unwrap().len(),
        old_question["options"].as_array().unwrap().len()
    );
    let (old_right, right) = (
        old_question["correctIndex"].as_u64().unwrap() as usize,
        question["correctIndex"].as_u64().unwrap() as usize,
    );
    assert_eq!(question["options"][right], old_question["options"][old_right]);
    assert_eq!(question["rationales"][right], "");
}
