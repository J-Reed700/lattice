//! Eval 1: problem builds (design section 9, evals/coding/README.md). Builds problems through the app's own source
//! on the configured model, one catalogue pattern each in order, and writes each build, its proof and its Markdown for
//! a person to read. The code checks are the design's: no statement or title names its pattern, and at least
//! `MIN_KILLED_MISREADINGS` misreadings are killed. Statement clarity, Easy, and the pattern tags are read by hand.

use std::collections::BTreeMap;
use std::time::Instant;

use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::problem_patterns::{
    named_in, problem_pattern, PROBLEM_PATTERNS,
};
use lattice_core::domain::learning::practice::work::KindId;
use lattice_core::shared::error::Result;
use lattice_learning::practice::coding::jobs::EASY_PATTERNS;
use lattice_learning::practice::coding::problem::{
    CodingTask, Problem, ProblemCase, ProblemDifficulty,
};
use lattice_learning::practice::coding::proof::MIN_KILLED_MISREADINGS;
use lattice_learning::practice::coding::source::{build_problem, ProblemRequest};
use lattice_learning::practice::kinds::program::KIND;
use lattice_learning::practice::session::builder::BuildSteps;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::{
    count, fenced, numbered, out_dir, percentiles, write_item, EvalLab, ModelCall, Percentiles,
};

/// Reports nothing and never stops the build.
struct Silent;

#[async_trait]
impl BuildSteps for Silent {
    async fn step(&self, _current: u32, _message: &str) -> Result<bool> {
        Ok(true)
    }
}

/// One build: what was asked, how it ended, the checks, and the problem when it was proven.
#[derive(Debug, Serialize)]
struct BuildItem {
    model: String,
    name: String,
    requested: Vec<String>,
    proven: bool,
    error: Option<String>,
    wall_ms: u64,
    model_calls: Vec<ModelCall>,
    /// The id of the tagged pattern the statement names, if any.
    statement_names: Option<String>,
    /// The id of the tagged pattern the title names, if any.
    title_names: Option<String>,
    killed_misreadings: Option<usize>,
    enough_killed: Option<bool>,
    problem: Option<Problem>,
}

/// How the builds of one pattern ended.
#[derive(Debug, Default, Serialize)]
struct PatternTally {
    builds: Vec<String>,
    proven: usize,
    errors: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Summary {
    model: String,
    builds: usize,
    proven: usize,
    wall_ms: Percentiles,
    model_calls_per_build: Percentiles,
    statements_naming_a_pattern: Vec<String>,
    titles_naming_a_pattern: Vec<String>,
    builds_killing_too_few_misreadings: Vec<String>,
    by_pattern: BTreeMap<String, PatternTally>,
}

/// A table cell: one line, with the bar escaped.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

/// The call of a case, as `function(args)`.
fn call_of(problem: &Problem, case: &ProblemCase) -> String {
    format!(
        "{}({})",
        problem.spec.signature.function,
        case.args.join(", ")
    )
}

/// The build for a person to read.
fn markdown(item: &BuildItem) -> String {
    let mut out = format!("# Build {}\n\n", item.name);
    out.push_str(&format!(
        "Model: {}\n\nRequested: {}\n\nWall: {} ms, {} model calls\n\n",
        item.model,
        item.requested.join(", "),
        item.wall_ms,
        item.model_calls.len()
    ));
    let Some(problem) = &item.problem else {
        out.push_str(&format!(
            "Not proven.\n\n{}\n",
            item.error.as_deref().unwrap_or_default()
        ));
        return out;
    };
    let spec = &problem.spec;
    if let Some(id) = &item.statement_names {
        out.push_str(&format!("**The statement names the pattern {id}.**\n\n"));
    }
    if let Some(id) = &item.title_names {
        out.push_str(&format!("**The title names the pattern {id}.**\n\n"));
    }
    if item.enough_killed == Some(false) {
        out.push_str(&format!(
            "**Only {} misreadings are killed; {MIN_KILLED_MISREADINGS} are needed.**\n\n",
            item.killed_misreadings.unwrap_or_default()
        ));
    }
    out.push_str(&format!("## {}\n\n{}\n\n", spec.title, spec.statement));
    let params: Vec<String> = spec
        .signature
        .params
        .iter()
        .map(|param| format!("{}: {}", param.name, param.annotation))
        .collect();
    out.push_str(&format!(
        "Signature: `def {}({}) -> {}`\n\nDifficulty: {:?}\n\nTarget complexity: {}\n\n",
        spec.signature.function,
        params.join(", "),
        spec.signature.returns,
        spec.difficulty,
        spec.target_complexity
    ));
    out.push_str("## Constraints\n\n");
    for constraint in &spec.constraints {
        out.push_str(&format!("- {constraint}\n"));
    }
    out.push_str("\n## Cases\n\n| id | visibility | category | scale | call | expected |\n| --- | --- | --- | --- | --- | --- |\n");
    for case in &problem.cases {
        out.push_str(&format!(
            "| {} | {:?} | {} | {} | `{}` | `{}` |\n",
            cell(&case.id),
            case.visibility,
            cell(&case.category),
            case.scale,
            cell(&call_of(problem, case)),
            cell(case.expected.as_deref().unwrap_or("(none)")),
        ));
    }
    out.push_str("\n## Clarifications\n\n");
    if spec.clarifications.is_empty() {
        out.push_str("None.\n");
    }
    for clarification in &spec.clarifications {
        out.push_str(&format!(
            "- Q: {}\n  A: {}\n",
            clarification.question, clarification.answer
        ));
    }
    out.push_str("\n## Pattern tags\n\n");
    for id in &spec.patterns {
        match problem_pattern(id) {
            Some(pattern) => out.push_str(&format!(
                "- {} (`{}`): {}\n",
                pattern.name, pattern.id, pattern.cue
            )),
            None => out.push_str(&format!("- `{id}`: not in the catalogue\n")),
        }
    }
    out.push_str(&format!(
        "\n## Reference\n\n{}\n",
        fenced("python", &numbered(&problem.reference))
    ));
    if !problem.reference_ms.is_empty() {
        out.push_str("Reference time on the scale cases:\n\n");
        for (id, ms) in &problem.reference_ms {
            out.push_str(&format!("- {id}: {ms} ms\n"));
        }
        out.push('\n');
    }
    out.push_str("## Misreadings\n\n");
    for misreading in &problem.misreadings {
        out.push_str(&format!(
            "### {}\n\n{}\n\n{}\nKilled by:\n\n",
            misreading.id,
            misreading.description,
            fenced("python", &numbered(&misreading.code))
        ));
        if misreading.killed_by.is_empty() {
            out.push_str("- no case\n");
        }
        for case_id in &misreading.killed_by {
            match problem.cases.iter().find(|case| &case.id == case_id) {
                Some(case) => out.push_str(&format!(
                    "- {}: {}, `{}` expected `{}`\n",
                    case.id,
                    case.category,
                    call_of(problem, case),
                    case.expected.as_deref().unwrap_or("(none)")
                )),
                None => out.push_str(&format!("- {case_id}: not a case of the problem\n")),
            }
        }
        out.push('\n');
    }
    out
}

/// Builds `LATTICE_CODING_EVAL_COUNT` problems (default 30) with Python at Easy, build `i` on pattern `i` of the
/// catalogue modulo its length, and writes each, with its proven problem, and a summary under
/// `$LATTICE_CODING_EVAL_OUT/builds/`.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_USE_LEARNING_MODEL=1 and LATTICE_CODING_EVAL_OUT; makes real model requests"]
async fn live_coding_build_eval() -> Result<()> {
    let total = count("LATTICE_CODING_EVAL_COUNT", 30)?;
    let out = out_dir("builds")?;
    let lab = EvalLab::new().await?;
    let model = lab.llm.model_name().to_owned();
    let kind = lab.kinds.get(&KindId::new(KIND))?;
    let mut wall = Vec::new();
    let mut calls = Vec::new();
    let mut proven_count = 0;
    let mut statements = Vec::new();
    let mut titles = Vec::new();
    let mut too_few = Vec::new();
    let mut by_pattern: BTreeMap<String, PatternTally> = BTreeMap::new();

    for position in 0..total {
        let name = format!("{:02}", position + 1);
        let requested: Vec<String> = PROBLEM_PATTERNS
            .iter()
            .cycle()
            .skip(position % PROBLEM_PATTERNS.len())
            .take(EASY_PATTERNS)
            .map(|pattern| pattern.id.to_owned())
            .collect();
        let request = ProblemRequest {
            program_id: lab.program_id.clone(),
            lesson: None,
            task: CodingTask::Problem,
            language: "python".into(),
            difficulty: ProblemDifficulty::Easy,
            patterns: requested.clone(),
            focus: None,
            details: None,
        };
        let started = Instant::now();
        let built =
            build_problem(kind, &request, &lab.llm, &Silent, CancellationToken::new()).await;
        let wall_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let model_calls = lab.llm.take();

        let dir = out.join(&name);
        std::fs::create_dir_all(&dir)?;
        let item = match built {
            Ok(problem) => {
                std::fs::write(
                    dir.join("problem.json"),
                    serde_json::to_string_pretty(&problem)?,
                )?;
                let named = |text: &str| {
                    named_in(text, &problem.spec.patterns).map(|pattern| pattern.id.to_owned())
                };
                let killed = problem
                    .misreadings
                    .iter()
                    .filter(|misreading| !misreading.killed_by.is_empty())
                    .count();
                BuildItem {
                    model: model.clone(),
                    name: name.clone(),
                    requested: requested.clone(),
                    proven: true,
                    error: None,
                    wall_ms,
                    model_calls,
                    statement_names: named(&problem.spec.statement),
                    title_names: named(&problem.spec.title),
                    killed_misreadings: Some(killed),
                    enough_killed: Some(killed >= MIN_KILLED_MISREADINGS),
                    problem: Some(problem),
                }
            }
            Err(error) => BuildItem {
                model: model.clone(),
                name: name.clone(),
                requested: requested.clone(),
                proven: false,
                error: Some(error.to_string()),
                wall_ms,
                model_calls,
                statement_names: None,
                title_names: None,
                killed_misreadings: None,
                enough_killed: None,
                problem: None,
            },
        };
        write_item(&dir, "build", &item, &markdown(&item))?;

        wall.push(item.wall_ms);
        calls.push(u64::try_from(item.model_calls.len()).unwrap_or(u64::MAX));
        if item.proven {
            proven_count += 1;
        }
        if item.statement_names.is_some() {
            statements.push(item.name.clone());
        }
        if item.title_names.is_some() {
            titles.push(item.name.clone());
        }
        if item.enough_killed == Some(false) {
            too_few.push(item.name.clone());
        }
        for id in &item.requested {
            let tally = by_pattern.entry(id.clone()).or_default();
            tally.builds.push(item.name.clone());
            if item.proven {
                tally.proven += 1;
            }
            if let Some(error) = &item.error {
                tally.errors.push(format!("{}: {error}", item.name));
            }
        }
        println!(
            "build {name} ({}): {} in {wall_ms} ms, {} model calls",
            requested.join(", "),
            if item.proven { "proven" } else { "not proven" },
            item.model_calls.len()
        );
    }

    let summary = Summary {
        model,
        builds: total,
        proven: proven_count,
        wall_ms: percentiles(&wall),
        model_calls_per_build: percentiles(&calls),
        statements_naming_a_pattern: statements,
        titles_naming_a_pattern: titles,
        builds_killing_too_few_misreadings: too_few,
        by_pattern,
    };
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    println!(
        "{} of {} builds proven; summary in {}",
        summary.proven,
        summary.builds,
        out.display()
    );
    Ok(())
}
