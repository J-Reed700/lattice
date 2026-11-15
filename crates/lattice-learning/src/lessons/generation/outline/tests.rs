//! The outline's two questions: what the model writes, what the app reads from it, and what it keeps.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use super::describe::{Bounds, Describe, Frame, Outline, Passage, Piece};
use super::prerequisites::{self, Earlier, Module, Needs};
use super::*;
use crate::content_verification::tests::fixture_prompts;
use crate::lessons::course_generation_tests::outline;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shown::Told;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use serde_json::Value as Json;

const GATE: &str = "The harbour gate opens at first light.";
const ROTA: &str = "Read the kiln rota before you start.";
const TABLES: &str = "Tide tables are revised each spring.";

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn task(sent: &Sent) -> Json {
    let prompt = fixture_prompts(&sent.request)
        .into_iter()
        .next()
        .unwrap_or_default();
    prompt
        .split("\n\n")
        .find_map(|chunk| {
            serde_json::from_str::<Json>(chunk)
                .ok()
                .filter(Json::is_object)
        })
        .unwrap()
}

fn request() -> GenerateLearningProgramRequestDto {
    GenerateLearningProgramRequestDto {
        goal: "Chart the harbour tides".into(),
        prior_knowledge: "Reads tide tables".into(),
        minutes_per_session: 30,
        document_ids: vec![],
        source_urls: vec![],
        course_depth: None,
    }
}

fn describe(llm: &dyn LLMPort) -> Describe {
    let supplied = vec![
        json!({"sourceIndex":0,"title":"Harbour notes","passages":[GATE, ROTA]}),
        json!({"sourceIndex":1,"title":"Tide book","passages":[TABLES]}),
    ];
    Describe::of(
        llm,
        &request(),
        true,
        &supplied,
        &json!({"totalSources":2,"sources":[]}),
        4000,
    )
}

/// The names the request shows its passages under, with their words.
fn named(sent: &Sent) -> Vec<(String, String)> {
    task(sent)["sources"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|source| source["passages"].as_array().unwrap().iter())
        .map(|passage| {
            (
                passage["id"].as_str().unwrap().to_owned(),
                passage["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn name_of(sent: &Sent, words: &str) -> String {
    named(sent)
        .into_iter()
        .find(|(_, text)| text == words)
        .unwrap()
        .0
}

fn module(title: &str, lessons: usize, outcome_cites: &[String], outcome: &str) -> Json {
    json!({
        "title": title,
        "summary": {"text": format!("{title} is read at the quay."), "sources": []},
        "outcomes": [{"text": outcome, "sources": outcome_cites}],
        "project": {
            "title": format!("{title} chart"),
            "brief": "Draw the chart for one tide.",
            "deliverables": [{"text": "A tide chart"}],
            "successCriteria": [{"text": "The chart is legible"}]
        },
        "lessons": (0..lessons).map(|n| json!({
            "title": format!("{title} lesson {n}"),
            "objective": "Read one table.",
            "estimatedMinutes": 20,
            "sources": []
        })).collect::<Vec<_>>()
    })
}

fn modules(count: usize, lessons: usize) -> Vec<Json> {
    (0..count)
        .map(|n| {
            module(
                &format!("Module {n} of the harbour"),
                lessons,
                &[],
                "Reads a tide table.",
            )
        })
        .collect()
}

fn answer(modules: Vec<Json>) -> String {
    json!({ "modules": modules }).to_string()
}

async fn asked(model: &ScriptedModel, input: &Describe) -> Result<Settled<Outline>> {
    ask::<OutlineDescription>(
        model,
        &Asking {
            progress: None,
            reasoning: None,
        },
        &mut Scope::for_question(),
        input,
        &model_call::answer_options(),
    )
    .await
}

fn answered(settled: Settled<Outline>) -> Outline {
    match settled {
        Settled::Answered(outline) => outline,
        Settled::Unanswered(why) => panic!("the outline stayed open: {why:?}"),
    }
}

#[test]
fn the_outline_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let input = Describe {
        goal: marks.told("Chart the harbour tides".to_owned()),
        prior_knowledge: marks.told("Reads tide tables".to_owned()),
        minutes_per_session: Told(30),
        bounds: Told(Bounds {
            min_modules: 2,
            max_modules: 3,
            min_lessons: 2,
            max_lessons: 3,
        }),
        sourced: marks.internal(true),
        output_tokens: marks.internal(654_321_usize),
        sources: vec![Frame {
            index: marks.internal(424_242_usize),
            fields: marks.told(json!({"title": "Harbour notes"})),
        }],
        catalog: marks.told(json!({"totalSources": 1, "about": "Notes from the quay"})),
        material: Packed::new(
            vec![
                Piece::Passage(Passage {
                    source: marks.reference(424_242_usize),
                    rank: marks.reference(535_353_usize),
                    text: marks.told("The harbour gate opens at first light.".to_owned()),
                }),
                Piece::Entry(marks.told(json!({"title": "The kiln rota"}))),
            ],
            |_| 10,
        ),
    };
    holds::<OutlineDescription>(&input, &marks);
}

#[test]
fn the_prerequisites_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let earlier = Earlier {
        position: marks.reference(100_001_usize),
        title: marks.told("Reading tide tables".to_owned()),
        summary: marks.told("How the tables are read.".to_owned()),
    };
    let input = Needs {
        goal: marks.told("Chart the harbour tides".to_owned()),
        modules: vec![Module {
            position: marks.reference(100_002_usize),
            title: marks.told("Plotting a tide chart".to_owned()),
            summary: marks.told("How a chart is drawn.".to_owned()),
            outcomes: vec![marks.told("Plots one tide".to_owned())],
            earlier: vec![earlier],
        }],
    };
    holds::<OutlinePrerequisites>(&input, &marks);
}

#[tokio::test]
async fn a_cited_passage_is_named_by_its_handle_and_stored_as_its_words() -> Result<()> {
    let model = ScriptedModel::new().on_request::<OutlineDescription>(|sent, _| {
        let gate = name_of(sent, GATE);
        let count = describe_bounds(sent).min_modules;
        let mut written = modules(count, describe_bounds(sent).min_lessons);
        written[0] = module("The quay", 2, &[gate], "Reads a tide table.");
        reply(answer(written))
    });
    let input = describe(&model);
    let outline = answered(asked(&model, &input).await?);
    let stored = input.stored(&outline);
    assert_eq!(stored[0].outcomes[0].quote, GATE);
    assert_eq!(stored[0].outcomes[0].source_index, Some(0));
    assert_eq!(stored[0].prerequisite_indices, Vec::<usize>::new());
    assert_eq!(model.count::<OutlineDescription>(), 1);
    Ok(())
}

fn describe_bounds(sent: &Sent) -> Bounds {
    let requirement = task(sent)["requirements"][0].as_str().unwrap().to_owned();
    let numbers: Vec<usize> = requirement
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse().ok())
        .collect();
    Bounds {
        min_modules: numbers[0],
        max_modules: numbers[1],
        min_lessons: numbers[2],
        max_lessons: numbers[3],
    }
}

#[tokio::test]
async fn a_handle_that_names_no_passage_costs_only_that_citation() -> Result<()> {
    let model = ScriptedModel::new().on_request::<OutlineDescription>(|sent, _| {
        let bounds = describe_bounds(sent);
        let mut written = modules(bounds.min_modules, bounds.min_lessons);
        written[0] = module(
            "The quay",
            bounds.min_lessons,
            &["p9999".to_owned()],
            "Reads a tide table.",
        );
        reply(answer(written))
    });
    let input = describe(&model);
    let outline = answered(asked(&model, &input).await?);
    let stored = input.stored(&outline);
    assert_eq!(stored[0].outcomes[0].quote, "");
    assert_eq!(stored[0].outcomes[0].source_index, None);
    assert_eq!(stored[0].title, "The quay");
    Ok(())
}

#[tokio::test]
async fn a_lesson_count_outside_the_asked_range_does_not_refuse_the_outline() -> Result<()> {
    let model = ScriptedModel::new().on_request::<OutlineDescription>(|sent, _| {
        let bounds = describe_bounds(sent);
        reply(answer(modules(bounds.min_modules, bounds.max_lessons + 1)))
    });
    let input = describe(&model);
    let outline = answered(asked(&model, &input).await?);
    assert!(outline
        .modules
        .iter()
        .all(|module| module.lessons.len() > input.bounds.0.max_lessons));
    assert_eq!(model.count::<OutlineDescription>(), 1);
    Ok(())
}

#[tokio::test]
async fn a_module_count_outside_the_asked_range_is_corrected() -> Result<()> {
    let model = ScriptedModel::new().on_request::<OutlineDescription>(|sent, nth| {
        let bounds = describe_bounds(sent);
        let count = if nth == 0 {
            bounds.max_modules + 1
        } else {
            bounds.min_modules
        };
        reply(answer(modules(count, bounds.min_lessons)))
    });
    let input = describe(&model);
    let outline = answered(asked(&model, &input).await?);
    assert_eq!(outline.modules.len(), input.bounds.0.min_modules);
    assert_eq!(model.count::<OutlineDescription>(), 2);
    Ok(())
}

#[tokio::test]
async fn a_blank_outcome_is_corrected() -> Result<()> {
    let model = ScriptedModel::new().on_request::<OutlineDescription>(|sent, nth| {
        let bounds = describe_bounds(sent);
        let mut written = modules(bounds.min_modules, bounds.min_lessons);
        if nth == 0 {
            written[0] = module("The quay", bounds.min_lessons, &[], "   ");
        }
        reply(answer(written))
    });
    let input = describe(&model);
    let outline = answered(asked(&model, &input).await?);
    assert_eq!(outline.modules[0].outcomes[0].text, "Reads a tide table.");
    assert_eq!(model.count::<OutlineDescription>(), 2);
    Ok(())
}

#[tokio::test]
async fn the_modules_of_the_outline_ask_which_earlier_ones_they_build_on_by_handle() -> Result<()> {
    let stored: GeneratedOutline = serde_json::from_str(&outline(3, 2).0).unwrap();
    let model = ScriptedModel::new().on_request::<OutlinePrerequisites>(|sent, _| {
        let task = task(sent);
        let entries: Vec<Json> = task["modules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|module| {
                let before: Vec<Json> = module["earlierModules"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|earlier| earlier["module"].clone())
                    .collect();
                json!({"of": module["module"], "prerequisites": before})
            })
            .collect();
        reply(json!({ "entries": entries }).to_string())
    });
    let built = prerequisites::asked(
        &model,
        &crate::outline_progress::OutlineProgress::default(),
        "Chart the harbour tides",
        &stored.modules,
    )
    .await?;
    assert_eq!(built, vec![vec![], vec![0], vec![0, 1]]);
    let sent = model.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(task(&sent[0])["modules"].as_array().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_module_whose_prerequisites_stay_open_records_none() -> Result<()> {
    let stored: GeneratedOutline = serde_json::from_str(&outline(3, 2).0).unwrap();
    let model = ScriptedModel::new().on_request::<OutlinePrerequisites>(|sent, _| {
        let task = task(sent);
        let entries: Vec<Json> = task["modules"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|module| module["earlierModules"].as_array().unwrap().len() == 1)
            .map(|module| {
                let first = module["earlierModules"][0]["module"].clone();
                json!({"of": module["module"], "prerequisites": [first]})
            })
            .collect();
        reply(json!({ "entries": entries }).to_string())
    });
    let built = prerequisites::asked(
        &model,
        &crate::outline_progress::OutlineProgress::default(),
        "Chart the harbour tides",
        &stored.modules,
    )
    .await?;
    assert_eq!(built, vec![vec![], vec![0], vec![]]);
    Ok(())
}

#[test]
fn a_lesson_count_is_a_request_the_stored_outline_does_not_refuse() {
    let raw = outline(2, 9).0;
    assert!(decode_outline(&raw, &request(), &[]).is_ok());
}
