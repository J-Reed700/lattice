#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::dto::LearningSourceDto;
use crate::lessons::structured_call::is_draft_request;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn source(id: &str, excerpt: String) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: format!("Reference {id}"),
        url: None,
        excerpt,
        acquired_at: 1,
    }
}

fn crane_source(i: usize) -> LearningSourceDto {
    source(
        &format!("crane-{i}"),
        format!(
            "The harbour crane lifts crate {i} before the tide turns. {}",
            "Cargo manifests remain sealed. ".repeat(34)
        ),
    )
}

fn shelf_source(i: usize) -> LearningSourceDto {
    source(
        &format!("shelf-{i}"),
        format!(
            "Item {i} sits under the shelf before noon. {}",
            "Inventory stickers remain intact. ".repeat(34)
        ),
    )
}

fn withheld_model(window: usize) -> ScriptedModel {
    ScriptedModel::new()
        .on_request::<ReviewEvidence>(move |sent, _| {
            if is_draft_request(&sent.request) {
                return reply("The findings are weighed.".to_owned());
            }
            let text = scripted_entries(&sent.prompt(), |_| {
                Some(decided(
                    "not_established",
                    "external_fact",
                    &[],
                    &[],
                    "none",
                    "none",
                ))
            });
            reply(text)
        })
        .window(window)
}

fn asks(model: &ScriptedModel) -> Vec<Sent> {
    model
        .sent()
        .into_iter()
        .filter(|sent| !is_draft_request(&sent.request))
        .collect()
}

fn passages_of(sent: &Sent) -> Vec<String> {
    task_of(&sent.prompt()).unwrap()["referencePassages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["text"].as_str().unwrap().to_owned())
        .collect()
}

fn run_check<'a>(
    model: &'a ScriptedModel,
    note: &str,
    references: &'a ReferenceCollection<'a>,
) -> impl std::future::Future<Output = Result<Checked>> + 'a {
    let issues = vec![ReviewFinding::reviewer(1, None, note)];
    let candidate =
        json!({"blocks":[{"title":"Part","body":"Read the instructions."}],"questions":[]});
    let requirements = json!({"deliverable":"Record the resulting state."});
    async move {
        check(
            model,
            &issues,
            Beside::NONE,
            &requirements,
            &candidate,
            references,
            None,
        )
        .await
    }
}

#[tokio::test]
async fn relevant_passages_beyond_six_thousand_tokens_are_included_when_the_window_fits(
) -> Result<()> {
    let sources: Vec<_> = (0..26).map(crane_source).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = withheld_model(64_000);
    let note = "The harbour crane lifts no crate before the tide turns.";
    run_check(&model, note, &references).await?;
    let shown = passages_of(&asks(&model)[0]);
    assert_eq!(shown.len(), 26);
    let total_tokens: usize = shown
        .iter()
        .map(|text| {
            item_tokens(
                &model,
                &json!({"id":"r0000","title":"Reference crane-0","url":null,"text":text}),
            )
        })
        .sum();
    assert!(
        total_tokens > 6_000,
        "expected > 6,000 tokens of relevant evidence, got {total_tokens}"
    );
    Ok(())
}

#[tokio::test]
async fn irrelevant_passages_below_the_relevance_cutoff_stop_retrieval_in_a_large_window(
) -> Result<()> {
    let mut sources: Vec<_> = (0..4).map(crane_source).collect();
    sources.extend((0..40).map(shelf_source));
    let references = ReferenceCollection::lexical(&sources)?;
    let model = withheld_model(128_000);
    let note = "The harbour crane lifts no crate before the tide turns.";
    run_check(&model, note, &references).await?;
    let shown = passages_of(&asks(&model)[0]);
    assert_eq!(shown.len(), 4);
    assert!(shown
        .iter()
        .all(|text| text.contains("harbour crane") && !text.contains("under the shelf")));
    Ok(())
}

#[tokio::test]
async fn relevant_passages_still_respect_the_models_context_window_room() -> Result<()> {
    let sources: Vec<_> = (0..26).map(crane_source).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let note = "The harbour crane lifts no crate before the tide turns.";
    let candidate =
        json!({"blocks":[{"title":"Part","body":"Read the instructions."}],"questions":[]});
    let requirements = json!({"deliverable":"Record the resulting state."});
    let skeleton = Input {
        issues: vec![Issue {
            at: 0,
            proposal: Told(note.to_owned()),
        }],
        requirements: Told(requirements),
        parts: parts_of(&candidate),
        references: Packed::default(),
        rulers: Vec::new(),
    };
    let call = crate::model_call::answer_options();
    let passage_cost = {
        let probe = ScriptedModel::new();
        item_tokens(
            &probe,
            &json!({"id":"r0000","title":"Reference crane-0","url":null,"text":sources[0].excerpt}),
        )
    };
    let window = (1_024..60_000).find(|window| {
        let probe = ScriptedModel::new().window(*window);
        ask::room::<ReviewEvidence>(&probe, &skeleton, &call)
            .is_ok_and(|room| room >= 4 * passage_cost && room < 8 * passage_cost)
    });
    assert!(
        window.is_some(),
        "a window with room for four to seven passages"
    );
    let model = withheld_model(window.unwrap());
    run_check(&model, note, &references).await?;
    let shown = passages_of(&asks(&model)[0]);
    assert!(shown.len() >= 4 && shown.len() < 8, "{}", shown.len());
    Ok(())
}

#[tokio::test]
async fn long_reviewer_note_keeps_domain_passages_and_cuts_single_word_boilerplate_tail(
) -> Result<()> {
    let mut sources: Vec<LearningSourceDto> = (0..6)
        .map(|i| {
            source(
                &format!("fhir-bin-{i}"),
                format!(
                    "FHIR DocumentReference Binary payload creation route {i} requires write access for Binary and DocumentReference scopes. {}",
                    "Validate Patient identifiers before storing the Binary resource. ".repeat(6)
                ),
            )
        })
        .collect();
    // Many domain documents mention DocumentReference/FHIR without Binary/write so
    // generic reviewer prose words (section, wrong, answer, check, step) have lower
    // corpus frequency than the core FHIR domain terms.
    for i in 0..12 {
        sources.push(source(
            &format!("fhir-profile-{i}"),
            format!(
                "FHIR DocumentReference Patient profile specification section {i}. {}",
                "Unrelated conformance metadata fields. ".repeat(8)
            ),
        ));
    }
    // Many large off-topic manuals share generic reviewer words ("step", "check", "answer", "write").
    for i in 0..30 {
        sources.push(source(
            &format!("linux-noise-{i}"),
            format!(
                "Linux kernel step {i}: check wrong block partition answer and write filesystem table. {}",
                "Disk geometry cylinders sectors. ".repeat(12)
            ),
        ));
    }
    let references = ReferenceCollection::lexical(&sources)?;
    let model = withheld_model(128_000);
    let site = Some(FindingSite::Section(6));
    let note = "Section 7 (candidate blocks-6): In Section 7 the check answer says to request only write access to DocumentReference records. The case states that the FHIR route sends a Binary payload, and the route creates the Binary before the DocumentReference. A token limited to DocumentReference write would leave out the Binary create step, so the answer token scope is wrong for this route.";
    let issues = vec![ReviewFinding::reviewer(1, site, note)];
    let candidate =
        json!({"blocks":[{"title":"Part","body":"Read the instructions."}],"questions":[]});
    let requirements = json!({"deliverable":"Record the resulting state."});
    check(
        &model,
        &issues,
        Beside::NONE,
        &requirements,
        &candidate,
        &references,
        None,
    )
    .await?;
    let shown = passages_of(&asks(&model)[0]);
    assert!(
        shown.iter().any(|text| text.contains("FHIR DocumentReference Binary payload")),
        "expected FHIR DocumentReference Binary passages to be kept, got {:?}",
        shown
    );
    assert!(
        shown.iter().all(|text| !text.contains("Linux kernel step")),
        "single-word generic overlap in off-topic tail must be cut off, got {:?}",
        shown
    );
    Ok(())
}
