//! Fakes and fixtures for the drill tests. Nothing here reaches a network: the model, the runner, the source check and
//! the grader are scripted, the code under review is a small Python project, and the passage under review is from a
//! course's source about spacing study sessions.

#![cfg_attr(not(test), allow(dead_code))]

use crate::drills::attempt_grading::AttemptGrading;
use crate::drills::attempts::{self, Attempt, AttemptMode, Submission};
use crate::drills::bank;
use crate::drills::build::{
    Baseline, BlindReview, CleanCode, CleanReview, DrillBuild, Evidence, LessonChecked, Mutation,
    Planted, Proved, ProvenMutation, SourcePassage, Trail, Verification,
};
use crate::drills::grader::{Review, ReviewGrader};
use crate::drills::material::passage::{SourceFinding, SourceSays};
use crate::drills::material::{CodeMaterial, PassageMaterial, SourceCheck};
use crate::drills::repository::{self, NewDrill, StoredDrill};
use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::drills::schedules;
use crate::drills::spec::DrillSpec;
use crate::persistence::now;
use async_trait::async_trait;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{
    apply, Assessment, Drill, DrillFile, Edit, Finding, FindingOutcome, IssueOutcome, KeyIssue,
    LessonBrief, LessonSentence, LineSpan, Material, PatternDue, Proof, Severity,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::{JobContext, JobStore};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::collections::VecDeque;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// The name the scripted model gives.
pub const MODEL: &str = "scripted-drill-model";

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
            "You decide whether a passage from a course's sources",
            "passage check",
        ),
        ("You decide whether a lesson of a course", "code fit check"),
        (
            "You write the code for a code review exercise",
            "clean code",
        ),
        (
            "You write a study note for a review exercise",
            "clean explanation",
        ),
        ("You correct one sentence of a study note", "rewrite"),
        ("You prepare a review exercise on an explanation", "plant"),
        ("You judge a review exercise on an explanation", "judge"),
        (
            "checking a classmate's study note against the source",
            "blind review",
        ),
        ("A reviewer commented on an explanation", "key entry"),
        ("You grade a review of an explanation", "grade"),
        (
            "You judge whether a learner's change to an explanation",
            "fix judge",
        ),
        (
            "You judge one change of a review exercise on an explanation",
            "error gone",
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
        (
            "You check one defect planted in code for a code review exercise on a lesson",
            "lesson check",
        ),
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
    async fn run_capturing(
        &self,
        _files: &[crate::lab_runtime::LearningLabFile],
        _read_only_paths: &[String],
        _cancel: CancellationToken,
    ) -> Result<(RunOutcome, String)> {
        Ok((RunOutcome::Passed, String::new()))
    }
}

/// What the grader was asked to judge.
pub(super) struct Seen {
    pub issues: Vec<String>,
    pub key: Vec<KeyIssue>,
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
                        key: one.key.clone(),
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
                key: review.drill.key.clone(),
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
pub const SUITE: &str = "from src.config import default_limiter\nfrom src.limiter import RateLimiter\n\nnow = [0]\nlimiter = RateLimiter(2, 10, lambda: now[0])\nassert limiter.allow(\"a\")\nassert limiter.allow(\"a\")\nassert not limiter.allow(\"a\")\nnow[0] = 11\nassert limiter.allow(\"a\")\nassert default_limiter(lambda: 0).allow(\"k\")\n";

/// A test that passes on the clean limiter and fails on the one with the operator changed.
pub const KILLING_TEST: &str = "from src.limiter import RateLimiter\n\nlimiter = RateLimiter(1, 10, lambda: 0)\nassert limiter.allow(\"a\")\nassert not limiter.allow(\"a\"), \"a second call inside the window must be refused\"\n";

pub(super) const OPERATOR_BEFORE: &str = "        if len(recent) >= self.limit:\n";
pub(super) const OPERATOR_AFTER: &str = "        if len(recent) > self.limit:\n";
pub(super) const LITERALS_BEFORE: &str =
    "    return RateLimiter(DEFAULT_LIMIT, DEFAULT_WINDOW, clock)\n";
pub(super) const LITERALS_AFTER: &str = "    return RateLimiter(100, 60, clock)\n";

pub const OPERATOR: &str = "correctness.wrong_operator";
pub const LITERALS: &str = "readability.magic_number";

pub(super) fn spec() -> DrillSpec {
    DrillSpec {
        material: Material::code("python"),
        lines: 40,
        patterns: vec![OPERATOR.into(), LITERALS.into()],
        program_id: None,
        lesson_id: None,
    }
}

/// A spec for code in Python on the course `course` built from its lesson `lesson`: ids only, for tests that queue or
/// read a spec and never build it.
pub(super) fn lesson_spec() -> DrillSpec {
    DrillSpec {
        program_id: Some("course".into()),
        lesson_id: Some("lesson".into()),
        ..spec()
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
        lesson_sentences: vec![],
    }
}

pub fn literals_mutation() -> Mutation {
    Mutation {
        pattern: LITERALS.into(),
        edit: literals_edit(),
        severity: Severity::Nit,
        consequence: "The default limit and window are written twice, so they can drift.".into(),
        explanation:
            "The constants are no longer used. Use them so the defaults change in one place.".into(),
        verification: Verification::NotAsked,
        lesson_sentences: vec![],
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

/// The lesson the code of the fixtures is written for: two units of its own teaching and one from a source. The
/// operator change breaks the first, the literals change the second.
pub fn lesson_brief() -> LessonBrief {
    let unit = |text: &str, source: Option<&str>| LessonSentence {
        text: text.into(),
        source: source.map(str::to_owned),
    };
    LessonBrief {
        lesson_id: "lesson-1".into(),
        course: "Rate limiting in services".into(),
        goal: "Protect a service from callers that send too much".into(),
        prior_knowledge: "Basic Python".into(),
        module: "Limiters".into(),
        module_summary: "How a limiter counts and refuses calls".into(),
        outcomes: vec!["Write a sliding-window limiter".into()],
        lesson: "A sliding-window limiter".into(),
        objective: "Refuse calls past the limit inside a window".into(),
        evidence: vec![
            unit(
                "A limiter refuses a call once the calls in its window reach the limit.",
                None,
            ),
            unit(
                "A limiter's defaults are named once, so they change in one place.",
                None,
            ),
            unit(
                "A sliding window forgets the calls older than its length.",
                Some("Rate limiting notes"),
            ),
        ],
    }
}

/// [`baseline`] with the brief of [`lesson_brief`], as the clean stage leaves it for a lesson.
pub(super) fn baseline_with_brief() -> Baseline {
    Baseline {
        brief: Some(lesson_brief()),
        ..baseline()
    }
}

/// What the lesson check has made of a drill none of whose issues is a lesson issue.
pub fn unchecked() -> LessonChecked {
    LessonChecked {
        links: Vec::new(),
        checks: Vec::new(),
    }
}

/// [`proved`] with the operator change a lesson issue citing `operator` and the literals change one citing
/// `literals` (an empty list for a catalogue issue).
pub(super) fn lesson_proved(operator: &[usize], literals: &[usize]) -> Proved {
    let mut proved = proved();
    for (item, sentences) in proved.proven.iter_mut().zip([operator, literals]) {
        item.mutation.lesson_sentences = sentences.to_vec();
    }
    proved
}

/// What the model answers to the lesson check.
pub(super) fn lesson_check_answer(breaks: bool, follows: bool) -> String {
    json!({
        "reason": "The sentences say a limiter refuses at its limit, and the edit lets one call more through.",
        "editBreaksTheSentences": breaks,
        "codeBeforeTheEditFollowsTheSentences": follows
    })
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

pub fn code() -> CleanCode {
    CleanCode {
        drill_id: "00000000-0000-4000-8000-000000000001".into(),
        title: "A sliding-window rate limiter".into(),
        domain: "rate limiting".into(),
        files: files(),
        tests: Vec::new(),
        suite: Some(SUITE.into()),
        source: None,
        brief: None,
        basis: None,
        trail: Trail::default(),
    }
}

pub fn review() -> CleanReview {
    CleanReview {
        files: files(),
        repairs: Vec::new(),
        trail: Trail::default(),
    }
}

pub(super) fn baseline() -> Baseline {
    Baseline::new(&code(), &review())
}

pub fn planted() -> Planted {
    Planted {
        mutations: vec![operator_mutation(), literals_mutation()],
        trail: Trail::default(),
    }
}

/// Both mutations proven: the first killed, the second judged.
pub fn proved() -> Proved {
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
    pub db: Database,
    pub store: JobStore,
    pub job: String,
}

impl Harness {
    /// A harness over a new course from [`code_spec`], and the spec of its build.
    pub(super) async fn lesson() -> Result<(Self, DrillSpec)> {
        let pool = crate::test_support::pool().await?;
        let db = Database::new(pool.clone());
        let spec = code_spec(&pool, &db).await?;
        Ok((Self::on(pool, db, &spec).await?, spec))
    }

    /// A harness over `pool`, with a build job queued for `spec` and claimed.
    pub(super) async fn on(pool: SqlitePool, db: Database, spec: &DrillSpec) -> Result<Self> {
        let job = bank::queue_build(&db, spec, "test").await?;
        let store = JobStore::new(db.clone());
        store
            .claim(&job)
            .await?
            .ok_or_else(|| AppError::InternalError("The job was not claimed.".into()))?;
        Ok(Self {
            pool,
            db,
            store,
            job,
        })
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
            &self.db,
            CancellationToken::new(),
        )
    }
}

/// A blind review that found nothing and judged nothing.
pub fn quiet() -> BlindReview {
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
    db: &'a Database,
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
        db,
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

/// An assessment in which every issue of the key was found.
pub(super) fn raised_all(drill: &Drill) -> Assessment {
    Assessment {
        issues: drill
            .key
            .iter()
            .map(|_| (IssueOutcome::Found { finding: 0 }, "Found.".to_owned()))
            .collect(),
        findings: Vec::new(),
    }
}

/// [`stock_on`] for [`spec`]: a drill built for no course and no lesson.
pub async fn stock(db: &Database, id: &str) -> Result<()> {
    stock_on(db, id, &spec()).await
}

/// Puts the drill of [`proved`] into the bank, ready, as a finished build for `spec` leaves it: with its suite, its
/// runner and its evidence. The course and the lesson of `spec` must exist for the drill to keep them.
pub(super) async fn stock_on(db: &Database, id: &str, spec: &DrillSpec) -> Result<()> {
    let proved = proved();
    let drill = Drill {
        id: id.into(),
        material: Material::code("python"),
        title: "A sliding-window rate limiter".into(),
        files: proved.files.clone(),
        key: proved.planted_key(&unchecked()),
    };
    let clean = files();
    let evidence = Evidence::new(
        &code(),
        &review(),
        &planted(),
        &proved,
        &unchecked(),
        &quiet(),
    );
    let mut tx = db.begin_write().await?;
    repository::insert_in(
        &mut tx,
        &NewDrill {
            drill: &drill,
            clean_files: &clean,
            spec,
            domain: "rate limiting",
            suite: Some(SUITE),
            source: None,
            runner: Some("fake-python"),
            model: MODEL,
            evidence: &evidence,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// The drill a build stored: the only one in the pool.
pub(super) async fn built_drill(pool: &SqlitePool) -> Result<Option<StoredDrill>> {
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM learning_drills")
        .fetch_optional(pool)
        .await?;
    let Some(id) = id else {
        return Ok(None);
    };
    let mut connection = pool.acquire().await?;
    repository::find_in(&mut connection, &id).await
}

/// Takes the stocked drill `drill_id` from the bank and opens an attempt on it, as starting a drill does, without the
/// bank choosing it.
pub(super) async fn opened(db: &Database, drill_id: &str, mode: AttemptMode) -> Result<Attempt> {
    let mut tx = db.begin_write().await?;
    let taken = repository::mark_taken_in(&mut tx, drill_id, now()).await?;
    assert!(taken, "the drill {drill_id} was ready to be taken");
    let attempt = attempts::repository::open_in(&mut tx, drill_id, mode).await?;
    tx.commit().await?;
    Ok(attempt)
}

/// A pool with every migration, a drill stocked and an attempt opened on it and submitted, and its grading job
/// claimed, as the job runtime would.
pub(super) struct Waiting {
    pub pool: SqlitePool,
    pub db: Database,
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
        let pool = crate::test_support::pool().await?;
        let db = Database::new(pool.clone());
        stock(&db, "drill").await?;
        let attempt = opened(&db, "drill", AttemptMode::Practice).await?;
        let queued = attempts::submit(
            &db,
            &attempt.id,
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
        let store = JobStore::new(db.clone());
        Ok(Self {
            pool,
            db,
            store,
            attempt: attempt.id,
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
        attempts::get(&self.db, &self.attempt)
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
            &self.db,
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
        let (attempt, stored) = attempts::waiting(&self.db, &self.attempt)
            .await?
            .ok_or_else(|| AppError::InternalError("The attempt is not waiting.".into()))?;
        let context = self.context().await?;
        let grading = self.grading(model, runner, grader);
        let outcome = grading.run(&context, &attempt, &stored).await;
        outcome
    }
}

/// The name the scripted source check gives.
pub(super) const CHECK_MODEL: &str = "scripted-source-check";

type Verdict = Box<dyn Fn(&str) -> Result<SourceFinding> + Send + Sync>;

/// A source check that answers each sentence with what `decide` says of it, and keeps every sentence and passage it
/// was asked about.
pub(super) struct ScriptedCheck {
    decide: Verdict,
    asked: Mutex<Vec<(String, String)>>,
}

impl ScriptedCheck {
    pub(super) fn new(
        decide: impl Fn(&str) -> Result<SourceFinding> + Send + Sync + 'static,
    ) -> Self {
        Self {
            decide: Box::new(decide),
            asked: Mutex::new(Vec::new()),
        }
    }

    /// The sentences it was asked about, in order.
    pub(super) fn sentences(&self) -> Vec<String> {
        self.asked()
            .into_iter()
            .map(|(sentence, _)| sentence)
            .collect()
    }

    /// The sentences and the passages it was asked about, in order.
    pub(super) fn asked(&self) -> Vec<(String, String)> {
        self.asked
            .lock()
            .map(|asked| asked.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl SourceCheck for ScriptedCheck {
    async fn check(&self, sentence: &str, passage: &str) -> Result<SourceFinding> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push((sentence.to_owned(), passage.to_owned()));
        }
        (self.decide)(sentence)
    }

    fn model_name(&self) -> &str {
        CHECK_MODEL
    }
}

pub(super) fn supported(quote: &str) -> Result<SourceFinding> {
    Ok(SourceFinding {
        says: SourceSays::Supported,
        quote: Some(quote.to_owned()),
        reason: "The passage states it.".to_owned(),
        confident: true,
        unusable: false,
    })
}

pub(super) fn contradicted(quote: &str, reason: &str) -> Result<SourceFinding> {
    Ok(SourceFinding {
        says: SourceSays::Contradicted,
        quote: Some(quote.to_owned()),
        reason: reason.to_owned(),
        confident: true,
        unusable: false,
    })
}

pub(super) fn unsupported(reason: &str) -> Result<SourceFinding> {
    Ok(SourceFinding {
        says: SourceSays::Unsupported,
        quote: None,
        reason: reason.to_owned(),
        confident: true,
        unusable: false,
    })
}

/// A finding the checker was not sure of: it found the sentence `says` what it does, but with too little confidence
/// to rest anything on.
pub(super) fn unsure(says: SourceSays, reason: &str) -> Result<SourceFinding> {
    Ok(SourceFinding {
        says,
        quote: None,
        reason: reason.to_owned(),
        confident: false,
        unusable: false,
    })
}

/// What a check the checker gave no usable verdict on comes to: a finding of nothing, unsure.
pub(super) fn no_verdict() -> Result<SourceFinding> {
    Ok(SourceFinding {
        says: SourceSays::Unsupported,
        quote: None,
        reason: "The source check gave no verdict that could be used.".to_owned(),
        confident: false,
        unusable: true,
    })
}

/// The first source of the course: five paragraphs of two sentences.
pub(super) const SPACING: &str = "In a 2006 study by Cepeda and colleagues, learners who spread their study over several sessions recalled 14% more a week later than learners who studied in one session. All of the learners were adults.\n\nThe benefit held in laboratory settings. It was smaller for material that was already well known.\n\nSpacing works best when each session ends with a test. Testing may strengthen memory more than rereading does.\n\nThe researchers did not study children, and they cautioned that classroom results could differ. Later work found the same pattern for vocabulary.\n\nGains were largest when the gap between sessions was about a tenth of the time the material had to be kept. Gaps much shorter than that helped little.\n";

/// The second source of the course.
pub(super) const SLEEP: &str = "Sleep after learning helps memory. In one study, people who slept after studying word pairs recalled more of them than people who stayed awake.\n\nThe effect was strongest for pairs that had been learned only weakly. Sleep deprivation the night before learning reduced how much was learned.\n";

/// The passage of [`SPACING`] a drill of four sentences takes first: its first two paragraphs.
pub(super) fn spacing_passage() -> SourcePassage {
    let end = SPACING
        .find("\n\nSpacing works best")
        .unwrap_or(SPACING.len());
    SourcePassage {
        version_id: "spacing-v1".into(),
        start_byte: 0,
        end_byte: end,
        title: "Spacing and memory".into(),
        text: SPACING.get(..end).unwrap_or_default().to_owned(),
    }
}

pub(super) const FIGURE: &str = "fidelity.misquoted_figure";
pub(super) const REVERSED: &str = "reasoning.reversed_relation";
pub(super) const CONTRADICTION: &str = "consistency.self_contradiction";

pub(super) const FIRST: &str = "Spreading study over several sessions improved recall by 14% a week later in a 2006 study by Cepeda and colleagues.";
pub(super) const SECOND: &str = "The benefit was found in adults in laboratory settings.";
pub(super) const THIRD: &str = "The benefit was smaller for material learners already knew well.";
pub(super) const FOURTH: &str = "Ending each session with a test makes spacing work best.";
pub(super) const FIRST_PLANTED: &str = "Spreading study over several sessions improved recall by 40% a week later in a 2006 study by Cepeda and colleagues.";
pub(super) const THIRD_PLANTED: &str =
    "The benefit was larger for material learners already knew well.";

/// The words of [`SPACING`] the first sentence rests on.
pub(super) const FIGURE_QUOTE: &str = "recalled 14% more a week later";
/// The words of [`SPACING`] the third sentence rests on.
pub(super) const SMALLER_QUOTE: &str = "It was smaller for material that was already well known.";

/// The explanation of [`spacing_passage`]: lines 1 and 2, a blank line, then lines 4 and 5.
pub(super) fn explanation() -> String {
    format!("{FIRST}\n{SECOND}\n{THIRD}\n{FOURTH}\n")
}

pub(super) fn explanation_files() -> Vec<DrillFile> {
    vec![DrillFile {
        path: "explanation.md".into(),
        text: explanation(),
    }]
}

/// What the model answers to the first call of a passage drill: [`explanation`] in two paragraphs.
pub(super) fn explanation_answer() -> String {
    json!({
        "title": "Why spacing study sessions helps",
        "paragraphs": [format!("{FIRST} {SECOND}"), format!("{THIRD} {FOURTH}")]
    })
    .to_string()
}

/// The reason [`teaches_answer`] gives for a passage that teaches the lesson.
pub(super) const TEACHES_REASON: &str = "The passage explains the lesson's subject.";

/// The reason [`teaches_answer`] gives for a passage that does not.
pub(super) const NOT_TEACHES_REASON: &str = "The passage is about something else.";

/// What the model answers to the passage check: `teaches`, with a reason.
pub(super) fn teaches_answer(teaches: bool) -> String {
    json!({
        "reason": if teaches { TEACHES_REASON } else { NOT_TEACHES_REASON },
        "passageTeachesTheLesson": teaches
    })
    .to_string()
}

/// The reason [`code_check_answer`] gives for a lesson that teaches what code can show.
pub(super) const CODE_TEACHES_REASON: &str =
    "The lesson teaches a rule a program can follow or break.";

/// The reason [`code_check_answer`] gives for a lesson that does not.
pub(super) const CODE_NOT_TEACHES_REASON: &str = "The lesson teaches only what people say aloud.";

/// What the model answers to the code fit check: `teaches`, with a reason.
pub(super) fn code_check_answer(teaches: bool) -> String {
    json!({
        "reason": if teaches { CODE_TEACHES_REASON } else { CODE_NOT_TEACHES_REASON },
        "lessonTeachesWhatCodeCanShow": teaches
    })
    .to_string()
}

/// The source the planted sentences go against: the figure is 14%, and the benefit smaller for known material.
pub(super) fn spacing_check() -> ScriptedCheck {
    ScriptedCheck::new(|sentence| {
        if sentence.contains("40%") {
            contradicted(FIGURE_QUOTE, "The source gives 14%, not 40%.")
        } else if sentence.contains("larger") {
            contradicted(
                SMALLER_QUOTE,
                "The source says the benefit was smaller, not larger.",
            )
        } else if sentence.contains("14%") {
            supported(FIGURE_QUOTE)
        } else if sentence.contains("smaller") {
            supported(SMALLER_QUOTE)
        } else {
            supported("learners who spread their study over several sessions")
        }
    })
}

/// A spec for a passage drill of four sentences on the course `program_id`, planting [`FIGURE`] and [`REVERSED`].
pub(super) fn passage_spec(program_id: &str) -> DrillSpec {
    DrillSpec {
        material: Material::Passage,
        lines: 4,
        patterns: vec![FIGURE.into(), REVERSED.into()],
        program_id: Some(program_id.into()),
        lesson_id: None,
    }
}

pub(super) fn figure_mutation() -> Mutation {
    Mutation {
        pattern: FIGURE.into(),
        edit: Edit {
            path: "explanation.md".into(),
            before: format!("{FIRST}\n"),
            after: format!("{FIRST_PLANTED}\n"),
        },
        severity: Severity::Minor,
        consequence: "A reader would think spacing helps nearly three times as much as it does."
            .into(),
        explanation: "The study found 14% better recall, not 40%.".into(),
        verification: Verification::NotAsked,
        lesson_sentences: vec![],
    }
}

pub(super) fn reversed_mutation() -> Mutation {
    Mutation {
        pattern: REVERSED.into(),
        edit: Edit {
            path: "explanation.md".into(),
            before: format!("{THIRD}\n"),
            after: format!("{THIRD_PLANTED}\n"),
        },
        severity: Severity::Major,
        consequence: "A reader would space their review of what they already know best.".into(),
        explanation: "The benefit was smaller for well known material, not larger.".into(),
        verification: Verification::NotAsked,
        lesson_sentences: vec![],
    }
}

/// What the model answers to the plant call of [`passage_spec`]: the figure planted as a minor error.
pub(super) fn passage_plant_answer() -> String {
    let figure = figure_mutation();
    let reversed = reversed_mutation();
    json!({"mutations": [
        {
            "pattern": FIGURE, "path": "explanation.md",
            "before": figure.edit.before, "after": figure.edit.after,
            "severity": "minor",
            "consequence": figure.consequence,
            "explanation": figure.explanation
        },
        {
            "pattern": REVERSED, "path": "explanation.md",
            "before": reversed.edit.before, "after": reversed.edit.after,
            "severity": "major",
            "consequence": reversed.consequence,
            "explanation": reversed.explanation
        }
    ]})
    .to_string()
}

/// The clean material of a passage drill on the source version `version_id`.
pub(super) fn passage_code() -> CleanCode {
    CleanCode {
        drill_id: "00000000-0000-4000-8000-000000000002".into(),
        title: "Why spacing study sessions helps".into(),
        domain: "Spacing and memory".into(),
        files: explanation_files(),
        tests: Vec::new(),
        suite: None,
        source: Some(spacing_passage()),
        brief: None,
        basis: None,
        trail: Trail::default(),
    }
}

pub(super) fn passage_baseline() -> Baseline {
    Baseline::new(
        &passage_code(),
        &CleanReview {
            files: explanation_files(),
            repairs: Vec::new(),
            trail: Trail::default(),
        },
    )
}

/// A build of a passage drill on these fakes, over a pool with every migration.
pub(super) fn passage_build<'a>(
    db: &'a Database,
    model: &'a ScriptedModel,
    check: &'a ScriptedCheck,
    grader: &'a FakeGrader,
) -> DrillBuild<'a> {
    DrillBuild::new(
        model,
        Box::new(PassageMaterial::new(check)),
        grader,
        db,
        CancellationToken::new(),
    )
}

/// The title of the first lesson of a course from [`course`]: its subject is what [`SPACING`] says.
pub(super) const SPACING_LESSON: &str = "Spacing study over sessions";

/// The title of the second lesson of a course from [`course`]: its subject is what [`SLEEP`] says.
pub(super) const SLEEP_LESSON: &str = "Sleep after learning";

/// A course with the sources [`SPACING`] and [`SLEEP`], in that order, and its id.
pub(super) async fn course(db: &Database) -> Result<String> {
    course_of(
        db,
        &[("Spacing and memory", SPACING), ("Sleep and memory", SLEEP)],
    )
    .await
}

/// A course with `sources` (a title and a text each), in that order, and its id. The first module's two lessons are
/// [`SPACING_LESSON`] and [`SLEEP_LESSON`], so that a lesson's query finds what the fixture sources say; the second
/// module keeps the fixture's.
pub(super) async fn course_of(db: &Database, sources: &[(&str, &str)]) -> Result<String> {
    use crate::dto::LearningSourceDto;
    use crate::repository::LearningRepository;
    let mut program = crate::test_support::fixture();
    if let Some(module) = program.modules.first_mut() {
        let named = [
            (
                SPACING_LESSON,
                "Explain why spreading study over several sessions improves recall",
            ),
            (
                SLEEP_LESSON,
                "Explain how sleep after learning helps memory",
            ),
        ];
        for (lesson, (title, objective)) in module.lessons.iter_mut().zip(named) {
            lesson.title = title.into();
            lesson.objective = objective.into();
        }
    }
    program.sources = sources
        .iter()
        .enumerate()
        .map(|(position, (title, text))| LearningSourceDto {
            id: format!("source-{position}-{}", uuid::Uuid::new_v4()),
            title: (*title).into(),
            url: None,
            excerpt: (*text).into(),
            acquired_at: i64::try_from(position).unwrap_or(0) + 1,
        })
        .collect();
    LearningRepository::new(db.clone()).create(&program).await?;
    Ok(program.summary.id)
}

/// The id of the lesson of the course `program_id` titled `title`.
pub(super) async fn lesson_titled(
    pool: &SqlitePool,
    program_id: &str,
    title: &str,
) -> Result<String> {
    sqlx::query_scalar("SELECT id FROM learning_lessons WHERE program_id=? AND title=?")
        .bind(program_id)
        .bind(title)
        .fetch_one(pool)
        .await
        .map_err(AppError::from)
}

/// Gives the lesson `lesson_id` these teaching blocks, in order: a kind, a body and the source versions it cites each.
pub(super) async fn add_blocks(
    pool: &SqlitePool,
    lesson_id: &str,
    blocks: &[(&str, &str, &[&str])],
) -> Result<()> {
    for (ordinal, (kind, body, cited)) in blocks.iter().enumerate() {
        sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json) VALUES(?,?,?,?,?,?)")
            .bind(lesson_id)
            .bind(i64::try_from(ordinal).unwrap_or(0))
            .bind(*kind)
            .bind("Title")
            .bind(*body)
            .bind(serde_json::to_string(cited).unwrap_or_default())
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// [`add_blocks`], and the lesson is prepared.
pub(super) async fn prepare_lesson(
    pool: &SqlitePool,
    lesson_id: &str,
    blocks: &[(&str, &str, &[&str])],
) -> Result<()> {
    add_blocks(pool, lesson_id, blocks).await?;
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=?")
        .bind(lesson_id)
        .execute(pool)
        .await
        .map(drop)
        .map_err(AppError::from)
}

/// A spec for code in Python on a new course from [`course`], built from its spacing lesson.
pub(super) async fn code_spec(pool: &SqlitePool, db: &Database) -> Result<DrillSpec> {
    let program = course(db).await?;
    let lesson = lesson_titled(pool, &program, SPACING_LESSON).await?;
    Ok(DrillSpec {
        program_id: Some(program),
        lesson_id: Some(lesson),
        ..spec()
    })
}

/// [`passage_spec`] for the course `program_id`, built from its lesson titled `lesson_title`.
pub(super) async fn passage_lesson_spec(
    pool: &SqlitePool,
    program_id: &str,
    lesson_title: &str,
) -> Result<DrillSpec> {
    Ok(DrillSpec {
        lesson_id: Some(lesson_titled(pool, program_id, lesson_title).await?),
        ..passage_spec(program_id)
    })
}

/// The lessons of the course `program_id`, in course order.
pub(super) async fn lessons_of(pool: &SqlitePool, program_id: &str) -> Result<Vec<String>> {
    sqlx::query_scalar("SELECT l.id FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? ORDER BY m.ordinal,l.ordinal")
        .bind(program_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::from)
}

/// Records that the lesson `lesson_id` of the course `program_id` does not fit `material`, on the lesson's basis now.
/// Code is turned down by a build whose code fit check answers no.
pub(super) async fn record_unfit(
    db: &Database,
    program_id: &str,
    lesson_id: &str,
    material: &Material,
) -> Result<()> {
    if material.language().is_some() {
        let spec = DrillSpec {
            material: material.clone(),
            program_id: Some(program_id.to_owned()),
            lesson_id: Some(lesson_id.to_owned()),
            ..spec()
        };
        let model = ScriptedModel::new([code_check_answer(false)]);
        let grader = idle_grader();
        return match build(db, &model, None, &grader).clean_code(&spec).await {
            Err(AppError::InvalidInput(_)) => Ok(()),
            other => Err(AppError::InternalError(format!(
                "The code fit check did not turn the lesson down: {:?}",
                other.map(|_| ())
            ))),
        };
    }
    let mut tx = db.begin_write().await?;
    let basis = schedules::fit_basis_in(&mut tx, program_id, lesson_id).await?;
    schedules::record_fit_in(
        &mut tx,
        program_id,
        lesson_id,
        material,
        false,
        &basis,
        now(),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Sets the curriculum state of the lesson `lesson_id`, such as `skipped` or `replaced`.
pub(super) async fn set_state(pool: &SqlitePool, lesson_id: &str, state: &str) -> Result<()> {
    sqlx::query("UPDATE learning_lessons SET curriculum_state=? WHERE id=?")
        .bind(state)
        .bind(lesson_id)
        .execute(pool)
        .await
        .map(drop)
        .map_err(AppError::from)
}

/// The patterns of a drill's key, each once and in the key's order, as the schedule holds them back while the drill
/// is out: pending, with no date.
pub(super) fn held_back(drill: &Drill) -> Vec<PatternDue> {
    let mut held: Vec<PatternDue> = Vec::new();
    for issue in &drill.key {
        if held.iter().all(|state| state.pattern != issue.pattern) {
            held.push(PatternDue {
                pattern: issue.pattern.clone(),
                due_at: None,
                pending: true,
            });
        }
    }
    held
}

/// A row in the pattern schedule of `material` (`code:python`, `passage`) for `pattern`, due at `due_at`.
pub(super) async fn schedule_pattern(
    pool: &SqlitePool,
    material: &str,
    pattern: &str,
    due_at: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO learning_drill_pattern_schedules(material,pattern_id,stability,difficulty,last_reviewed_at,due_at,interval_days,review_count,lapses) VALUES(?,?,1.0,5.0,0,?,1,1,0)")
        .bind(material)
        .bind(pattern)
        .bind(due_at)
        .execute(pool)
        .await
        .map(drop)
        .map_err(AppError::from)
}
