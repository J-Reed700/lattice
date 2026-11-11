//! Eval 3 of the coding sessions (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md section 9): the
//! counterpart, the debrief and the latency. Scripted learners of five levels, from strong to weak, play each problem
//! of a builds run in Coached, Mock and Watched through the app's own session service, on the configured model. After
//! every step that leaves a cue pending, the counterpart answers, as the screen asks it to. One run of the first
//! problem's reference comes first and is timed by nothing: it fills the module cache, so every timed run is warm.
//!
//! Code checks: the setting's question rule on every stored reply, and every evidence item of every debrief, written
//! out here from the design's definition and not taken from the debrief module. Everything else is read by hand from
//! the Markdown of each session: whether the counterpart tells the truth about the code and the runs, whether its
//! answers to clarifying questions agree with the clarifications, whether Watched interjections are useful and
//! realistic, and whether a debrief's points are within one of a careful grader's.
//!
//! Reads `LATTICE_CODING_EVAL_PROBLEMS` (the `builds` folder of a run of eval 1), `LATTICE_CODING_EVAL_OUT`, and
//! `LATTICE_CODING_EVAL_SESSION_PROBLEMS` (the number of problems, 4 without it: 20 sessions per setting).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Instant;

use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::{
    problem_pattern, ScoreStatus, Scorecard, ScorecardEvidence,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_learning::coding::dto::{
    AskCodingHintRequestDto, CheckCodingExampleRequestDto, CodeHintDto, CodeHintRung, CodingRunDto,
    CodingRunScope, CodingSessionDto, CodingSessionRequestDto, CodingStage, DebriefFactsDto,
    DraftObservation, FinishCodingSessionRequestDto, MoveCodingStageRequestDto,
    ObserveCodingDraftRequestDto, RunCodingSessionRequestDto, SendCodingTurnRequestDto,
    StartCodingSessionRequestDto,
};
use lattice_learning::coding::policy::CodingCue;
use lattice_learning::coding::problem::{CaseStatus, CodingSetting, Problem, SOLUTION_PATH};
use lattice_learning::coding::records::StoredRun;
use lattice_learning::coding::session::{session_view, CodingSessions};
use lattice_learning::coding::solution_check::passes_every_case;
use lattice_learning::coding::store::session_of;
use lattice_learning::lab_runtime::LearningLabFile;
use lattice_learning::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
use lattice_learning::practice::kinds::program::shape::cue_of;
use lattice_learning::practice::session::observe::observe;
use lattice_learning::practice::session::policy::policy_for;
use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::{
    count, fenced, lab_sessions, numbered, out_dir, percentiles, problems, write_item, EvalLab,
    ModelCall, Percentiles,
};

/// The levels of a script, from strong to weak.
const LEVELS: [&str; 5] = [
    "L1 strong",
    "L2 one right example, then a misreading",
    "L3 a misreading, two hints",
    "L4 a misreading, every hint, no fix",
    "L5 weak: nothing done",
];

/// The settings, in the order they are played.
const SETTINGS: [CodingSetting; 3] = [
    CodingSetting::Coached,
    CodingSetting::Mock,
    CodingSetting::Watched,
];

/// How many hints the ladder holds: Nudge, Idea, Plan and Line.
const RUNGS: usize = 4;

fn setting_name(setting: CodingSetting) -> &'static str {
    match setting {
        CodingSetting::Coached => "coached",
        CodingSetting::Mock => "mock",
        CodingSetting::Watched => "watched",
    }
}

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn files(code: &str) -> Vec<LearningLabFile> {
    vec![LearningLabFile {
        path: SOLUTION_PATH.into(),
        content: code.into(),
    }]
}

/// The first half of the lines of `code`: what a learner has typed at the first pause.
fn first_half(code: &str) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let kept = (lines.len() / 2).max(1);
    let mut half = lines.into_iter().take(kept).collect::<Vec<_>>().join("\n");
    half.push('\n');
    half
}

/// A case that failed, as the learner saw it, and as the problem holds it.
#[derive(Debug, Clone, Serialize)]
struct Failing {
    case_id: String,
    category: String,
    status: String,
    args: Option<Vec<String>>,
    expected: Option<String>,
    actual: Option<String>,
    error: Option<String>,
    problem_args: Option<Vec<String>>,
    problem_expected: Option<String>,
}

/// A run, as the session reported it.
#[derive(Debug, Clone, Serialize)]
struct RunResult {
    run_id: String,
    scope: CodingRunScope,
    status: String,
    passed: usize,
    total: usize,
    failing: Vec<Failing>,
    stdout: String,
    stderr: String,
}

fn run_result(problem: &Problem, run: &CodingRunDto) -> RunResult {
    let failing = run
        .cases
        .iter()
        .filter(|case| !matches!(case.status, CaseStatus::Passed | CaseStatus::Ran))
        .map(|case| {
            let held = problem.cases.iter().find(|held| held.id == case.case_id);
            Failing {
                case_id: case.case_id.clone(),
                category: case.category.clone(),
                status: format!("{:?}", case.status),
                args: case.args.clone(),
                expected: case.expected.clone(),
                actual: case.actual.clone(),
                error: case.error.clone(),
                problem_args: held.map(|held| held.args.clone()),
                problem_expected: held.and_then(|held| held.expected.clone()),
            }
        })
        .collect();
    RunResult {
        run_id: run.run_id.clone(),
        scope: run.scope,
        status: format!("{:?}", run.status),
        passed: run
            .cases
            .iter()
            .filter(|case| case.status == CaseStatus::Passed)
            .count(),
        total: run.cases.len(),
        failing,
        stdout: run.stdout.clone(),
        stderr: run.stderr.clone(),
    }
}

/// What the counterpart did with a cue: its text or silence, the text the model saw, and whether the setting's
/// question rule held.
#[derive(Debug, Clone, Serialize)]
struct Reply {
    cue: Option<CodingCue>,
    ms: u64,
    calls: Vec<ModelCall>,
    text: Option<String>,
    error: Option<String>,
    rule_failure: Option<String>,
    observation: String,
}

/// One thing the scripted learner did, what the service returned, and the reply it drew.
#[derive(Debug, Clone, Serialize)]
struct Step {
    action: String,
    kind: &'static str,
    detail: Option<String>,
    ms: u64,
    calls: Vec<ModelCall>,
    error: Option<String>,
    result: Option<String>,
    run: Option<RunResult>,
    reply: Option<Reply>,
}

/// One scripted learner in one session, through the app's service.
struct Player<'a> {
    lab: &'a EvalLab,
    sessions: &'a CodingSessions<'a>,
    problem: &'a Problem,
    setting: CodingSetting,
    session: CodingSessionDto,
    draft: Vec<LearningLabFile>,
    steps: Vec<Step>,
}

impl<'a> Player<'a> {
    async fn begin(
        lab: &'a EvalLab,
        sessions: &'a CodingSessions<'a>,
        problem: &'a Problem,
        activity_id: &str,
        setting: CodingSetting,
    ) -> Result<Self> {
        let request = StartCodingSessionRequestDto {
            operation_id: id(),
            program_id: lab.program_id.clone(),
            activity_id: activity_id.to_owned(),
            setting,
            budget_seconds: None,
        };
        let started = Instant::now();
        let session = sessions.start(&request).await?;
        let mut player = Self {
            lab,
            sessions,
            problem,
            setting,
            draft: session.draft.clone(),
            session: session.clone(),
            steps: Vec::new(),
        };
        player
            .record(
                "start the session".into(),
                "start",
                None,
                started,
                Ok(session),
            )
            .await?;
        Ok(player)
    }

    async fn record(
        &mut self,
        action: String,
        kind: &'static str,
        detail: Option<String>,
        started: Instant,
        result: Result<CodingSessionDto>,
    ) -> Result<()> {
        let ms = elapsed_ms(started);
        let calls = self.lab.llm.take();
        let mut step = Step {
            action,
            kind,
            detail,
            ms,
            calls,
            error: None,
            result: None,
            run: None,
            reply: None,
        };
        let ok = match result {
            Ok(next) => {
                self.session = next;
                true
            }
            Err(error) => {
                step.error = Some(error.to_string());
                false
            }
        };
        if ok {
            match kind {
                "run" => {
                    step.run = self
                        .session
                        .runs
                        .last()
                        .map(|run| run_result(self.problem, run));
                }
                "example" => {
                    step.result = self
                        .session
                        .examples
                        .iter()
                        .max_by_key(|example| example.ordinal)
                        .map(|example| match &example.expects {
                            Some(expects) => {
                                format!("{:?}; the problem expects {expects}", example.verdict)
                            }
                            None => format!("{:?}", example.verdict),
                        });
                }
                "hint" => {
                    step.result = self
                        .session
                        .hints
                        .last()
                        .map(|hint| format!("{:?}: {}", hint.rung, hint.text));
                }
                "stage" => step.result = Some(format!("stage is now {:?}", self.session.stage)),
                _ => {}
            }
        }
        self.steps.push(step);
        if ok {
            let reply = self.answer().await?;
            if let Some(step) = self.steps.last_mut() {
                step.reply = reply;
            }
        }
        Ok(())
    }

    /// Answers the pending cue, as the screen does, and checks the reply against the setting's question rule.
    async fn answer(&mut self) -> Result<Option<Reply>> {
        if !self.session.cue_pending {
            return Ok(None);
        }
        let request = CodingSessionRequestDto {
            program_id: self.lab.program_id.clone(),
            session_id: self.session.session_id.clone(),
        };
        let bundle = self
            .lab
            .repo
            .coding_session(&request.program_id, &request.session_id)
            .await?;
        let cue = bundle.row.pending_cue.clone();
        let view = session_view(&bundle)?;
        let engine = session_of(&bundle)?;
        let kind = self.lab.kinds.get(&engine.exercise.kind)?;
        let observation = observe(kind, &engine, &engine.row.draft, engine.draft_seen.as_ref())?;
        let known = view.turns.len();
        let started = Instant::now();
        let result = self.sessions.respond(&request).await;
        let ms = elapsed_ms(started);
        let calls = self.lab.llm.take();
        let mut reply = Reply {
            cue: cue.clone(),
            ms,
            calls,
            text: None,
            error: None,
            rule_failure: None,
            observation: observation.render(kind)?,
        };
        match result {
            Ok(next) => {
                reply.text = next
                    .turns
                    .iter()
                    .skip(known)
                    .find(|turn| turn.speaker != LearningSimulationSpeaker::Learner)
                    .map(|turn| turn.content.clone());
                self.session = next;
                if let (Some(text), Some(cue)) = (&reply.text, &cue) {
                    reply.rule_failure = policy_for(engine.row.setting, kind, engine.row.form)
                        .check(text, &cue_of(cue), &observation)
                        .err()
                        .map(|error| error.to_string());
                }
            }
            Err(error) => {
                reply.error = Some(error.to_string());
                self.session = self.sessions.get(&request).await?;
            }
        }
        Ok(Some(reply))
    }

    async fn move_to(&mut self, stage: CodingStage) -> Result<()> {
        let request = MoveCodingStageRequestDto {
            operation_id: id(),
            program_id: self.lab.program_id.clone(),
            session_id: self.session.session_id.clone(),
            stage,
            expected_revision: self.session.revision,
        };
        let started = Instant::now();
        let result = self.sessions.move_stage(&request).await;
        self.record(format!("move to {stage:?}"), "stage", None, started, result)
            .await
    }

    async fn say(&mut self, content: &str) -> Result<()> {
        let request = SendCodingTurnRequestDto {
            operation_id: id(),
            program_id: self.lab.program_id.clone(),
            session_id: self.session.session_id.clone(),
            content: content.to_owned(),
            spoken: false,
            files: self.draft.clone(),
            expected_revision: self.session.revision,
        };
        let started = Instant::now();
        let result = self.sessions.send_turn(&request).await;
        self.record(format!("say: {content}"), "turn", None, started, result)
            .await
    }

    async fn example(&mut self, args: Vec<String>, expected: String) -> Result<()> {
        let request = CheckCodingExampleRequestDto {
            operation_id: id(),
            program_id: self.lab.program_id.clone(),
            session_id: self.session.session_id.clone(),
            ordinal: None,
            args: args.clone(),
            expected: expected.clone(),
        };
        let started = Instant::now();
        let result = self.sessions.check_example(&request).await;
        self.record(
            format!("check the example ({}) expects {expected}", args.join(", ")),
            "example",
            None,
            started,
            result,
        )
        .await
    }

    async fn pause(&mut self, code: &str, which: &str) -> Result<()> {
        self.draft = files(code);
        let request = ObserveCodingDraftRequestDto {
            program_id: self.lab.program_id.clone(),
            session_id: self.session.session_id.clone(),
            files: self.draft.clone(),
            reason: DraftObservation::Pause,
        };
        let started = Instant::now();
        let result = self.sessions.observe_draft(&request).await;
        self.record(
            format!("pause typing, {which} ({} lines)", code.lines().count()),
            "draft",
            None,
            started,
            result,
        )
        .await
    }

    /// In Watched, two pauses with a changed draft: the first half of `code`, then all of it.
    async fn pauses(&mut self, code: &str) -> Result<()> {
        if self.setting != CodingSetting::Watched {
            return Ok(());
        }
        self.pause(&first_half(code), "part way").await?;
        self.pause(code, "done").await
    }

    async fn run(&mut self, scope: CodingRunScope, code: &str, label: &str) -> Result<()> {
        self.draft = files(code);
        let request = RunCodingSessionRequestDto {
            operation_id: id(),
            run_id: id(),
            program_id: self.lab.program_id.clone(),
            session_id: self.session.session_id.clone(),
            scope,
            files: self.draft.clone(),
            input: None,
        };
        let started = Instant::now();
        let result = self.sessions.run(&request).await;
        self.record(
            format!("run {scope:?} on {label}"),
            "run",
            Some(scope.as_str().to_owned()),
            started,
            result,
        )
        .await
    }

    async fn hint(&mut self) -> Result<()> {
        let rung = self.session.next_rung;
        let request = AskCodingHintRequestDto {
            operation_id: id(),
            program_id: self.lab.program_id.clone(),
            session_id: self.session.session_id.clone(),
            files: self.draft.clone(),
            expected_revision: self.session.revision,
        };
        let started = Instant::now();
        let result = self.sessions.ask_hint(&request).await;
        self.record(
            format!("ask for a hint ({rung:?})"),
            "hint",
            Some(format!("{rung:?}").to_lowercase()),
            started,
            result,
        )
        .await
    }

    async fn finish(&mut self) -> Result<()> {
        let request = FinishCodingSessionRequestDto {
            operation_id: id(),
            program_id: self.lab.program_id.clone(),
            session_id: self.session.session_id.clone(),
            files: self.draft.clone(),
        };
        let started = Instant::now();
        let result = self.sessions.finish(&request).await;
        self.record("finish".into(), "finish", None, started, result)
            .await
    }
}

/// The plan of the strong learner: the tagged pattern's catalogue cue and the target complexity.
fn plan_text(problem: &Problem) -> String {
    let target = &problem.spec.target_complexity;
    match problem
        .spec
        .patterns
        .iter()
        .find_map(|id| problem_pattern(id))
    {
        Some(pattern) => format!("My plan: {} I am aiming for {target}.", pattern.cue),
        None => format!("My plan is to aim for {target}."),
    }
}

/// Plays one script. Every word and every line of code comes from the problem.
async fn play(player: &mut Player<'_>, level: usize) -> Result<()> {
    let problem = player.problem;
    let reference = problem.reference.clone();
    let wrong = problem
        .misreadings
        .first()
        .map_or_else(|| problem.spec.starter.clone(), |m| m.code.clone());
    let starter = problem.spec.starter.clone();
    match level {
        1 => {
            for clarification in &problem.spec.clarifications {
                player.say(&clarification.question).await?;
            }
            player.move_to(CodingStage::Examples).await?;
            let examples: Vec<(Vec<String>, String)> = problem
                .examples()
                .filter_map(|case| case.expected.clone().map(|e| (case.args.clone(), e)))
                .take(2)
                .collect();
            for (args, expected) in examples {
                player.example(args, expected).await?;
            }
            player.move_to(CodingStage::Plan).await?;
            player.say(&plan_text(problem)).await?;
            player.move_to(CodingStage::Code).await?;
            player.pauses(&reference).await?;
            player.move_to(CodingStage::Test).await?;
            player
                .run(CodingRunScope::Examples, &reference, "the reference")
                .await?;
            player
                .run(CodingRunScope::All, &reference, "the reference")
                .await?;
            player.move_to(CodingStage::Review).await?;
            player
                .say(&format!(
                    "The complexity is {}.",
                    problem.spec.target_complexity
                ))
                .await?;
        }
        2 => {
            if let Some((args, expected)) = problem
                .examples()
                .find_map(|case| case.expected.clone().map(|e| (case.args.clone(), e)))
            {
                player.example(args, expected).await?;
            }
            player.move_to(CodingStage::Code).await?;
            player.pauses(&wrong).await?;
            player
                .run(CodingRunScope::All, &wrong, "the first misreading")
                .await?;
            let failing = problem
                .misreadings
                .first()
                .and_then(|m| m.killed_by.first())
                .and_then(|id| problem.cases.iter().find(|case| &case.id == id))
                .map(|case| case.category.clone());
            if let Some(category) = failing {
                player.say(&format!("It fails on {category}.")).await?;
            }
            player
                .run(CodingRunScope::All, &reference, "the reference")
                .await?;
        }
        3 => {
            player.move_to(CodingStage::Code).await?;
            player.pauses(&wrong).await?;
            player
                .run(CodingRunScope::All, &wrong, "the first misreading")
                .await?;
            for _ in 0..2 {
                player.hint().await?;
            }
            player
                .run(CodingRunScope::All, &reference, "the reference")
                .await?;
        }
        4 => {
            player.move_to(CodingStage::Code).await?;
            player.pauses(&wrong).await?;
            player
                .run(CodingRunScope::All, &wrong, "the first misreading")
                .await?;
            for _ in 0..RUNGS {
                player.hint().await?;
            }
            player
                .run(CodingRunScope::All, &wrong, "the first misreading")
                .await?;
        }
        _ => player.draft = files(&starter),
    }
    player.finish().await
}

/// One evidence item and whether it holds, by the design's definition.
#[derive(Debug, Clone, Serialize)]
struct EvidenceCheck {
    criterion_id: String,
    evidence: ScorecardEvidence,
    valid: bool,
    why: Option<String>,
}

/// Why `item` is not valid, or None when it is. A quote is an exact substring of a learner turn; lines exist in the
/// named run's files; a run, an example or a hint exists in the session.
fn evidence_problem(
    item: &ScorecardEvidence,
    turns: &[LearningSimulationTurnDto],
    runs: &[StoredRun],
    examples: &[u32],
    hints: &[u32],
) -> Option<String> {
    match item {
        ScorecardEvidence::Quote { turn_id, text } => {
            let Some(turn) = turns.iter().find(|turn| &turn.id == turn_id) else {
                return Some(format!("no turn {turn_id}"));
            };
            if turn.speaker != LearningSimulationSpeaker::Learner {
                return Some(format!("turn {turn_id} is not the learner's"));
            }
            if text.is_empty() || !turn.content.contains(text.as_str()) {
                return Some(format!("the words are not in turn {turn_id}"));
            }
            None
        }
        ScorecardEvidence::Lines {
            run_id,
            path,
            start,
            end,
        } => {
            let Some(run) = runs.iter().find(|run| &run.run_id == run_id) else {
                return Some(format!("no run {run_id}"));
            };
            let Some(file) = run.files.iter().find(|file| &file.path == path) else {
                return Some(format!("run {run_id} has no file {path}"));
            };
            let total = file.content.lines().count();
            let within =
                *start >= 1 && start <= end && usize::try_from(*end).is_ok_and(|end| end <= total);
            (!within).then(|| format!("{start} to {end} is outside the {total} lines of {path}"))
        }
        ScorecardEvidence::Run { run_id } => {
            (!runs.iter().any(|run| &run.run_id == run_id)).then(|| format!("no run {run_id}"))
        }
        ScorecardEvidence::Example { ordinal } => {
            (!examples.contains(ordinal)).then(|| format!("no example {ordinal}"))
        }
        ScorecardEvidence::Hint { ordinal } => {
            (!hints.contains(ordinal)).then(|| format!("no hint {ordinal}"))
        }
    }
}

/// The debrief of a session, with every evidence item checked.
#[derive(Debug, Clone, Serialize)]
struct DebriefItem {
    scorecard: Scorecard,
    evidence_checks: Vec<EvidenceCheck>,
    row_issues: Vec<String>,
    not_enough_evidence: usize,
    approach: String,
    complexity: String,
    facts: DebriefFactsDto,
}

/// The session as the eval stores it.
#[derive(Debug, Clone, Serialize)]
struct SessionItem {
    model: String,
    name: String,
    setting: CodingSetting,
    level: usize,
    level_name: &'static str,
    problem: String,
    title: String,
    finished: bool,
    final_draft: String,
    steps: Vec<Step>,
    hints: Vec<CodeHintDto>,
    question_rule_failures: usize,
    debrief: Option<DebriefItem>,
}

fn debrief_item(
    session: &CodingSessionDto,
    turns: &[LearningSimulationTurnDto],
    runs: &[StoredRun],
    examples: &[u32],
) -> Option<DebriefItem> {
    let debrief = session.debrief.as_ref()?;
    let hints: Vec<u32> = session.hints.iter().map(|hint| hint.ordinal).collect();
    let mut evidence_checks = Vec::new();
    let mut row_issues = Vec::new();
    for row in &debrief.scorecard.rows {
        if row.status == ScoreStatus::Observed && row.evidence.is_empty() {
            row_issues.push(format!("{} is scored with no evidence", row.criterion_id));
        }
        for item in &row.evidence {
            let why = evidence_problem(item, turns, runs, examples, &hints);
            evidence_checks.push(EvidenceCheck {
                criterion_id: row.criterion_id.clone(),
                evidence: item.clone(),
                valid: why.is_none(),
                why,
            });
        }
    }
    Some(DebriefItem {
        not_enough_evidence: debrief
            .scorecard
            .rows
            .iter()
            .filter(|row| row.status == ScoreStatus::InsufficientEvidence)
            .count(),
        scorecard: debrief.scorecard.clone(),
        evidence_checks,
        row_issues,
        approach: debrief.strong_answer.approach.clone(),
        complexity: debrief.strong_answer.complexity.clone(),
        facts: debrief.facts.clone(),
    })
}

fn failing_text(md: &mut String, failing: &[Failing]) {
    for case in failing {
        let args = case
            .args
            .as_ref()
            .map_or("hidden".to_owned(), |args| args.join(", "));
        let expected = case.expected.as_deref().unwrap_or("hidden");
        let actual = case.actual.as_deref().unwrap_or("-");
        let error = case
            .error
            .as_deref()
            .map_or(String::new(), |error| format!(", raised {error}"));
        let held = match (&case.problem_args, &case.problem_expected) {
            (Some(args), expected) => format!(
                " (the problem's case: input ({}), expected {})",
                args.join(", "),
                expected.as_deref().unwrap_or("-")
            ),
            _ => String::new(),
        };
        let _ = writeln!(
            md,
            "  - {} [{}]: input {args}, expected {expected}, got {actual}{error}{held}",
            case.category, case.status
        );
    }
}

fn markdown(item: &SessionItem) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "# {}: {}", item.name, item.title);
    let _ = writeln!(
        md,
        "\nModel {}. Setting {}. Script {}. Problem {}. Question-rule failures: {}.\n",
        item.model,
        setting_name(item.setting),
        item.level_name,
        item.problem,
        item.question_rule_failures
    );
    let _ = writeln!(md, "## Steps\n");
    for (at, step) in item.steps.iter().enumerate() {
        let _ = writeln!(md, "{}. {} ({} ms)", at + 1, step.action, step.ms);
        if let Some(error) = &step.error {
            let _ = writeln!(md, "   - REFUSED OR FAILED: {error}");
        }
        if let Some(result) = &step.result {
            let _ = writeln!(md, "   - result: {result}");
        }
        if let Some(run) = &step.run {
            let _ = writeln!(
                md,
                "   - run {:?} {}: {} of {} cases passed",
                run.scope, run.status, run.passed, run.total
            );
            failing_text(&mut md, &run.failing);
            if !run.stderr.is_empty() {
                let _ = writeln!(md, "   - error output: {}", run.stderr);
            }
        }
        if let Some(reply) = &step.reply {
            let cue = reply
                .cue
                .as_ref()
                .map_or("no cue".to_owned(), |cue| format!("{cue:?}"));
            let _ = writeln!(md, "   - counterpart, cue {cue}, {} ms:", reply.ms);
            match (&reply.text, &reply.error) {
                (_, Some(error)) => {
                    let _ = writeln!(md, "     - ERROR: {error}");
                }
                (Some(text), None) => {
                    for line in text.lines() {
                        let _ = writeln!(md, "     > {line}");
                    }
                }
                (None, None) => {
                    let _ = writeln!(md, "     - (silent)");
                }
            }
            if let Some(failure) = &reply.rule_failure {
                let _ = writeln!(md, "     - QUESTION RULE BROKEN: {failure}");
            }
            let _ = writeln!(md, "\n{}", fenced("text", &reply.observation));
        }
    }
    let _ = writeln!(
        md,
        "\n## Final draft\n\n{}",
        fenced("python", &numbered(&item.final_draft))
    );
    let _ = writeln!(md, "\n## Hints\n");
    if item.hints.is_empty() {
        let _ = writeln!(md, "None.");
    }
    for hint in &item.hints {
        let _ = writeln!(
            md,
            "- {:?} (reason: {}): {}{}",
            hint.rung,
            hint.reason,
            hint.text,
            if hint.rung == CodeHintRung::Line {
                format!(" [path {:?}, lines {:?}]", hint.path, hint.lines)
            } else {
                String::new()
            }
        );
    }
    let _ = writeln!(md, "\n## Debrief\n");
    match &item.debrief {
        None => {
            let _ = writeln!(md, "None: the session did not finish.");
        }
        Some(debrief) => {
            for row in &debrief.scorecard.rows {
                let score = match (row.status, row.points) {
                    (ScoreStatus::Observed, Some(points)) => {
                        format!("{points} of {}", row.max_points)
                    }
                    _ => "not enough evidence".to_owned(),
                };
                let _ = writeln!(md, "### {}: {score}\n", row.title);
                let _ = writeln!(md, "{}\n\nNext step: {}\n", row.feedback, row.next_step);
                for check in debrief
                    .evidence_checks
                    .iter()
                    .filter(|check| check.criterion_id == row.criterion_id)
                {
                    let verdict = check
                        .why
                        .as_ref()
                        .map_or("valid".to_owned(), |why| format!("INVALID ({why})"));
                    let _ = writeln!(md, "- {:?}: {verdict}", check.evidence);
                }
                let _ = writeln!(md);
            }
            for issue in &debrief.row_issues {
                let _ = writeln!(md, "- ROW ISSUE: {issue}");
            }
            let _ = writeln!(
                md,
                "Approach: {}\n\nComplexity: {}\n\nFacts: {:?}",
                debrief.approach, debrief.complexity, debrief.facts
            );
        }
    }
    md
}

/// Something that failed a code check, and where.
#[derive(Debug, Clone, Serialize)]
struct Located {
    session: String,
    detail: String,
}

/// What one setting did across its sessions.
#[derive(Debug, Clone, Serialize)]
struct SettingSummary {
    setting: &'static str,
    sessions: usize,
    replies: usize,
    silences: usize,
    respond_errors: usize,
    step_errors: usize,
    checkpoint_cues: usize,
    checkpoint_spoken: usize,
    interjection_rate: Option<f64>,
}

/// Latency in milliseconds, each as p50 and p90.
#[derive(Debug, Clone, Serialize)]
struct Latency {
    respond: Percentiles,
    respond_by_setting: BTreeMap<String, Percentiles>,
    ask_hint_by_rung: BTreeMap<String, Percentiles>,
    run_by_scope: BTreeMap<String, Percentiles>,
    finish: Percentiles,
}

#[derive(Debug, Clone, Serialize)]
struct Summary {
    model: String,
    problems: Vec<String>,
    sessions: usize,
    settings: Vec<SettingSummary>,
    question_rule_failures: Vec<Located>,
    invalid_evidence: Vec<Located>,
    row_issues: Vec<Located>,
    rows_not_enough_evidence: usize,
    debriefs_missing: Vec<String>,
    latency: Latency,
}

fn grouped(map: BTreeMap<String, Vec<u64>>) -> BTreeMap<String, Percentiles> {
    map.into_iter()
        .map(|(key, ms)| (key, percentiles(&ms)))
        .collect()
}

fn summarize(model: &str, problem_names: Vec<String>, items: &[SessionItem]) -> Summary {
    let mut settings = Vec::new();
    let mut respond_all = Vec::new();
    let mut respond_by: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut hint_by: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut run_by: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut finish = Vec::new();
    let mut question_rule_failures = Vec::new();
    let mut invalid_evidence = Vec::new();
    let mut row_issues = Vec::new();
    let mut rows_not_enough_evidence = 0;
    let mut debriefs_missing = Vec::new();
    for setting in SETTINGS {
        let mine: Vec<&SessionItem> = items.iter().filter(|i| i.setting == setting).collect();
        let mut summary = SettingSummary {
            setting: setting_name(setting),
            sessions: mine.len(),
            replies: 0,
            silences: 0,
            respond_errors: 0,
            step_errors: 0,
            checkpoint_cues: 0,
            checkpoint_spoken: 0,
            interjection_rate: None,
        };
        for item in mine {
            for step in &item.steps {
                if step.error.is_some() {
                    summary.step_errors += 1;
                } else {
                    match step.kind {
                        "hint" => {
                            if let Some(rung) = &step.detail {
                                hint_by.entry(rung.clone()).or_default().push(step.ms);
                            }
                        }
                        "run" => {
                            if let Some(scope) = &step.detail {
                                run_by.entry(scope.clone()).or_default().push(step.ms);
                            }
                        }
                        "finish" => finish.push(step.ms),
                        _ => {}
                    }
                }
                let Some(reply) = &step.reply else { continue };
                if reply.error.is_some() {
                    summary.respond_errors += 1;
                    continue;
                }
                respond_all.push(reply.ms);
                respond_by
                    .entry(setting_name(setting).to_owned())
                    .or_default()
                    .push(reply.ms);
                let spoke = reply.text.is_some();
                if spoke {
                    summary.replies += 1;
                } else {
                    summary.silences += 1;
                }
                if reply.cue == Some(CodingCue::Checkpoint) {
                    summary.checkpoint_cues += 1;
                    summary.checkpoint_spoken += usize::from(spoke);
                }
                if let Some(failure) = &reply.rule_failure {
                    question_rule_failures.push(Located {
                        session: item.name.clone(),
                        detail: failure.clone(),
                    });
                }
            }
            match &item.debrief {
                None => debriefs_missing.push(item.name.clone()),
                Some(debrief) => {
                    rows_not_enough_evidence += debrief.not_enough_evidence;
                    for check in debrief.evidence_checks.iter().filter(|c| !c.valid) {
                        invalid_evidence.push(Located {
                            session: item.name.clone(),
                            detail: format!(
                                "{}: {:?}: {}",
                                check.criterion_id,
                                check.evidence,
                                check.why.clone().unwrap_or_default()
                            ),
                        });
                    }
                    for issue in &debrief.row_issues {
                        row_issues.push(Located {
                            session: item.name.clone(),
                            detail: issue.clone(),
                        });
                    }
                }
            }
        }
        if summary.checkpoint_cues > 0 {
            summary.interjection_rate =
                Some(summary.checkpoint_spoken as f64 / summary.checkpoint_cues as f64);
        }
        settings.push(summary);
    }
    Summary {
        model: model.to_owned(),
        problems: problem_names,
        sessions: items.len(),
        settings,
        question_rule_failures,
        invalid_evidence,
        row_issues,
        rows_not_enough_evidence,
        debriefs_missing,
        latency: Latency {
            respond: percentiles(&respond_all),
            respond_by_setting: grouped(respond_by),
            ask_hint_by_rung: grouped(hint_by),
            run_by_scope: grouped(run_by),
            finish: percentiles(&finish),
        },
    }
}

fn summary_markdown(summary: &Summary) -> String {
    let mut md = String::new();
    let _ = writeln!(
        md,
        "# Coding sessions eval\n\nModel {}. {} sessions over {} problems.\n",
        summary.model,
        summary.sessions,
        summary.problems.len()
    );
    for setting in &summary.settings {
        let _ = writeln!(
            md,
            "- {}: {} sessions, {} replies, {} silences, {} respond errors, {} step errors, {} of {} checkpoint cues spoken",
            setting.setting,
            setting.sessions,
            setting.replies,
            setting.silences,
            setting.respond_errors,
            setting.step_errors,
            setting.checkpoint_spoken,
            setting.checkpoint_cues
        );
    }
    let _ = writeln!(
        md,
        "\nQuestion-rule failures: {}. Invalid evidence items: {}. Scored rows with no evidence: {}. Rows with not enough evidence: {}. Sessions with no debrief: {}.",
        summary.question_rule_failures.len(),
        summary.invalid_evidence.len(),
        summary.row_issues.len(),
        summary.rows_not_enough_evidence,
        summary.debriefs_missing.len()
    );
    for failure in &summary.question_rule_failures {
        let _ = writeln!(md, "- {}: {}", failure.session, failure.detail);
    }
    for invalid in &summary.invalid_evidence {
        let _ = writeln!(md, "- {}: {}", invalid.session, invalid.detail);
    }
    let _ = writeln!(md, "\nLatency is in the JSON, as p50 and p90.");
    md
}

/// Replays 5 scripted learners on each of the first `LATTICE_CODING_EVAL_SESSION_PROBLEMS` problems (4 without it)
/// in Coached, Mock and Watched, through the app's session service.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_USE_LEARNING_MODEL=1, LATTICE_CODING_EVAL_OUT and LATTICE_CODING_EVAL_PROBLEMS (the builds folder of eval 1); makes real model requests"]
async fn live_coding_session_eval() -> Result<()> {
    let out = out_dir("sessions")?;
    let wanted = count("LATTICE_CODING_EVAL_SESSION_PROBLEMS", 4)?;
    let chosen: Vec<(String, Problem)> = problems()?.into_iter().take(wanted).collect();
    let lab = EvalLab::new().await?;
    let model = lab.llm.model_name().to_owned();
    let mut activities = Vec::new();
    for (_, problem) in &chosen {
        activities.push(lab.activity(problem).await?);
    }
    if let Some((_, first)) = chosen.first() {
        let warmed = passes_every_case(
            lab.runner.as_ref(),
            lab.harness.as_ref(),
            first,
            &first.reference,
            CancellationToken::new(),
        )
        .await?;
        if !warmed {
            return Err(AppError::InvalidState(
                "The warm-up run of the first problem's reference did not pass every case.".into(),
            ));
        }
    }
    lab_sessions!(sessions, lab);
    let mut items = Vec::new();
    for setting in SETTINGS {
        for (at, ((folder, problem), activity)) in chosen.iter().zip(&activities).enumerate() {
            for (index, &level_name) in LEVELS.iter().enumerate() {
                let level = index + 1;
                let name = format!("{}-{:02}", setting_name(setting), at * LEVELS.len() + index);
                let mut player = Player::begin(&lab, &sessions, problem, activity, setting).await?;
                play(&mut player, level).await?;
                let bundle = lab
                    .repo
                    .coding_session(&lab.program_id, &player.session.session_id)
                    .await?;
                let example_ordinals: Vec<u32> = bundle
                    .examples
                    .iter()
                    .map(|example| example.ordinal)
                    .collect();
                let debrief = debrief_item(
                    &player.session,
                    &bundle.turns,
                    &bundle.runs,
                    &example_ordinals,
                );
                let question_rule_failures = player
                    .steps
                    .iter()
                    .filter_map(|step| step.reply.as_ref())
                    .filter(|reply| reply.rule_failure.is_some())
                    .count();
                let item = SessionItem {
                    model: model.clone(),
                    name: name.clone(),
                    setting,
                    level,
                    level_name,
                    problem: folder.clone(),
                    title: problem.spec.title.clone(),
                    finished: player.session.finished,
                    final_draft: player
                        .draft
                        .iter()
                        .map(|file| file.content.clone())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    steps: player.steps,
                    hints: player.session.hints.clone(),
                    question_rule_failures,
                    debrief,
                };
                write_item(&out, &name, &item, &markdown(&item))?;
                println!(
                    "{name}: {} steps, finished {}",
                    item.steps.len(),
                    item.finished
                );
                items.push(item);
            }
        }
    }
    let names = chosen.iter().map(|(folder, _)| folder.clone()).collect();
    let summary = summarize(&model, names, &items);
    write_item(&out, "summary", &summary, &summary_markdown(&summary))?;
    println!("{}", summary_markdown(&summary));
    Ok(())
}
