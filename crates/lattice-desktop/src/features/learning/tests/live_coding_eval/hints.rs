//! Eval 2 of the coding sessions: the hint ladder on wrong drafts (design §9). Each killed misreading of the proven
//! problems is a wrong draft. It goes through the app's own Coached session, which runs it on every case and is
//! asked for Nudge, Idea, Plan and Line in order. Every code block in a stored hint is run against the problem's
//! cases: a block that passes is a release blocker. Whether a hint is on its rung, and whether a Line hint points at
//! the line that is wrong, is read by hand from the Markdown.

use lattice_core::domain::learning::practice::work::{Rung, Setting, Stage};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_learning::lab_runtime::LearningLabFile;
use lattice_learning::practice::coding::dto::{
    AskCodingHintRequestDto, CaseViewDto, CodeHintDto, CodingRunScope, CodingSessionDto,
    DraftObservation, MoveCodingStageRequestDto, ObserveCodingDraftRequestDto,
    RunCodingSessionRequestDto, StartCodingSessionRequestDto,
};
use lattice_learning::practice::coding::observation::{diff_lines, LineChange, LineChangeKind};
use lattice_learning::practice::coding::problem::{CaseStatus, Problem, SOLUTION_PATH};
use lattice_learning::practice::coding::solution_check::{code_blocks, passes_every_case};
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::{
    count, fenced, lab_sessions, numbered, out_dir, percentiles, problems, write_item, EvalLab,
    Percentiles,
};

const RUNGS: [Rung; 4] = [Rung::Nudge, Rung::Idea, Rung::Plan, Rung::Point];

/// A code block of a stored hint and whether it solves the problem.
#[derive(Debug, Clone, Serialize)]
struct BlockCheck {
    /// "text" or "reason": where the block is in the hint.
    part: &'static str,
    code: String,
    passes_every_case: bool,
    /// Why the block could not be run, when it could not.
    not_run: Option<String>,
}

/// One rung's hint, or the error that stopped it.
#[derive(Debug, Clone, Serialize)]
struct HintRecord {
    rung: Rung,
    ms: u64,
    hint: Option<CodeHintDto>,
    error: Option<String>,
    blocks: Vec<BlockCheck>,
}

/// A failing case of the draft's last run.
#[derive(Debug, Clone, Serialize)]
struct FailingCase {
    case_id: String,
    category: String,
    args: Option<Vec<String>>,
    expected: Option<String>,
    actual: Option<String>,
    error: Option<String>,
}

/// One wrong draft and the hints it drew.
#[derive(Debug, Serialize)]
struct DraftRecord {
    model: String,
    problem: String,
    title: String,
    statement: String,
    misreading_id: String,
    misreading: String,
    draft: String,
    failing: Vec<FailingCase>,
    hints: Vec<HintRecord>,
    /// Draft against reference: added lines are the draft's, removed lines the reference's.
    diff: Vec<LineChange>,
}

/// What the hints of one rung came to.
#[derive(Debug, Serialize)]
struct RungSummary {
    rung: Rung,
    given: usize,
    errors: usize,
    ms: Percentiles,
}

/// A hint block that solves the problem.
#[derive(Debug, Serialize)]
struct Blocker {
    draft: String,
    rung: Rung,
    part: &'static str,
    code: String,
}

#[derive(Debug, Serialize)]
struct Summary {
    model: String,
    drafts: usize,
    drafts_wanted: usize,
    problems_used: usize,
    problems_available: usize,
    rungs: Vec<RungSummary>,
    blocks_checked: usize,
    blocks_not_run: usize,
    blockers: Vec<Blocker>,
}

fn rung_name(rung: Rung) -> &'static str {
    match rung {
        Rung::Nudge => "Nudge",
        Rung::Idea => "Idea",
        Rung::Plan => "Plan",
        Rung::Point => "Line",
    }
}

fn file(code: &str) -> Vec<LearningLabFile> {
    vec![LearningLabFile {
        path: SOLUTION_PATH.into(),
        content: code.into(),
    }]
}

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn failing_of(session: &CodingSessionDto) -> Vec<FailingCase> {
    let Some(run) = session.runs.last() else {
        return Vec::new();
    };
    run.cases
        .iter()
        .filter(|case| !matches!(case.status, CaseStatus::Passed | CaseStatus::Ran))
        .map(|case: &CaseViewDto| FailingCase {
            case_id: case.case_id.clone(),
            category: case.category.clone(),
            args: case.args.clone(),
            expected: case.expected.clone(),
            actual: case.actual.clone(),
            error: case.error.clone(),
        })
        .collect()
}

/// Runs every code block of `hint` as `solution.py` against all the problem's cases.
async fn check_blocks(lab: &EvalLab, problem: &Problem, hint: &CodeHintDto) -> Vec<BlockCheck> {
    let mut checks = Vec::new();
    for (part, markdown) in [("text", &hint.text), ("reason", &hint.reason)] {
        for code in code_blocks(markdown) {
            let outcome = passes_every_case(
                &*lab.runner,
                &*lab.harness,
                problem,
                &code,
                CancellationToken::new(),
            )
            .await;
            checks.push(match outcome {
                Ok(passes) => BlockCheck {
                    part,
                    code,
                    passes_every_case: passes,
                    not_run: None,
                },
                Err(error) => BlockCheck {
                    part,
                    code,
                    passes_every_case: false,
                    not_run: Some(error.to_string()),
                },
            });
        }
    }
    checks
}

/// The draft with its line numbers, the lines the Line hint points at marked with `>>`.
fn marked(draft: &str, lines: &[u32]) -> String {
    numbered(draft)
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let hit = lines
                .iter()
                .any(|number| usize::try_from(*number).is_ok_and(|number| number == index + 1));
            format!("{} {line}", if hit { ">>" } else { "  " })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn markdown_of(index: usize, record: &DraftRecord) -> String {
    let mut out = String::new();
    let blockers: Vec<&HintRecord> = record
        .hints
        .iter()
        .filter(|hint| hint.blocks.iter().any(|block| block.passes_every_case))
        .collect();
    let _ = writeln!(out, "# Hint draft {index:02}: {}\n", record.title);
    if !blockers.is_empty() {
        let _ = writeln!(
            out,
            "**RELEASE BLOCKER: a code block in a hint solves the problem.**\n"
        );
        for hint in blockers {
            let _ = writeln!(out, "- {} hint", rung_name(hint.rung));
            for block in hint.blocks.iter().filter(|block| block.passes_every_case) {
                let _ = writeln!(out, "\n{}", fenced("python", &block.code));
            }
        }
        out.push('\n');
    }
    let _ = writeln!(
        out,
        "Model: {}. Problem folder: {}.\n\n## Statement\n\n{}\n",
        record.model, record.problem, record.statement
    );
    let _ = writeln!(
        out,
        "## What the draft gets wrong\n\n{}\n\n## Draft\n\n{}",
        record.misreading,
        fenced("python", &numbered(&record.draft))
    );
    let _ = writeln!(out, "## Cases the draft fails\n");
    if record.failing.is_empty() {
        let _ = writeln!(out, "None.\n");
    } else {
        let _ = writeln!(
            out,
            "| case | category | args | expected | actual | error |\n|---|---|---|---|---|---|"
        );
        for case in &record.failing {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} | {} |",
                case.case_id,
                case.category,
                case.args
                    .as_ref()
                    .map(|args| args.join(", "))
                    .unwrap_or_default()
                    .replace('|', "\\|"),
                case.expected
                    .clone()
                    .unwrap_or_default()
                    .replace('|', "\\|"),
                case.actual.clone().unwrap_or_default().replace('|', "\\|"),
                case.error
                    .clone()
                    .unwrap_or_default()
                    .replace('|', "\\|")
                    .replace('\n', " "),
            );
        }
        out.push('\n');
    }
    let _ = writeln!(out, "## Hints\n");
    for record_hint in &record.hints {
        let _ = writeln!(
            out,
            "### {} ({} ms)\n",
            rung_name(record_hint.rung),
            record_hint.ms
        );
        if let Some(error) = &record_hint.error {
            let _ = writeln!(out, "No hint: {error}\n");
            continue;
        }
        let Some(hint) = &record_hint.hint else {
            continue;
        };
        let _ = writeln!(out, "{}\n\nReason: {}\n", hint.text, hint.reason);
        for block in &record_hint.blocks {
            let verdict = match (&block.not_run, block.passes_every_case) {
                (Some(why), _) => format!("did not run ({why})"),
                (None, true) => "PASSES EVERY CASE".into(),
                (None, false) => "does not pass every case".into(),
            };
            let _ = writeln!(out, "Code block in {}: {verdict}\n", block.part);
        }
        if hint.rung == Rung::Point {
            let path = hint.path.as_deref().unwrap_or("(no path)");
            let _ = writeln!(
                out,
                "Points at {path}, lines {:?}. Marked `>>` in the draft:\n\n{}",
                hint.lines,
                fenced("python", &marked(&record.draft, &hint.lines))
            );
            let _ = writeln!(out, "Line diff, draft against reference:\n");
            if record.diff.is_empty() {
                let _ = writeln!(out, "No difference.\n");
            }
            for change in &record.diff {
                let (sign, whose) = match change.kind {
                    LineChangeKind::Added => ('+', "draft"),
                    LineChangeKind::Removed => ('-', "reference"),
                };
                let _ = writeln!(
                    out,
                    "- `{sign}` {whose} line {}: `{}`",
                    change.line, change.text
                );
            }
            out.push('\n');
        }
    }
    out
}

fn summary_markdown(summary: &Summary) -> String {
    let mut out = format!(
        "# Hint eval summary\n\nModel: {}. Drafts: {} of {} wanted, over {} of {} problems.\n\n",
        summary.model,
        summary.drafts,
        summary.drafts_wanted,
        summary.problems_used,
        summary.problems_available
    );
    let _ = writeln!(
        out,
        "Code blocks checked: {}, not run: {}. Blockers (a block that solves the problem): {}.\n",
        summary.blocks_checked,
        summary.blocks_not_run,
        summary.blockers.len()
    );
    for blocker in &summary.blockers {
        let _ = writeln!(
            out,
            "- draft {}, {} hint, in the {}:\n\n{}",
            blocker.draft,
            rung_name(blocker.rung),
            blocker.part,
            fenced("python", &blocker.code)
        );
    }
    let _ = writeln!(
        out,
        "| rung | given | errors | p50 ms | p90 ms |\n|---|---|---|---|---|"
    );
    for rung in &summary.rungs {
        let show = |ms: Option<u64>| ms.map_or("-".to_owned(), |ms| ms.to_string());
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} |",
            rung_name(rung.rung),
            rung.given,
            rung.errors,
            show(rung.ms.p50),
            show(rung.ms.p90)
        );
    }
    out
}

/// Takes the drafts: each killed misreading's code, at most `per_problem` of each problem, until `wanted`.
fn drafts_of(
    problems: &[(String, Problem)],
    wanted: usize,
    per_problem: usize,
) -> Vec<(usize, usize)> {
    let mut picked = Vec::new();
    for (p, (_, problem)) in problems.iter().enumerate() {
        let killed = problem
            .misreadings
            .iter()
            .enumerate()
            .filter(|(_, misreading)| !misreading.killed_by.is_empty())
            .map(|(m, _)| m)
            .take(per_problem);
        for m in killed {
            if picked.len() == wanted {
                return picked;
            }
            picked.push((p, m));
        }
    }
    picked
}

async fn hint_ladder(
    lab: &EvalLab,
    problem: &Problem,
    activity_id: &str,
    code: &str,
) -> Result<(Vec<FailingCase>, Vec<HintRecord>)> {
    lab_sessions!(sessions, lab);
    let started = sessions
        .start(&StartCodingSessionRequestDto {
            operation_id: id(),
            program_id: lab.program_id.clone(),
            activity_id: activity_id.to_owned(),
            setting: Setting::Coached,
            budget_seconds: None,
        })
        .await?;
    let session_id = started.session_id;
    sessions
        .move_stage(&MoveCodingStageRequestDto {
            operation_id: id(),
            program_id: lab.program_id.clone(),
            session_id: session_id.clone(),
            stage: Stage::Make,
            expected_revision: started.revision,
        })
        .await?;
    sessions
        .observe_draft(&ObserveCodingDraftRequestDto {
            program_id: lab.program_id.clone(),
            session_id: session_id.clone(),
            files: file(code),
            reason: DraftObservation::Pause,
        })
        .await?;
    let mut view = sessions
        .run(&RunCodingSessionRequestDto {
            operation_id: id(),
            run_id: id(),
            program_id: lab.program_id.clone(),
            session_id: session_id.clone(),
            scope: CodingRunScope::All,
            files: file(code),
            input: None,
        })
        .await?;
    let failing = failing_of(&view);
    let mut records = Vec::new();
    for _ in RUNGS {
        let rung = view.next_rung;
        let before = view.hints.len();
        let at = Instant::now();
        let asked = sessions
            .ask_hint(&AskCodingHintRequestDto {
                operation_id: id(),
                program_id: lab.program_id.clone(),
                session_id: session_id.clone(),
                files: file(code),
                expected_revision: view.revision,
            })
            .await;
        let ms = u64::try_from(at.elapsed().as_millis()).unwrap_or(u64::MAX);
        match asked {
            Ok(after) => {
                let hint = after.hints.get(before).cloned();
                let blocks = match &hint {
                    Some(hint) => check_blocks(lab, problem, hint).await,
                    None => Vec::new(),
                };
                records.push(HintRecord {
                    rung: hint.as_ref().map_or(rung, |hint| hint.rung),
                    ms,
                    hint,
                    error: None,
                    blocks,
                });
                view = after;
            }
            Err(error) => {
                records.push(HintRecord {
                    rung,
                    ms,
                    hint: None,
                    error: Some(error.to_string()),
                    blocks: Vec::new(),
                });
                // A refused hint is not stored, so the ladder cannot climb past this rung.
                break;
            }
        }
    }
    Ok((failing, records))
}

async fn hint_eval() -> Result<()> {
    let out = out_dir("hints")?;
    let wanted = count("LATTICE_CODING_EVAL_HINT_DRAFTS", 40)?;
    let per_problem = wanted.div_ceil(10);
    let available = problems()?;
    let picked = drafts_of(&available, wanted, per_problem);
    let lab = EvalLab::new().await?;
    let model = lab.llm.model_name().to_owned();

    let mut activities: BTreeMap<usize, String> = BTreeMap::new();
    let mut all: Vec<(String, Vec<HintRecord>)> = Vec::new();
    for (n, (p, m)) in picked.iter().enumerate() {
        let Some((folder, problem)) = available.get(*p) else {
            continue;
        };
        let Some(misreading) = problem.misreadings.get(*m) else {
            continue;
        };
        let activity_id = match activities.get(p) {
            Some(activity_id) => activity_id.clone(),
            None => {
                let activity_id = lab.activity(problem).await?;
                activities.insert(*p, activity_id.clone());
                activity_id
            }
        };
        let name = format!("{:02}", n + 1);
        println!("hint draft {name}: {folder} / {}", misreading.id);
        let (failing, hints) = hint_ladder(&lab, problem, &activity_id, &misreading.code).await?;
        let record = DraftRecord {
            model: model.clone(),
            problem: folder.clone(),
            title: problem.spec.title.clone(),
            statement: problem.spec.statement.clone(),
            misreading_id: misreading.id.clone(),
            misreading: misreading.description.clone(),
            draft: misreading.code.clone(),
            failing,
            hints,
            diff: diff_lines(SOLUTION_PATH, &problem.reference, &misreading.code),
        };
        write_item(&out, &name, &record, &markdown_of(n + 1, &record))?;
        all.push((name, record.hints));
    }

    let mut rungs = Vec::new();
    for rung in RUNGS {
        let of_rung: Vec<&HintRecord> = all
            .iter()
            .flat_map(|(_, hints)| hints)
            .filter(|hint| hint.rung == rung)
            .collect();
        let ms: Vec<u64> = of_rung.iter().map(|hint| hint.ms).collect();
        rungs.push(RungSummary {
            rung,
            given: of_rung.iter().filter(|hint| hint.hint.is_some()).count(),
            errors: of_rung.iter().filter(|hint| hint.error.is_some()).count(),
            ms: percentiles(&ms),
        });
    }
    let blocks = all.iter().flat_map(|(name, hints)| {
        hints.iter().flat_map(move |hint| {
            hint.blocks
                .iter()
                .map(move |block| (name, hint.rung, block))
        })
    });
    let mut blocks_checked = 0;
    let mut blocks_not_run = 0;
    let mut blockers = Vec::new();
    for (name, rung, block) in blocks {
        blocks_checked += 1;
        if block.not_run.is_some() {
            blocks_not_run += 1;
        }
        if block.passes_every_case {
            blockers.push(Blocker {
                draft: name.clone(),
                rung,
                part: block.part,
                code: block.code.clone(),
            });
        }
    }
    let summary = Summary {
        model,
        drafts: all.len(),
        drafts_wanted: wanted,
        problems_used: activities.len(),
        problems_available: available.len(),
        rungs,
        blocks_checked,
        blocks_not_run,
        blockers,
    };
    write_item(&out, "summary", &summary, &summary_markdown(&summary))?;
    println!(
        "hint eval: {} drafts, {} blockers; written to {}",
        summary.drafts,
        summary.blockers.len(),
        Path::new(&out).display()
    );
    Ok(())
}

/// Hints on wrong drafts: the ladder as the app serves it, and the code check that no hint block solves the problem.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_USE_LEARNING_MODEL, LATTICE_CODING_EVAL_OUT and LATTICE_CODING_EVAL_PROBLEMS (the builds folder of eval 1); makes real model requests"]
async fn live_coding_hint_eval() -> Result<()> {
    hint_eval().await
}
