//! How real the planted issues of a drill are (docs/design/2026-11-04-code-review-drills.md section 6). Builds drills on
//! the configured model with the real runner and the judge that grades reviews, and writes each one as JSON and as
//! Markdown for a person to read, with the counts: what was planted, what a test and what the judge proved, what was
//! dropped and why, and what the blind review found. Whether a planted issue is real is a person's judgement, so
//! nothing here scores it.
//!
//! The passage case builds drills on passages of a course in a copy of the app's database, and writes for each the
//! source passage, the clean explanation, every planted edit with its proof, and what was dropped and why.

use std::path::{Path, PathBuf};
use std::time::Instant;

use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{
    line_diff, pattern, targets, DiffKind, KeyIssue, Material, Origin, PatternDue, Proof,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome, JobStore};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use super::support::{
    code, literals_mutation, planted, proved, quiet, review, stock, unchecked, KILLING_TEST,
    LITERALS, MODEL, OPERATOR, SUITE,
};
use crate::features::learning::drills::bank;
use crate::features::learning::drills::build::{
    BlindReview, DrillBuild, DroppedMutation, Evidence, ModelCall, Stage, Trail,
};
use crate::features::learning::drills::grading::live_grading_eval::model;
use crate::features::learning::drills::grading::JudgeGrader;
use crate::features::learning::drills::material::{material_for, ClaimSourceCheck};
use crate::features::learning::drills::repository::{self, StoredDrill};
use crate::features::learning::drills::runner::{runner_for, RunOutcome};
use crate::features::learning::drills::spec::DrillSpec;
use crate::features::learning::embedded_runtime::BuiltinRuntimes;
use crate::features::learning::persistence::{db, now};

/// What a drill's build planted, proved and dropped, and what its blind review added to the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Counts {
    /// The defects the model proposed: those that were proven and those that were dropped.
    planted: usize,
    proven_by_test: usize,
    proven_by_judge: usize,
    dropped: Vec<Dropped>,
    /// Issues of the key that the blind review found and the judge confirmed.
    found_in_review: usize,
}

/// A defect that was not proven, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Dropped {
    pattern: String,
    why: String,
}

fn counts_of(stored: &StoredDrill) -> Counts {
    let evidence = &stored.evidence;
    let proven_by = |killed: bool| {
        evidence
            .proofs
            .iter()
            .filter(|proof| matches!(proof.proof, Proof::Killed { .. }) == killed)
            .count()
    };
    Counts {
        planted: evidence.proofs.len() + evidence.dropped.len(),
        proven_by_test: proven_by(true),
        proven_by_judge: proven_by(false),
        dropped: evidence
            .dropped
            .iter()
            .map(|dropped| Dropped {
                pattern: dropped.mutation.pattern.clone(),
                why: dropped.why.clone(),
            })
            .collect(),
        found_in_review: stored
            .drill
            .key
            .iter()
            .filter(|issue| issue.origin == Origin::FoundInReview)
            .count(),
    }
}

/// A drill with everything its build kept, as JSON: its code, its clean code, its key with each proof, its evidence
/// and the model of every call.
fn drill_json(stored: &StoredDrill, counts: &Counts, seconds: f64) -> Value {
    let models: Vec<Value> = models_of(stored)
        .iter()
        .map(|call| json!({"purpose": call.purpose, "model": call.model}))
        .collect();
    json!({
        "id": stored.drill.id,
        "language": stored.drill.material.language(),
        "title": stored.drill.title,
        "domain": stored.domain,
        "model": stored.model,
        "runner": stored.runner,
        "seconds": seconds,
        "spec": stored.spec,
        "counts": counts,
        "files": stored.drill.files,
        "clean_files": stored.clean_files,
        "suite": stored.suite,
        "key": stored.drill.key,
        "evidence": stored.evidence,
        "models": models,
    })
}

/// Every model call the build made, in the order of the stages.
fn models_of(stored: &StoredDrill) -> Vec<ModelCall> {
    let evidence = &stored.evidence;
    let mut trails: Vec<&Trail> = vec![
        &evidence.clean_code,
        &evidence.clean_review.trail,
        &evidence.plant,
    ];
    trails.extend(evidence.proofs.iter().map(|proof| &proof.trail));
    trails.extend(evidence.dropped.iter().map(|dropped| &dropped.trail));
    trails.extend(evidence.lesson_checks.iter().map(|check| &check.trail));
    trails.push(&evidence.blind_review.trail);
    trails
        .into_iter()
        .flat_map(|trail| trail.calls.iter().cloned())
        .collect()
}

/// The backticks that fence `text` in Markdown: more than any run of them inside it.
fn fence(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for character in text.chars() {
        if character == '`' {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    "`".repeat(longest.max(2) + 1)
}

fn block(language: &str, text: &str) -> String {
    let fence = fence(text);
    format!(
        "{fence}{language}\n{}\n{fence}\n\n",
        text.trim_end_matches('\n')
    )
}

/// The lines of `text` with their numbers, as the learner sees them.
fn numbered(text: &str) -> String {
    let width = text.lines().count().to_string().len();
    text.lines()
        .enumerate()
        .map(|(position, line)| format!("{:>width$} | {line}", position + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

fn outcome_word(outcome: &RunOutcome) -> String {
    match outcome {
        RunOutcome::Passed => "passed".to_owned(),
        RunOutcome::Failed { output } => format!(
            "failed with {:?}",
            output.lines().last().unwrap_or_default()
        ),
        RunOutcome::TimedOut { .. } => "ran out of time".to_owned(),
        RunOutcome::Cancelled => "was cancelled".to_owned(),
        RunOutcome::Unavailable { why } => format!("could not run: {why}"),
    }
}

fn runs(trail: &Trail) -> String {
    trail
        .runs
        .iter()
        .map(|run| format!("- {} {}\n", run.what, outcome_word(&run.outcome)))
        .collect()
}

/// The drill for a person to read: the code as the learner reads it, the key issue by issue with its proof and its
/// fix as a diff, what was dropped, the clean code, the clean review, the blind review and the models.
fn markdown(stored: &StoredDrill, counts: &Counts, seconds: f64) -> String {
    let evidence = &stored.evidence;
    let mut out = format!(
        "# {} ({})\n\n",
        stored.drill.title,
        stored.drill.material.language().unwrap_or_default()
    );
    out.push_str(&format!(
        "- Domain: {}\n- Built by: {}\n- Runner: {}\n- Built in {seconds:.0} seconds\n- Patterns asked for: {}\n\n",
        stored.domain,
        stored.model,
        stored.runner.as_deref().unwrap_or("none, every defect was judged"),
        stored.spec.patterns.join(", ")
    ));
    out.push_str(&format!(
        "## Counts\n\n- Planted: {}\n- Proven by a test: {}\n- Proven by the judge: {}\n- Dropped: {}\n- Found in review: {}\n\n",
        counts.planted,
        counts.proven_by_test,
        counts.proven_by_judge,
        counts.dropped.len(),
        counts.found_in_review
    ));

    out.push_str("## The code\n\n");
    for file in &stored.drill.files {
        out.push_str(&format!("### {}\n\n", file.path));
        out.push_str(&block("text", &numbered(&file.text)));
    }

    out.push_str("## The key\n\n");
    for (position, issue) in stored.drill.key.iter().enumerate() {
        let name = pattern(&issue.pattern).map_or(issue.pattern.as_str(), |known| known.name);
        let origin = match issue.origin {
            Origin::Planted => "planted",
            Origin::FoundInReview => "found in review",
        };
        out.push_str(&format!(
            "### {}. {name} ({}), {:?}, {origin}, {} lines {} to {}\n\n",
            position + 1,
            issue.pattern,
            issue.severity,
            issue.span.path,
            issue.span.start,
            issue.span.end
        ));
        out.push_str(&format!(
            "- Consequence: {}\n- In an interview: {}\n\n",
            issue.consequence, issue.explanation
        ));
        match &issue.proof {
            Proof::Killed { test } => {
                out.push_str(
                    "Proven by a test that fails on the drill and passes on the clean code:\n\n",
                );
                out.push_str(&block("text", test));
            }
            Proof::Judged { reason } => {
                out.push_str(&format!("Proven by the judge: {reason}\n\n"));
            }
            Proof::Sourced { quote, reason, .. } => {
                out.push_str(&format!(
                    "Proven by the source, which says \"{quote}\": {reason}\n\n"
                ));
            }
        }
        if let Some(proof) = evidence.proofs.iter().find(|proof| proof.issue == issue.id) {
            out.push_str(&runs(&proof.trail));
            out.push('\n');
        }
        let diff: String = line_diff(&issue.fix.before, &issue.fix.after)
            .iter()
            .map(|line| {
                let mark = match line.kind {
                    DiffKind::Unchanged => ' ',
                    DiffKind::Removed => '-',
                    DiffKind::Added => '+',
                };
                format!("{mark}{}\n", line.text)
            })
            .collect();
        out.push_str(&format!("The fix, in {}:\n\n", issue.fix.path));
        out.push_str(&block("diff", &diff));
    }

    out.push_str("## Dropped\n\n");
    if counts.dropped.is_empty() {
        out.push_str("Nothing was dropped.\n\n");
    }
    for dropped in &evidence.dropped {
        out.push_str(&format!(
            "- {}: {}\n",
            dropped.mutation.pattern, dropped.why
        ));
        out.push_str(&format!(
            "  Consequence claimed: {}\n",
            dropped.mutation.consequence
        ));
    }
    out.push('\n');

    out.push_str("## The clean code\n\n");
    for file in &stored.clean_files {
        out.push_str(&format!("### {}\n\n", file.path));
        out.push_str(&block("text", &numbered(&file.text)));
    }
    if let Some(suite) = &stored.suite {
        out.push_str("### The hidden suite\n\n");
        out.push_str(&block("text", suite));
    }

    out.push_str("## The clean review\n\n");
    if evidence.clean_review.repairs.is_empty() {
        out.push_str("It found nothing to repair.\n\n");
    }
    for repair in &evidence.clean_review.repairs {
        out.push_str(&format!(
            "- {:?}: {}\n",
            repair.severity, repair.description
        ));
    }
    out.push('\n');

    out.push_str("## The blind review\n\n");
    for (position, finding) in evidence.blind_review.findings.iter().enumerate() {
        out.push_str(&format!(
            "{}. {:?} in {}: {}\n   Quoted: {}\n",
            position + 1,
            finding.severity,
            finding.path,
            finding.comment,
            finding.quote.replace('\n', " ")
        ));
        if let Some((outcome, reason)) = evidence
            .blind_review
            .assessment
            .as_ref()
            .and_then(|assessment| assessment.findings.get(position))
        {
            out.push_str(&format!("   Judged: {outcome:?}. {reason}\n"));
        }
    }
    out.push('\n');
    for found in &evidence.blind_review.findability {
        out.push_str(&format!(
            "- {} ({}): the review {:?}. {}\n",
            found.issue, found.pattern, found.outcome, found.reason
        ));
    }

    out.push_str("\n## Models\n\n");
    let calls = models_of(stored);
    let mut distinct: Vec<&str> = calls.iter().map(|call| call.model.as_str()).collect();
    distinct.sort_unstable();
    distinct.dedup();
    out.push_str(&format!("Every call ran on: {}\n\n", distinct.join(", ")));
    for call in &calls {
        out.push_str(&format!("- {}: {}\n", call.purpose, call.model));
    }
    out
}

/// Why a build produced no drill, and what its stages had saved by then.
struct Failure {
    error: String,
    checkpoints: Value,
}

/// What the stages of a build saved.
async fn checkpoints(store: &JobStore, job: &str) -> Value {
    let mut saved = Map::new();
    for stage in Stage::ALL {
        if let Ok(Some(value)) = store.checkpoint(job, &stage.key()).await {
            saved.insert(stage.name().to_owned(), value);
        }
    }
    Value::Object(saved)
}

/// Builds the drill `spec` asks for, the way the build job does, and reads it back as stored. A build that makes no
/// drill is a failure with what its stages saved, not an error of the eval.
async fn build(
    llm: &dyn LLMPort,
    runtimes: &BuiltinRuntimes,
    pool: &SqlitePool,
    spec: &DrillSpec,
    seed: &str,
) -> Result<std::result::Result<StoredDrill, Failure>> {
    let job = bank::queue_build(pool, spec, seed).await?;
    let store = JobStore::new(pool.clone());
    if store.claim(&job).await?.is_none() {
        return Err(AppError::InternalError(
            "The build job was not claimed.".into(),
        ));
    }
    let context = JobContext::detached(&store, &job).await?;
    let runner = spec
        .material
        .language()
        .and_then(|language| runner_for(language, runtimes));
    let check = ClaimSourceCheck::new(llm);
    let material = material_for(&spec.material, runner.as_deref(), &check);
    let grader = JudgeGrader::new(llm);
    let outcome = DrillBuild::new(llm, material, &grader, pool, CancellationToken::new())
        .run(&context, spec)
        .await;
    match outcome {
        Ok(JobOutcome::Committed) => {
            let id = store
                .get(&job)
                .await?
                .result_ref
                .and_then(|reference| reference.strip_prefix("drill:").map(str::to_owned))
                .ok_or_else(|| AppError::InternalError("The build names no drill.".into()))?;
            let mut connection = pool.acquire().await.map_err(db)?;
            let stored = repository::find_in(&mut connection, &id)
                .await?
                .ok_or_else(|| AppError::InternalError("The stored drill is not there.".into()))?;
            Ok(Ok(stored))
        }
        Ok(other) => Ok(Err(Failure {
            error: format!("The build ended as {other:?}."),
            checkpoints: checkpoints(&store, &job).await,
        })),
        Err(error) => Ok(Err(Failure {
            error: error.to_string(),
            checkpoints: checkpoints(&store, &job).await,
        })),
    }
}

fn required(name: &str) -> Result<String> {
    std::env::var(name)
        .map_err(|_| AppError::InvalidConfig(format!("Set {name} for the drill eval.")))
}

fn number<T: std::str::FromStr>(name: &str, default: Option<T>) -> Result<T> {
    match (std::env::var(name), default) {
        (Ok(value), _) => value
            .trim()
            .parse()
            .map_err(|_| AppError::InvalidConfig(format!("{name} is not a number: {value}"))),
        (Err(_), Some(default)) => Ok(default),
        (Err(_), None) => Err(AppError::InvalidConfig(format!(
            "Set {name} for the drill eval."
        ))),
    }
}

/// The languages in a comma list, without blanks.
fn languages(list: &str) -> Vec<String> {
    list.split(',')
        .map(|language| language.trim().to_owned())
        .filter(|language| !language.is_empty())
        .collect()
}

fn write(directory: &Path, name: &str, text: &str) -> Result<()> {
    std::fs::write(directory.join(name), text)?;
    Ok(())
}

/// Builds `LATTICE_DRILL_EVAL_COUNT` drills for each language of the comma list `LATTICE_DRILL_EVAL_LANGUAGES`, of about
/// `LATTICE_DRILL_EVAL_LINES` lines (60 without it) and `LATTICE_DRILL_EVAL_PATTERNS` patterns (3 without it), and writes
/// a JSON and a Markdown file for each, and a summary, to the directory `LATTICE_DRILL_EVAL_OUT`.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_DRILL_EVAL_OUT, LATTICE_DRILL_EVAL_LANGUAGES and LATTICE_DRILL_EVAL_COUNT; makes real model requests"]
async fn live_build_eval() -> Result<()> {
    let out = PathBuf::from(required("LATTICE_DRILL_EVAL_OUT")?);
    let wanted = languages(&required("LATTICE_DRILL_EVAL_LANGUAGES")?);
    if wanted.is_empty() {
        return Err(AppError::InvalidConfig(
            "LATTICE_DRILL_EVAL_LANGUAGES names no language.".into(),
        ));
    }
    let count: usize = number("LATTICE_DRILL_EVAL_COUNT", None)?;
    let lines: u32 = number("LATTICE_DRILL_EVAL_LINES", Some(60))?;
    let patterns: usize = number("LATTICE_DRILL_EVAL_PATTERNS", Some(3))?;
    std::fs::create_dir_all(&out)?;

    let llm = model().await?;
    let described = json!({
        "name": llm.model_name(),
        "provider": llm.provider_name(),
        "context_window": llm.max_context_tokens(),
    });
    let runtimes = BuiltinRuntimes::for_tests();
    let pool = crate::features::learning::tests::pool().await?;
    let mut summary = Vec::new();
    for language in &wanted {
        // Each drill targets the patterns the earlier ones of its language left alone, so the eval covers the catalogue.
        let mut tried: Vec<PatternDue> = Vec::new();
        for index in 1..=count {
            let name = format!("{language}-{index:02}");
            let material = Material::code(language.clone());
            let spec = DrillSpec {
                domain: None,
                lines,
                patterns: targets(&material, &tried, now(), patterns)
                    .iter()
                    .map(|pattern| pattern.id.to_owned())
                    .collect(),
                material,
                program_id: None,
                lesson_id: None,
            };
            tried.extend(spec.patterns.iter().map(|pattern| PatternDue {
                pattern: pattern.clone(),
                due_at: Some(i64::MAX),
                pending: false,
            }));
            let started = Instant::now();
            let built = build(llm.as_ref(), &runtimes, &pool, &spec, &name).await?;
            let seconds = started.elapsed().as_secs_f64();
            match built {
                Ok(stored) => {
                    let counts = counts_of(&stored);
                    println!(
                        "{name}: built in {seconds:.0} s, planted {}, proven by test {}, by judge {}, dropped {}, found in review {}",
                        counts.planted,
                        counts.proven_by_test,
                        counts.proven_by_judge,
                        counts.dropped.len(),
                        counts.found_in_review
                    );
                    let shown = drill_json(&stored, &counts, seconds);
                    write(
                        &out,
                        &format!("{name}.json"),
                        &serde_json::to_string_pretty(&shown)?,
                    )?;
                    write(
                        &out,
                        &format!("{name}.md"),
                        &markdown(&stored, &counts, seconds),
                    )?;
                    summary.push(json!({"name": name, "built": true, "seconds": seconds, "model": stored.model, "counts": counts}));
                }
                Err(failure) => {
                    println!("{name}: no drill in {seconds:.0} s: {}", failure.error);
                    let shown = json!({
                        "name": name,
                        "spec": spec,
                        "seconds": seconds,
                        "error": failure.error,
                        "checkpoints": failure.checkpoints,
                    });
                    write(
                        &out,
                        &format!("{name}.json"),
                        &serde_json::to_string_pretty(&shown)?,
                    )?;
                    summary.push(json!({"name": name, "built": false, "seconds": seconds, "error": failure.error}));
                }
            }
        }
    }
    let report = json!({"model": described, "drills": summary});
    write(
        &out,
        "summary.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    println!("{described}: report in {}", out.display());
    Ok(())
}

/// A drill on a passage as a person reads it: the source passage, the explanation before and after the errors were
/// planted, each issue of the key with its edit and its proof, the repairs of the clean review, and the mutations
/// dropped and why.
fn passage_json(name: &str, stored: &StoredDrill, seconds: f64) -> Value {
    let issues: Vec<Value> = stored
        .drill
        .key
        .iter()
        .map(|issue| {
            json!({
                "id": issue.id,
                "pattern": issue.pattern,
                "severity": issue.severity,
                "origin": issue.origin,
                "span": issue.span,
                "clean": issue.fix.after,
                "planted": issue.fix.before,
                "consequence": issue.consequence,
                "explanation": issue.explanation,
                "proof": issue.proof,
            })
        })
        .collect();
    let dropped: Vec<Value> = stored
        .evidence
        .dropped
        .iter()
        .map(|dropped| {
            json!({
                "pattern": dropped.mutation.pattern,
                "clean": dropped.mutation.edit.before,
                "planted": dropped.mutation.edit.after,
                "why": dropped.why,
            })
        })
        .collect();
    let models: Vec<Value> = models_of(stored)
        .iter()
        .map(|call| json!({"purpose": call.purpose, "model": call.model}))
        .collect();
    json!({
        "name": name,
        "id": stored.drill.id,
        "title": stored.drill.title,
        "model": stored.model,
        "seconds": seconds,
        "spec": stored.spec,
        "source": stored.source,
        "clean_explanation": stored.clean_files.first().map(|file| file.text.as_str()),
        "explanation": stored.drill.files.first().map(|file| file.text.as_str()),
        "clean_review": stored.evidence.clean_review.repairs,
        "issues": issues,
        "dropped": dropped,
        "models": models,
    })
}

/// A copy of the app's database, its migrations brought up to this build's.
async fn copied_database(path: &str) -> Result<SqlitePool> {
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .foreign_keys(true)
        .create_if_missing(false);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(db)?;
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
        .run(&pool)
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    Ok(pool)
}

/// Builds `LATTICE_DRILL_EVAL_COUNT` drills on passages of the course `LATTICE_COURSE`, in the copy of the app's
/// database at `LATTICE_DB_COPY` (never the live one: the drills are stored in it, so a second run takes passages the
/// first did not), each an explanation of about `LATTICE_DRILL_EVAL_LINES` sentences (8 without it) with
/// `LATTICE_DRILL_EVAL_PATTERNS` patterns (3 without it). Writes a JSON file for each, and a summary, to the directory
/// `LATTICE_DRILL_EVAL_OUT`.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_DB_COPY (a copy of the app's database), LATTICE_COURSE, LATTICE_DRILL_EVAL_OUT and LATTICE_DRILL_EVAL_COUNT; makes real model requests"]
async fn live_passage_build_eval() -> Result<()> {
    let out = PathBuf::from(required("LATTICE_DRILL_EVAL_OUT")?);
    let database = required("LATTICE_DB_COPY")?;
    let course = required("LATTICE_COURSE")?;
    let count: usize = number("LATTICE_DRILL_EVAL_COUNT", None)?;
    let lines: u32 = number("LATTICE_DRILL_EVAL_LINES", Some(8))?;
    let patterns: usize = number("LATTICE_DRILL_EVAL_PATTERNS", Some(3))?;
    std::fs::create_dir_all(&out)?;

    let llm = model().await?;
    let described = json!({
        "name": llm.model_name(),
        "provider": llm.provider_name(),
        "context_window": llm.max_context_tokens(),
    });
    let runtimes = BuiltinRuntimes::for_tests();
    let pool = copied_database(&database).await?;
    let run = uuid::Uuid::new_v4();
    let mut tried: Vec<PatternDue> = Vec::new();
    let mut summary = Vec::new();
    for index in 1..=count {
        let name = format!("passage-{index:02}");
        let material = Material::Passage;
        let spec = DrillSpec {
            domain: None,
            lines,
            patterns: targets(&material, &tried, now(), patterns)
                .iter()
                .map(|pattern| pattern.id.to_owned())
                .collect(),
            material,
            program_id: Some(course.clone()),
            lesson_id: None,
        };
        tried.extend(spec.patterns.iter().map(|pattern| PatternDue {
            pattern: pattern.clone(),
            due_at: Some(i64::MAX),
            pending: false,
        }));
        let started = Instant::now();
        let seed = format!("{run}:{name}");
        let built = build(llm.as_ref(), &runtimes, &pool, &spec, &seed).await?;
        let seconds = started.elapsed().as_secs_f64();
        let shown = match built {
            Ok(stored) => {
                println!(
                    "{name}: built in {seconds:.0} s, {} issues, {} dropped",
                    stored.drill.key.len(),
                    stored.evidence.dropped.len()
                );
                summary.push(json!({"name": name, "built": true, "seconds": seconds, "model": stored.model, "issues": stored.drill.key.len(), "dropped": stored.evidence.dropped.len()}));
                passage_json(&name, &stored, seconds)
            }
            Err(failure) => {
                println!("{name}: no drill in {seconds:.0} s: {}", failure.error);
                summary.push(json!({"name": name, "built": false, "seconds": seconds, "error": failure.error}));
                json!({
                    "name": name,
                    "spec": spec,
                    "seconds": seconds,
                    "error": failure.error,
                    "checkpoints": failure.checkpoints,
                })
            }
        };
        write(
            &out,
            &format!("{name}.json"),
            &serde_json::to_string_pretty(&shown)?,
        )?;
    }
    let report = json!({"model": described, "course": course, "drills": summary});
    write(
        &out,
        "summary.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    println!("{described}: report in {}", out.display());
    Ok(())
}

async fn stored() -> Result<StoredDrill> {
    let pool = crate::features::learning::tests::pool().await?;
    stock(&pool, "drill").await?;
    let mut connection = pool.acquire().await.map_err(db)?;
    repository::find_in(&mut connection, "drill")
        .await?
        .ok_or_else(|| AppError::InternalError("The drill is not there.".into()))
}

#[tokio::test]
async fn the_counts_are_what_was_planted_proved_and_dropped() -> Result<()> {
    let mut drill = stored().await?;
    assert_eq!(
        counts_of(&drill),
        Counts {
            planted: 2,
            proven_by_test: 1,
            proven_by_judge: 1,
            dropped: Vec::new(),
            found_in_review: 0,
        }
    );

    let mut trail = Trail::default();
    trail.call("judge", MODEL);
    drill.evidence.dropped.push(DroppedMutation {
        mutation: literals_mutation(),
        why: "The original code already has this problem.".into(),
        trail,
    });
    let mut extra: KeyIssue = drill.drill.key[0].clone();
    extra.id = "review-1".into();
    extra.origin = Origin::FoundInReview;
    drill.drill.key.push(extra);
    assert_eq!(
        counts_of(&drill),
        Counts {
            planted: 3,
            proven_by_test: 1,
            proven_by_judge: 1,
            dropped: vec![Dropped {
                pattern: LITERALS.into(),
                why: "The original code already has this problem.".into()
            }],
            found_in_review: 1,
        }
    );
    Ok(())
}

#[tokio::test]
async fn the_markdown_holds_what_a_person_needs_to_judge_each_planted_issue() -> Result<()> {
    let drill = stored().await?;
    let counts = counts_of(&drill);
    let text = markdown(&drill, &counts, 12.4);

    assert!(text.starts_with("# A sliding-window rate limiter (python)"));
    assert!(text.contains("- Built by: scripted-drill-model"));
    assert!(text.contains("- Planted: 2"));
    assert!(text.contains("- Proven by a test: 1"));
    assert!(text.contains("- Proven by the judge: 1"));
    // The code as the learner reads it, with line numbers.
    assert!(text.contains(&format!("13 | {}", "        if len(recent) > self.limit:")));
    // Each issue with its pattern, severity, place, consequence and explanation.
    let name = pattern(OPERATOR).expect("a pattern").name;
    assert!(text.contains(&format!(
        "### 1. {name} ({OPERATOR}), Major, planted, src/limiter.py lines 13 to 13"
    )));
    assert!(text.contains("- Consequence: A caller gets one call more than the limit."));
    assert!(text.contains("- In an interview: The comparison lets one call too many through."));
    // How each was proven: the killing test whole, the judge's reason.
    assert!(text.contains(
        "assert not limiter.allow(\"a\"), \"a second call inside the window must be refused\""
    ));
    assert!(text.contains("Proven by the judge: The constants are no longer used."));
    // The fix as a diff.
    assert!(text
        .contains("-        if len(recent) > self.limit:\n+        if len(recent) >= self.limit:"));
    // The clean code, the suite and the models.
    assert!(text.contains("## The clean code"));
    assert!(text.contains(SUITE.lines().next().expect("a suite")));
    assert!(text.contains("## Dropped\n\nNothing was dropped."));
    assert!(text.contains("## Models"));
    Ok(())
}

#[tokio::test]
async fn the_markdown_says_why_a_defect_was_dropped_and_what_the_blind_review_made_of_the_code(
) -> Result<()> {
    let mut drill = stored().await?;
    drill.evidence.dropped.push(DroppedMutation {
        mutation: literals_mutation(),
        why: "The original code already has this problem.".into(),
        trail: Trail::default(),
    });
    drill.evidence.blind_review = {
        let mut blind: BlindReview = quiet();
        blind.trail.call("blind review", "another-model");
        Evidence::new(
            &code(),
            &review(),
            &planted(),
            &proved(),
            &unchecked(),
            &blind,
        )
        .blind_review
    };
    let counts = counts_of(&drill);
    let text = markdown(&drill, &counts, 1.0);
    assert!(text.contains(&format!(
        "- {LITERALS}: The original code already has this problem."
    )));
    assert!(text.contains("Every call ran on: another-model"));
    assert!(text.contains("- blind review: another-model"));
    Ok(())
}

#[tokio::test]
async fn the_json_holds_the_code_the_clean_code_the_key_the_proofs_the_evidence_and_the_models(
) -> Result<()> {
    let drill = stored().await?;
    let counts = counts_of(&drill);
    let shown = drill_json(&drill, &counts, 3.5);
    for field in [
        "id",
        "language",
        "title",
        "domain",
        "model",
        "runner",
        "seconds",
        "spec",
        "counts",
        "files",
        "clean_files",
        "suite",
        "key",
        "evidence",
        "models",
    ] {
        assert!(shown.get(field).is_some(), "{field}");
    }
    assert_eq!(shown["model"], MODEL);
    assert_eq!(shown["files"].as_array().map(Vec::len), Some(2));
    assert_eq!(shown["clean_files"].as_array().map(Vec::len), Some(2));
    assert_eq!(shown["key"][0]["proof"]["killed"]["test"], KILLING_TEST);
    assert_eq!(shown["counts"]["planted"], 2);
    assert_eq!(shown["suite"], SUITE);
    Ok(())
}

#[test]
fn a_fence_is_longer_than_any_run_of_backticks_inside_what_it_fences() {
    assert_eq!(fence("plain"), "```");
    assert_eq!(fence("a ``` b"), "````");
    assert_eq!(fence("`````"), "``````");
    let block = block("text", "x\n```\ny\n");
    assert!(block.starts_with("````text\nx\n```\ny\n````\n"));
}

#[test]
fn code_is_numbered_to_the_width_of_its_last_line() {
    let text = (1..=10).map(|n| format!("line {n}\n")).collect::<String>();
    let shown = numbered(&text);
    assert!(shown.starts_with(" 1 | line 1\n"));
    assert!(shown.ends_with("10 | line 10"));
}

#[test]
fn languages_come_from_a_comma_list_without_blanks() {
    assert_eq!(
        languages("python, javascript,,rust , "),
        ["python", "javascript", "rust"]
    );
    assert!(languages(" , ").is_empty());
}
