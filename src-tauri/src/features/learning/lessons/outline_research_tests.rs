use super::*;
use crate::features::{
    function_calling::dto::{
        FetchUrlContentOutput, WebSearchInput, WebSearchOutput, WebSearchResult,
    },
    learning::{
        outline_draft::LearningOutlineReviewStatus,
        outline_progress::{OutlineProgress, OutlineRun, OutlineStage},
    },
    web::{mocks::MockWebService, traits::WebServiceTrait},
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

const ORIGINAL: &str = "A randomized comparison isolates the intervention from other influences.";
const ADDITIONAL: &str = "Random assignment limits confounding when treatment groups are compared.";
const SECOND: &str =
    "Observed differences require an uncertainty estimate before causal interpretation.";

struct ResearchWeb {
    inner: MockWebService,
    pages: Vec<String>,
    queries: Mutex<Vec<String>>,
    fetches: Mutex<Vec<String>>,
}
impl ResearchWeb {
    fn new(passages: &[&str]) -> Self {
        let inner = MockWebService::new();
        let pages: Vec<_> = passages
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let url = format!("https://example.org/reference-{i}");
                inner.set_url_content(
                    &url,
                    FetchUrlContentOutput {
                        url: url.clone(),
                        title: Some(format!("Research reference {i}")),
                        content: (*text).into(),
                        content_truncated: false,
                        word_count: text.split_whitespace().count(),
                        fetch_time_ms: 1.,
                        content_type: None,
                        from_cache: false,
                    },
                );
                url
            })
            .collect();
        Self {
            inner,
            pages,
            queries: Mutex::new(vec![]),
            fetches: Mutex::new(vec![]),
        }
    }
}
#[async_trait::async_trait]
impl WebServiceTrait for ResearchWeb {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        let index = {
            let mut queries = self.queries.lock().unwrap();
            queries.push(input.query.clone());
            queries.len() - 1
        };
        if self.pages.is_empty() {
            return Err(AppError::ServiceNotAvailable(
                "Search fixture unavailable".into(),
            ));
        }
        self.inner.set_search_results(vec![WebSearchResult {
            title: "Research reference".into(),
            url: self.pages[index.min(self.pages.len() - 1)].clone(),
            snippet: "SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE".into(),
            published_date: None,
            source: None,
        }]);
        self.inner.search_web(input).await
    }
    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.fetches.lock().unwrap().push(url.into());
        self.inner.fetch_reference_content(url).await
    }
    async fn fetch_url_content(&self, _: &str) -> Result<FetchUrlContentOutput> {
        panic!("Research must use complete reference captures, not chat previews")
    }
    fn validate_url(&self, url: &str) -> Result<()> {
        self.inner.validate_url(url)
    }
}

fn cite(value: &mut Value, index: usize, quote: &str) {
    match value {
        Value::Object(object) => {
            if object.contains_key("sourceIndex") {
                object.insert("sourceIndex".into(), json!(index));
                object.insert("quote".into(), json!(quote));
            }
            for child in object.values_mut() {
                cite(child, index, quote);
            }
        }
        Value::Array(values) => {
            for child in values {
                cite(child, index, quote);
            }
        }
        _ => {}
    }
}
fn candidate() -> Value {
    serde_json::from_str(&outline(4, 3).0).unwrap()
}
fn issue(index: usize) -> Value {
    json!({"issues":[{"path":format!("/modules/{index}"),"claim":"The comparison needs stronger evidence.","reason":"The saved passage does not support this specific comparison. Find a supporting reference."}]})
}
fn model(outputs: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into_iter().map(|v| v.to_string()).collect()),
        prompts: Mutex::new(vec![]),
    }
}
fn original_source() -> LearningSourceDto {
    LearningSourceDto {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Brief reference overview".into(),
        excerpt: ORIGINAL.into(),
        url: Some("https://example.org/overview".into()),
        acquired_at: 0,
    }
}
fn references(
    sources: &[LearningSourceDto],
) -> Vec<crate::features::learning::sources::InitialReference> {
    sources
        .iter()
        .map(|s| crate::features::learning::sources::InitialReference {
            source_id: s.id.clone(),
            origin: s.url.clone().unwrap(),
            captured: crate::features::learning::source_library::CapturedLearningSource {
                title: s.title.clone(),
                publisher: None,
                requested_url: s.url.clone(),
                resolved_url: s.url.clone(),
                text: s.excerpt.clone(),
                truncated: false,
                extraction_version: "complete_test_capture".into(),
            },
        })
        .collect()
}
async fn generate_online(
    repo: &LearningRepository,
    model: &ScriptedModel,
    web: &ResearchWeb,
    sources: Vec<LearningSourceDto>,
) -> Result<LearningProgramDto> {
    let captured = references(&sources);
    service::generate_with_references_and_progress(
        repo,
        model,
        request(LearningCourseDepth::Course),
        sources,
        &captured,
        &OutlineProgress::default(),
        Some(web),
    )
    .await
}

#[tokio::test]
async fn initial_generation_researches_review_gaps_before_repair_without_opt_in() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    cite(&mut initial, 0, ORIGINAL);
    let mut fixed = initial["modules"][0].clone();
    cite(&mut fixed, 1, ADDITIONAL);
    let llm = model(vec![
        initial,
        issue(0),
        json!({"module":fixed}),
        json!({"issues":[]}),
    ]);
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let stages = Arc::new(Mutex::new(vec![]));
    let observed = stages.clone();
    let run = OutlineRun::register(None, move |update| {
        observed.lock().unwrap().push(update.stage)
    })?;
    let sources = vec![original_source()];
    let captured = references(&sources);
    let program = run
        .0
        .run(service::generate_with_references_and_progress(
            &repo,
            &llm,
            request(LearningCourseDepth::Course),
            sources,
            &captured,
            &run.0,
            Some(&web),
        ))
        .await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    assert_eq!(program.sources.len(), 2);
    assert_eq!(web.queries.lock().unwrap().len(), 1);
    assert_eq!(web.fetches.lock().unwrap().len(), 1);
    let saved = repo.verification_sources(&program.summary.id).await?;
    assert!(saved.iter().any(|source| source.excerpt == ADDITIONAL));
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 4);
    assert!(prompts[2].contains(ADDITIONAL));
    // Array positions must never be confused with the module numbers seen by
    // learners, in either the reviewer or the subsequent repair request.
    let review_context: Value = serde_json::from_str(prompts[1].rsplit("\n\n").next().unwrap())?;
    let repair_context: Value = serde_json::from_str(prompts[2].rsplit("\n\n").next().unwrap())?;
    for context in [&review_context, &repair_context] {
        assert_eq!(
            context["moduleNumbering"]["modules"][1]["path"],
            "/modules/1"
        );
        assert_eq!(context["moduleNumbering"]["modules"][1]["moduleIndex"], 1);
        assert_eq!(context["moduleNumbering"]["modules"][1]["moduleNumber"], 2);
    }
    assert_eq!(repair_context["moduleIndex"], 0);
    assert_eq!(repair_context["moduleNumber"], 1);
    assert!(!prompts[2].contains("SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE"));
    let stages = stages.lock().unwrap();
    assert!(
        stages
            .iter()
            .position(|s| *s == OutlineStage::Researching)
            .unwrap()
            < stages
                .iter()
                .position(|s| *s == OutlineStage::Repairing)
                .unwrap()
    );
    Ok(())
}

#[tokio::test]
async fn topic_only_generation_acquires_references_and_repairs_missing_citations() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    let mut cited = initial.clone();
    cite(&mut cited, 0, ADDITIONAL);
    let mut outputs = vec![initial, json!({"issues":[]})];
    outputs.extend(
        cited["modules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| json!({"module":m})),
    );
    outputs.push(json!({"issues":[]}));
    let llm = model(outputs);
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let program = generate_online(&repo, &llm, &web, vec![]).await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    assert_eq!(program.sources.len(), 1);
    assert!(llm.prompts.lock().unwrap()[1].contains(ADDITIONAL));
    assert_eq!(web.fetches.lock().unwrap().len(), 1);
    assert_eq!(repo.list().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn unchanged_findings_stop_without_repeating_research_even_when_module_is_renamed(
) -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    cite(&mut initial, 0, ORIGINAL);
    let mut changed = initial["modules"][0].clone();
    changed["title"] = json!("Renamed experimental comparison");
    let llm = model(vec![initial, issue(0), json!({"module":changed}), issue(0)]);
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let program = generate_online(&repo, &llm, &web, vec![original_source()]).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::NeedsRepair);
    assert_eq!(review.issues.len(), 1);
    assert_eq!(review.repair_passes, 1);
    assert!(review.note.contains("did not decrease"));
    assert_eq!(program.modules[0].title, "Renamed experimental comparison");
    assert_eq!(web.queries.lock().unwrap().len(), 1);
    assert_eq!(llm.prompts.lock().unwrap().len(), 4);
    Ok(())
}

#[tokio::test]
async fn new_evidence_allows_repair_when_finding_count_is_unchanged() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    cite(&mut initial, 0, ORIGINAL);
    let mut module0 = initial["modules"][0].clone();
    cite(&mut module0, 1, ADDITIONAL);
    let mut module1 = initial["modules"][1].clone();
    cite(&mut module1, 2, SECOND);
    let llm = model(vec![
        initial,
        issue(0),
        json!({"module":module0}),
        issue(1),
        json!({"module":module1}),
        json!({"issues":[]}),
    ]);
    let web = ResearchWeb::new(&[ADDITIONAL, SECOND]);
    let program = generate_online(&repo, &llm, &web, vec![original_source()]).await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    assert_eq!(program.outline_review.as_ref().unwrap().repair_passes, 2);
    assert_eq!(program.sources.len(), 3);
    assert_eq!(web.queries.lock().unwrap().len(), 2);
    assert!(llm.prompts.lock().unwrap()[4].contains(SECOND));
    Ok(())
}

#[tokio::test]
async fn unavailable_research_keeps_a_topic_draft_without_rewriting_or_approving_it() -> Result<()>
{
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    // Keep the original unsupported quotation visible alongside a search failure.
    cite(&mut initial["modules"][0]["summary"], 0, ORIGINAL);
    let llm = model(vec![initial]);
    let web = ResearchWeb::new(&[]);
    let program = generate_online(&repo, &llm, &web, vec![]).await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::NeedsRepair
    );
    assert!(program
        .outline_review
        .as_ref()
        .unwrap()
        .note
        .contains("could not obtain"));
    assert_eq!(program.outline_review.as_ref().unwrap().issues.len(), 2);
    assert_eq!(web.queries.lock().unwrap().len(), 2);
    assert_eq!(llm.prompts.lock().unwrap().len(), 1);
    assert_eq!(repo.get(&program.summary.id).await?.modules.len(), 4);
    Ok(())
}

#[tokio::test]
async fn resumed_repair_researches_saved_findings_before_repeating_model_review() -> Result<()> {
    use crate::features::learning::outline_draft::{repair_saved, RepairLearningOutlineRequestDto};
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    cite(&mut initial, 0, ORIGINAL);
    let llm = model(vec![
        initial.clone(),
        issue(0),
        json!({"module":initial["modules"][0]}),
    ]);
    let sources = vec![original_source()];
    let captured = references(&sources);
    let saved = service::generate_with_references_and_progress(
        &repo,
        &llm,
        request(LearningCourseDepth::Course),
        sources,
        &captured,
        &OutlineProgress::default(),
        None,
    )
    .await?;
    assert_eq!(
        saved.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::NeedsRepair
    );
    let mut fixed = initial["modules"][0].clone();
    cite(&mut fixed, 1, ADDITIONAL);
    let llm = model(vec![
        issue(0),
        json!({"module":fixed}),
        json!({"issues":[]}),
    ]);
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let program = repair_saved(
        &repo,
        &llm,
        &RepairLearningOutlineRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
        },
        &OutlineProgress::default(),
        Some(&web),
    )
    .await?;
    assert_eq!(program.summary.id, saved.summary.id);
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    assert!(llm.prompts.lock().unwrap()[0].contains(ADDITIONAL));
    assert_eq!(web.queries.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn typography_repairs_use_saved_text_before_research_and_skip_model_rewrites() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let exact = "The cell’s response changes when experimental conditions differ.";
    let mut source = original_source();
    source.excerpt = exact.into();
    let mut initial = candidate();
    cite(&mut initial, 0, exact);
    for path in [
        "/modules/0/summary",
        "/modules/0/outcomes/0",
        "/modules/0/lessons/0",
    ] {
        initial.pointer_mut(path).unwrap()["quote"] = json!(exact.replace('’', "'"));
    }
    let llm = model(vec![initial, json!({"issues":[]})]);
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let program = generate_online(&repo, &llm, &web, vec![source]).await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    assert_eq!(program.outline_review.as_ref().unwrap().repair_passes, 0);
    assert_eq!(llm.prompts.lock().unwrap().len(), 2);
    assert!(web.queries.lock().unwrap().is_empty());
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(saved.candidate["modules"][0]["summary"]["quote"], exact);
    assert!(crate::features::learning::outline_repair::quote_issues(
        &saved.candidate,
        &repo.verification_sources(&program.summary.id).await?
    )
    .is_empty());
    Ok(())
}

#[tokio::test]
async fn resumed_typography_repair_is_checkpointed_and_still_requires_semantic_review() -> Result<()>
{
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let exact = "The cell’s response changes when experimental conditions differ.";
    let mut source = original_source();
    source.excerpt = exact.into();
    let mut initial = candidate();
    cite(&mut initial, 0, exact);
    let author = model(vec![initial.clone(), json!({"issues":[]})]);
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let mut program = generate_online(&repo, &author, &web, vec![source.clone()]).await?;
    let mut saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    saved.candidate["modules"][0]["summary"]["quote"] = json!(exact.replace('’', "'"));
    saved.review.status = LearningOutlineReviewStatus::NeedsRepair;
    saved.review.issues = crate::features::learning::outline_repair::quote_issues(
        &saved.candidate,
        std::slice::from_ref(&source),
    );
    repo.checkpoint_outline(&mut program, &mut saved, &[], &[])
        .await?;
    let checker = model(vec![issue(0), json!({"module":initial["modules"][0]})]);
    let result = crate::features::learning::outline_draft::repair_saved(
        &repo,
        &checker,
        &crate::features::learning::outline_draft::RepairLearningOutlineRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
        },
        &OutlineProgress::default(),
        None,
    )
    .await?;
    let review = result.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::NeedsRepair);
    assert_eq!(review.issues.len(), 1);
    assert_eq!(
        review.issues[0].kind,
        crate::features::learning::outline_draft::LearningOutlineIssueKind::Content
    );
    assert_eq!(checker.prompts.lock().unwrap().len(), 2);
    assert_eq!(
        repo.outline_draft(&program.summary.id)
            .await?
            .unwrap()
            .candidate["modules"][0]["summary"]["quote"],
        exact
    );
    assert!(service::accept(
        &repo,
        AcceptLearningProgramRequestDto {
            program_id: program.summary.id,
            expected_revision: result.summary.revision,
            title: result.summary.title
        }
    )
    .await
    .is_err());
    Ok(())
}

fn review_payload(prompt: &str) -> Value {
    serde_json::from_str(prompt.rsplit("\n\n").next().unwrap()).unwrap()
}

#[tokio::test]
async fn single_module_recheck_reuses_completed_checks_and_keeps_whole_course_connections(
) -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    let mut source = original_source();
    let quote = ORIGINAL.repeat(12);
    source.excerpt = quote.clone();
    cite(&mut initial, 0, &quote);
    let mut fixed = initial["modules"][1].clone();
    fixed["project"]["brief"] = json!("Extend the Experimental design module 0 project with a measurement protocol and observable outcomes.");
    let llm = model(vec![
        initial,
        issue(1),
        json!({"module":fixed}),
        json!({"issues":[]}),
    ]);
    let sources = vec![source];
    let program = service::generate_with_references(
        &repo,
        &llm,
        request(LearningCourseDepth::Course),
        sources.clone(),
        &references(&sources),
    )
    .await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 4);
    let full = review_payload(&prompts[1]);
    let recheck = review_payload(&prompts[3]);
    assert_eq!(full["reviewScope"]["mode"], "full");
    assert_eq!(recheck["reviewScope"]["mode"], "changed_modules");
    assert_eq!(recheck["reviewScope"]["moduleIndices"], json!([1]));
    assert!(recheck.get("candidate").is_none());
    assert_eq!(recheck["modulesToReview"].as_array().unwrap().len(), 1);
    assert_eq!(recheck["modulesToReview"][0]["path"], "/modules/1");
    assert_eq!(recheck["modulesToReview"][0]["moduleNumber"], 2);
    assert_eq!(
        recheck["modulesToReview"][0]["module"]["summary"]["quote"],
        quote
    );
    assert_eq!(recheck["courseSequence"].as_array().unwrap().len(), 4);
    assert_eq!(
        recheck["courseSequence"][3]["prerequisiteIndices"],
        json!([2])
    );
    assert!(recheck["courseSequence"][3]["lessons"][0]["objective"]
        .as_str()
        .unwrap()
        .contains("limitations"));
    assert!(recheck["courseSequence"][3]["summary"]
        .get("quote")
        .is_none());
    assert!(prompts[3].len() * 100 < prompts[1].len() * 70, "A one-module citation-heavy repair should remove at least 30% of the review input: {} -> {} bytes", prompts[1].len(), prompts[3].len());
    println!(
        "Incremental outline review fixture: {} -> {} prompt bytes",
        prompts[1].len(),
        prompts[3].len()
    );
    Ok(())
}

#[tokio::test]
async fn review_receipts_invalidate_on_evidence_model_request_structure_or_protocol_changes(
) -> Result<()> {
    use crate::features::learning::{
        outline_draft::OutlineDraft,
        outline_review_scope::{ReviewReceipt, ReviewScope},
    };
    let llm = model(vec![]);
    let mut initial = candidate();
    let sources = vec![original_source()];
    cite(&mut initial, 0, ORIGINAL);
    let mut draft = OutlineDraft::new(request(LearningCourseDepth::Course), initial, &sources);
    assert!(!ReviewScope::new(&draft, &sources, &llm).incremental);
    draft.completed_review = Some(ReviewReceipt::new(&draft, &sources, &llm));
    draft.candidate["modules"][1]["summary"]["text"] =
        json!("A corrected explanation of the prerequisite.");
    assert!(ReviewScope::new(&draft, &sources, &llm).incremental);
    let roundtrip: OutlineDraft = serde_json::from_str(&serde_json::to_string(&draft)?)?;
    assert!(ReviewScope::new(&roundtrip, &sources, &llm).incremental);
    for variant in 0..8 {
        let mut changed = draft.clone();
        let mut evidence = sources.clone();
        match variant {
            0 => evidence[0]
                .excerpt
                .push_str(" A correction to the reference."),
            1 => evidence[0].id = "different-version".into(),
            2 => changed
                .request
                .prior_knowledge
                .push_str(" No algebra knowledge."),
            3 => changed.request.minutes_per_session = 15,
            4 => changed.candidate["modules"]
                .as_array_mut()
                .unwrap()
                .swap(0, 2),
            5 => changed.candidate["modules"][2]["lessons"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            6 => {
                let mut state = serde_json::to_value(&changed)?;
                state["completed_review"]["context_hash"] = json!("an-earlier-review-protocol");
                changed = serde_json::from_value(state)?;
            }
            _ => evidence.push(original_source()),
        }
        assert!(
            !ReviewScope::new(&changed, &evidence, &llm).incremental,
            "variant {variant} must invalidate prior checks"
        );
    }
    assert!(
        !ReviewScope::new(&draft, &sources, &outline(4, 3)).incremental,
        "Changing the reviewer must invalidate previous checks"
    );
    draft.review.issues.push(
        crate::features::learning::outline_draft::LearningOutlineIssueDto {
            path: String::new(),
            kind: crate::features::learning::outline_draft::LearningOutlineIssueKind::Content,
            claim: "Course-wide contradiction".into(),
            quote: String::new(),
            source_id: None,
            message: "Revisit the whole course sequence.".into(),
        },
    );
    assert!(!ReviewScope::new(&draft, &sources, &llm).incremental);
    Ok(())
}

#[tokio::test]
async fn interrupted_incremental_review_resumes_from_last_completed_receipt_without_approval(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut initial = candidate();
    let sources = vec![original_source()];
    cite(&mut initial, 0, ORIGINAL);
    let mut fixed = initial["modules"][1].clone();
    fixed["summary"]["text"] =
        json!("Use the earlier experiment protocol to compare observable outcomes.");
    let llm = model(vec![
        initial,
        issue(1),
        json!({"module":fixed}),
        json!({"invalid":"unfinished review"}),
    ]);
    let saved = service::generate_with_references(
        &repo,
        &llm,
        request(LearningCourseDepth::Course),
        sources.clone(),
        &references(&sources),
    )
    .await?;
    assert_eq!(
        saved.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Unchecked
    );
    assert!(service::accept(
        &repo,
        AcceptLearningProgramRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
            title: saved.summary.title.clone(),
        }
    )
    .await
    .is_err());
    let reopened = LearningRepository::new(pool);
    let llm = model(vec![json!({"issues":[]})]);
    let result = crate::features::learning::outline_draft::repair_saved(
        &reopened,
        &llm,
        &crate::features::learning::outline_draft::RepairLearningOutlineRequestDto {
            program_id: saved.summary.id,
            expected_revision: saved.summary.revision,
        },
        &OutlineProgress::default(),
        None,
    )
    .await?;
    assert_eq!(
        result.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(
        review_payload(&prompts[0])["reviewScope"]["moduleIndices"],
        json!([1])
    );
    assert_eq!(
        review_payload(&prompts[0])["reviewScope"]["mode"],
        "changed_modules"
    );
    Ok(())
}

#[tokio::test]
async fn incremental_review_can_flag_an_unchanged_dependent_module() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    let sources = vec![original_source()];
    cite(&mut initial, 0, ORIGINAL);
    let mut fixed = initial["modules"][1].clone();
    fixed["project"]["brief"] =
        json!("Revise the experiment to use an observational measurement protocol.");
    let llm = model(vec![
        initial,
        issue(1),
        json!({"module":fixed}),
        json!({"issues":[{
            "path":"/modules/3", "claim":"Interpret results and explain uncertainty.",
            "reason":"The changed earlier project no longer supplies the randomized comparison needed by the final project. Correct that dependency."
        }]}),
    ]);
    let program = service::generate_with_references(
        &repo,
        &llm,
        request(LearningCourseDepth::Course),
        sources.clone(),
        &references(&sources),
    )
    .await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::NeedsRepair
    );
    assert_eq!(
        program.outline_review.as_ref().unwrap().issues[0].path,
        "/modules/3"
    );
    assert_eq!(llm.prompts.lock().unwrap().len(), 4);
    Ok(())
}
