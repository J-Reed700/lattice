use super::quotes::{authored, lesson_quote_checks, reanchor_quotes_in, restore_quotes_in};
use super::*;
use crate::lessons::course_generation_tests::{clean_review, verdict, ScriptedModel};
use std::sync::Mutex;

fn restore_lesson_quotes(candidate: &mut Value, authoring: &Value) -> usize {
    restore_quotes_in(candidate, &authored(authoring).unwrap())
}

fn reanchor_lesson_quotes(candidate: &mut Value, authoring: &Value) -> usize {
    let table = authored(authoring).unwrap();
    reanchor_quotes_in(candidate, &table, &table)
}

const EXCERPT: &str = "This saved reference describes the exact behavior of the example tool.";

fn lesson(quote: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Example","body":"A substantive teaching explanation with practice context. ".repeat(20),"rubric":[],"sourceIndex":0,"quote":quote}],"questions":[]})
}

fn context() -> Value {
    json!({"sources":[{"sourceIndex":0,"excerpt":EXCERPT}]})
}

/// A lesson schema whose blocks take citations, which is what lets a repair recite one.
fn citing_schema() -> Value {
    json!({"type":"object","properties":{"blocks":{"type":"array","items":{"type":"object","properties":{"citations":{"type":"array","items":{"type":"object"}}}}}}})
}

/// A repair's answer that gives the quotation of a part the passage of the source it names.
fn recite(part: usize, quote: &str) -> String {
    json!({"recites":[{"path":format!("/blocks/{part}"),"sourceIndex":0,"quote":quote}]})
        .to_string()
}

fn approval() -> Value {
    json!({"issues":[],"sectionDefects":[]})
}

#[tokio::test]
async fn lesson_review_only_rewrites_established_findings() -> Result<()> {
    let references = crate::reference_collection::ReferenceCollection::lexical(&[
        crate::dto::LearningSourceDto {
            id: "saved-reference".into(),
            title: "Example reference".into(),
            url: None,
            acquired_at: 0,
            excerpt: EXCERPT.into(),
        },
    ])?;
    for actionable in [false, true] {
        let original = lesson(EXCERPT);
        let mut corrected = original.clone();
        if let Some(title) = corrected.pointer_mut("/blocks/0/title") {
            *title = json!("Corrected example");
        }
        let mut review = approval();
        if let Some(issues) = review.get_mut("issues") {
            *issues = json!([{"note":"The lesson promises that the tutor runs the learner's code; correct the promise using the reference.","concern":"promise"}]);
        }
        let decision = json!({"entries":[{"of":"i1","verdict":if actionable {"actionable"} else {"not_established"},"basis":"external_fact","candidateIds":[],"referenceIds":["r1"],"partId":"none","leftTo":"none","reason":"The source comparison determines whether this proposed correction is justified."}]});
        let mut outputs = vec![review.to_string(), decision.to_string()];
        if actionable {
            outputs.extend([json!({"edits":[{"path":"/blocks/0/title","before":"Example","after":"Corrected example"}]}).to_string(), clean_review(&[verdict("reviewer/1", "resolved")])]);
        }
        let model = ScriptedModel {
            outputs: Mutex::new(outputs.into()),
            prompts: Mutex::new(Vec::new()),
        };
        let result = review_lesson(
            &model,
            LESSON_REPAIR_SYSTEM,
            &context().to_string(),
            ReviewSchema::Other(&json!({"type":"object"})),
            original.to_string(),
            4000,
            (&references, &references.sources),
            &[],
        )
        .await?;
        assert_eq!(
            serde_json::from_str::<Value>(&result)?,
            if actionable { corrected } else { original }
        );
        let prompts = model
            .prompts
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?;
        assert_eq!(prompts.len(), if actionable { 4 } else { 2 });
        assert_eq!(
            prompts
                .iter()
                .filter(|prompt| prompt.contains("You are editing a rejected draft"))
                .count(),
            usize::from(actionable)
        );
    }
    Ok(())
}

#[tokio::test]
async fn source_typography_is_restored_without_a_model_rewrite() -> Result<()> {
    let exact = "We’ll download Rust through rustup, a\ncommand line tool for managing Rust versions and associated tools.";
    let authoring = json!({"sources":[{"sourceIndex":0,"excerpt":exact}]});
    let original = lesson("We'll download Rust through rustup, a command line tool for managing Rust versions and associated tools.");
    let model = ScriptedModel {
        outputs: Mutex::new(vec![approval().to_string()].into()),
        prompts: Mutex::new(Vec::new()),
    };
    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &authoring.to_string(),
        &json!({"type":"object"}),
        original.to_string(),
        4000,
    )
    .await?;
    assert_eq!(serde_json::from_str::<Value>(&result)?, lesson(exact));
    Ok(())
}

#[test]
fn typography_recovery_never_changes_words_quantities_or_the_cited_source() {
    let exact = "The cell’s response does not increase by 5 units: x != 10.";
    let authoring = json!({"sources":[{"sourceIndex":0,"excerpt":exact},{"sourceIndex":1,"excerpt":"Another source describes a different biological mechanism."}]});
    for quote in [
        exact.replace("does not", "does"),
        exact.replace("5 units", "6 units"),
        exact.replace("!=", "="),
        "This unsupported quotation has no matching source passage.".into(),
    ] {
        let mut candidate = lesson(&quote);
        let before = candidate.clone();
        assert_eq!(restore_lesson_quotes(&mut candidate, &authoring), 0);
        assert_eq!(candidate, before);
    }
    let mut wrong_source = json!({"questions":[{"sourceIndex":1,"quote":exact.replace('’', "'")}]});
    let before = wrong_source.clone();
    assert_eq!(restore_lesson_quotes(&mut wrong_source, &authoring), 0);
    assert_eq!(wrong_source, before);
}

#[test]
fn lesson_citations_match_the_published_source_index_and_quote_rules() {
    let mut candidate =
        lesson("This saved reference\n describes the exact behavior of the example tool.");
    assert!(lesson_quote_checks(&candidate, &context()).is_empty());
    if let Some(object) = candidate.as_object_mut() {
        object.insert(
            "questions".into(),
            json!([{"sourceIndex":1,"quote":EXCERPT}]),
        );
    }
    let checks = lesson_quote_checks(&candidate, &context());
    assert_eq!(checks.len(), 1);
    assert_eq!(
        checks.first().map(|check| check.path.as_str()),
        Some("questions[0]")
    );
}

#[tokio::test]
async fn a_model_approval_cannot_skip_repair_of_an_invalid_lesson_quotation() -> Result<()> {
    let corrected = lesson(EXCERPT);
    let model = ScriptedModel {
        outputs: Mutex::new(
            vec![
                approval().to_string(),
                recite(0, EXCERPT),
                approval().to_string(),
            ]
            .into(),
        ),
        prompts: Mutex::new(Vec::new()),
    };
    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context().to_string(),
        &citing_schema(),
        lesson("An invented quotation that does not appear in the selected source.").to_string(),
        4000,
    )
    .await?;
    assert_eq!(serde_json::from_str::<Value>(&result)?, corrected);
    Ok(())
}

#[test]
fn reanchoring_prefers_the_quote_then_the_item_and_never_invents_a_relation() {
    let authoring = json!({"sources":[
        {"sourceIndex":0,"excerpt":"Navigation home page. The cache stores computed values so repeated requests return quickly."},
        {"sourceIndex":1,"excerpt":"Replication copies each write to several machines so a single failure loses nothing."}
    ]});
    // A paraphrase of a sentence in another source moves to that source.
    let mut candidate = json!({"blocks":[{"body":"Why replicas matter.","sourceIndex":0,"quote":"Replication copies every write to several machines so one failure loses nothing."}],"questions":[]});
    assert_eq!(reanchor_lesson_quotes(&mut candidate, &authoring), 1);
    assert_eq!(candidate["blocks"][0]["sourceIndex"], 1);
    assert_eq!(
        candidate["blocks"][0]["quote"],
        "Replication copies each write to several machines so a single failure loses nothing."
    );
    // No usable quote: the item's own text chooses the passage.
    let mut candidate = json!({"blocks":[{"body":"A cache returns repeated requests quickly from stored values.","sourceIndex":null,"quote":""}],"questions":[]});
    assert_eq!(reanchor_lesson_quotes(&mut candidate, &authoring), 1);
    assert_eq!(candidate["blocks"][0]["sourceIndex"], 0);
    assert_eq!(
        candidate["blocks"][0]["quote"],
        "The cache stores computed values so repeated requests return quickly."
    );
    // Nothing in common with any excerpt: left alone for the strict check.
    let mut candidate = json!({"blocks":[{"body":"Unrelated material.","sourceIndex":0,"quote":"Completely different wording appears here instead."}],"questions":[]});
    let before = candidate.clone();
    assert_eq!(reanchor_lesson_quotes(&mut candidate, &authoring), 0);
    assert_eq!(candidate, before);
}

#[tokio::test]
async fn a_repaired_quotation_that_is_still_invalid_is_repaired_again() {
    const INVENTED: &str = "An invented quotation that does not appear in the selected source.";
    let retitle =
        json!({"edits":[{"path":"/blocks/0/title","before":"Example","after":"Example, revised"}]})
            .to_string();
    let model = ScriptedModel {
        outputs: Mutex::new(
            vec![
                approval().to_string(),
                retitle,
                approval().to_string(),
                recite(0, EXCERPT),
                approval().to_string(),
            ]
            .into(),
        ),
        prompts: Mutex::new(Vec::new()),
    };
    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context().to_string(),
        &citing_schema(),
        lesson(INVENTED).to_string(),
        4000,
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
}

const REPLICATION: &str =
    "Replication copies each write to several machines so a single failure loses nothing.";

#[test]
fn a_cited_sentences_quote_is_checked_like_the_sections() {
    let mut candidate = lesson(EXCERPT);
    candidate["blocks"][0]["citations"] = json!([{"sentence":"The tool behaves exactly as described.","sourceIndex":0,"quote":EXCERPT}]);
    assert!(lesson_quote_checks(&candidate, &context()).is_empty());

    candidate["blocks"][0]["citations"][0]["quote"] =
        json!("An invented quotation that does not appear in the selected source.");
    let checks = lesson_quote_checks(&candidate, &context());
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].path, "blocks[0]");
    assert!(checks[0].error.starts_with("Citation 1 of this part:"));

    // A citation of a source the lesson was not given is refused too.
    candidate["blocks"][0]["citations"][0] =
        json!({"sentence":"A sentence.","sourceIndex":3,"quote":EXCERPT});
    assert_eq!(lesson_quote_checks(&candidate, &context()).len(), 1);
}

#[test]
fn the_typography_of_a_cited_sentences_quote_is_restored_with_the_sections() {
    let exact =
        "We’ll download Rust through rustup, a command line tool for managing Rust versions.";
    let authoring = json!({"sources":[{"sourceIndex":0,"excerpt":exact}]});
    let mut candidate = lesson(exact);
    candidate["blocks"][0]["citations"] = json!([{"sentence":"Rust is downloaded through rustup.",
        "sourceIndex":0,"quote":exact.replace('’', "'")}]);

    assert_eq!(restore_lesson_quotes(&mut candidate, &authoring), 1);

    assert_eq!(candidate["blocks"][0]["citations"][0]["quote"], exact);
}

#[test]
fn a_cited_sentence_whose_quote_matches_nothing_is_cited_to_the_nearest_passage_of_the_sentence() {
    let authoring = json!({"sources":[
        {"sourceIndex":0,"excerpt":"Navigation home page. The cache stores computed values so repeated requests return quickly."},
        {"sourceIndex":1,"excerpt":REPLICATION}
    ]});
    let mut candidate = json!({"blocks":[{"body":"Why replicas matter. Writes are copied to several machines.","sourceIndex":0,
        "quote":"The cache stores computed values so repeated requests return quickly.",
        "citations":[{"sentence":"Replication copies every write to several machines so one failure loses nothing.",
            "sourceIndex":0,"quote":"Completely different wording appears here instead."}]}],"questions":[]});

    assert_eq!(reanchor_lesson_quotes(&mut candidate, &authoring), 1);

    assert_eq!(
        candidate["blocks"][0]["sourceIndex"], 0,
        "the section's quote holds"
    );
    assert_eq!(candidate["blocks"][0]["citations"][0]["sourceIndex"], 1);
    assert_eq!(candidate["blocks"][0]["citations"][0]["quote"], REPLICATION);
}

#[test]
fn a_citation_the_repair_appends_is_checked_against_its_source_like_any_other() {
    let candidate = lesson(EXCERPT);
    let sentence = "A substantive teaching explanation with practice context.";
    let appended = |quote: &str| {
        let mut appended = candidate.clone();
        appended["blocks"][0]["citations"] =
            json!([{"sentence":sentence,"sourceIndex":0,"quote":quote}]);
        appended
    };

    // A quote the source holds stands.
    assert!(lesson_quote_checks(&appended(EXCERPT), &context()).is_empty());

    // One it does not is refused, at the part that cites it.
    let refused = lesson_quote_checks(
        &appended("An invented quotation that does not appear in the selected source."),
        &context(),
    );
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].path, "blocks[0]");
    assert!(refused[0].error.starts_with("Citation 1 of this part:"));
}

#[tokio::test]
async fn a_repair_that_goes_wrong_in_one_block_is_asked_again_for_that_finding_alone() -> Result<()>
{
    let invented = "An invented quotation that does not appear in the selected source.";
    let mut original = lesson(EXCERPT);
    let block = original["blocks"][0].clone();
    original["blocks"] = json!([block.clone(), block.clone(), block]);
    original["blocks"][0]["quote"] = json!(invented);
    original["blocks"][1]["title"] = json!("Example");
    let approved = || json!({"issues":[],"sectionDefects":[]}).to_string();
    let reaching = json!({
        "edits":[{"path":"/blocks/1/title","before":"Example","after":"Corrected example"}],
        "recites":[{"path":"/blocks/0","sourceIndex":0,"quote":EXCERPT}]
    });
    let model = ScriptedModel {
        outputs: Mutex::new(
            vec![
                approved(),
                reaching.to_string(),
                recite(0, EXCERPT),
                clean_review(&[]),
            ]
            .into(),
        ),
        prompts: Mutex::new(Vec::new()),
    };

    let result = review_and_repair(
        &model,
        LESSON_REPAIR_SYSTEM,
        &context().to_string(),
        &citing_schema(),
        original.to_string(),
        4000,
    )
    .await?;

    // Block 1's title is no finding's to change: the entry that reached for it
    // is refused whole and asked again, and its second answer applies.
    let mut expected = original.clone();
    expected["blocks"][0]["quote"] = json!(EXCERPT);
    assert_eq!(serde_json::from_str::<Value>(&result)?, expected);
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(
        prompts
            .iter()
            .filter(|prompt| prompt.contains("You are editing a rejected draft"))
            .count(),
        2,
        "the refused entry is asked again once"
    );
    Ok(())
}

#[test]
fn reanchoring_keeps_a_sentence_whole_across_an_abbreviation() {
    let authoring = json!({"sources":[{"sourceIndex":0,
        "excerpt":"Most teams pick Rust for their services vs. Go and Python. Both compile fast."}]});
    let mut candidate = json!({"blocks":[{"body":"Unrelated.","sourceIndex":0,"quote":"Teams choose Rust for their services"}],"questions":[]});
    assert_eq!(reanchor_lesson_quotes(&mut candidate, &authoring), 1);
    assert_eq!(
        candidate["blocks"][0]["quote"],
        "Most teams pick Rust for their services vs. Go and Python."
    );
}
