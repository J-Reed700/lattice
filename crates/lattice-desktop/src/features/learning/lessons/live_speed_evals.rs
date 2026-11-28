//! Targeted live evals for the three lesson-generation speed optimizations:
//! 1. `live_coverage_fidelity_eval` — 24 statement-coverage fidelity cases (`coverage.rs`).
//! 2. `live_contract_reports_eval` — 8 reviewer-report routing & adjudication cases (`contract::rule_on_reports`).
//! 3. `live_contract_confirm_eval` — 24 confirming-read cases (`ConfirmPlanned`, `ConfirmUse`, `Confirm<Conflict>`).
//!
//! Each test records calls, input/output tokens, wall time, and per-case verdicts
//! under `LATTICE_LESSON_CALLS` so before/after runs on remote 27B are directly comparable.

use super::*;
use std::time::Instant;

fn eval_dir(sub: &str) -> Result<std::path::PathBuf> {
    let root = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").map_err(|_| {
        AppError::InvalidConfig("Set LATTICE_LESSON_CALLS to an artifact directory.".into())
    })?);
    let dir = root.join(sub);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[tokio::test]
#[ignore = "runs 24 standalone statement-coverage fidelity cases against a live model"]
async fn live_coverage_fidelity_eval() -> Result<()> {
    let config = crate::tests::live_llama_cpp::settings()?;
    let directory = eval_dir("coverage-fidelity")?;
    let model = ObservedModel::new(&config, directory.clone(), 0).await?;
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "content_verification/coverage_scope_cases.json"
    ))?;
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    let started = Instant::now();
    let mut results = Vec::new();
    let mut failures = Vec::new();
    let mut tp = 0usize;
    let mut tn = 0usize;
    let mut fp = 0usize;
    let mut fn_count = 0usize;

    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap_or("unnamed");
        let (calls_before, in_before, out_before) = model.usage();
        let case_started = Instant::now();
        let report = crate::features::learning::content_verification::live_coverage_fixture(
            &model, case, &progress,
        )
        .await?;
        let (calls_after, in_after, out_after) = model.usage();
        let incomplete: Vec<u64> = report["units"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|unit| unit["complete"] == false)
            .filter_map(|unit| unit["index"].as_u64())
            .collect();
        let expected: Vec<u64> = case["expectedIncomplete"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_u64)
            .collect();
        let expect_incomplete = !expected.is_empty();
        let got_incomplete = !incomplete.is_empty();
        match (expect_incomplete, got_incomplete) {
            (true, true) => tp += 1,
            (false, false) => tn += 1,
            (false, true) => fp += 1,
            (true, false) => fn_count += 1,
        }
        let mut missing_terms = Vec::new();
        if incomplete == expected {
            if let Some(required) = case["requiredMissingTerms"].as_object() {
                for (unit_key, terms) in required {
                    let unit: usize = unit_key.parse().unwrap();
                    let reason = report["units"][unit]["reason"]
                        .as_str()
                        .unwrap_or_default()
                        .to_lowercase();
                    for term in terms.as_array().unwrap() {
                        let needle = term.as_str().unwrap().to_lowercase();
                        if !reason.contains(&needle) {
                            missing_terms.push(format!("unit {unit} missing term {needle:?}"));
                        }
                    }
                }
            }
        }
        let passed = incomplete == expected && missing_terms.is_empty();
        if !passed {
            failures.push(format!(
                "case {index} ({name}): expected={expected:?}, actual={incomplete:?}, missing_terms={missing_terms:?}, report={report}"
            ));
        }
        let row = serde_json::json!({
            "index": index,
            "name": name,
            "passed": passed,
            "expectedIncomplete": expected,
            "actualIncomplete": incomplete,
            "missingTerms": missing_terms,
            "calls": calls_after - calls_before,
            "inputTokens": in_after - in_before,
            "outputTokens": out_after - out_before,
            "seconds": case_started.elapsed().as_secs_f64(),
            "report": report,
        });
        println!(
            "COVERAGE [{:02}/{}] {:46} {} | calls={} in={} out={} {:.2}s",
            index + 1,
            cases.len(),
            name,
            if passed { "PASS" } else { "FAIL" },
            calls_after - calls_before,
            in_after - in_before,
            out_after - out_before,
            case_started.elapsed().as_secs_f64()
        );
        results.push(row);
    }

    let (total_calls, total_in, total_out) = model.usage();
    let passed_count = cases.len() - failures.len();
    let summary = serde_json::json!({
        "suite": "coverage_fidelity",
        "model": model.model_name(),
        "supportsNativeThinking": model.supports_native_thinking(),
        "totalCases": cases.len(),
        "passed": passed_count,
        "failed": failures.len(),
        "tp": tp,
        "tn": tn,
        "fp": fp,
        "fn": fn_count,
        "totalCalls": total_calls,
        "totalInputTokens": total_in,
        "totalOutputTokens": total_out,
        "wallSeconds": started.elapsed().as_secs_f64(),
        "results": results,
    });
    std::fs::write(
        directory.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!(
        "COVERAGE SUMMARY: {passed_count}/{} passed (TP={tp} TN={tn} FP={fp} FN={fn_count}) | calls={total_calls} in={total_in} out={total_out} wall={:.1}s",
        cases.len(),
        started.elapsed().as_secs_f64()
    );
    assert!(
        failures.is_empty(),
        "Coverage fidelity eval failures ({}/{}):\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    Ok(())
}

#[tokio::test]
#[ignore = "runs 8 contract::rule_on_reports cases against a live model"]
async fn live_contract_reports_eval() -> Result<()> {
    let config = crate::tests::live_llama_cpp::settings()?;
    let directory = eval_dir("contract-reports")?;
    let model = ObservedModel::new(&config, directory.clone(), 0).await?;
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("live_fixtures/contract_reports_cases.json"))?;
    let started = Instant::now();
    let mut results = Vec::new();
    let mut failures = Vec::new();

    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap_or("unnamed");
        let (calls_before, in_before, out_before) = model.usage();
        let case_started = Instant::now();
        let output =
            crate::features::learning::teaching::contract_eval::live_rule_on_reports(&model, case)
                .await?;
        let (calls_after, in_after, out_after) = model.usage();
        let actual_outside: Vec<u64> = output["outside"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_u64)
            .collect();
        let expected_outside: Vec<u64> = case["expectedOutside"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_u64)
            .collect();
        let findings = output["findings"].as_array().unwrap();
        let expect_findings = case["expectFindings"].as_bool().unwrap();
        let mut issues = Vec::new();
        if actual_outside != expected_outside {
            issues.push(format!(
                "outside mismatch: expected={expected_outside:?}, actual={actual_outside:?}"
            ));
        }
        if expect_findings && findings.is_empty() {
            issues.push("expected at least one finding, got none".into());
        }
        if !expect_findings && !findings.is_empty() {
            issues.push(format!("expected no findings, got {findings:?}"));
        }
        if let Some(substrings) = case
            .get("expectedNoteSubstrings")
            .and_then(|v| v.as_array())
        {
            let joined = findings
                .iter()
                .filter_map(|f| f["note"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
                .to_lowercase();
            for needle in substrings {
                let needle = needle.as_str().unwrap().to_lowercase();
                if !joined.contains(&needle) {
                    issues.push(format!(
                        "finding notes missing substring {needle:?}: {joined}"
                    ));
                }
            }
        }
        let passed = issues.is_empty();
        if !passed {
            failures.push(format!("case {index} ({name}): {}", issues.join("; ")));
        }
        let row = serde_json::json!({
            "index": index,
            "name": name,
            "passed": passed,
            "issues": issues,
            "calls": calls_after - calls_before,
            "inputTokens": in_after - in_before,
            "outputTokens": out_after - out_before,
            "seconds": case_started.elapsed().as_secs_f64(),
            "output": output,
        });
        println!(
            "REPORTS  [{:02}/{}] {:46} {} | calls={} in={} out={} {:.2}s",
            index + 1,
            cases.len(),
            name,
            if passed { "PASS" } else { "FAIL" },
            calls_after - calls_before,
            in_after - in_before,
            out_after - out_before,
            case_started.elapsed().as_secs_f64()
        );
        results.push(row);
    }

    let (total_calls, total_in, total_out) = model.usage();
    let passed_count = cases.len() - failures.len();
    let summary = serde_json::json!({
        "suite": "contract_reports",
        "model": model.model_name(),
        "supportsNativeThinking": model.supports_native_thinking(),
        "totalCases": cases.len(),
        "passed": passed_count,
        "failed": failures.len(),
        "totalCalls": total_calls,
        "totalInputTokens": total_in,
        "totalOutputTokens": total_out,
        "wallSeconds": started.elapsed().as_secs_f64(),
        "results": results,
    });
    std::fs::write(
        directory.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!(
        "REPORTS SUMMARY: {passed_count}/{} passed | calls={total_calls} in={total_in} out={total_out} wall={:.1}s",
        cases.len(),
        started.elapsed().as_secs_f64()
    );
    assert!(
        failures.is_empty(),
        "Contract reports eval failures ({}/{}):\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    Ok(())
}

#[tokio::test]
#[ignore = "runs 24 contract::confirm cases (planned, use, clash) against a live model"]
async fn live_contract_confirm_eval() -> Result<()> {
    let config = crate::tests::live_llama_cpp::settings()?;
    let directory = eval_dir("contract-confirm")?;
    let model = ObservedModel::new(&config, directory.clone(), 0).await?;
    let suite: serde_json::Value =
        serde_json::from_str(include_str!("live_fixtures/contract_confirm_cases.json"))?;
    let started = Instant::now();
    let mut results = Vec::new();
    let mut failures = Vec::new();
    let mut counts = serde_json::json!({
        "planned": {"passed": 0, "total": 0},
        "uses": {"passed": 0, "total": 0},
        "clashes": {"passed": 0, "total": 0},
    });

    for (index, case) in suite["planned"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap_or("unnamed");
        let (c0, i0, o0) = model.usage();
        let t0 = Instant::now();
        let out =
            crate::features::learning::teaching::contract_eval::live_confirm_planned(&model, case)
                .await?;
        let (c1, i1, o1) = model.usage();
        let expected = case["expectedTaught"].as_bool().unwrap();
        let actual = out["taught"].as_bool().unwrap();
        let passed = actual == expected;
        counts["planned"]["total"] =
            serde_json::json!(counts["planned"]["total"].as_u64().unwrap() + 1);
        if passed {
            counts["planned"]["passed"] =
                serde_json::json!(counts["planned"]["passed"].as_u64().unwrap() + 1);
        } else {
            failures.push(format!(
                "planned[{index}] ({name}): expected taught={expected}, got {out}"
            ));
        }
        println!(
            "CONFIRM planned [{:02}] {:42} {} | calls={} in={} out={} {:.2}s",
            index + 1,
            name,
            if passed { "PASS" } else { "FAIL" },
            c1 - c0,
            i1 - i0,
            o1 - o0,
            t0.elapsed().as_secs_f64()
        );
        results.push(serde_json::json!({"judge":"planned","index":index,"name":name,"passed":passed,"expected":expected,"actual":out,"calls":c1-c0,"inputTokens":i1-i0,"outputTokens":o1-o0,"seconds":t0.elapsed().as_secs_f64()}));
    }

    for (index, case) in suite["uses"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap_or("unnamed");
        let (c0, i0, o0) = model.usage();
        let t0 = Instant::now();
        let out =
            crate::features::learning::teaching::contract_eval::live_confirm_use(&model, case)
                .await?;
        let (c1, i1, o1) = model.usage();
        let expected = case["expectedOutcome"].as_str().unwrap();
        let actual = out["outcome"].as_str().unwrap();
        let passed = actual == expected;
        counts["uses"]["total"] = serde_json::json!(counts["uses"]["total"].as_u64().unwrap() + 1);
        if passed {
            counts["uses"]["passed"] =
                serde_json::json!(counts["uses"]["passed"].as_u64().unwrap() + 1);
        } else {
            failures.push(format!(
                "uses[{index}] ({name}): expected outcome={expected}, got {out}"
            ));
        }
        println!(
            "CONFIRM use     [{:02}] {:42} {} | calls={} in={} out={} {:.2}s",
            index + 1,
            name,
            if passed { "PASS" } else { "FAIL" },
            c1 - c0,
            i1 - i0,
            o1 - o0,
            t0.elapsed().as_secs_f64()
        );
        results.push(serde_json::json!({"judge":"use","index":index,"name":name,"passed":passed,"expected":expected,"actual":out,"calls":c1-c0,"inputTokens":i1-i0,"outputTokens":o1-o0,"seconds":t0.elapsed().as_secs_f64()}));
    }

    for (index, case) in suite["clashes"].as_array().unwrap().iter().enumerate() {
        let name = case["name"].as_str().unwrap_or("unnamed");
        let (c0, i0, o0) = model.usage();
        let t0 = Instant::now();
        let out =
            crate::features::learning::teaching::contract_eval::live_confirm_clash(&model, case)
                .await?;
        let (c1, i1, o1) = model.usage();
        let expected = case["expectedContradict"].as_bool().unwrap();
        let actual = out["contradict"].as_bool().unwrap();
        let passed = actual == expected;
        counts["clashes"]["total"] =
            serde_json::json!(counts["clashes"]["total"].as_u64().unwrap() + 1);
        if passed {
            counts["clashes"]["passed"] =
                serde_json::json!(counts["clashes"]["passed"].as_u64().unwrap() + 1);
        } else {
            failures.push(format!(
                "clashes[{index}] ({name}): expected contradict={expected}, got {out}"
            ));
        }
        println!(
            "CONFIRM clash   [{:02}] {:42} {} | calls={} in={} out={} {:.2}s",
            index + 1,
            name,
            if passed { "PASS" } else { "FAIL" },
            c1 - c0,
            i1 - i0,
            o1 - o0,
            t0.elapsed().as_secs_f64()
        );
        results.push(serde_json::json!({"judge":"clash","index":index,"name":name,"passed":passed,"expected":expected,"actual":out,"calls":c1-c0,"inputTokens":i1-i0,"outputTokens":o1-o0,"seconds":t0.elapsed().as_secs_f64()}));
    }

    let (total_calls, total_in, total_out) = model.usage();
    let total_cases = results.len();
    let passed_count = total_cases - failures.len();
    let summary = serde_json::json!({
        "suite": "contract_confirm",
        "model": model.model_name(),
        "supportsNativeThinking": model.supports_native_thinking(),
        "totalCases": total_cases,
        "passed": passed_count,
        "failed": failures.len(),
        "byJudge": counts,
        "totalCalls": total_calls,
        "totalInputTokens": total_in,
        "totalOutputTokens": total_out,
        "wallSeconds": started.elapsed().as_secs_f64(),
        "results": results,
    });
    std::fs::write(
        directory.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!(
        "CONFIRM SUMMARY: {passed_count}/{total_cases} passed (planned={}/{} uses={}/{} clashes={}/{}) | calls={total_calls} in={total_in} out={total_out} wall={:.1}s",
        counts["planned"]["passed"],
        counts["planned"]["total"],
        counts["uses"]["passed"],
        counts["uses"]["total"],
        counts["clashes"]["passed"],
        counts["clashes"]["total"],
        started.elapsed().as_secs_f64()
    );
    assert!(
        failures.is_empty(),
        "Contract confirm eval failures ({}/{}):\n{}",
        failures.len(),
        total_cases,
        failures.join("\n")
    );
    Ok(())
}

#[test]
fn speed_eval_fixtures_parse_and_have_balanced_cases() -> Result<()> {
    let coverage: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "content_verification/coverage_scope_cases.json"
    ))?;
    assert_eq!(coverage.len(), 24);
    let incomplete = coverage
        .iter()
        .filter(|c| !c["expectedIncomplete"].as_array().unwrap().is_empty())
        .count();
    assert_eq!(incomplete, 12, "coverage eval should be balanced 12/12");

    let reports: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("live_fixtures/contract_reports_cases.json"))?;
    assert_eq!(reports.len(), 8);

    let confirm: serde_json::Value =
        serde_json::from_str(include_str!("live_fixtures/contract_confirm_cases.json"))?;
    assert_eq!(confirm["planned"].as_array().unwrap().len(), 8);
    assert_eq!(confirm["uses"].as_array().unwrap().len(), 8);
    assert_eq!(confirm["clashes"].as_array().unwrap().len(), 8);
    crate::features::learning::teaching::contract_eval::validate_confirm_quotes(&confirm)?;
    Ok(())
}
