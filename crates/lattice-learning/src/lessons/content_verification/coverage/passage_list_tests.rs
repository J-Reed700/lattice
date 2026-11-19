//! The coverage mapping: what its answer decides, and which passages a unit leaves to the judge.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::checkpoints;
use super::mapping::{
    judged, passages_of, scripted_decisions, scripted_entries, task_of, Claim, CoverageMapping,
    Input, Part, Passage,
};
use super::*;
use crate::content_verification::tests::fixture_prompts;
use crate::lessons::course_generation_tests::{length_bounds, RequestLog};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::shape::Held;
use lattice_core::application::services::model_contract::test_support::samples::schema_of;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::shared::persistence::Database;

const GATE: &str = "The harbour gate opens at first light.";
const ROTA: &str = "Read the kiln rota before you start.";
const TABLES: &str = "Tide tables are revised each spring.";

fn unit(index: usize, texts: &[&str], claims: &[&str]) -> Value {
    json!({"index":index,"kind":"teaching",
        "passages":texts.iter().enumerate().map(|(n, text)| json!({"id":format!("unit-{index}-passage-{n}"),"field":"/body","text":text})).collect::<Vec<_>>(),
        "claims":claims.iter().enumerate().map(|(n, statement)| json!({"id":format!("unit-{index}-claim-{n}"),"statement":statement})).collect::<Vec<_>>(),
        "nonFactualReason":""})
}

fn ids(index: usize, passages: &[usize]) -> BTreeSet<String> {
    passages
        .iter()
        .map(|n| format!("unit-{index}-passage-{n}"))
        .collect()
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn prompt_of(sent: &Sent) -> String {
    fixture_prompts(&sent.request)
        .into_iter()
        .next()
        .unwrap_or_default()
}

fn task(sent: &Sent) -> Value {
    task_of(&prompt_of(sent)).unwrap()
}

fn texts(entries: &Value) -> Vec<String> {
    entries
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["text"].as_str().unwrap().to_owned())
        .collect()
}

fn covered(claims: &[String]) -> Value {
    json!({"claimIds":claims,"nonFactualReason":"","missingClaims":[]})
}

fn labelled() -> Value {
    json!({"claimIds":[],"nonFactualReason":"A label for the kiln rota, not an assertion.","missingClaims":[]})
}

fn omission(statement: &str) -> Value {
    json!({"claimIds":[],"nonFactualReason":"","missingClaims":[{"statement":statement}]})
}

fn empty() -> Value {
    json!({"claimIds":[],"nonFactualReason":"","missingClaims":[]})
}

fn by_text(text: &str, claims: &[String]) -> Value {
    match text {
        GATE => covered(claims),
        ROTA => labelled(),
        _ => omission("Tide tables are revised each spring."),
    }
}

#[tokio::test]
async fn a_complete_answer_decides_every_passage() -> Result<()> {
    let unit = unit(0, &[GATE, ROTA, TABLES], &[GATE]);
    let model = ScriptedModel::new().on_request::<CoverageMapping>(|sent, _| {
        reply(scripted_decisions(&prompt_of(sent), by_text))
    });
    assert_eq!(judged(&model, &unit, None).await?, ids(0, &[0, 2]));
    assert_eq!(model.count::<CoverageMapping>(), 1);
    Ok(())
}

#[tokio::test]
async fn a_passage_left_out_is_asked_for_again_alone_and_the_others_stand() -> Result<()> {
    let unit = unit(0, &[GATE, ROTA, TABLES], &[GATE]);
    let model = ScriptedModel::new().on_request::<CoverageMapping>(|sent, nth| {
        let prompt = prompt_of(sent);
        reply(if nth == 0 {
            scripted_entries(&prompt, |text, claims| match text {
                TABLES => None,
                _ => Some(by_text(text, claims)),
            })
        } else {
            scripted_decisions(&prompt, by_text)
        })
    });
    assert_eq!(judged(&model, &unit, None).await?, ids(0, &[0, 2]));
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    let (first, second) = (task(&sent[0]), task(&sent[1]));
    assert_eq!(texts(&first["unit"]["passages"]), [GATE, ROTA, TABLES]);
    assert!(first["unit"].get("contextPassages").is_none());
    assert_eq!(texts(&second["unit"]["passages"]), [TABLES]);
    assert_eq!(
        texts(&second["unit"]["contextPassages"]),
        [GATE, ROTA, TABLES]
    );
    assert_eq!(second["unit"]["claims"].as_array().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn an_empty_decision_is_corrected_for_that_passage_alone() -> Result<()> {
    let unit = unit(0, &[GATE, ROTA], &[GATE]);
    let model = ScriptedModel::new().on_request::<CoverageMapping>(|sent, nth| {
        let prompt = prompt_of(sent);
        reply(if nth == 0 {
            scripted_decisions(&prompt, |text, claims| match text {
                GATE => covered(claims),
                _ => empty(),
            })
        } else {
            scripted_decisions(&prompt, |_, _| labelled())
        })
    });
    assert_eq!(judged(&model, &unit, None).await?, ids(0, &[0]));
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    assert!(prompt_of(&sent[1]).contains("leaving all three empty"));
    assert_eq!(texts(&task(&sent[1])["unit"]["passages"]), [ROTA]);
    Ok(())
}

#[tokio::test]
async fn a_blank_missing_claim_is_asked_for_again_for_that_passage_alone() -> Result<()> {
    let unit = unit(0, &[ROTA, TABLES], &[GATE]);
    let model = ScriptedModel::new().on_request::<CoverageMapping>(|sent, nth| {
        let prompt = prompt_of(sent);
        reply(if nth == 0 {
            scripted_decisions(&prompt, |text, _| match text {
                ROTA => json!({"claimIds":[],"nonFactualReason":"","missingClaims":[{"statement":"   "}]}),
                _ => labelled(),
            })
        } else {
            scripted_decisions(&prompt, |_, _| omission("The kiln rota is read before the start."))
        })
    });
    assert_eq!(judged(&model, &unit, None).await?, ids(0, &[0]));
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(texts(&task(&sent[1])["unit"]["passages"]), [ROTA]);
    Ok(())
}

#[tokio::test]
async fn a_passage_left_undecided_is_judged() -> Result<()> {
    let unit = unit(0, &[GATE, ROTA, TABLES], &[GATE]);
    let model = ScriptedModel::new()
        .on_request::<CoverageMapping>(|_, _| reply(json!({"entries":[]}).to_string()));
    assert_eq!(judged(&model, &unit, None).await?, ids(0, &[0, 1, 2]));
    Ok(())
}

#[tokio::test]
async fn a_claim_the_request_does_not_show_costs_only_that_citation() -> Result<()> {
    let unit = unit(0, &[GATE, ROTA], &[GATE]);
    let model = ScriptedModel::new().on_request::<CoverageMapping>(|sent, _| {
        reply(scripted_decisions(
            &prompt_of(sent),
            |text, claims| match text {
                GATE => covered(&[claims[0].clone(), "c9".to_owned()]),
                _ => labelled(),
            },
        ))
    });
    assert_eq!(judged(&model, &unit, None).await?, ids(0, &[0]));
    assert_eq!(model.count::<CoverageMapping>(), 1);
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_coverage_mapping() -> Result<()> {
    let units = [
        unit(0, &[GATE, ROTA], &[GATE]),
        unit(1, &[TABLES], &[TABLES]),
    ];
    let inner = ScriptedModel::new().on_request::<CoverageMapping>(|sent, _| {
        reply(scripted_decisions(&prompt_of(sent), by_text))
    });
    let recording = RequestLog::new(&inner);
    for unit in &units {
        judged(&recording, unit, None).await?;
    }
    recording.assert_snapshot::<CoverageMapping>();
    Ok(())
}

#[test]
fn the_mapping_request_shows_what_the_model_is_told_and_nothing_internal() {
    let mut marks = Marks::new();
    let part = |marks: &mut Marks, field: &str, text: &str| Part {
        field: marks.told(field.to_owned()),
        text: marks.told(text.to_owned()),
    };
    let input = Input {
        unit: marks.internal(123_456_789_u64),
        kind: Some(marks.told("assessment".to_owned())),
        correct_index: Some(marks.told(1_u64)),
        check_role: Some(marks.told("A check names misconceptions as wrong.".to_owned())),
        context: vec![part(&mut marks, "/options/0", "The kiln glows at dusk.")],
        context_role: Some(marks.told("Options are read, never answered.".to_owned())),
        passages: vec![Passage {
            id: marks.reference("unit-9-passage-0".to_owned()),
            field: marks.told("/explanation".to_owned()),
            text: marks.told("Harbour gates open at first light.".to_owned()),
        }],
        section: vec![part(
            &mut marks,
            "/rationales/0",
            "Tide tables are revised each spring.",
        )],
        claims: vec![Claim {
            id: marks.reference("unit-9-claim-0".to_owned()),
            statement: marks.told("The harbour gate opens at dawn.".to_owned()),
        }],
        non_factual_reason: marks.told("The kiln rota is a label.".to_owned()),
    };
    holds::<CoverageMapping>(&input, &marks);
}

#[test]
fn the_mapping_schema_bounds_no_text_but_a_blank() {
    let unit = unit(0, &[GATE, ROTA], &[GATE]);
    let passages = passages_of(&unit);
    let input = Input::of(&unit, &passages.iter().collect::<Vec<_>>());
    let schema = schema_of::<CoverageMapping>(&input, Held::Enums).unwrap();
    let bounds = length_bounds(schema.as_json());
    assert!(bounds.is_empty(), "{bounds:?}");
}

#[tokio::test]
async fn a_saved_mapping_is_reused_and_one_for_other_passages_is_not() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) =
        crate::content_verification::tests::batch_scheduler_tests::setup(&db).await?;
    let unit = unit(0, &[GATE, ROTA], &[GATE]);
    let asking = || {
        ScriptedModel::new().on_request::<CoverageMapping>(|sent, _| {
            reply(scripted_decisions(&prompt_of(sent), by_text))
        })
    };
    let first = asking();
    crate::lesson_drafts::run(&db, &repo, &job, &lesson, async {
        assert_eq!(judged(&first, &unit, None).await?, ids(0, &[0]));
        assert_eq!(first.count::<CoverageMapping>(), 1);
        Ok(())
    })
    .await?;
    let silent = ScriptedModel::new();
    crate::lesson_drafts::run(&db, &repo, &job, &lesson, async {
        assert_eq!(judged(&silent, &unit, None).await?, ids(0, &[0]));
        assert_eq!(silent.count::<CoverageMapping>(), 0);
        crate::lesson_drafts::record_checkpoint(
            &checkpoints::mapping_key(&silent, &unit),
            json!({"judged":["unit-9-passage-0"]}),
        )
        .await?;
        let foreign = asking();
        assert_eq!(judged(&foreign, &unit, None).await?, ids(0, &[0]));
        assert_eq!(foreign.count::<CoverageMapping>(), 1);
        let mut changed = unit.clone();
        changed["passages"][0]["text"] = json!("The harbour gate opens at noon.");
        let other = asking();
        judged(&other, &changed, None).await?;
        assert_eq!(other.count::<CoverageMapping>(), 1);
        Ok(())
    })
    .await
}
