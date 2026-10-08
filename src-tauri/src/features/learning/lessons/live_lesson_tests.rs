//! Opt-in full production-worker exercise against an explicitly chosen library.
//! This retries and publishes the chosen job through repositories, so callers
//! must deliberately select the database. No model responses are mocked.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use crate::features::llm::llama_cpp::LlamaCppLlm;

#[path = "live_small_lesson.rs"]
mod small;

struct ObservedModel {
    inner: LlamaCppLlm,
    directory: std::path::PathBuf,
    call: std::sync::atomic::AtomicUsize,
}

#[tokio::test]
#[ignore = "compares serial and batched verification with a live model; never writes to the library"]
async fn live_batched_verification_matches_individual_checks_and_measures_cost() -> Result<()> {
    use crate::application::services::claim_verification::{
        ClaimJudgment, ClaimVerdict, LocatedClaim,
    };
    use crate::features::learning::content_verification::{
        judge_batch_for_publication, judge_for_publication,
    };
    use std::sync::atomic::Ordering;
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let mut cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "content_verification/claim_prerequisite_cases.json"
    ))?;
    cases.extend(serde_json::from_str::<Vec<serde_json::Value>>(
        include_str!("content_verification/evidence_challenge_cases.json"),
    )?);
    // Evidence present for a different claim must not fill this claim's gap.
    cases.insert(0,serde_json::json!({"claim":"The measured value of sample A is 12.","passages":["Sample A was measured."],"expected":"unsupported"}));
    cases.insert(1,serde_json::json!({"claim":"The measured value of sample A is 12.","passages":["The measured value of sample A is 12."],"expected":"supported"}));
    let claims: Vec<_> = cases
        .iter()
        .map(|case| LocatedClaim {
            claim: case["claim"].as_str().unwrap().into(),
            passages: case["passages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| p.as_str().unwrap().to_owned())
                .collect(),
        })
        .collect();
    fn label(result: &ClaimJudgment) -> &'static str {
        match result {
            ClaimJudgment::Judged(finding) => match finding.verdict {
                ClaimVerdict::Supported => "supported",
                ClaimVerdict::Unsupported => "unsupported",
                ClaimVerdict::Contradicted => "contradicted",
                ClaimVerdict::Unverified => "unverified",
            },
            _ => "unusable",
        }
    }
    let mut runs = Vec::new();
    let modes = match std::env::var("LATTICE_BATCH_BENCH_MODE").as_deref() {
        Ok("batch") => vec![true],
        Ok("serial") => vec![false],
        _ => vec![false, true],
    };
    for batched in modes {
        let started = std::time::Instant::now();
        let before = model.call.load(Ordering::SeqCst);
        let mut outcomes = Vec::new();
        if batched {
            for batch in claims.chunks(8) {
                outcomes.extend(judge_batch_for_publication(&model, batch).await?);
            }
        } else {
            for claim in &claims {
                outcomes.push(judge_for_publication(&model, &claim.claim, &claim.passages).await);
            }
        }
        let failures: Vec<_> = cases
            .iter()
            .zip(&outcomes)
            .enumerate()
            .filter_map(|(index, (case, result))| {
                let actual = label(result);
                let expected = case["expected"].as_str().unwrap();
                let valid = if expected == "not_supported" {
                    matches!(actual, "unsupported" | "contradicted")
                } else {
                    actual == expected
                };
                (!valid).then_some(
                    serde_json::json!({"case":index,"expected":expected,"actual":actual}),
                )
            })
            .collect();
        let result = serde_json::json!({"mode":if batched {"batch"}else{"serial"},"seconds":started.elapsed().as_secs_f64(),"calls":model.call.load(Ordering::SeqCst)-before,"outcomes":outcomes.iter().map(label).collect::<Vec<_>>(),"failures":failures});
        println!("Verification benchmark: {result}");
        runs.push(result);
        std::fs::write(
            directory.join("benchmark.json"),
            serde_json::to_vec_pretty(&runs)?,
        )?;
    }
    assert!(
        runs.iter()
            .all(|run| run["failures"].as_array().unwrap().is_empty()),
        "A verification benchmark case failed; see benchmark.json"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires live settings and an artifact directory; makes real model requests"]
async fn live_coverage_mapping_recovers_existing_claims_without_inventing_missing_ones(
) -> Result<()> {
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let mut cases = Vec::new();
    for (text, selected, omitted, unrelated) in [
        (
            "DNA stores genetic information. RNA uses uracil.",
            "DNA stores genetic information.",
            "RNA uses uracil.",
            "DNA contains nucleotides.",
        ),
        (
            "Yeast produces carbon dioxide, which can leaven dough.",
            "Yeast is a microorganism.",
            "Yeast produces carbon dioxide, which can leaven dough.",
            "Flour is an ingredient in this dough.",
        ),
    ] {
        for complete in [true, false] {
            cases.push((serde_json::json!({"units":[{"index":0,"kind":"teaching",
                "passages":[{"id":"target","field":"/body","text":text}],
                "claims":[{"id":"selected","statement":selected},{"id":"unselected","statement":if complete { omitted } else { unrelated }}]}],
                "mapping":{"target":{"claimIds":["selected"],"nonFactualReason":"","missingClaims":[]}}}), complete));
        }
    }
    // An optional recorded teaching passage reproduces a real mapping omission.
    // Its expected result is supplied only to this test, never to the model.
    if let Ok(path) = std::env::var("LATTICE_MAPPING_FIXTURE") {
        let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let expected = recorded["expectedComplete"].as_bool().unwrap();
        cases.push((recorded["fixture"].clone(), expected));
    }
    for (index, (fixture, expected)) in cases.into_iter().enumerate() {
        let result =
            crate::features::learning::content_verification::live_mapping_fixture(&model, &fixture)
                .await?;
        std::fs::write(
            directory.join(format!("mapping-{index}.json")),
            serde_json::to_vec_pretty(&result)?,
        )?;
        assert_eq!(
            result[0]["complete"].as_bool(),
            Some(expected),
            "Mapping fixture {index}"
        );
        println!("Mapping fixture {index} passed (complete={expected})");
    }
    Ok(())
}

impl ObservedModel {
    fn record(
        &self,
        call: usize,
        request: &CompletionRequest,
        response: &CompletionResponse,
    ) -> Result<()> {
        std::fs::write(
            self.directory.join(format!("call-{call}.json")),
            serde_json::to_vec_pretty(&serde_json::json!({
                "input":request.input,"answer":response.text,"finishReason":response.finish_reason,
                "inputTokens":response.input_tokens,"outputTokens":response.output_tokens,
                "firstTokenLogprobs":response.first_token_logprobs
            }))?,
        )?;
        println!(
            "Live call {call} completed: {} input / {} output tokens, {}",
            response.input_tokens, response.output_tokens, response.finish_reason
        );
        Ok(())
    }
}
#[async_trait::async_trait]
impl LLMPort for ObservedModel {
    fn supports_typed_completions(&self) -> bool {
        true
    }
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let call = self.call.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let response = self.inner.complete(request).await?;
        self.record(call, request, &response)?;
        Ok(response)
    }
    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let call = self.call.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let response = self.inner.complete_with_progress(request, on_text).await?;
        self.record(call, request, &response)?;
        Ok(response)
    }
    async fn generate(&self, _: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
        unreachable!()
    }
    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let call = self.call.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let response = self
            .inner
            .complete_with_retry_progress(request, on_text, on_retry)
            .await?;
        self.record(call, request, &response)?;
        Ok(response)
    }
    async fn generate_streaming(
        &self,
        _: &str,
        _: &[String],
        _: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        unreachable!()
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }
}

#[tokio::test]
#[ignore = "requires explicit live settings, evidence cases and artifact directory"]
async fn live_source_passage_judgments() -> Result<()> {
    use crate::application::{
        ports::llm_port::SamplingOverride,
        services::claim_verification::{CheckPolicy, ClaimChecker, ClaimJudgment},
    };
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_CLAIM_CASES").unwrap(),
    )?)?;
    let policy = match std::env::var("LATTICE_CLAIM_POLICY").as_deref() {
        Ok("fidelity") => CheckPolicy::Fidelity,
        Ok("strict" | "publication") | Err(_) => CheckPolicy::Strict,
        Ok(other) => panic!("Unknown claim-check policy: {other}"),
    };
    let checker = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, policy);
    // Optional real-library retrieval exercises the same evidence windows used
    // by publication. Fixture verdicts still never enter a model request.
    let retrieval = if let Ok(program) = std::env::var("LATTICE_CLAIM_PROGRAM") {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(std::env::var("LATTICE_LESSON_DATABASE").unwrap())
                    .foreign_keys(true)
                    .create_if_missing(false),
            )
            .await
            .map_err(|error| AppError::Database(error.to_string()))?;
        let models =
            crate::infrastructure::persistence::repositories::DownloadedModelRepository::new(
                pool.clone(),
            );
        let model = models
            .get_active_embedding_model()
            .await?
            .expect("active embedding model");
        let location = model.location().enclosing_dir().unwrap();
        let identity = model.embedding_artifact_identity().cloned().unwrap();
        let embedding = tokio::task::spawn_blocking(move || {
            crate::features::embedding::candle_service::CandleEmbeddingService::open(
                location, identity,
            )
        })
        .await
        .unwrap()?;
        Some((pool, program, embedding))
    } else {
        None
    };
    let references = if let Some((pool, program, embedding)) = &retrieval {
        let sources = LearningRepository::new(pool.clone())
            .verification_sources(program)
            .await?;
        Some(
            crate::features::learning::reference_collection::ReferenceCollection::load(
                pool,
                program,
                &sources,
                Some(embedding),
            )
            .await?,
        )
    } else {
        None
    };
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for case in cases {
        let passages: Vec<String> = if let Some(references) = &references {
            crate::features::learning::content_verification::live_retrieved_passages(
                case["claim"].as_str().unwrap(),
                references,
            )
            .await?
        } else {
            serde_json::from_value(case["passages"].clone())?
        };
        let judgment = if std::env::var("LATTICE_CLAIM_POLICY").as_deref() == Ok("publication") {
            crate::features::learning::content_verification::judge_for_publication(
                &model,
                case["claim"].as_str().unwrap(),
                &passages,
            )
            .await
        } else if let Some(context) = case.get("context") {
            if case["contextKind"] == "teaching" {
                checker
                    .check_fidelity_in_teaching_context(
                        case["claim"].as_str().unwrap(),
                        &passages,
                        &context.to_string(),
                    )
                    .await
            } else {
                checker
                    .check_fidelity_in_context(
                        case["claim"].as_str().unwrap(),
                        &passages,
                        &context.to_string(),
                    )
                    .await
            }
        } else {
            checker
                .check_passages_without_deadline(case["claim"].as_str().unwrap(), &passages)
                .await
        };
        let ClaimJudgment::Judged(outcome) = judgment else {
            panic!("No valid evidence verdict: {judgment:?}")
        };
        let verdict = serde_json::to_value(outcome.verdict)?;
        results.push(serde_json::json!({"claim":case["claim"],"verdict":verdict,"reason":outcome.reason,"quote":outcome.quote,"confidence":outcome.confidence}));
        std::fs::write(
            directory.join("judgments.json"),
            serde_json::to_vec_pretty(&results)?,
        )?;
        let matches = if case["expected"] == "not_supported" {
            matches!(
                outcome.verdict,
                crate::application::services::claim_verification::ClaimVerdict::Unsupported
                    | crate::application::services::claim_verification::ClaimVerdict::Contradicted
            )
        } else {
            verdict == case["expected"]
        };
        if !matches {
            failures.push(format!(
                "{}: expected {}, received {}",
                case["claim"], case["expected"], verdict
            ));
        }
        if let Some(quote) = outcome.quote {
            assert!(passages.iter().any(|p| p == &quote));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicit live settings and artifact directory; uses committed source-selection controls by default"]
async fn live_reference_selection() -> Result<()> {
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let cases: Vec<serde_json::Value> = match std::env::var("LATTICE_REFERENCE_CASES") {
        Ok(path) => serde_json::from_slice(&std::fs::read(path)?)?,
        Err(_) => {
            serde_json::from_str(include_str!("live_fixtures/reference_scope_controls.json"))?
        }
    };
    for (index, case) in cases.into_iter().enumerate() {
        let results = serde_json::from_value(case["results"].clone())?;
        let chosen = crate::features::learning::content_verification::research::select_references(
            &model,
            case["topic"].as_str().unwrap(),
            case["query"].as_str().unwrap(),
            case["gaps"].as_array().map(Vec::as_slice).unwrap_or(&[]),
            results,
        )
        .await?;
        let mut urls: Vec<_> = chosen.into_iter().map(|result| result.url).collect();
        std::fs::write(
            directory.join(format!("selection-{index}.json")),
            serde_json::to_vec_pretty(&urls)?,
        )?;
        urls.sort();
        let mut expected: Vec<String> = serde_json::from_value(case["expectedUrls"].clone())?;
        expected.sort();
        assert_eq!(urls, expected);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicit live settings, a recorded teaching review and artifact directory"]
async fn live_teaching_review_evidence_scope() -> Result<()> {
    use crate::application::ports::llm_port::CompletionInput;
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_TEACHING_REVIEW_CALL").unwrap(),
    )?)?;
    let input: Vec<CompletionInput> = serde_json::from_value(recorded["input"].clone())?;
    let message = |role: &str| {
        input
            .iter()
            .find_map(|item| match item {
                CompletionInput::Message {
                    role: found,
                    content,
                } if found == role => Some(content.clone()),
                _ => None,
            })
            .unwrap()
    };
    let system = format!(
        "{}\n{}",
        message("system"),
        crate::features::learning::teaching::LESSON_REVIEW_EVIDENCE_SCOPE
    );
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    let prompt: serde_json::Value = serde_json::from_str(&message("user"))?;
    let mut candidate = prompt["candidate"].clone();
    for block in candidate["blocks"].as_array_mut().unwrap() {
        let body: String = block["bodyPassages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|passage| passage["text"].as_str().unwrap())
            .collect();
        block.as_object_mut().unwrap().remove("bodyPassages");
        block.as_object_mut().unwrap().remove("index");
        block["body"] = serde_json::json!(body);
    }
    let issues = crate::features::learning::teaching_review::review(
        &model,
        &system,
        prompt,
        &candidate,
        Some(&progress),
    )
    .await?;
    std::fs::write(
        directory.join("replay-issues.json"),
        serde_json::to_vec_pretty(&issues)?,
    )?;
    // Other concrete defects (for example an attached citation mismatch) must
    // remain reportable. This replay only targets the original false objection
    // that RUSTUP_AUTO_INSTALL is absent from the authoring excerpts.
    let stale_excerpt_objection = issues.iter().any(|issue| {
        let issue = issue.to_lowercase();
        issue.contains("rustup_auto_install")
            && [
                "not present",
                "not provided",
                "not described",
                "absent",
                "no source",
                "unsupported",
            ]
            .iter()
            .any(|phrase| issue.contains(phrase))
    });

    // Evidence scope must not excuse a demonstrably wrong worked result.
    let wrong = serde_json::json!({"blocks":[{"kind":"explanation","title":"Adding whole numbers","body":"To add two whole numbers, combine their quantities. Worked example: start with two apples and add two more apples. The total is five apples, so 2 + 2 = 5. This is ordinary integer addition, with no rounding or special convention.","rubric":[]}],"questions":[]});
    let issues = crate::features::learning::teaching_review::review(&model, &system, serde_json::json!({"task":"Review instructional quality","authoringContext":{"goal":"Learn ordinary whole-number addition","sources":[]}}), &wrong, Some(&progress)).await?;
    std::fs::write(
        directory.join("incorrect-example-issues.json"),
        serde_json::to_vec_pretty(&issues)?,
    )?;
    assert!(
        !issues.is_empty(),
        "The evidence-scope clarification excused a concrete arithmetic error"
    );
    assert!(
        !stale_excerpt_objection,
        "The reviewer again treated partial authoring excerpts as the entire evidence collection"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires live settings, a recorded coverage audit and artifact directory"]
async fn live_passage_coverage_preserves_complete_assertions() -> Result<()> {
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_COVERAGE_CALL").unwrap(),
    )?)?;
    let input = recorded["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["role"] == "user")
        .unwrap()["content"]
        .as_str()
        .unwrap();
    let input: serde_json::Value = serde_json::from_str(input)?;
    let indices: Vec<usize> =
        serde_json::from_str(&std::env::var("LATTICE_COVERAGE_UNITS").unwrap())?;
    let units: Vec<_> = indices.iter().map(|index| {
        let claims = input["inventory"].as_array().unwrap().iter().find(|unit| unit["index"].as_u64()==Some(*index as u64)).unwrap();
        serde_json::json!({"content":input["units"][*index]["content"],"statements":claims["statements"],"nonFactualReason":claims["nonFactualReason"]})
    }).collect();
    let required: serde_json::Value = serde_json::from_str(
        &std::env::var("LATTICE_COVERAGE_REQUIRED_MISSING").unwrap_or_else(|_| "{}".into()),
    )?;
    let mut cases = vec![
        serde_json::json!({"units":units,"expectedIncomplete":(0..indices.len()).collect::<Vec<_>>(),"requiredMissingTerms":required}),
    ];
    cases.extend(serde_json::from_str::<Vec<serde_json::Value>>(
        include_str!("content_verification/coverage_scope_cases.json"),
    )?);
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    for (index, case) in cases.into_iter().enumerate() {
        let report = crate::features::learning::content_verification::live_coverage_fixture(
            &model, &case, &progress,
        )
        .await?;
        std::fs::write(
            directory.join(format!("coverage-{index}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        let incomplete: Vec<_> = report["units"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|unit| unit["complete"] == false)
            .map(|unit| unit["index"].clone())
            .collect();
        assert_eq!(
            serde_json::json!(incomplete),
            case["expectedIncomplete"],
            "Coverage case {index}"
        );
        if let Some(required) = case["requiredMissingTerms"].as_object() {
            for (unit, terms) in required {
                let unit: usize = unit.parse().unwrap();
                let reason = report["units"][unit]["reason"]
                    .as_str()
                    .unwrap()
                    .to_lowercase();
                for term in terms.as_array().unwrap() {
                    assert!(reason.contains(&term.as_str().unwrap().to_lowercase()), "Coverage case {index}, unit {unit} missed the specific regression: {reason}");
                }
            }
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires live settings, saved draft, review call, database and artifact directory"]
async fn live_review_findings_require_evidence() -> Result<()> {
    use crate::features::learning::{reference_collection::ReferenceCollection, review_evidence};
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let draft: crate::features::learning::lesson_drafts::Draft = serde_json::from_slice(
        &std::fs::read(std::env::var("LATTICE_REVIEW_DRAFT").unwrap())?,
    )?;
    let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_TEACHING_REVIEW_CALL").unwrap(),
    )?)?;
    let review: serde_json::Value = serde_json::from_str(recorded["answer"].as_str().unwrap())?;
    let issues: Vec<String> = serde_json::from_value(review["issues"].clone())?;
    let authoring = serde_json::from_str(&draft.authoring.unwrap().prompt)?;
    let candidate = serde_json::from_str(&draft.candidate)?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(std::env::var("LATTICE_LESSON_DATABASE").unwrap())
        .foreign_keys(true)
        .create_if_missing(false);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let models = crate::infrastructure::persistence::repositories::DownloadedModelRepository::new(
        pool.clone(),
    );
    let embedding_model = models.get_active_embedding_model().await?.unwrap();
    let location = embedding_model.location().enclosing_dir().unwrap();
    let identity = embedding_model
        .embedding_artifact_identity()
        .cloned()
        .unwrap();
    let embedding = tokio::task::spawn_blocking(move || {
        crate::features::embedding::candle_service::CandleEmbeddingService::open(location, identity)
    })
    .await
    .unwrap()?;
    let program = std::env::var("LATTICE_REVIEW_PROGRAM").unwrap();
    let sources = LearningRepository::new(pool.clone())
        .verification_sources(&program)
        .await?;
    let references = ReferenceCollection::load(&pool, &program, &sources, Some(&embedding)).await?;
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    let accepted = review_evidence::check(
        &model,
        &issues,
        &authoring,
        &candidate,
        &references,
        Some(&progress),
    )
    .await?;
    std::fs::write(
        directory.join("accepted-replay-issues.json"),
        serde_json::to_vec_pretty(&accepted)?,
    )?;
    assert!(
        accepted.is_empty(),
        "False reviewer assertions triggered editing: {accepted:?}"
    );
    // The expected findings live only in test data and are never sent to the model.
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("content_verification/review_scope_cases.json"))?;
    for (index, case) in cases.into_iter().enumerate() {
        let sources = serde_json::from_value::<
            Vec<crate::features::learning::dto::LearningSourceDto>,
        >(case["sources"].clone())?;
        let references = ReferenceCollection::lexical(&sources)?;
        let issues: Vec<String> = serde_json::from_value(case["issues"].clone())?;
        let accepted = review_evidence::check(
            &model,
            &issues,
            &case["requirements"],
            &case["candidate"],
            &references,
            Some(&progress),
        )
        .await?;
        std::fs::write(
            directory.join(format!("accepted-control-{index}.json")),
            serde_json::to_vec_pretty(&accepted)?,
        )?;
        let expected: Vec<String> = serde_json::from_value(case["expected"].clone())?;
        assert_eq!(accepted, expected);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "enqueues a retry in LATTICE_LESSON_DATABASE; stop its native worker before running"]
async fn live_enqueue_saved_lesson_for_native_resume() -> Result<()> {
    use crate::features::learning::{
        curriculum::LearningGenerationJobStatus as Status,
        plan_dto::LearningGenerationJobActionRequestDto,
    };
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(std::env::var("LATTICE_LESSON_DATABASE").unwrap())
        .foreign_keys(true)
        .create_if_missing(false);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let old = repo
        .job(&std::env::var("LATTICE_LESSON_JOB").unwrap())
        .await?;
    assert!(matches!(
        old.status,
        Status::Failed | Status::Interrupted | Status::Cancelled
    ));
    assert!(
        !repo
            .jobs(&old.program_id)
            .await?
            .iter()
            .any(|job| matches!(job.status, Status::Pending | Status::Running)),
        "Another job is active"
    );
    let program = LearningRepository::new(pool.clone())
        .get(&old.program_id)
        .await?;
    let job = repo
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: old.program_id,
            job_id: old.id,
            expected_revision: program.summary.revision,
        })
        .await?;
    assert_eq!(job.status, Status::Pending);
    println!("Native lesson retry queued: {}", job.id);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "writes to LATTICE_LESSON_DATABASE; requires settings, failed job and artifact directory"]
async fn live_retry_saved_lesson_through_publication() -> Result<()> {
    use crate::features::learning::{
        curriculum::LearningGenerationJobStatus, dto::LearningSourcePolicy,
        plan_dto::LearningGenerationJobActionRequestDto,
    };
    let _ = tracing_subscriber::fmt().with_env_filter("lattice::features::learning=info,lattice::application::services::claim_verification=warn").try_init();
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let llm: Arc<dyn LLMPort> = Arc::new(ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: std::sync::atomic::AtomicUsize::new(0),
    });
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(std::env::var("LATTICE_LESSON_DATABASE").unwrap())
        .foreign_keys(true)
        .create_if_missing(false);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let old = repo
        .job(&std::env::var("LATTICE_LESSON_JOB").unwrap())
        .await?;
    // Explicitly recover only the selected orphan after a crashed test process.
    // Never run this option while its original worker is still alive.
    if std::env::var("LATTICE_LESSON_RECOVER_ORPHAN").as_deref() == Ok("1") {
        assert_eq!(old.status, LearningGenerationJobStatus::Running);
        repo.interrupt_job(&old.id).await?;
    }
    let program = LearningRepository::new(pool.clone())
        .get(&old.program_id)
        .await?;
    assert!(
        !repo.jobs(&old.program_id).await?.iter().any(|j| matches!(
            j.status,
            LearningGenerationJobStatus::Pending | LearningGenerationJobStatus::Running
        )),
        "Another job is active"
    );
    // Explicit cleanup of captures produced by a failed research run. Keep the
    // immutable snapshots/tombstones and use the same repository operation as
    // the source UI; never mutate job approval or lesson content here.
    if let Ok(path) = std::env::var("LATTICE_REMOVE_RESEARCH_SOURCES") {
        let removals: Vec<
            crate::features::learning::portability_dto::DeleteLearningSourceRequestDto,
        > = serde_json::from_slice(&std::fs::read(path)?)?;
        let library =
            crate::features::learning::source_library::LearningSourceLibraryRepository::new(
                pool.clone(),
            );
        for removal in removals {
            assert_eq!(removal.program_id, old.program_id);
            library.delete_source(&removal).await?;
            println!("Removed irrelevant research capture {}", removal.source_id);
        }
    }
    let models = crate::infrastructure::persistence::repositories::DownloadedModelRepository::new(
        pool.clone(),
    );
    let model = models
        .get_active_embedding_model()
        .await?
        .expect("active embedding model");
    let model_dir = model.location().enclosing_dir().unwrap();
    let identity = model.embedding_artifact_identity().cloned().unwrap();
    let embedding: Arc<dyn crate::application::ports::EmbeddingPort> = Arc::new(
        tokio::task::spawn_blocking(move || {
            crate::features::embedding::candle_service::CandleEmbeddingService::open(
                model_dir, identity,
            )
        })
        .await
        .unwrap()?,
    );
    let source_pool = pool.clone();
    let web_dir = tempfile::tempdir()?;
    let worker = LessonGenerationWorker {
        research_web: Some(Arc::new(
            crate::features::web::services::web::WebService::new(web_dir.path())?,
        )),
        pool: pool.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(llm) })
        }),
        load_embedding: Arc::new(move || {
            let embedding = embedding.clone();
            Box::pin(async move { Some(embedding) })
        }),
        refresh_sources: Arc::new(move |program_id| {
            let pool = source_pool.clone();
            Box::pin(async move {
                let workspace = crate::features::learning::source_library::LearningSourceLibraryRepository::new(pool).workspace(&program_id).await?;
                assert!(
                    workspace
                        .sources
                        .iter()
                        .all(|s| s.freshness_policy != LearningSourcePolicy::BeforeUse),
                    "This runner requires pinned sources; use native app for before-use refresh"
                );
                Ok(())
            })
        }),
    };
    let job = repo
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: old.program_id,
            job_id: old.id,
            expected_revision: program.summary.revision,
        })
        .await?;
    std::fs::write(directory.join("job-id.txt"), &job.id)?;
    println!("Live lesson job {} started", job.id);
    worker.run(&job.id, CancellationToken::new()).await;
    let result = repo.job(&job.id).await?;
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    assert_eq!(
        result.status,
        LearningGenerationJobStatus::Completed,
        "Full live worker did not publish: {:?}",
        result
    );
    let published = LearningRepository::new(pool).get(&job.program_id).await?;
    std::fs::write(
        directory.join("program.json"),
        serde_json::to_vec_pretty(&published)?,
    )?;
    println!(
        "Published verified lesson through the production worker: {}",
        job.id
    );
    Ok(())
}
