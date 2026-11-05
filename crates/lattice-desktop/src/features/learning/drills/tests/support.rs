//! Fakes and fixtures for the drill tests. Nothing here reaches a network: the model, the runner and the grader are
//! scripted, and the code under review is a small Python project.

use crate::features::learning::drills::attempt_grading::AttemptGrading;
use crate::features::learning::drills::attempts::{self, Attempt, AttemptMode, Submission};
use crate::features::learning::drills::bank;
use crate::features::learning::drills::build::{
    Baseline, BlindReview, CleanCode, CleanReview, DrillBuild, Evidence, Mutation, Planted, Proved,
    ProvenMutation, Trail, Verification,
};
use crate::features::learning::drills::grader::{Review, ReviewGrader};
use crate::features::learning::drills::material::CodeMaterial;
use crate::features::learning::drills::repository::{self, NewDrill};
use crate::features::learning::drills::runner::{CodeRunner, RunOutcome};
use crate::features::learning::drills::spec::DrillSpec;
use crate::features::learning::persistence::db;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{
    apply, Assessment, Drill, DrillFile, Edit, Finding, FindingOutcome, IssueOutcome, LineSpan,
    Material, Proof, Severity,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobContext, JobStore};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// The name the scripted model gives.
pub(super) const MODEL: &str = "scripted-drill-model";

/// The role and the text of each message a request carries.
pub(super) fn messages(request: &CompletionRequest) -> Vec<(String, String)> {
    request
        .input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { role, content } => Some((role.clone(), content.clone())),
            _ => None,
        })
        .collect()
}

/// The system message of a request.
pub(super) fn system(request: &CompletionRequest) -> String {
    messages(request)
        .into_iter()
        .find(|(role, _)| role == "system")
        .map(|(_, content)| content)
        .unwrap_or_default()
}

/// The last message of a request.
pub(super) fn last(request: &CompletionRequest) -> String {
    messages(request)
        .pop()
        .map(|(_, content)| content)
        .unwrap_or_default()
}

/// Which call of the build a request is, read from its system message.
pub(super) fn purpose(request: &CompletionRequest) -> &'static str {
    let system = system(request);
    for (needle, name) in [
        (
            "You write the code for a code review exercise",
            "clean code",
        ),
        (
            "reviewing a colleague's code in a pull request before it merges",
            "clean review",
        ),
        (
            "You prepare a code review exercise. You are given clean",
            "plant",
        ),
        (
            "You prepare a code review exercise. A change to the code was proposed",
            "new edit",
        ),
        ("You judge a code review exercise", "judge"),
        ("You write a test that shows a defect", "killing test"),
        ("Report every problem you would raise", "blind review"),
        ("A reviewer commented on code", "key entry"),
        ("You grade a code review against an answer key", "grade"),
        (
            "You judge whether a learner's change to some code fixes one known issue",
            "fix judge",
        ),
    ] {
        if system.contains(needle) {
            return name;
        }
    }
    "unknown"
}

/// A model that answers with the next canned text of its script and keeps every request it was sent. A call past
/// the end of the script fails.
pub(super) struct ScriptedModel {
    answers: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedModel {
    pub(super) fn new(answers: impl IntoIterator<Item = String>) -> Self {
        Self {
            answers: Mutex::new(answers.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn requests(&self) -> Vec<CompletionRequest> {
        self.requests
            .lock()
            .map(|requests| requests.clone())
            .unwrap_or_default()
    }

    /// Which call each request was, in order.
    pub(super) fn purposes(&self) -> Vec<&'static str> {
        self.requests().iter().map(purpose).collect()
    }

    pub(super) fn unused(&self) -> usize {
        self.answers
            .lock()
            .map(|answers| answers.len())
            .unwrap_or(0)
    }
}

#[async_trait]
impl LLMPort for ScriptedModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        let next = self
            .answers
            .lock()
            .ok()
            .and_then(|mut answers| answers.pop_front());
        next.map(CompletionResponse::from_text)
            .ok_or_else(|| AppError::InternalError("Unexpected extra model call".into()))
    }
    fn model_name(&self) -> &str {
        MODEL
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

type Decide = Box<dyn Fn(&[DrillFile], &str) -> RunOutcome + Send + Sync>;

/// A runner that decides each run from the files and the test, and keeps every run it was asked for.
pub(super) struct FakeRunner {
    decide: Decide,
    runs: Mutex<Vec<(Vec<DrillFile>, String)>>,
}

impl FakeRunner {
    pub(super) fn new(
        decide: impl Fn(&[DrillFile], &str) -> RunOutcome + Send + Sync + 'static,
    ) -> Self {
        Self {
            decide: Box::new(decide),
            runs: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn passing() -> Self {
        Self::new(|_, _| RunOutcome::Passed)
    }

    /// Answers each run with the next outcome; a run past the end passes.
    pub(super) fn scripted(outcomes: Vec<RunOutcome>) -> Self {
        let outcomes = Mutex::new(VecDeque::from(outcomes));
        Self::new(move |_, _| {
            outcomes
                .lock()
                .ok()
                .and_then(|mut outcomes| outcomes.pop_front())
                .unwrap_or(RunOutcome::Passed)
        })
    }

    /// The suite passes everywhere; the killing test passes on code without the operator mutation and fails on
    /// code with it.
    pub(super) fn killing_the_operator() -> Self {
        Self::new(|files, test| {
            let mutated = files
                .iter()
                .any(|file| file.text.contains(OPERATOR_AFTER.trim()));
            if test == KILLING_TEST && mutated {
                RunOutcome::Failed {
                    output: "AssertionError: a second call inside the window must be refused"
                        .into(),
                }
            } else {
                RunOutcome::Passed
            }
        })
    }

    pub(super) fn runs(&self) -> Vec<(Vec<DrillFile>, String)> {
        self.runs
            .lock()
            .map(|runs| runs.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl CodeRunner for FakeRunner {
    fn name(&self) -> &str {
        "fake-python"
    }
    fn entrypoint(&self) -> &str {
        "checks.py"
    }
    fn contract(&self) -> &str {
        "Fake contract: programs run on a fake."
    }
    fn running(&self, path: &str) -> String {
        format!("run {path}\n")
    }
    async fn run(
        &self,
        files: &[DrillFile],
        test: &str,
        _cancel: CancellationToken,
    ) -> Result<RunOutcome> {
        let outcome = (self.decide)(files, test);
        if let Ok(mut runs) = self.runs.lock() {
            runs.push((files.to_vec(), test.to_owned()));
        }
        Ok(outcome)
    }
}

/// What the grader was asked to judge.
pub(super) struct Seen {
    pub issues: Vec<String>,
    pub files: Vec<DrillFile>,
    pub clean: Vec<DrillFile>,
    pub findings: Vec<Finding>,
}

/// A grader that gives one assessment, or one failure, whatever the review, and keeps every review it was shown.
pub(super) struct FakeGrader {
    reply: std::result::Result<Assessment, String>,
    seen: Mutex<Vec<Seen>>,
}

impl FakeGrader {
    pub(super) fn new(assessment: Assessment) -> Self {
        Self {
            reply: Ok(assessment),
            seen: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn failing(message: &str) -> Self {
        Self {
            reply: Err(message.to_owned()),
            seen: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn seen(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .map(|seen| {
                seen.iter()
                    .map(|one| Seen {
                        issues: one.issues.clone(),
                        files: one.files.clone(),
                        clean: one.clean.clone(),
                        findings: one.findings.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl ReviewGrader for FakeGrader {
    async fn assess(&self, review: &Review<'_>) -> Result<Assessment> {
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(Seen {
                issues: review
                    .drill
                    .key
                    .iter()
                    .map(|issue| issue.id.clone())
                    .collect(),
                files: review.drill.files.clone(),
                clean: review.clean.to_vec(),
                findings: review.findings.to_vec(),
            });
        }
        self.reply.clone().map_err(AppError::ServiceNotAvailable)
    }
}

/// Lines joined with `\n`, the last one ended.
pub(super) fn text(lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// The first file of the project: line 13 is the comparison the operator mutation changes, line 8 the dictionary
/// that grows, line 21 the method that forgets a key.
pub(super) fn limiter() -> String {
    text(&[
        "class RateLimiter:",
        "    \"\"\"Allows at most `limit` calls per key in a sliding window.\"\"\"",
        "",
        "    def __init__(self, limit, window, clock):",
        "        self.limit = limit",
        "        self.window = window",
        "        self.clock = clock",
        "        self.hits = {}",
        "",
        "    def allow(self, key):",
        "        now = self.clock()",
        "        recent = [t for t in self.hits.get(key, []) if now - t < self.window]",
        "        if len(recent) >= self.limit:",
        "            self.hits[key] = recent",
        "            return False",
        "        recent.append(now)",
        "        self.hits[key] = recent",
        "        return True",
        "",
        "    def forget(self, key):",
        "        self.hits.pop(key, None)",
    ])
}

/// [`limiter`] with a flaw a clean review repairs: `forget` raises for a key never seen.
pub(super) fn flawed_limiter() -> String {
    limiter().replace(
        "        self.hits.pop(key, None)\n",
        "        del self.hits[key]\n",
    )
}

/// The second file: line 8 is the call the magic number mutation changes.
pub(super) fn config() -> String {
    text(&[
        "DEFAULT_LIMIT = 100",
        "DEFAULT_WINDOW = 60",
        "",
        "",
        "def default_limiter(clock):",
        "    from src.limiter import RateLimiter",
        "",
        "    return RateLimiter(DEFAULT_LIMIT, DEFAULT_WINDOW, clock)",
    ])
}

pub(super) fn files() -> Vec<DrillFile> {
    vec![
        DrillFile {
            path: "src/limiter.py".into(),
            text: limiter(),
        },
        DrillFile {
            path: "src/config.py".into(),
            text: config(),
        },
    ]
}

/// The hidden suite.
pub(super) const SUITE: &str = "from src.config import default_limiter\nfrom src.limiter import RateLimiter\n\nnow = [0]\nlimiter = RateLimiter(2, 10, lambda: now[0])\nassert limiter.allow(\"a\")\nassert limiter.allow(\"a\")\nassert not limiter.allow(\"a\")\nnow[0] = 11\nassert limiter.allow(\"a\")\nassert default_limiter(lambda: 0).allow(\"k\")\n";

/// A test that passes on the clean limiter and fails on the one with the operator changed.
pub(super) const KILLING_TEST: &str = "from src.limiter import RateLimiter\n\nlimiter = RateLimiter(1, 10, lambda: 0)\nassert limiter.allow(\"a\")\nassert not limiter.allow(\"a\"), \"a second call inside the window must be refused\"\n";

pub(super) const OPERATOR_BEFORE: &str = "        if len(recent) >= self.limit:\n";
pub(super) const OPERATOR_AFTER: &str = "        if len(recent) > self.limit:\n";
pub(super) const LITERALS_BEFORE: &str =
    "    return RateLimiter(DEFAULT_LIMIT, DEFAULT_WINDOW, clock)\n";
pub(super) const LITERALS_AFTER: &str = "    return RateLimiter(100, 60, clock)\n";

pub(super) const OPERATOR: &str = "correctness.wrong_operator";
pub(super) const LITERALS: &str = "readability.magic_number";

pub(super) fn spec() -> DrillSpec {
    DrillSpec {
        material: Material::code("python"),
        domain: Some("rate limiting".into()),
        lines: 40,
        patterns: vec![OPERATOR.into(), LITERALS.into()],
        program_id: None,
    }
}

pub(super) fn operator_edit() -> Edit {
    Edit {
        path: "src/limiter.py".into(),
        before: OPERATOR_BEFORE.into(),
        after: OPERATOR_AFTER.into(),
    }
}

pub(super) fn literals_edit() -> Edit {
    Edit {
        path: "src/config.py".into(),
        before: LITERALS_BEFORE.into(),
        after: LITERALS_AFTER.into(),
    }
}

pub(super) fn operator_mutation() -> Mutation {
    Mutation {
        pattern: OPERATOR.into(),
        edit: operator_edit(),
        severity: Severity::Major,
        consequence: "A caller gets one call more than the limit.".into(),
        explanation:
            "The comparison lets one call too many through. It should refuse at the limit.".into(),
        verification: Verification::KillingTest(KILLING_TEST.into()),
    }
}

pub(super) fn literals_mutation() -> Mutation {
    Mutation {
        pattern: LITERALS.into(),
        edit: literals_edit(),
        severity: Severity::Nit,
        consequence: "The default limit and window are written twice, so they can drift.".into(),
        explanation:
            "The constants are no longer used. Use them so the defaults change in one place.".into(),
        verification: Verification::NotAsked,
    }
}

/// What the model answers to the first call: the project, and its suite.
pub(super) fn clean_code_answer() -> String {
    json!({
        "title": "A sliding-window rate limiter",
        "files": [
            {"path": "src/limiter.py", "kind": "source", "text": limiter()},
            {"path": "src/config.py", "kind": "source", "text": config()}
        ],
        "suite": SUITE
    })
    .to_string()
}

pub(super) fn review_answer(issues: Vec<Value>) -> String {
    json!({"issues": issues}).to_string()
}

/// What the model answers to the plant call for [`spec`].
pub(super) fn plant_answer() -> String {
    json!({"mutations": [
        {
            "pattern": OPERATOR, "path": "src/limiter.py",
            "before": OPERATOR_BEFORE, "after": OPERATOR_AFTER,
            "severity": "major",
            "consequence": "A caller gets one call more than the limit.",
            "explanation": "The comparison lets one call too many through. It should refuse at the limit.",
            "killingTest": KILLING_TEST, "noTestBecause": ""
        },
        {
            "pattern": LITERALS, "path": "src/config.py",
            "before": LITERALS_BEFORE, "after": LITERALS_AFTER,
            "severity": "nit",
            "consequence": "The default limit and window are written twice, so they can drift.",
            "explanation": "The constants are no longer used. Use them so the defaults change in one place.",
            "killingTest": "", "noTestBecause": ""
        }
    ]})
    .to_string()
}

pub(super) fn judge_answer(confirmed: bool) -> String {
    let reason = if confirmed {
        "The edit changes the comparison, so one call too many is allowed, and the original code refuses at the limit."
    } else {
        "The original code already has this problem."
    };
    json!({
        "reason": reason,
        "editIntroducesIt": confirmed,
        "cleanLacksIt": confirmed
    })
    .to_string()
}

pub(super) fn code() -> CleanCode {
    CleanCode {
        drill_id: "00000000-0000-4000-8000-000000000001".into(),
        title: "A sliding-window rate limiter".into(),
        domain: "rate limiting".into(),
        files: files(),
        tests: Vec::new(),
        suite: Some(SUITE.into()),
        trail: Trail::default(),
    }
}

pub(super) fn review() -> CleanReview {
    CleanReview {
        files: files(),
        repairs: Vec::new(),
        trail: Trail::default(),
    }
}

pub(super) fn baseline() -> Baseline {
    Baseline::new(&code(), &review())
}

pub(super) fn planted() -> Planted {
    Planted {
        mutations: vec![operator_mutation(), literals_mutation()],
        trail: Trail::default(),
    }
}

/// Both mutations proven: the first killed, the second judged.
pub(super) fn proved() -> Proved {
    let edits = vec![operator_edit(), literals_edit()];
    let (changed, spans) = apply(&files(), &edits).unwrap_or_default();
    Proved {
        proven: vec![
            ProvenMutation {
                mutation: operator_mutation(),
                proof: Proof::Killed {
                    test: KILLING_TEST.into(),
                },
                trail: Trail::default(),
            },
            ProvenMutation {
                mutation: literals_mutation(),
                proof: Proof::Judged {
                    reason: "The constants are no longer used.".into(),
                },
                trail: Trail::default(),
            },
        ],
        dropped: Vec::new(),
        files: changed,
        spans,
    }
}

/// What the blind reviewer says: a finding on the changed operator, one whose quote is in the file twice, one on
/// `forget` that the grader will call a real extra, and one whose quote is nowhere.
pub(super) fn blind_answer() -> String {
    json!({"findings": [
        {"path": "src/limiter.py", "quote": "if len(recent) > self.limit:", "comment": "This lets one call more than the limit through.", "severity": "major"},
        {"path": "src/limiter.py", "quote": "self.hits[key] = recent", "comment": "The dictionary is written twice.", "severity": "nit"},
        {"path": "src/limiter.py", "quote": "self.hits.pop(key, None)", "comment": "Nothing calls forget, so the dictionary grows without bound.", "severity": "minor"},
        {"path": "src/limiter.py", "quote": "this text is not in the file", "comment": "Something is wrong here.", "severity": "nit"}
    ]})
    .to_string()
}

/// The assessment of [`blind_answer`] against the key of [`proved`].
pub(super) fn blind_assessment() -> Assessment {
    Assessment {
        issues: vec![
            (
                IssueOutcome::Found { finding: 0 },
                "The comment names the operator.".into(),
            ),
            (
                IssueOutcome::Missed,
                "No comment mentions the literals.".into(),
            ),
        ],
        findings: vec![
            (
                FindingOutcome::Matches { issues: vec![0] },
                "It is the changed operator.".into(),
            ),
            (
                FindingOutcome::FalsePositive,
                "That is how the code is meant to read.".into(),
            ),
            (
                FindingOutcome::ValidExtra,
                "The dictionary really does grow without bound.".into(),
            ),
            (FindingOutcome::FalsePositive, "It names nothing.".into()),
        ],
    }
}

/// What the judge answers when asked to grade the findings of [`blind_answer`] against the key of [`proved`]: the
/// assessment [`blind_assessment`] is.
pub(super) fn grade_answer() -> String {
    json!({
        "issues": [
            {
                "issue": 1, "reason": "The comment names the operator.", "outcome": "found",
                "finding": 1, "quote": "lets one call more than the limit through"
            },
            {
                "issue": 2, "reason": "No comment mentions the literals.", "outcome": "missed",
                "finding": null, "quote": null
            }
        ],
        "findings": [
            {"finding": 1, "reason": "It is the changed operator.", "outcome": "matches", "issues": [1]},
            {"finding": 2, "reason": "That is how the code is meant to read.", "outcome": "false_positive", "issues": []},
            {"finding": 3, "reason": "The dictionary really does grow without bound.", "outcome": "valid_extra", "issues": []},
            {"finding": 4, "reason": "It names nothing.", "outcome": "false_positive", "issues": []}
        ]
    })
    .to_string()
}

/// What the model answers when asked to describe the extra finding as an entry of the key.
pub(super) fn entry_answer() -> String {
    json!({
        "pattern": "resources.unbounded_growth",
        "severity": "minor",
        "consequence": "Every key ever seen stays in memory.",
        "fix": {
            "path": "src/limiter.py",
            "before": "        self.hits.pop(key, None)\n",
            "after": "        self.hits.pop(key, None)\n        self.hits = {k: v for k, v in self.hits.items() if v}\n"
        },
        "explanation": "The dictionary only grows. Drop keys whose window has passed."
    })
    .to_string()
}

/// A pool with every migration, and a drill build job queued for [`spec`] and claimed, as the runtime would.
pub(super) struct Harness {
    pub pool: SqlitePool,
    pub store: JobStore,
    pub job: String,
}

impl Harness {
    pub(super) async fn new(spec: &DrillSpec) -> Result<Self> {
        let pool = crate::features::learning::tests::pool().await?;
        let job = bank::queue_build(&pool, spec, "test").await?;
        let store = JobStore::new(pool.clone());
        store
            .claim(&job)
            .await?
            .ok_or_else(|| AppError::InternalError("The job was not claimed.".into()))?;
        Ok(Self { pool, store, job })
    }

    pub(super) async fn context(&self) -> Result<JobContext> {
        JobContext::detached(&self.store, &self.job).await
    }

    /// A build on these fakes. A runner of `None` builds without one.
    pub(super) fn build<'a>(
        &'a self,
        model: &'a ScriptedModel,
        runner: Option<&'a FakeRunner>,
        grader: &'a FakeGrader,
    ) -> DrillBuild<'a> {
        DrillBuild::new(
            model,
            Box::new(CodeMaterial::new(
                runner.map(|runner| runner as &dyn CodeRunner),
            )),
            grader,
            &self.pool,
            CancellationToken::new(),
        )
    }
}

/// A blind review that found nothing and judged nothing.
pub(super) fn quiet() -> BlindReview {
    BlindReview {
        findings: Vec::new(),
        assessment: None,
        findability: Vec::new(),
        extras: Vec::new(),
        trail: Trail::default(),
    }
}

/// A build on these fakes, over a pool with every migration but no job.
pub(super) fn build<'a>(
    pool: &'a SqlitePool,
    model: &'a ScriptedModel,
    runner: Option<&'a FakeRunner>,
    grader: &'a FakeGrader,
) -> DrillBuild<'a> {
    DrillBuild::new(
        model,
        Box::new(CodeMaterial::new(
            runner.map(|runner| runner as &dyn CodeRunner),
        )),
        grader,
        pool,
        CancellationToken::new(),
    )
}

/// A grader for a stage that never grades.
pub(super) fn idle_grader() -> FakeGrader {
    FakeGrader::failing("The grader is not used here.")
}

/// The drill's files with the operator repaired, as a learner who fixed the first issue leaves them.
pub(super) fn operator_fixed() -> Vec<DrillFile> {
    proved()
        .files
        .into_iter()
        .map(|file| DrillFile {
            text: file.text.replace(OPERATOR_AFTER, OPERATOR_BEFORE),
            path: file.path,
        })
        .collect()
}

/// What the fix judge answers.
pub(super) fn fix_answer(outcome: &str, reason: &str) -> String {
    json!({"reason": reason, "outcome": outcome}).to_string()
}

/// A finding on the changed operator that says what is wrong with it.
pub(super) fn operator_finding() -> Finding {
    Finding {
        span: Some(LineSpan {
            path: "src/limiter.py".into(),
            start: 13,
            end: 13,
        }),
        text: "This lets one call more than the limit through.".into(),
        severity: Some(Severity::Major),
    }
}

/// A finding on no lines, about the dictionary that grows.
pub(super) fn extra_finding() -> Finding {
    Finding {
        span: None,
        text: "The dictionary grows without bound.".into(),
        severity: None,
    }
}

/// The assessment of [`operator_finding`] and [`extra_finding`] against the key of [`proved`]: the operator found
/// by the first, the literals missed, and the second a real problem the key does not hold.
pub(super) fn review_assessment() -> Assessment {
    Assessment {
        issues: vec![
            (
                IssueOutcome::Found { finding: 0 },
                "The finding names the operator.".into(),
            ),
            (
                IssueOutcome::Missed,
                "No finding mentions the literals.".into(),
            ),
        ],
        findings: vec![
            (
                FindingOutcome::Matches { issues: vec![0] },
                "It is the changed operator.".into(),
            ),
            (
                FindingOutcome::ValidExtra,
                "The dictionary really does grow.".into(),
            ),
        ],
    }
}

/// Puts the drill of [`proved`] into the bank, ready, as a finished build leaves it: with its suite, its runner and
/// its evidence.
pub(super) async fn stock(pool: &SqlitePool, id: &str) -> Result<()> {
    let proved = proved();
    let drill = Drill {
        id: id.into(),
        material: Material::code("python"),
        title: "A sliding-window rate limiter".into(),
        files: proved.files.clone(),
        key: proved.planted_key(),
    };
    let clean = files();
    let spec = spec();
    let evidence = Evidence::new(&code(), &review(), &planted(), &proved, &quiet());
    let mut connection = pool.acquire().await.map_err(db)?;
    repository::insert_in(
        &mut connection,
        &NewDrill {
            drill: &drill,
            clean_files: &clean,
            spec: &spec,
            domain: "rate limiting",
            suite: Some(SUITE),
            runner: Some("fake-python"),
            model: MODEL,
            evidence: &evidence,
        },
    )
    .await
}

/// A pool with every migration, a drill stocked and an attempt opened on it and submitted, and its grading job
/// claimed, as the job runtime would.
pub(super) struct Waiting {
    pub pool: SqlitePool,
    pub store: JobStore,
    pub attempt: String,
    pub job: String,
}

impl Waiting {
    /// An attempt waiting for its grade, with its job claimed.
    pub(super) async fn new(
        findings: Vec<Finding>,
        fix_files: Option<Vec<DrillFile>>,
    ) -> Result<Self> {
        let waiting = Self::unclaimed(findings, fix_files).await?;
        Self::claim(&waiting.store, &waiting.job).await?;
        Ok(waiting)
    }

    /// An attempt waiting for its grade, with its job still pending, as the job runtime finds it.
    pub(super) async fn unclaimed(
        findings: Vec<Finding>,
        fix_files: Option<Vec<DrillFile>>,
    ) -> Result<Self> {
        let pool = crate::features::learning::tests::pool().await?;
        stock(&pool, "drill").await?;
        let begun = attempts::start(&pool, "python", AttemptMode::Practice)
            .await?
            .ok_or_else(|| AppError::InternalError("No drill was ready.".into()))?;
        let queued = attempts::submit(
            &pool,
            &begun.attempt.id,
            Submission {
                findings,
                fix_files,
                time_spent_ms: 90_000,
            },
        )
        .await?;
        let job = queued
            .job
            .ok_or_else(|| AppError::InternalError("No job was queued.".into()))?;
        let store = JobStore::new(pool.clone());
        Ok(Self {
            pool,
            store,
            attempt: begun.attempt.id,
            job,
        })
    }

    /// Claims a grading job, as the runtime does before it runs.
    pub(super) async fn claim(store: &JobStore, job: &str) -> Result<()> {
        store
            .claim(job)
            .await?
            .map(drop)
            .ok_or_else(|| AppError::InternalError("The job was not claimed.".into()))
    }

    pub(super) async fn context(&self) -> Result<JobContext> {
        JobContext::detached(&self.store, &self.job).await
    }

    pub(super) async fn attempt(&self) -> Result<Attempt> {
        attempts::get(&self.pool, &self.attempt)
            .await
            .map(|(attempt, _)| attempt)
    }

    /// The grading of this attempt on these fakes. A runner of `None` judges the whole fix.
    pub(super) fn grading<'a>(
        &'a self,
        model: &'a ScriptedModel,
        runner: Option<&'a FakeRunner>,
        grader: &'a FakeGrader,
    ) -> AttemptGrading<'a> {
        AttemptGrading::new(
            model,
            Box::new(CodeMaterial::new(
                runner.map(|runner| runner as &dyn CodeRunner),
            )),
            grader,
            &self.pool,
            CancellationToken::new(),
        )
    }

    /// Runs the grading job's work on these fakes with the attempt as it now stands.
    pub(super) async fn grade(
        &self,
        model: &ScriptedModel,
        runner: Option<&FakeRunner>,
        grader: &FakeGrader,
    ) -> Result<lattice_core::shared::runtime::jobs::JobOutcome> {
        let (attempt, stored) = attempts::waiting(&self.pool, &self.attempt)
            .await?
            .ok_or_else(|| AppError::InternalError("The attempt is not waiting.".into()))?;
        let context = self.context().await?;
        let grading = self.grading(model, runner, grader);
        let outcome = grading.run(&context, &attempt, &stored).await;
        outcome
    }
}
