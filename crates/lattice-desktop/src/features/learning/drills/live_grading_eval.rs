//! How far the judge agrees with a careful human (docs/design/2026-11-04-code-review-drills.md §6). Runs
//! `JudgeGrader` on the configured model over every case in `tests/fixtures/drill_grading` and reports its agreement
//! with the labels per issue (found, half found, missed) and per finding outcome, every disagreement shown whole.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use lattice_core::application::contracts::settings::{LLMProvider, LLMSettingsDto};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{
    check_key, overlapping, validate, Assessment, Drill, DrillFile, Finding, FindingOutcome,
    IssueOutcome,
};
use lattice_core::domain::models::model_ref::{CloudProvider, ModelRef};
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use super::JudgeGrader;
use crate::features::learning::drills::grader::{Review, ReviewGrader};
use crate::features::llm::cloud::CloudLlm;
use crate::features::llm::llama_cpp::LlamaCppLlm;
use crate::features::llm::scheduler::SchedulerRegistry;

/// A drill, a review of it and what a careful human made of every issue and every finding.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    /// Which sort of review it is, from the design's list of what the cases cover.
    kind: String,
    /// Why the labels are what they are.
    why: String,
    drill: Drill,
    clean: Vec<DrillFile>,
    review: Vec<Finding>,
    labels: Labels,
}

/// The human's outcome for every issue of the key and every finding, by position.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Labels {
    issues: Vec<IssueOutcome>,
    findings: Vec<FindingOutcome>,
}

/// Every case under `tests/fixtures/drill_grading`, by file name, each with the name of its file.
fn cases() -> Result<Vec<(String, Case)>> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drill_grading");
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(&directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        }
    }
    paths.sort();
    let mut cases = Vec::with_capacity(paths.len());
    for path in paths {
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let case: Case = serde_json::from_slice(&std::fs::read(&path)?)?;
        cases.push((stem, case));
    }
    Ok(cases)
}

/// The labels as an assessment, each with an empty reason.
fn labelled(case: &Case) -> Assessment {
    Assessment {
        issues: case
            .labels
            .issues
            .iter()
            .map(|outcome| (*outcome, String::new()))
            .collect(),
        findings: case
            .labels
            .findings
            .iter()
            .map(|outcome| (outcome.clone(), String::new()))
            .collect(),
    }
}

fn issue_class(outcome: &IssueOutcome) -> &'static str {
    match outcome {
        IssueOutcome::Found { .. } => "found",
        IssueOutcome::HalfFound { .. } => "half_found",
        IssueOutcome::Missed => "missed",
    }
}

fn finding_class(outcome: &FindingOutcome) -> &'static str {
    match outcome {
        FindingOutcome::Matches { .. } => "matches",
        FindingOutcome::ValidExtra => "valid_extra",
        FindingOutcome::FalsePositive => "false_positive",
    }
}

/// One case after grading: what the judge made of it, or why it could not grade it.
struct Graded {
    case: Case,
    result: Result<Assessment>,
    seconds: f64,
}

/// How often the judge's class of outcome is the human's, and how often it also credits the same finding or names
/// the same issues, with every pairing of label and outcome counted.
#[derive(Default)]
struct Tally {
    total: usize,
    agree: usize,
    exact: usize,
    confusion: BTreeMap<String, BTreeMap<String, usize>>,
}

impl Tally {
    fn record(&mut self, label: &str, got: &str, exact: bool) {
        self.total += 1;
        self.agree += usize::from(label == got);
        self.exact += usize::from(exact);
        *self
            .confusion
            .entry(label.to_owned())
            .or_default()
            .entry(got.to_owned())
            .or_default() += 1;
    }

    fn value(&self) -> Value {
        json!({
            "total": self.total,
            "agree": self.agree,
            "agreement": ratio(self.agree, self.total),
            "exact": self.exact,
            "exact_agreement": ratio(self.exact, self.total),
            "confusion": self.confusion,
        })
    }
}

fn ratio(part: usize, whole: usize) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

/// The report: the model, the agreement per issue outcome and per finding outcome, the disagreements (the class of
/// outcome differs) and the differences of detail (the same class, another finding or other issues) each shown
/// whole with the case, the key and the review, and every case's labels beside what the judge answered.
fn report(model: Value, graded: &[Graded]) -> Value {
    let mut issues = Tally::default();
    let mut findings = Tally::default();
    let mut disagreements = Vec::new();
    let mut differences = Vec::new();
    let mut results = Vec::new();
    let mut failed = 0;
    for run in graded {
        let case = &run.case;
        let assessment = match &run.result {
            Ok(assessment) => assessment,
            Err(error) => {
                failed += 1;
                results.push(json!({
                    "case": case.name,
                    "kind": case.kind,
                    "seconds": run.seconds,
                    "error": error.to_string(),
                }));
                continue;
            }
        };
        for (index, label) in case.labels.issues.iter().enumerate() {
            let Some((got, reason)) = assessment.issues.get(index) else {
                continue;
            };
            let exact = got == label;
            issues.record(issue_class(label), issue_class(got), exact);
            let shown = json!({
                "case": case.name,
                "why": case.why,
                "kind": "issue",
                "index": index,
                "issue": case.drill.key.get(index),
                "label": label,
                "got": got,
                "reason": reason,
                "review": case.review,
            });
            if issue_class(label) != issue_class(got) {
                disagreements.push(shown);
            } else if !exact {
                differences.push(shown);
            }
        }
        for (index, label) in case.labels.findings.iter().enumerate() {
            let Some((got, reason)) = assessment.findings.get(index) else {
                continue;
            };
            let exact = got == label;
            findings.record(finding_class(label), finding_class(got), exact);
            let shown = json!({
                "case": case.name,
                "why": case.why,
                "kind": "finding",
                "index": index,
                "finding": case.review.get(index),
                "label": label,
                "got": got,
                "reason": reason,
                "key": case.drill.key,
            });
            if finding_class(label) != finding_class(got) {
                disagreements.push(shown);
            } else if !exact {
                differences.push(shown);
            }
        }
        results.push(json!({
            "case": case.name,
            "kind": case.kind,
            "seconds": run.seconds,
            "labels": {"issues": case.labels.issues, "findings": case.labels.findings},
            "got": assessment,
        }));
    }
    json!({
        "model": model,
        "cases": graded.len(),
        "failed": failed,
        "issues": issues.value(),
        "findings": findings.value(),
        "disagreements": disagreements,
        "detail_differences": differences,
        "results": results,
    })
}

/// The model the live tests are configured for: the llama.cpp server `LATTICE_LLAMACPP_SETTINGS` describes or, with
/// `LATTICE_USE_LEARNING_MODEL=1`, the Learning model those settings name, reached as the app reaches it.
pub(in crate::features::learning::drills) async fn model() -> Result<Arc<dyn LLMPort>> {
    let path = std::env::var("LATTICE_LLAMACPP_SETTINGS").map_err(|_| {
        AppError::InvalidConfig(
            "Set LATTICE_LLAMACPP_SETTINGS to the settings file the live tests read.".into(),
        )
    })?;
    let settings: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let mut config: LLMSettingsDto = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let schedulers = SchedulerRegistry::default();
    if std::env::var("LATTICE_USE_LEARNING_MODEL").as_deref() == Ok("1") {
        match config.learning_model.clone() {
            Some(ModelRef::Cloud { provider, model }) => {
                let (name, provider) = match provider {
                    CloudProvider::Openai => ("openai", LLMProvider::Openai),
                    CloudProvider::Anthropic => ("anthropic", LLMProvider::Anthropic),
                    CloudProvider::Google => ("google", LLMProvider::Google),
                };
                config.provider = provider;
                config.model = model;
                let key = crate::infrastructure::security::keyring_storage::SecureStorage::new()
                    .cloud_api_key(name)?
                    .ok_or_else(|| {
                        AppError::InvalidConfig(format!(
                            "No {name} API key saved for the Learning eval"
                        ))
                    })?;
                return Ok(CloudLlm::new(&config, key)?.schedule(&schedulers));
            }
            Some(ModelRef::LlamaCpp { model }) => config.llama_cpp.model = model,
            _ => {
                return Err(AppError::InvalidConfig(
                    "The Learning eval needs a cloud or llama.cpp Learning model.".into(),
                ))
            }
        }
    }
    Ok(LlamaCppLlm::new(&config)?.schedule(&schedulers).await)
}

#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS and LATTICE_DRILL_GRADING_OUT; makes real model requests"]
async fn live_grading_eval() -> Result<()> {
    let out = std::env::var("LATTICE_DRILL_GRADING_OUT").map_err(|_| {
        AppError::InvalidConfig(
            "Set LATTICE_DRILL_GRADING_OUT to the file the report is written to.".into(),
        )
    })?;
    let llm = model().await?;
    let grader = JudgeGrader::new(llm.as_ref());
    let mut graded = Vec::new();
    for (_, case) in cases()? {
        let review = Review {
            drill: &case.drill,
            clean: &case.clean,
            findings: &case.review,
        };
        let started = Instant::now();
        let result = grader.assess(&review).await;
        let seconds = started.elapsed().as_secs_f64();
        println!(
            "{}: {} in {seconds:.1} s",
            case.name,
            if result.is_ok() {
                "graded"
            } else {
                "not graded"
            }
        );
        graded.push(Graded {
            case,
            result,
            seconds,
        });
    }
    let described = json!({
        "name": llm.model_name(),
        "provider": llm.provider_name(),
        "context_window": llm.max_context_tokens(),
    });
    let written = report(described, &graded);
    std::fs::write(&out, serde_json::to_vec_pretty(&written)?)?;
    let rate = |agreement: &Value| {
        agreement
            .as_f64()
            .map_or_else(|| "n/a".to_owned(), |rate| format!("{rate:.3}"))
    };
    println!(
        "{} ({}): issues {} of {} agree ({}), findings {} of {} agree ({}), {} of {} cases not graded; report in {out}",
        written["model"]["name"],
        written["model"]["provider"],
        written["issues"]["agree"],
        written["issues"]["total"],
        rate(&written["issues"]["agreement"]),
        written["findings"]["agree"],
        written["findings"]["total"],
        rate(&written["findings"]["agreement"]),
        written["failed"],
        written["cases"],
    );
    Ok(())
}

fn case_of_kind(kind: &str) -> Result<Case> {
    cases()?
        .into_iter()
        .map(|(_, case)| case)
        .find(|case| case.kind == kind)
        .ok_or_else(|| AppError::InvalidState(format!("There is no case of kind {kind}")))
}

#[test]
fn the_report_counts_agreement_by_outcome_and_shows_each_disagreement_whole() -> Result<()> {
    let find = case_of_kind("find")?;
    let duplicate = case_of_kind("duplicate")?;
    let ungraded = case_of_kind("no_span")?;

    // The off-by-one is half found where the human found it.
    let mut first = labelled(&find);
    first.issues[2] = (IssueOutcome::HalfFound { finding: 0 }, "Too vague.".into());
    // The issue is credited to the later of two findings that name it: the class is the human's, the finding is not.
    let mut second = labelled(&duplicate);
    second.issues[0] = (IssueOutcome::Found { finding: 1 }, "The later one.".into());

    let graded = vec![
        Graded {
            case: find,
            result: Ok(first),
            seconds: 1.5,
        },
        Graded {
            case: duplicate,
            result: Ok(second),
            seconds: 2.5,
        },
        Graded {
            case: ungraded,
            result: Err(AppError::ModelAnswerUnusable("Nothing usable.".into())),
            seconds: 0.5,
        },
    ];
    let model =
        json!({"name": "named-model", "provider": "named-provider", "context_window": 32768});
    let report = report(model, &graded);

    assert_eq!(report["model"]["name"], "named-model");
    assert_eq!(
        (report["cases"].as_u64(), report["failed"].as_u64()),
        (Some(3), Some(1))
    );
    let issues = &report["issues"];
    assert_eq!(
        (
            issues["total"].as_u64(),
            issues["agree"].as_u64(),
            issues["exact"].as_u64()
        ),
        (Some(5), Some(4), Some(3))
    );
    assert_eq!(issues["agreement"].as_f64(), Some(0.8));
    assert_eq!(issues["confusion"]["found"]["half_found"].as_u64(), Some(1));
    assert_eq!(issues["confusion"]["found"]["found"].as_u64(), Some(2));
    assert_eq!(issues["confusion"]["missed"]["missed"].as_u64(), Some(2));
    let findings = &report["findings"];
    assert_eq!(
        (
            findings["total"].as_u64(),
            findings["agree"].as_u64(),
            findings["exact"].as_u64()
        ),
        (Some(4), Some(4), Some(4))
    );
    assert_eq!(
        findings["confusion"]["matches"]["matches"].as_u64(),
        Some(4)
    );

    let disagreements = report["disagreements"].as_array().unwrap();
    assert_eq!(disagreements.len(), 1);
    let shown = &disagreements[0];
    assert_eq!(shown["kind"], "issue");
    assert_eq!(shown["index"], 2);
    assert_eq!(shown["issue"]["pattern"], "correctness.off_by_one");
    assert_eq!(shown["label"], json!({"found": {"finding": 0}}));
    assert_eq!(shown["got"], json!({"half_found": {"finding": 0}}));
    assert_eq!(shown["reason"], "Too vague.");
    assert_eq!(shown["review"].as_array().map(Vec::len), Some(2));
    assert!(shown["why"].as_str().is_some_and(|why| !why.is_empty()));

    let differences = report["detail_differences"].as_array().unwrap();
    assert_eq!(differences.len(), 1);
    assert_eq!(differences[0]["case"], "08-duplicate");
    assert_eq!(differences[0]["got"], json!({"found": {"finding": 1}}));

    let results = report["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(
        results[0]["got"]["issues"][2][0],
        json!({"half_found": {"finding": 0}})
    );
    assert_eq!(
        results[2]["error"],
        "The model's answer could not be used: Nothing usable."
    );
    Ok(())
}

#[test]
fn every_labelled_case_holds_together() -> Result<()> {
    let cases = cases()?;
    assert!(cases.len() >= 40, "the forty cases are written");
    let mut names = BTreeSet::new();
    for (stem, case) in &cases {
        assert_eq!(&case.name, stem, "a case is named for its file");
        assert!(
            names.insert(case.name.clone()),
            "{stem}: the name is used twice"
        );
        assert!(
            !case.why.trim().is_empty(),
            "{stem}: say why the labels are what they are"
        );
        if let Err(fault) = check_key(&case.clean, &case.drill) {
            panic!("{stem}: the key does not fit the code: {fault:?}");
        }
        assert_eq!(
            case.labels.issues.len(),
            case.drill.key.len(),
            "{stem}: a label for every issue"
        );
        assert_eq!(
            case.labels.findings.len(),
            case.review.len(),
            "{stem}: a label for every finding"
        );
        assert_eq!(
            validate(case.drill.key.len(), case.review.len(), &labelled(case)),
            Ok(()),
            "{stem}: the labels contradict each other"
        );
        for found in &case.review {
            let Some(lines) = &found.span else { continue };
            let file = case.drill.files.iter().find(|file| file.path == lines.path);
            let count = file.map_or(0, |file| file.text.lines().count());
            assert!(
                lines.start >= 1 && lines.start <= lines.end && (lines.end as usize) <= count,
                "{stem}: a finding is on lines the drill does not have: {lines:?}"
            );
        }
    }
    Ok(())
}

/// Whether the case shows the sort of review its kind names.
fn shows_its_kind(case: &Case) -> bool {
    let labels = &case.labels;
    let overlap = overlapping(&case.review, &case.drill.key);
    let matching = |issue: usize| {
        labels
            .findings
            .iter()
            .filter(|outcome| matches!(outcome, FindingOutcome::Matches { issues } if issues.contains(&issue)))
            .count()
    };
    match case.kind.as_str() {
        "find" => labels.issues.iter().any(|outcome| matches!(outcome, IssueOutcome::Found { .. })),
        "wrong_reason" | "vague" => labels.issues.iter().any(|outcome| matches!(outcome, IssueOutcome::HalfFound { .. })),
        "exposing_line" => labels.issues.iter().enumerate().any(|(issue, outcome)| {
            matches!(outcome, IssueOutcome::Found { finding } if !overlap.contains(&(*finding, issue)))
        }),
        "valid_extra" => labels.findings.contains(&FindingOutcome::ValidExtra),
        "false_positive" | "false_claim_on_issue_lines" => labels.findings.contains(&FindingOutcome::FalsePositive),
        "two_issues" => labels
            .findings
            .iter()
            .any(|outcome| matches!(outcome, FindingOutcome::Matches { issues } if issues.len() > 1)),
        "duplicate" => (0..case.drill.key.len()).any(|issue| matching(issue) > 1),
        "no_span" => case
            .review
            .iter()
            .zip(&labels.findings)
            .any(|(found, outcome)| found.span.is_none() && matches!(outcome, FindingOutcome::Matches { .. })),
        _ => true,
    }
}

#[test]
fn the_cases_cover_what_the_design_asks_for_and_each_shows_its_kind() -> Result<()> {
    let cases = cases()?;
    let kinds: BTreeSet<&str> = cases.iter().map(|(_, case)| case.kind.as_str()).collect();
    for kind in [
        "find",
        "wrong_reason",
        "exposing_line",
        "vague",
        "valid_extra",
        "false_positive",
        "two_issues",
        "duplicate",
        "no_span",
    ] {
        assert!(kinds.contains(kind), "there is no case of kind {kind}");
    }
    for (stem, case) in &cases {
        assert!(shows_its_kind(case), "{stem} is not a {} case", case.kind);
    }
    let issue_classes: BTreeSet<&str> = cases
        .iter()
        .flat_map(|(_, case)| case.labels.issues.iter().map(issue_class))
        .collect();
    let finding_classes: BTreeSet<&str> = cases
        .iter()
        .flat_map(|(_, case)| case.labels.findings.iter().map(finding_class))
        .collect();
    assert_eq!(
        issue_classes,
        BTreeSet::from(["found", "half_found", "missed"])
    );
    assert_eq!(
        finding_classes,
        BTreeSet::from(["false_positive", "matches", "valid_extra"])
    );
    Ok(())
}
