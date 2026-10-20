//! Scores the shipped strict claim judge on labelled claim/passage pairs.
//! Each pair becomes one claim whose only evidence is its passage, and pairs
//! are sent in batches the way lesson verification sends them: a batch that
//! does not fit is halved, and a claim the batch leaves unusable is rechecked
//! on its own. This is the whole model check a lesson claim gets; whether its
//! support is enough is the provenance gate's decision, which makes no call.
use super::*;

#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_CLAIM_PAIRS and LATTICE_CLAIM_RESULTS; makes real model requests"]
async fn live_claim_pairs_eval() -> Result<()> {
    use lattice_core::application::ports::llm_port::SamplingOverride;
    use lattice_core::application::services::claim_verification::{
        CheckPolicy, ClaimChecker, ClaimJudgment, ClaimVerdict, LocatedClaim,
    };
    use std::io::Write as _;

    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let model = LlamaCppLlm::new(&config)?;
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
    let checker = ClaimChecker::new(
        &model,
        SamplingOverride::deterministic(),
        512,
        CheckPolicy::Strict,
    );
    let target = |pair: &serde_json::Value| LocatedClaim {
        claim: pair["claim"].as_str().unwrap().into(),
        passages: vec![pair["passage"].as_str().unwrap().into()],
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
        let mut judgments = if group.len() == 1 {
            vec![
                checker
                    .check_passages_without_deadline(&targets[0].claim, &targets[0].passages)
                    .await,
            ]
        } else {
            checker
                .check_batch_without_deadline(&targets, &|_| Ok(()), &|_| Ok(()))
                .await?
                .judgments
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
                    "difficulty": pair["difficulty"], "verdict": verdict, "reason": reason,
                    "latency_ms": latency_ms, "batch": group.len(), "calls": calls,
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
