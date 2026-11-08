//! How real the planted issues of a drill are, and how well it is built on its lesson
//! (docs/design/2026-11-04-code-review-drills.md section 6, docs/design/2026-11-07-drills-in-the-course.md section 9).
//! Builds drills the way the app does, on a copy of its database, on the configured model with the real runner and the
//! judge that grades reviews. Each drill is on a lesson of a course: the courses in turn, and in each course the
//! lessons the app offers for the material, spread evenly over its list. Each is written as JSON and as Markdown for a
//! person to read. The code Markdown has the course, the lesson, the brief the model was shown, what was planted, what
//! a test and what the judge proved, what was dropped and why, the lesson sentences each lesson issue cites, the lesson
//! checks, and what the blind review found. The passage Markdown has the source passage, the explanation, every planted
//! edit with its proof, and what was dropped and why. Whether a planted issue is real, and whether a lesson link really
//! breaks the sentences it cites, is a person's judgement, so nothing here scores either. `summary.json` counts the
//! lessons drilled, those none of whose drills kept a lesson issue, and the lesson checks made and failed.
//!
//! Both evals read `LATTICE_DB_COPY` (a copy of the app's database: the drills are stored in it), `LATTICE_COURSES` (a
//! comma list of course ids), `LATTICE_DRILL_EVAL_COUNT` and `LATTICE_DRILL_EVAL_OUT`. `LATTICE_DRILL_EVAL_LINES` and
//! `LATTICE_DRILL_EVAL_PATTERNS` default to the app's Standard size: 80 lines and 4 patterns for code, 10 sentences and
//! 3 patterns for a passage. Code takes each course's own language, or `LATTICE_DRILL_EVAL_LANGUAGE` for every course.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{
    line_diff, pattern, targets, DiffKind, DrillFile, Edit, KeyIssue, LessonBrief, LineSpan,
    Material, Origin, PatternDue, Proof, Severity, SourceVerdict,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobOutcome, JobStore};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha384};
use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use super::support::{
    code, lesson_brief, literals_mutation, planted, proved, quiet, review, stock, unchecked,
    KILLING_TEST, LITERALS, MODEL, OPERATOR, SUITE,
};
use crate::features::learning::drills::bank;
use crate::features::learning::drills::build::{
    BlindReview, DrillBuild, DroppedMutation, Evidence, ModelCall, SourcePassage, Stage, Trail,
};
use crate::features::learning::drills::grading::JudgeGrader;
use crate::features::learning::drills::material::{material_for, ClaimSourceCheck};
use crate::features::learning::drills::repository::{self, StoredDrill};
use crate::features::learning::drills::review::course_drills;
use crate::features::learning::drills::runner::{runner_for, RunOutcome};
use crate::features::learning::drills::spec::DrillSpec;
use crate::features::learning::embedded_runtime::BuiltinRuntimes;
use crate::features::learning::persistence::{db, now};
use crate::features::learning::tests::grading::live_grading_eval::model;

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
    /// Issues of the key tied to the lesson's sentences.
    lesson_issues: usize,
    lesson_checks: usize,
    /// Lesson checks that did not find the change breaking the sentences and the clean code following them.
    lesson_checks_failed: usize,
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
        lesson_issues: stored
            .drill
            .key
            .iter()
            .filter(|issue| issue.lesson.is_some())
            .count(),
        lesson_checks: evidence.lesson_checks.len(),
        lesson_checks_failed: evidence
            .lesson_checks
            .iter()
            .filter(|check| !(check.breaks && check.follows))
            .count(),
    }
}

/// Over the drills built: the lessons drilled, those of them none of whose drills kept a lesson issue, and the lesson
/// checks made and failed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
struct LessonTotals {
    lessons: usize,
    lessons_with_no_lesson_issue: usize,
    lesson_checks: usize,
    lesson_checks_failed: usize,
}

/// The totals over the (lesson id, counts) of each drill built.
fn lesson_totals(built: &[(String, Counts)]) -> LessonTotals {
    let mut lessons: BTreeMap<&str, bool> = BTreeMap::new();
    for (lesson, counts) in built {
        let kept = lessons.entry(lesson.as_str()).or_insert(false);
        *kept |= counts.lesson_issues > 0;
    }
    LessonTotals {
        lessons: lessons.len(),
        lessons_with_no_lesson_issue: lessons.values().filter(|kept| !**kept).count(),
        lesson_checks: built.iter().map(|(_, counts)| counts.lesson_checks).sum(),
        lesson_checks_failed: built
            .iter()
            .map(|(_, counts)| counts.lesson_checks_failed)
            .sum(),
    }
}

/// The course and lesson of each of `count` drills: the courses in turn, and in each course lessons spread evenly
/// over its list, in order; a course with no lesson is left out, and one with fewer lessons than drills repeats them.
fn lesson_plan(courses: &[(String, Vec<String>)], count: usize) -> Vec<(String, String)> {
    let active: Vec<&(String, Vec<String>)> = courses
        .iter()
        .filter(|(_, lessons)| !lessons.is_empty())
        .collect();
    let courses_in_turn = active.len();
    if courses_in_turn == 0 {
        return Vec::new();
    }
    (0..count)
        .filter_map(|drill| {
            let position = drill % courses_in_turn;
            let (course, lessons) = active.get(position)?;
            let drills_of_course =
                count / courses_in_turn + usize::from(position < count % courses_in_turn);
            let index = (drill / courses_in_turn * lessons.len()).checked_div(drills_of_course)?;
            Some((course.clone(), lessons.get(index)?.clone()))
        })
        .collect()
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
        "lesson_id": stored.lesson_id,
        "lesson_title": stored.domain,
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
    if let Some(brief) = &evidence.brief {
        out.push_str(&format!("- Course: {}\n", brief.course));
    }
    out.push_str(&format!(
        "- Lesson: {}\n- Built by: {}\n- Runner: {}\n- Built in {seconds:.0} seconds\n- Patterns asked for: {}\n\n",
        stored.domain,
        stored.model,
        stored.runner.as_deref().unwrap_or("none, every defect was judged"),
        stored.spec.patterns.join(", ")
    ));
    out.push_str(&format!(
        "## Counts\n\n- Planted: {}\n- Proven by a test: {}\n- Proven by the judge: {}\n- Dropped: {}\n- Found in review: {}\n- Lesson issues: {}\n- Lesson checks: {}, failed: {}\n\n",
        counts.planted,
        counts.proven_by_test,
        counts.proven_by_judge,
        counts.dropped.len(),
        counts.found_in_review,
        counts.lesson_issues,
        counts.lesson_checks,
        counts.lesson_checks_failed
    ));
    out.push_str(&brief_section(evidence.brief.as_ref()));

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
        if let Some(link) = &issue.lesson {
            out.push_str("From the lesson:\n\n");
            for sentence in &link.sentences {
                for line in sentence.text.lines() {
                    out.push_str(&format!("> {line}\n"));
                }
                out.push_str(&format!(
                    "({})\n",
                    sentence
                        .source
                        .as_deref()
                        .unwrap_or("the lesson's own teaching")
                ));
            }
            out.push('\n');
        }
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

    out.push_str("## The lesson checks\n\n");
    if evidence.lesson_checks.is_empty() {
        out.push_str("No issue was a lesson issue.\n");
    }
    for check in &evidence.lesson_checks {
        let sentences: Vec<String> = check
            .sentences
            .iter()
            .map(|number| format!("L{number}"))
            .collect();
        let yes = |answer: bool| if answer { "yes" } else { "no" };
        out.push_str(&format!(
            "- {} ({}), {}: breaks them: {}, the clean code follows them: {}, {}. {}\n",
            check.issue,
            check.pattern,
            sentences.join(", "),
            yes(check.breaks),
            yes(check.follows),
            if check.breaks && check.follows {
                "passed"
            } else {
                "failed"
            },
            check.reason
        ));
    }
    out.push('\n');

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

    out.push('\n');
    out.push_str(&models_section(stored));
    out
}

/// The lesson brief a drill was built from, in a text fence, or a line saying it was built from none.
fn brief_section(brief: Option<&LessonBrief>) -> String {
    match brief {
        Some(brief) => format!("## The brief\n\n{}", block("text", &brief.render())),
        None => "## The brief\n\nThe drill was built from no lesson.\n\n".to_owned(),
    }
}

/// Every model call the build made: the distinct models, then each call.
fn models_section(stored: &StoredDrill) -> String {
    let calls = models_of(stored);
    let mut distinct: Vec<&str> = calls.iter().map(|call| call.model.as_str()).collect();
    distinct.sort_unstable();
    distinct.dedup();
    let mut out = format!(
        "## Models\n\nEvery call ran on: {}\n\n",
        distinct.join(", ")
    );
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

/// The course ids in a comma list, without blanks.
fn course_ids(list: &str) -> Vec<String> {
    list.split(',')
        .map(|course| course.trim().to_owned())
        .filter(|course| !course.is_empty())
        .collect()
}

fn write(directory: &Path, name: &str, text: &str) -> Result<()> {
    std::fs::write(directory.join(name), text)?;
    Ok(())
}

/// Which material an eval builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Code,
    Passage,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Passage => "passage",
        }
    }
}

/// A course of an eval: its title, the language its code drills are built in, and the lessons the material fits, each
/// with its title, in the order the app offers them.
struct EvalCourse {
    id: String,
    title: String,
    language: String,
    lessons: Vec<(String, String)>,
}

/// The courses `ids` as the app offers them for drills: each course's lessons that fit the material, and its language,
/// or `language` for every course when one is given.
async fn eval_courses(
    pool: &SqlitePool,
    ids: &[String],
    kind: Kind,
    language: Option<&str>,
) -> Result<Vec<EvalCourse>> {
    let mut courses = Vec::with_capacity(ids.len());
    for id in ids {
        let offered = course_drills(pool, id, language).await?;
        let title: String = sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(db)?;
        let lessons = offered
            .modules
            .iter()
            .flat_map(|module| module.lessons.iter())
            .filter(|lesson| match kind {
                Kind::Code => lesson.fits_code,
                Kind::Passage => lesson.fits_passage,
            })
            .map(|lesson| (lesson.lesson_id.clone(), lesson.title.clone()))
            .collect();
        courses.push(EvalCourse {
            id: id.clone(),
            title,
            language: offered.language,
            lessons,
        });
    }
    Ok(courses)
}

/// Builds `LATTICE_DRILL_EVAL_COUNT` drills of the material across the courses `LATTICE_COURSES` of the copy of the app's
/// database at `LATTICE_DB_COPY`, each on a lesson of a course (see [`lesson_plan`]), and writes a JSON and a Markdown
/// file for each, and a summary, to the directory `LATTICE_DRILL_EVAL_OUT`.
async fn build_eval(kind: Kind) -> Result<()> {
    let out = PathBuf::from(required("LATTICE_DRILL_EVAL_OUT")?);
    let database = required("LATTICE_DB_COPY")?;
    let ids = course_ids(&required("LATTICE_COURSES")?);
    if ids.is_empty() {
        return Err(AppError::InvalidConfig(
            "LATTICE_COURSES names no course.".into(),
        ));
    }
    let count: usize = number("LATTICE_DRILL_EVAL_COUNT", None)?;
    let (standard_lines, standard_patterns) = match kind {
        Kind::Code => (80, 4),
        Kind::Passage => (10, 3),
    };
    let lines: u32 = number("LATTICE_DRILL_EVAL_LINES", Some(standard_lines))?;
    let patterns: usize = number("LATTICE_DRILL_EVAL_PATTERNS", Some(standard_patterns))?;
    let language = match kind {
        Kind::Code => std::env::var("LATTICE_DRILL_EVAL_LANGUAGE")
            .ok()
            .map(|language| language.trim().to_owned())
            .filter(|language| !language.is_empty()),
        Kind::Passage => None,
    };
    std::fs::create_dir_all(&out)?;

    let llm = model().await?;
    let described = json!({
        "name": llm.model_name(),
        "provider": llm.provider_name(),
        "context_window": llm.max_context_tokens(),
    });
    let runtimes = crate::features::learning::tests::runtimes();
    let pool = copied_database(&database).await?;
    let courses = eval_courses(&pool, &ids, kind, language.as_deref()).await?;
    let lessons_of: Vec<(String, Vec<String>)> = courses
        .iter()
        .map(|course| {
            let lessons = course.lessons.iter().map(|(id, _)| id.clone());
            (course.id.clone(), lessons.collect())
        })
        .collect();
    let plan = lesson_plan(&lessons_of, count);
    if plan.is_empty() {
        return Err(AppError::InvalidConfig(format!(
            "No lesson of the courses fits {} drills.",
            kind.label()
        )));
    }

    let run = uuid::Uuid::new_v4();
    // Each drill targets the patterns the earlier ones of its material left alone, so the eval covers the catalogue.
    let mut tried: BTreeMap<String, Vec<PatternDue>> = BTreeMap::new();
    let mut built_counts: Vec<(String, Counts)> = Vec::new();
    let mut summary = Vec::new();
    for (position, (course_id, lesson_id)) in plan.iter().enumerate() {
        let Some(course) = courses.iter().find(|course| &course.id == course_id) else {
            continue;
        };
        let lesson_title = course
            .lessons
            .iter()
            .find(|(id, _)| id == lesson_id)
            .map(|(_, title)| title.clone())
            .unwrap_or_default();
        let name = format!("{}-{:02}", kind.label(), position + 1);
        let material = match kind {
            Kind::Code => Material::code(course.language.clone()),
            Kind::Passage => Material::Passage,
        };
        let tried_here = tried
            .entry(material.language().unwrap_or_default().to_owned())
            .or_default();
        let spec = DrillSpec {
            lines,
            patterns: targets(&material, tried_here.as_slice(), now(), patterns)
                .iter()
                .map(|pattern| pattern.id.to_owned())
                .collect(),
            material,
            program_id: Some(course_id.clone()),
            lesson_id: Some(lesson_id.clone()),
        };
        tried_here.extend(spec.patterns.iter().map(|pattern| PatternDue {
            pattern: pattern.clone(),
            due_at: Some(i64::MAX),
            pending: false,
        }));
        let started = Instant::now();
        let seed = format!("{run}:{name}");
        let built = build(llm.as_ref(), &runtimes, &pool, &spec, &seed).await?;
        let seconds = started.elapsed().as_secs_f64();
        let (shown, entry) = match built {
            Ok(stored) => {
                let (shown, text, counts) = match kind {
                    Kind::Code => {
                        let counts = counts_of(&stored);
                        println!(
                            "{name}: built in {seconds:.0} s, planted {}, proven by test {}, by judge {}, dropped {}, found in review {}, lesson issues {}, lesson checks failed {} of {}",
                            counts.planted,
                            counts.proven_by_test,
                            counts.proven_by_judge,
                            counts.dropped.len(),
                            counts.found_in_review,
                            counts.lesson_issues,
                            counts.lesson_checks_failed,
                            counts.lesson_checks
                        );
                        built_counts.push((lesson_id.clone(), counts.clone()));
                        (
                            drill_json(&stored, &counts, seconds),
                            markdown(&stored, &counts, seconds),
                            json!(counts),
                        )
                    }
                    Kind::Passage => {
                        println!(
                            "{name}: built in {seconds:.0} s, {} issues, {} dropped",
                            stored.drill.key.len(),
                            stored.evidence.dropped.len()
                        );
                        (
                            passage_json(&name, &stored, seconds),
                            passage_markdown(&name, &stored, seconds),
                            json!({"issues": stored.drill.key.len(), "dropped": stored.evidence.dropped.len()}),
                        )
                    }
                };
                write(&out, &format!("{name}.md"), &text)?;
                let entry = json!({
                    "name": name, "course": course_id, "lesson_id": lesson_id, "lesson_title": lesson_title,
                    "built": true, "seconds": seconds, "model": stored.model, "counts": counts,
                });
                (shown, entry)
            }
            Err(failure) => {
                println!("{name}: no drill in {seconds:.0} s: {}", failure.error);
                let shown = json!({
                    "name": name,
                    "course": course_id,
                    "lesson_id": lesson_id,
                    "lesson_title": lesson_title,
                    "spec": spec,
                    "seconds": seconds,
                    "error": failure.error,
                    "checkpoints": failure.checkpoints,
                });
                let entry = json!({
                    "name": name, "course": course_id, "lesson_id": lesson_id, "lesson_title": lesson_title,
                    "built": false, "seconds": seconds, "error": failure.error,
                });
                (shown, entry)
            }
        };
        write(
            &out,
            &format!("{name}.json"),
            &serde_json::to_string_pretty(&shown)?,
        )?;
        summary.push(entry);
    }
    let shown_courses: Vec<Value> = courses
        .iter()
        .map(|course| match kind {
            Kind::Code => {
                json!({"id": course.id, "title": course.title, "language": course.language})
            }
            Kind::Passage => json!({"id": course.id, "title": course.title}),
        })
        .collect();
    let report = json!({
        "model": described,
        "material": kind.label(),
        "courses": shown_courses,
        "drills": summary,
        "lessons": lesson_totals(&built_counts),
    });
    write(
        &out,
        "summary.json",
        &serde_json::to_string_pretty(&report)?,
    )?;
    println!("{described}: report in {}", out.display());
    Ok(())
}

/// Builds code drills across the courses of `LATTICE_COURSES`, in `LATTICE_DRILL_EVAL_LANGUAGE` when it is set and
/// otherwise in each course's own language, of about `LATTICE_DRILL_EVAL_LINES` lines (80 without it) and
/// `LATTICE_DRILL_EVAL_PATTERNS` patterns (4 without it).
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_DB_COPY (a copy of the app's database), LATTICE_COURSES, LATTICE_DRILL_EVAL_OUT and LATTICE_DRILL_EVAL_COUNT; makes real model requests"]
async fn live_build_eval() -> Result<()> {
    build_eval(Kind::Code).await
}

/// Builds drills on passages across the courses of `LATTICE_COURSES`, each an explanation of about
/// `LATTICE_DRILL_EVAL_LINES` sentences (10 without it) with `LATTICE_DRILL_EVAL_PATTERNS` patterns (3 without it). The
/// drills are stored in the copy of the database, so a second run takes passages the first did not.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_DB_COPY (a copy of the app's database), LATTICE_COURSES, LATTICE_DRILL_EVAL_OUT and LATTICE_DRILL_EVAL_COUNT; makes real model requests"]
async fn live_passage_build_eval() -> Result<()> {
    build_eval(Kind::Passage).await
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
        "lesson_id": stored.lesson_id,
        "lesson_title": stored.domain,
        "model": stored.model,
        "seconds": seconds,
        "spec": stored.spec,
        "brief": stored.evidence.brief,
        "source": stored.source,
        "clean_explanation": stored.clean_files.first().map(|file| file.text.as_str()),
        "explanation": stored.drill.files.first().map(|file| file.text.as_str()),
        "clean_review": stored.evidence.clean_review.repairs,
        "issues": issues,
        "dropped": dropped,
        "models": models,
    })
}

/// The passage drill for a person to read: the source, the explanation as the learner reviews it, each planted error
/// with its proof, what was dropped, the clean review and the models.
fn passage_markdown(name: &str, stored: &StoredDrill, seconds: f64) -> String {
    let evidence = &stored.evidence;
    let mut out = format!("# {} ({name})\n\n", stored.drill.title);
    if let Some(brief) = &evidence.brief {
        out.push_str(&format!("- Course: {}\n", brief.course));
    }
    out.push_str(&format!(
        "- Lesson: {}\n- Built by: {}\n- Built in {seconds:.0} seconds\n\n",
        stored.domain, stored.model
    ));
    out.push_str(&brief_section(evidence.brief.as_ref()));

    out.push_str("## The source passage\n\n");
    match &stored.source {
        Some(source) => {
            out.push_str(&format!("### {}\n\n", source.title));
            out.push_str(&block("text", &source.text));
        }
        None => out.push_str("No source passage was stored.\n\n"),
    }

    out.push_str("## The explanation as reviewed\n\n");
    for file in &stored.drill.files {
        out.push_str(&block("text", &numbered(&file.text)));
    }

    out.push_str("## The key\n\n");
    for (position, issue) in stored.drill.key.iter().enumerate() {
        let name = pattern(&issue.pattern).map_or(issue.pattern.as_str(), |known| known.name);
        out.push_str(&format!(
            "### {}. {name} ({}), {}, {:?}, sentences {} to {}\n\n",
            position + 1,
            issue.pattern,
            issue.id,
            issue.severity,
            issue.span.start,
            issue.span.end
        ));
        out.push_str(&format!(
            "- Planted: {}\n- Clean: {}\n- Consequence: {}\n- In an interview: {}\n\n",
            issue.fix.before.trim_end_matches('\n'),
            issue.fix.after.trim_end_matches('\n'),
            issue.consequence,
            issue.explanation
        ));
        match &issue.proof {
            Proof::Sourced { quote, reason, .. } => out.push_str(&format!(
                "Proven by the source, which says \"{quote}\": {reason}\n\n"
            )),
            Proof::Judged { reason } => {
                out.push_str(&format!("Proven by the judge: {reason}\n\n"));
            }
            Proof::Killed { test } => out.push_str(&block("text", test)),
        }
    }

    out.push_str("## Dropped\n\n");
    if evidence.dropped.is_empty() {
        out.push_str("Nothing was dropped.\n");
    }
    for dropped in &evidence.dropped {
        out.push_str(&format!(
            "- {}: {}\n  Planted: {}\n  Clean: {}\n",
            dropped.mutation.pattern,
            dropped.why,
            dropped.mutation.edit.after.trim().replace('\n', " "),
            dropped.mutation.edit.before.trim().replace('\n', " ")
        ));
    }

    out.push_str("\n## The clean review\n\n");
    if evidence.clean_review.repairs.is_empty() {
        out.push_str("It found nothing to repair.\n");
    }
    for repair in &evidence.clean_review.repairs {
        out.push_str(&format!(
            "- {:?}: {}\n",
            repair.severity, repair.description
        ));
    }
    out.push('\n');
    out.push_str(&models_section(stored));
    out
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

#[tokio::test]
async fn a_copy_migrated_with_the_other_line_endings_still_opens() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("app.sqlite");
    let path = path
        .to_str()
        .ok_or_else(|| AppError::InternalError("The path is not UTF-8.".into()))?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(db)?;
    crate::infrastructure::persistence::database::run_migrations(&pool).await?;
    pool.close().await;

    let migrator = sqlx::migrate!("./migrations");
    let migration = migrator
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .max_by_key(|migration| migration.version)
        .ok_or_else(|| AppError::InternalError("There is no migration.".into()))?;
    let lf = migration.sql.replace("\r\n", "\n");
    let crlf = lf.replace('\n', "\r\n");
    let other = [lf, crlf]
        .into_iter()
        .map(|sql| Sha384::digest(sql.as_bytes()).to_vec())
        .find(|checksum| checksum.as_slice() != &*migration.checksum)
        .ok_or_else(|| AppError::InternalError("Both line endings hash alike.".into()))?;
    assert_ne!(other.as_slice(), &*migration.checksum);

    let options = sqlx::sqlite::SqliteConnectOptions::new().filename(path);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(db)?;
    let changed = sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
        .bind(other)
        .bind(migration.version)
        .execute(&pool)
        .await
        .map_err(db)?
        .rows_affected();
    assert_eq!(changed, 1);
    pool.close().await;

    let pool = copied_database(path).await?;
    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = ?")
            .bind(migration.version)
            .fetch_one(&pool)
            .await
            .map_err(db)?;
    assert_eq!(stored.as_slice(), &*migration.checksum);
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
            lesson_issues: 0,
            lesson_checks: 0,
            lesson_checks_failed: 0,
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
            lesson_issues: 0,
            lesson_checks: 0,
            lesson_checks_failed: 0,
        }
    );
    Ok(())
}

/// The lesson checks of [`lesson_drill`]: the first passes, the second does not break its sentence.
fn lesson_checks_json() -> Value {
    json!([
        {"issue": "planted-1", "pattern": OPERATOR, "sentences": [1, 3], "breaks": true, "follows": true,
         "reason": "The edit lets one call more through.", "trail": {"calls": [], "runs": []}},
        {"issue": "planted-2", "pattern": LITERALS, "sentences": [2], "breaks": false, "follows": true,
         "reason": "The literals are still named.", "trail": {"calls": [], "runs": []}}
    ])
}

/// The stored drill with the brief of a lesson, a link on its first issue and two lesson checks, one failing.
async fn lesson_drill() -> Result<StoredDrill> {
    let mut drill = stored().await?;
    let brief = lesson_brief();
    drill.domain = brief.lesson.clone();
    drill.drill.key[0].lesson = brief.link(&[1, 3]);
    drill.evidence.lesson_checks = serde_json::from_value(lesson_checks_json())?;
    drill.evidence.brief = Some(brief);
    Ok(drill)
}

#[tokio::test]
async fn the_counts_hold_the_lesson_issues_and_the_lesson_checks_that_failed() -> Result<()> {
    let mut drill = lesson_drill().await?;
    let counts = counts_of(&drill);
    assert_eq!(
        (
            counts.lesson_issues,
            counts.lesson_checks,
            counts.lesson_checks_failed
        ),
        (1, 2, 1)
    );
    assert_eq!(counts.planted, 2);

    // A check whose clean code does not follow the sentences has failed too.
    let mut checks = lesson_checks_json();
    checks[0]["follows"] = json!(false);
    drill.evidence.lesson_checks = serde_json::from_value(checks)?;
    drill.drill.key[1].lesson = drill.drill.key[0].lesson.clone();
    let counts = counts_of(&drill);
    assert_eq!(
        (
            counts.lesson_issues,
            counts.lesson_checks,
            counts.lesson_checks_failed
        ),
        (2, 2, 2)
    );
    Ok(())
}

#[test]
fn the_lesson_totals_count_the_lessons_drilled_those_with_no_lesson_issue_and_the_failed_checks() {
    let counts = |lesson_issues, lesson_checks, lesson_checks_failed| Counts {
        planted: 3,
        proven_by_test: 2,
        proven_by_judge: 1,
        dropped: Vec::new(),
        found_in_review: 0,
        lesson_issues,
        lesson_checks,
        lesson_checks_failed,
    };
    let built = vec![
        ("lesson-a".to_owned(), counts(1, 1, 0)),
        ("lesson-a".to_owned(), counts(0, 2, 1)),
        ("lesson-b".to_owned(), counts(0, 1, 1)),
        ("lesson-c".to_owned(), counts(2, 2, 0)),
        ("lesson-d".to_owned(), counts(0, 0, 0)),
    ];
    assert_eq!(
        lesson_totals(&built),
        LessonTotals {
            lessons: 4,
            lessons_with_no_lesson_issue: 2,
            lesson_checks: 6,
            lesson_checks_failed: 2,
        }
    );
    assert_eq!(lesson_totals(&[]), LessonTotals::default());
}

#[tokio::test]
async fn the_markdown_shows_the_course_the_lesson_its_brief_each_issues_lesson_words_and_the_lesson_checks(
) -> Result<()> {
    let drill = lesson_drill().await?;
    let counts = counts_of(&drill);
    let text = markdown(&drill, &counts, 12.4);

    assert!(!text.contains("- Domain:"));
    assert!(text.contains(
        "- Course: Rate limiting in services\n- Lesson: A sliding-window limiter\n- Built by: scripted-drill-model"
    ));
    assert!(
        text.contains("- Found in review: 0\n- Lesson issues: 1\n- Lesson checks: 2, failed: 1\n")
    );
    // The brief, as the model was shown it, in a text fence.
    let brief = lesson_brief().render();
    assert!(text.contains(&format!("## The brief\n\n```text\n{brief}\n```\n")));
    // The sentences the first issue cites, each with its source or as the lesson's own teaching.
    assert!(text.contains(
        "From the lesson:\n\n> A limiter refuses a call once the calls in its window reach the limit.\n(the lesson's own teaching)\n"
    ));
    assert!(text.contains(
        "> A sliding window forgets the calls older than its length.\n(Rate limiting notes)\n"
    ));
    let key = text
        .split_once("## The key")
        .map(|(_, key)| key)
        .expect("the markdown has a key section");
    assert_eq!(key.matches("From the lesson:").count(), 1);
    // Each check: the issue, the pattern, the sentences, both answers, whether it passed, and why.
    assert!(text.contains("## The lesson checks\n\n"));
    assert!(text.contains(&format!(
        "- planted-1 ({OPERATOR}), L1, L3: breaks them: yes, the clean code follows them: yes, passed. The edit lets one call more through.\n"
    )));
    assert!(text.contains(&format!(
        "- planted-2 ({LITERALS}), L2: breaks them: no, the clean code follows them: yes, failed. The literals are still named.\n"
    )));
    Ok(())
}

/// The stored drill as a drill on a passage: two sentences, each with a planted error and a proof from the source.
async fn passage_drill() -> Result<StoredDrill> {
    let mut drill = lesson_drill().await?;
    let path = "explanation.md";
    let clean = [
        "Spreading study over sessions improved recall by 14% a week later.",
        "The benefit was smaller for material learners already knew well.",
    ];
    let planted = [
        "Spreading study over sessions improved recall by 40% a week later.",
        "The benefit was larger for material learners already knew well.",
    ];
    let file = |lines: &[&str; 2]| {
        vec![DrillFile {
            path: path.into(),
            text: format!("{}\n{}\n", lines[0], lines[1]),
        }]
    };
    drill.drill.material = Material::Passage;
    drill.drill.title = "Why spacing study sessions helps".into();
    drill.drill.files = file(&planted);
    drill.clean_files = file(&clean);
    drill.suite = None;
    drill.runner = None;
    drill.source = Some(SourcePassage {
        version_id: "spacing-v1".into(),
        start_byte: 0,
        end_byte: 27,
        title: "Spacing and memory".into(),
        text: "Learners recalled 14% more.".into(),
    });
    let issues = [
        (
            "planted-1",
            "fidelity.misquoted_figure",
            Severity::Major,
            "recalled 14% more a week later",
            "The source gives 14%, not 40%.",
        ),
        (
            "planted-2",
            "reasoning.reversed_relation",
            Severity::Minor,
            "It was smaller for material that was already well known.",
            "The source says smaller, not larger.",
        ),
    ];
    for (position, (id, pattern, severity, quote, reason)) in issues.into_iter().enumerate() {
        let issue = &mut drill.drill.key[position];
        issue.id = id.into();
        issue.pattern = pattern.into();
        issue.severity = severity;
        issue.lesson = None;
        issue.span = LineSpan {
            path: path.into(),
            start: position as u32 + 1,
            end: position as u32 + 1,
        };
        issue.fix = Edit {
            path: path.into(),
            before: format!("{}\n", planted[position]),
            after: format!("{}\n", clean[position]),
        };
        issue.consequence = format!("Consequence {position}.");
        issue.explanation = format!("Explanation {position}.");
        issue.proof = Proof::Sourced {
            quote: quote.into(),
            planted: SourceVerdict::Contradicted,
            reason: reason.into(),
        };
    }
    drill.evidence.dropped.push(DroppedMutation {
        mutation: literals_mutation(),
        why: "The source supports the planted sentence.".into(),
        trail: Trail::default(),
    });
    drill.evidence.clean_review.repairs = serde_json::from_value(json!([
        {"description": "A sentence said more than the source.", "severity": "minor",
         "edit": {"path": path, "before": "a", "after": "b"}}
    ]))?;
    drill.evidence.plant.call("plant", "another-model");
    Ok(drill)
}

#[tokio::test]
async fn the_passage_markdown_shows_the_source_the_explanation_and_each_planted_error_with_its_proof(
) -> Result<()> {
    let drill = passage_drill().await?;
    let text = passage_markdown("passage-03", &drill, 7.6);

    assert!(text.starts_with("# Why spacing study sessions helps (passage-03)\n\n"));
    assert!(text.contains(
        "- Course: Rate limiting in services\n- Lesson: A sliding-window limiter\n- Built by: scripted-drill-model\n- Built in 8 seconds\n"
    ));
    assert!(text.contains("## The brief\n\n```text\n<brief>"));
    // The source passage, then the explanation as the learner reviews it, numbered.
    assert!(text.contains(
        "## The source passage\n\n### Spacing and memory\n\n```text\nLearners recalled 14% more.\n```\n"
    ));
    assert!(text.contains(
        "## The explanation as reviewed\n\n```text\n1 | Spreading study over sessions improved recall by 40% a week later.\n2 | The benefit was larger for material learners already knew well.\n```\n"
    ));
    // Each issue: pattern, id, severity, sentences, planted and clean text, consequence, explanation and proof.
    let figure = pattern("fidelity.misquoted_figure")
        .expect("a pattern")
        .name;
    assert!(text.contains(&format!(
        "### 1. {figure} (fidelity.misquoted_figure), planted-1, Major, sentences 1 to 1\n\n"
    )));
    assert!(text.contains(
        "- Planted: Spreading study over sessions improved recall by 40% a week later.\n"
    ));
    assert!(text
        .contains("- Clean: Spreading study over sessions improved recall by 14% a week later.\n"));
    assert!(text.contains("- Consequence: Consequence 0.\n- In an interview: Explanation 0.\n"));
    assert!(text.contains(
        "Proven by the source, which says \"recalled 14% more a week later\": The source gives 14%, not 40%."
    ));
    assert!(text.contains("reasoning.reversed_relation), planted-2, Minor, sentences 2 to 2"));
    assert!(text
        .contains("- Planted: The benefit was larger for material learners already knew well.\n"));
    // What was dropped and why, the clean review and the models.
    assert!(text.contains(&format!(
        "- {LITERALS}: The source supports the planted sentence."
    )));
    assert!(text.contains("- Minor: A sentence said more than the source."));
    assert!(text.contains("Every call ran on: another-model"));
    assert!(!text.contains("The hidden suite"));
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
    assert!(text.contains("- Lesson: rate limiting\n"));
    assert!(text.contains("The drill was built from no lesson."));
    assert!(text.contains("No issue was a lesson issue."));
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
        "lesson_id",
        "lesson_title",
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
fn courses_come_from_a_comma_list_without_blanks() {
    assert_eq!(
        course_ids("course-1, course-2,,course-3 , "),
        ["course-1", "course-2", "course-3"]
    );
    assert!(course_ids(" , ").is_empty());
}

fn lessons(prefix: &str, count: usize) -> Vec<String> {
    (1..=count).map(|n| format!("{prefix}{n}")).collect()
}

fn pairs(plan: &[(&str, &str)]) -> Vec<(String, String)> {
    plan.iter()
        .map(|(course, lesson)| ((*course).to_owned(), (*lesson).to_owned()))
        .collect()
}

#[test]
fn lesson_plan_takes_the_courses_in_turn_and_spreads_each_courses_drills_over_its_lessons() {
    let courses = vec![
        ("A".to_owned(), lessons("a", 6)),
        ("B".to_owned(), lessons("b", 3)),
    ];
    assert_eq!(
        lesson_plan(&courses, 6),
        pairs(&[
            ("A", "a1"),
            ("B", "b1"),
            ("A", "a3"),
            ("B", "b2"),
            ("A", "a5"),
            ("B", "b3")
        ])
    );
}

#[test]
fn lesson_plan_repeats_a_courses_lessons_when_it_has_fewer_than_its_drills_and_leaves_out_a_course_with_none(
) {
    let courses = vec![
        ("A".to_owned(), lessons("a", 2)),
        ("B".to_owned(), Vec::new()),
        ("C".to_owned(), lessons("c", 1)),
    ];
    assert_eq!(
        lesson_plan(&courses, 5),
        pairs(&[
            ("A", "a1"),
            ("C", "c1"),
            ("A", "a1"),
            ("C", "c1"),
            ("A", "a2")
        ])
    );
}

#[test]
fn lesson_plan_is_empty_when_no_course_has_a_lesson() {
    assert!(lesson_plan(&[], 4).is_empty());
    assert!(lesson_plan(&[("A".to_owned(), Vec::new())], 4).is_empty());
}
