//! Scores the shipped strict claim judge on labelled claim/passage pairs.
//! Each pair becomes one claim whose only evidence is its passage, and pairs
//! are sent in batches the way lesson verification sends them: a batch that
//! does not fit is halved, and a claim the batch leaves unusable is rechecked
//! on its own. This is the whole model check a lesson claim gets; whether its
//! support is enough is the provenance gate's decision, which makes no call.
//! A pair's evidence is one `passage`, or a page of `passages` each with the
//! source label the lesson check puts above it (`requests::labelled`).
use super::*;

#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_CLAIM_PAIRS and LATTICE_CLAIM_RESULTS; makes real model requests"]
async fn live_claim_pairs_eval() -> Result<()> {
    use lattice_core::application::ports::llm_port::SamplingOverride;
    use lattice_core::application::services::claim_verification::{
        CheckPolicy, ClaimChecker, ClaimJudgment, ClaimVerdict, LocatedClaim, PromptVariant,
        VerdictForm,
    };
    use std::io::Write as _;

    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let mut config: lattice_core::application::contracts::settings::LLMSettingsDto =
        serde_json::from_value(settings["settings"]["llm"].clone())?;
    // `LATTICE_CLAIM_MODEL=anthropic/claude-haiku-5-5` judges on that cloud
    // model, as a lesson whose Learning model it is does.
    let model: std::sync::Arc<dyn LLMPort> = match std::env::var("LATTICE_CLAIM_MODEL") {
        Ok(spec) => {
            use lattice_core::application::contracts::settings::LLMProvider;
            let (name, model) = spec.split_once('/').ok_or_else(|| {
                AppError::InvalidConfig(format!("LATTICE_CLAIM_MODEL {spec} is not provider/model"))
            })?;
            config.provider = match name {
                "anthropic" => LLMProvider::Anthropic,
                "openai" => LLMProvider::Openai,
                "google" => LLMProvider::Google,
                other => {
                    return Err(AppError::InvalidConfig(format!("no cloud provider {other}")))
                }
            };
            config.model = model.into();
            let key = crate::infrastructure::security::keyring_storage::SecureStorage::new()
                .cloud_api_key(name)?
                .ok_or_else(|| AppError::InvalidConfig(format!("No {name} API key saved")))?;
            crate::features::llm::cloud::CloudLlm::new(&config, key)?
                .schedule(&crate::features::llm::scheduler::SchedulerRegistry::default())
        }
        Err(_) => std::sync::Arc::new(LlamaCppLlm::new(&config)?),
    };
    let checker_name =
        std::env::var("LATTICE_CLAIM_CHECKER").unwrap_or_else(|_| "lesson-judge".into());
    let batch_size: usize = std::env::var("LATTICE_CLAIM_BATCH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8);
    let pairs: Vec<serde_json::Value> =
        std::fs::read_to_string(std::env::var("LATTICE_CLAIM_PAIRS").unwrap())?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str)
            .collect::<std::result::Result<_, _>>()?;
    // Results are appended one batch at a time, so a stopped run resumes
    // where it left off instead of paying for finished pairs again.
    let results_path = std::path::PathBuf::from(std::env::var("LATTICE_CLAIM_RESULTS").unwrap());
    let finished: std::collections::HashSet<String> = std::fs::read_to_string(&results_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|row| row["id"].as_str().map(str::to_owned))
        .collect();
    let pending: Vec<_> = pairs
        .iter()
        .filter(|pair| !finished.contains(pair["id"].as_str().unwrap()))
        .collect();
    println!(
        "{} pairs, {} already scored, batches of {batch_size}",
        pairs.len(),
        finished.len()
    );
    // `LATTICE_CLAIM_FORM=fields` asks for the comparison as fields before the
    // verdict instead of the three labelled lines, `parts` for the claim split
    // into parts. `LATTICE_CLAIM_VARIANT` turns on `closed-book` and `brief`
    // (comma-separated), and `LATTICE_CLAIM_THINKING=on|off` overrides what
    // the judge is asked about thinking.
    let form = match std::env::var("LATTICE_CLAIM_FORM").as_deref() {
        Ok("fields") => VerdictForm::Fields,
        Ok("parts") => VerdictForm::Parts,
        _ => VerdictForm::Lines,
    };
    let named = std::env::var("LATTICE_CLAIM_VARIANT").unwrap_or_default();
    let variant = PromptVariant {
        closed_book: named.split(',').any(|name| name == "closed-book"),
        brief: named.split(',').any(|name| name == "brief"),
        thinking: match std::env::var("LATTICE_CLAIM_THINKING").as_deref() {
            Ok("on") => Some(true),
            Ok("off") => Some(false),
            _ => None,
        },
    };
    let checker = ClaimChecker::new(
        &*model,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    )
    .with_verdict_form(form)
    .with_prompt_variant(variant);
    // A pair holds one `passage`, or a page of `passages`, each `{label,
    // text}`: a labelled passage reaches the judge as the lesson check shows
    // it, its label line above its text (`requests::labelled`).
    let target = |pair: &serde_json::Value| LocatedClaim {
        claim: pair["claim"].as_str().unwrap().into(),
        passages: match pair["passages"].as_array() {
            Some(page) => page
                .iter()
                .map(|passage| {
                    let text = passage["text"].as_str().unwrap();
                    match passage["label"].as_str() {
                        Some(label) => format!("{label}\n{text}"),
                        None => text.to_owned(),
                    }
                })
                .collect(),
            None => vec![pair["passage"].as_str().unwrap().into()],
        },
    };
    let mut results = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&results_path)?;
    let mut groups: Vec<Vec<&serde_json::Value>> = pending
        .chunks(batch_size)
        .rev()
        .map(<[_]>::to_vec)
        .collect();
    while let Some(mut group) = groups.pop() {
        let targets: Vec<_> = group.iter().map(|pair| target(pair)).collect();
        if group.len() > 1 && !checker.batch_fits(&targets) {
            let rest = group.split_off(group.len() / 2);
            groups.push(rest);
            groups.push(group);
            continue;
        }
        let started = std::time::Instant::now();
        let mut output_tokens = 0;
        let mut judgments = if group.len() == 1 {
            vec![
                checker
                    .check_passages_without_deadline(&targets[0].claim, &targets[0].passages)
                    .await,
            ]
        } else {
            let answered = checker
                .check_batch_without_deadline(&targets, &|_| Ok(()), &|_| Ok(()))
                .await?;
            output_tokens = answered.output_tokens;
            answered.judgments
        };
        let mut calls = 1;
        for (judgment, located) in judgments.iter_mut().zip(&targets) {
            if group.len() > 1 && matches!(judgment, ClaimJudgment::Unusable) {
                *judgment = checker
                    .check_passages_without_deadline(&located.claim, &located.passages)
                    .await;
                calls += 1;
            }
        }
        let latency_ms = started.elapsed().as_secs_f64() * 1000.0 / group.len() as f64;
        for (pair, judgment) in group.iter().zip(judgments) {
            let (verdict, reason) = match judgment {
                ClaimJudgment::Judged(outcome) => (
                    match outcome.verdict {
                        ClaimVerdict::Supported => "supported",
                        ClaimVerdict::Contradicted => "contradicted",
                        ClaimVerdict::Unsupported => "unsupported",
                        ClaimVerdict::Unverified => "unverified",
                    },
                    outcome.reason,
                ),
                ClaimJudgment::Failed(error) => return Err(error),
                ClaimJudgment::Unusable => ("unverified", None),
            };
            writeln!(
                results,
                "{}",
                serde_json::json!({
                    "id": pair["id"], "checker": checker_name, "label": pair["label"],
                    "difficulty": pair["difficulty"], "kind": pair["kind"],
                    "verdict": verdict, "reason": reason,
                    "latency_ms": latency_ms, "batch": group.len(), "calls": calls,
                    "batch_output_tokens": output_tokens,
                })
            )?;
        }
        results.flush()?;
        println!(
            "Scored {} pairs in {:.0} s ({} calls)",
            group.len(),
            latency_ms * group.len() as f64 / 1000.0,
            calls
        );
    }
    Ok(())
}
