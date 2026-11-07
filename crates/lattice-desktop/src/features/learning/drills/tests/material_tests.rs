//! Code as a drill's material: every text that reaches a model for a code drill is, byte for byte, the text the
//! stages carried before the material was a Strategy. The texts of that time are kept in `tests/golden`, taken from
//! the stages as they were, except the clean code's system text, which is the one the writer has had since drills
//! were built from a lesson; a spec stored before passages is still read.

use super::support::{
    baseline, build, code, code_spec, idle_grader, last, review_answer, spec, system, FakeRunner,
    ScriptedModel, OPERATOR,
};
use crate::features::learning::drills::build::ask::render_files;
use crate::features::learning::drills::build::Baseline;
use crate::features::learning::drills::material::{CodeMaterial, DrillMaterial, JudgeWording};
use crate::features::learning::drills::runner::CodeRunner;
use crate::features::learning::drills::spec::DrillSpec;
use crate::features::learning::tests::pool;
use lattice_core::domain::learning::drills::{Material, Pattern};
use lattice_core::shared::error::Result;
use serde_json::json;

const CLEAN_SYSTEM: &str = include_str!("golden/clean_system.txt");
const CLEAN_RUNNER: &str = include_str!("golden/clean_runner.txt");
const CLEAN_TESTS_RUNNER: &str = include_str!("golden/clean_tests_runner.txt");
const CLEAN_TESTS: &str = include_str!("golden/clean_tests.txt");
const REVIEW_SYSTEM: &str = include_str!("golden/review_system.txt");
const PLANT_SYSTEM: &str = include_str!("golden/plant_system.txt");
const PLANT_TESTING: &str = include_str!("golden/plant_testing.txt");
const PLANT_RUNNER: &str = include_str!("golden/plant_runner.txt");
const REVIEWER: &str = include_str!("golden/reviewer.txt");
const DESCRIBER: &str = include_str!("golden/describer.txt");
const JUDGE_SYSTEM: &str = include_str!("golden/judge_system.txt");
const JUDGE_BRIEF: &str = include_str!("golden/judge_brief.txt");

/// A golden text as the stages carried it: a checkout may have turned its line breaks into `\r\n`.
fn golden(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn spec_of(patterns: &[&str]) -> DrillSpec {
    DrillSpec {
        patterns: patterns.iter().map(|id| (*id).to_owned()).collect(),
        ..spec()
    }
}

fn targets_of(spec: &DrillSpec) -> Vec<&'static Pattern> {
    spec.targets().expect("the patterns are in the catalogue")
}

/// The first request the clean code stage made for a spec of a new course's spacing lesson, with `patterns` when
/// there are any. The model's script is empty, so the stage fails after it; the failure is not what is looked at.
async fn first_request_of_clean_code(
    patterns: &[&str],
    runner: Option<&FakeRunner>,
) -> Result<(String, String)> {
    let pool = pool().await?;
    let mut spec = code_spec(&pool).await?;
    if !patterns.is_empty() {
        spec.patterns = patterns.iter().map(|id| (*id).to_owned()).collect();
    }
    let model = ScriptedModel::new(["{}".to_owned()]);
    let grader = idle_grader();
    let _ = build(&pool, &model, runner, &grader)
        .clean_code(&spec)
        .await;
    let request = model.requests().remove(0);
    Ok((system(&request), last(&request)))
}

#[tokio::test]
async fn the_clean_code_request_for_code_without_a_runner_is_what_it_was() -> Result<()> {
    let (system, user) = first_request_of_clean_code(&[], None).await?;
    assert_eq!(system, golden(CLEAN_SYSTEM));
    assert!(
        user.starts_with(
            "Language: python\nSize: about 40 lines of code in all\n\n<brief>\nCourse: "
        ),
        "{user}"
    );
    assert!(user.ends_with("\n</brief>"), "{user}");
    Ok(())
}

#[tokio::test]
async fn the_clean_code_request_with_a_runner_names_its_contract_and_the_suite_it_asks_for(
) -> Result<()> {
    let runner = FakeRunner::passing();
    let (system, _) = first_request_of_clean_code(&[], Some(&runner)).await?;
    let runs = golden(CLEAN_RUNNER)
        .replace("{entrypoint}", runner.entrypoint())
        .replace("{extension}", "py");
    assert_eq!(
        system,
        format!("{}\n\n{} {runs}", golden(CLEAN_SYSTEM), runner.contract())
    );
    Ok(())
}

#[tokio::test]
async fn the_clean_code_request_for_a_testing_pattern_asks_for_a_test_file() -> Result<()> {
    let testing = ["testing.untested_branch"];
    let (system, _) = first_request_of_clean_code(&testing, None).await?;
    assert_eq!(
        system,
        format!("{}\n\n{}", golden(CLEAN_SYSTEM), golden(CLEAN_TESTS))
    );

    let runner = FakeRunner::passing();
    let (system, _) = first_request_of_clean_code(&testing, Some(&runner)).await?;
    let runs = golden(CLEAN_RUNNER)
        .replace("{entrypoint}", runner.entrypoint())
        .replace("{extension}", "py");
    assert_eq!(
        system,
        format!(
            "{}\n\n{} {runs}\n\n{}",
            golden(CLEAN_SYSTEM),
            runner.contract(),
            golden(CLEAN_TESTS_RUNNER)
        )
    );
    Ok(())
}

#[tokio::test]
async fn the_clean_review_request_is_what_it_was() -> Result<()> {
    let pool = pool().await?;
    let model = ScriptedModel::new([review_answer(vec![])]);
    let grader = idle_grader();
    let code = code();
    let reviewed = build(&pool, &model, None, &grader)
        .clean_review(&spec(), &code)
        .await?;
    assert!(reviewed.repairs.is_empty());
    let request = model.requests().remove(0);
    assert_eq!(system(&request), golden(REVIEW_SYSTEM));
    assert_eq!(
        last(&request),
        format!("Language: python\n\n{}", render_files(&code.files))
    );
    Ok(())
}

/// A baseline with a suite, test files and the code of the fixtures.
fn full_baseline() -> Baseline {
    Baseline {
        tests: vec!["tests/test_limiter.py".to_owned()],
        ..baseline()
    }
}

#[test]
fn the_plant_request_without_a_runner_is_what_it_was() {
    let spec = spec();
    let targets = targets_of(&spec);
    let baseline = Baseline {
        suite: None,
        ..baseline()
    };
    let wording = CodeMaterial::new(None).plant_wording(&spec, &baseline, &targets);
    assert_eq!(wording.system, golden(PLANT_SYSTEM));
    assert!(!wording.asks_verification);
    let mut expected = format!(
        "Language: python\n\nThe code:\n\n{}\n\nThe patterns, one change for each:\n",
        render_files(&baseline.files)
    );
    for target in &targets {
        expected.push_str(&format!(
            "- {} ({}, family {}): {}\n",
            target.id,
            target.name,
            target.family.slug(),
            target.definition
        ));
    }
    assert_eq!(wording.prompt, expected);
}

#[test]
fn the_plant_request_with_a_runner_shows_the_suite_and_the_test_files() {
    let runner = FakeRunner::passing();
    let spec = spec_of(&["testing.untested_branch", OPERATOR]);
    let targets = targets_of(&spec);
    let baseline = full_baseline();
    let suite = baseline.suite.clone().expect("the fixtures have a suite");
    let wording = CodeMaterial::new(Some(&runner)).plant_wording(&spec, &baseline, &targets);
    assert_eq!(
        wording.system,
        format!(
            "{}\n\n{}\n\n{}\n\n{}",
            golden(PLANT_SYSTEM),
            golden(PLANT_TESTING),
            runner.contract(),
            golden(PLANT_RUNNER).replace("{entrypoint}", runner.entrypoint())
        )
    );
    assert!(wording.asks_verification);
    let mut expected = format!(
        "Language: python\n\nThe code:\n\n{}\n\nThe hidden suite, which passes on the code:\n<suite>\n{suite}\n</suite>\n\nThese files are tests, part of the code under review: tests/test_limiter.py.\n\nThe patterns, one change for each:\n",
        render_files(&baseline.files)
    );
    for target in &targets {
        expected.push_str(&format!(
            "- {} ({}, family {}): {}\n",
            target.id,
            target.name,
            target.family.slug(),
            target.definition
        ));
    }
    assert_eq!(wording.prompt, expected);
}

#[test]
fn the_reviewer_and_the_describer_are_what_they_were() {
    let wording = CodeMaterial::new(None).reviewer_wording(&spec());
    assert_eq!(wording.reviewer, golden(REVIEWER));
    assert_eq!(wording.describer, golden(DESCRIBER));
    assert_eq!(wording.subject, "Language: python");
    assert_eq!(wording.files, "The code:");
    assert_eq!(CodeMaterial::new(None).reviewer_context(&baseline()), None);
}

#[test]
fn the_judge_is_what_it_was() -> Result<()> {
    let wording = JudgeWording::of(&Material::code("python"))?;
    assert_eq!(wording.system, golden(JUDGE_SYSTEM));
    assert_eq!(wording.brief, golden(JUDGE_BRIEF));
    assert_eq!(wording.kind, "python");
    assert_eq!(
        wording.reviewed,
        "The code as the reviewer saw it, with line numbers:"
    );
    assert_eq!(
        wording.original,
        "The same code before any issue was planted, with its own line numbers:"
    );
    assert_eq!((wording.line, wording.lines), ("line", "lines"));
    assert_eq!(wording.source, None);
    Ok(())
}

#[test]
fn code_material_runs_what_it_was_given() {
    assert!(CodeMaterial::new(None).runner().is_none());
    let runner = FakeRunner::passing();
    let material = CodeMaterial::new(Some(&runner));
    assert_eq!(
        material.runner().map(|runner| runner.name()),
        Some("fake-python")
    );
}

#[test]
fn a_passage_is_built_from_a_course_and_judged_in_sentences() {
    let wording = JudgeWording::of(&Material::Passage).expect("a passage has a judge");
    assert_eq!((wording.line, wording.lines), ("sentence", "sentences"));
    let without_course = DrillSpec {
        material: Material::Passage,
        patterns: vec!["fidelity.misquoted_figure".into()],
        ..spec()
    };
    assert!(without_course.validate().is_err());
    let on_a_course = DrillSpec {
        program_id: Some("course".into()),
        ..without_course
    };
    assert!(on_a_course.validate().is_ok());
    let with_code_patterns = DrillSpec {
        patterns: spec().patterns,
        ..on_a_course
    };
    assert!(with_code_patterns.validate().is_err());
}

#[test]
fn a_spec_stored_with_a_language_is_read_as_code_in_it() {
    let stored = json!({
        "language": "python",
        "domain": "rate limiting",
        "lines": 40,
        "patterns": [OPERATOR],
        "programId": null
    });
    let read: DrillSpec = serde_json::from_value(stored).unwrap();
    assert_eq!(read.material, Material::code("python"));
    assert_eq!(read.domain.as_deref(), Some("rate limiting"));
    assert_eq!(read.lines, 40);
    assert_eq!(read.patterns, [OPERATOR]);
}

#[test]
fn a_spec_is_written_with_its_material_and_read_back() {
    let written = serde_json::to_value(spec()).unwrap();
    assert_eq!(
        written["material"],
        json!({"kind": "code", "language": "python"})
    );
    assert!(written.get("language").is_none());
    let read: DrillSpec = serde_json::from_value(written).unwrap();
    assert_eq!(read, spec());
    assert!(serde_json::from_value::<DrillSpec>(json!({
        "domain": null, "lines": 1, "patterns": [], "programId": null
    }))
    .is_err());
}
