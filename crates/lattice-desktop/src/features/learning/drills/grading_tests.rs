//! The judge against a scripted model: what it is asked, which answers it accepts and what it asks again.

use super::*;
use crate::features::learning::lessons::structured_call::task_messages;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::domain::learning::drills::{
    overlapping, pattern, Drill, DrillFile, Edit, Finding, FindingOutcome, KeyIssue, LessonLink,
    LessonSentence, LineSpan, Material, Origin, Proof, Severity,
};
use std::collections::VecDeque;
use std::sync::Mutex;

/// Answers the prose step of a two-step call with a draft and every schema step with the next scripted answer, and
/// keeps what it was sent.
struct ScriptedJudge {
    window: usize,
    answers: Mutex<VecDeque<String>>,
    sent: Mutex<Vec<CompletionRequest>>,
}

impl ScriptedJudge {
    fn answering(answers: Vec<String>) -> Self {
        Self {
            window: 64_000,
            answers: Mutex::new(answers.into()),
            sent: Mutex::new(Vec::new()),
        }
    }

    fn with_window(mut self, window: usize) -> Self {
        self.window = window;
        self
    }

    fn sent(&self) -> Vec<CompletionRequest> {
        self.sent.lock().unwrap().clone()
    }

    /// The requests that asked for an answer under the schema, not for a draft.
    fn asked(&self) -> Vec<CompletionRequest> {
        self.sent()
            .into_iter()
            .filter(|request| request.json_schema.is_some())
            .collect()
    }
}

#[async_trait::async_trait]
impl LLMPort for ScriptedJudge {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.sent.lock().unwrap().push(request.clone());
        if request.json_schema.is_none() {
            return Ok(CompletionResponse::from_text(
                "A draft of the grade, worked through in prose.",
            ));
        }
        let answer = self.answers.lock().unwrap().pop_front();
        answer
            .map(CompletionResponse::from_text)
            .ok_or_else(|| AppError::Network("the scripted judge has no answer left".into()))
    }
    fn model_name(&self) -> &str {
        "scripted-judge"
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn span(path: &str, start: u32, end: u32) -> LineSpan {
    LineSpan {
        path: path.into(),
        start,
        end,
    }
}

fn file(path: &str, text: &str) -> DrillFile {
    DrillFile {
        path: path.into(),
        text: text.into(),
    }
}

fn key_issue(pattern: &str, severity: Severity, span: LineSpan, consequence: &str) -> KeyIssue {
    KeyIssue {
        id: format!("{pattern}-{}", span.start),
        pattern: pattern.into(),
        severity,
        fix: Edit {
            path: span.path.clone(),
            before: "flawed".into(),
            after: "sound".into(),
        },
        span,
        consequence: consequence.into(),
        explanation: "Say what is wrong and what it costs.".into(),
        proof: Proof::Judged {
            reason: "The edit introduces it.".into(),
        },
        origin: Origin::Planted,
        lesson: None,
    }
}

fn finding(span: Option<LineSpan>, text: &str, severity: Option<Severity>) -> Finding {
    Finding {
        span,
        text: text.into(),
        severity,
    }
}

const ORDERS: &str = "def total(prices):\n    result = 0\n    for i in range(len(prices) + 1):\n        result += prices[i]\n    return result\n\n\ndef find(db, name):\n    return db.execute(f\"SELECT * FROM orders WHERE name = '{name}'\")\n";
const NOTIFY: &str =
    "def send(mail):\n    try:\n        mail.deliver()\n    except Exception:\n        pass\n";
const CLEAN_ORDERS: &str = "def total(prices):\n    result = 0\n    for i in range(len(prices)):\n        result += prices[i]\n    return result\n\n\ndef find(db, name):\n    return db.execute(\"SELECT * FROM orders WHERE name = ?\", (name,))\n";
const CLEAN_NOTIFY: &str = "def send(mail):\n    try:\n        mail.deliver()\n    except Exception as error:\n        log.warning(\"delivery failed: %s\", error)\n";

/// A drill of two files with three planted issues in severity order, its clean code and four findings: one on each of
/// the first two issues, a general one, and one over the third.
struct Scene {
    drill: Drill,
    clean: Vec<DrillFile>,
    findings: Vec<Finding>,
}

impl Scene {
    fn new() -> Self {
        Self {
            drill: Drill {
                id: "orders".into(),
                material: Material::code("python"),
                title: "Orders".into(),
                files: vec![file("orders.py", ORDERS), file("notify.py", NOTIFY)],
                key: vec![
                    key_issue(
                        "correctness.off_by_one",
                        Severity::Critical,
                        span("orders.py", 3, 3),
                        "The last pass reads past the end of prices and raises IndexError.",
                    ),
                    key_issue(
                        "security.sql_injection",
                        Severity::Critical,
                        span("orders.py", 9, 9),
                        "A name with a quote in it changes the query.",
                    ),
                    key_issue(
                        "errors.swallowed_exception",
                        Severity::Minor,
                        span("notify.py", 4, 5),
                        "A failed delivery is never noticed.",
                    ),
                ],
            },
            clean: vec![
                file("orders.py", CLEAN_ORDERS),
                file("notify.py", CLEAN_NOTIFY),
            ],
            findings: vec![
                finding(
                    Some(span("orders.py", 3, 3)),
                    "The loop runs one step too far, so the last pass indexes past the end of prices.",
                    Some(Severity::Major),
                ),
                finding(
                    Some(span("orders.py", 9, 9)),
                    "Building SQL from name lets a crafted name change the query.",
                    None,
                ),
                finding(None, "Nothing here is tested.", Some(Severity::Minor)),
                finding(
                    Some(span("notify.py", 3, 5)),
                    "This hides failures.\nNobody will ever know a mail did not go out.",
                    None,
                ),
            ],
        }
    }

    /// The same, with the key in the order swallowed exception (minor), SQL injection (critical), off-by-one
    /// (major), so that the numbers of the task are not the key's positions.
    fn unsorted() -> Self {
        let mut scene = Self::new();
        let mut key = scene.drill.key.clone();
        key[0].severity = Severity::Major;
        key.swap(0, 2);
        scene.drill.key = key;
        scene
    }

    fn review(&self) -> Review<'_> {
        Review {
            drill: &self.drill,
            clean: &self.clean,
            source: None,
            findings: &self.findings,
        }
    }
}

async fn grade(llm: &ScriptedJudge, scene: &Scene) -> Result<Assessment> {
    JudgeGrader::new(llm).assess(&scene.review()).await
}

fn issue_entry(
    issue: usize,
    reason: &str,
    outcome: &str,
    finding: Option<usize>,
    quote: Option<&str>,
) -> Value {
    json!({"issue": issue, "reason": reason, "outcome": outcome, "finding": finding, "quote": quote})
}

fn finding_entry(finding: usize, reason: &str, outcome: &str, issues: &[usize]) -> Value {
    json!({"finding": finding, "reason": reason, "outcome": outcome, "issues": issues})
}

/// An answer for the scene that `assessment` is the grade of: the first two issues found, the third half found, the
/// general finding a false positive.
fn valid() -> Value {
    json!({
        "issues": [
            issue_entry(1, "Finding 1 says the loop runs one step too far.", "found", Some(1), Some("indexes past the end of prices")),
            issue_entry(2, "Finding 2 names the injection.", "found", Some(2), Some("lets a crafted name change the query")),
            issue_entry(3, "Finding 4 points at the swallowed error without saying what it costs.", "half_found", Some(4), None),
        ],
        "findings": [
            finding_entry(1, "It names the off-by-one.", "matches", &[1]),
            finding_entry(2, "It names the injection.", "matches", &[2]),
            finding_entry(3, "A remark about tests, not a problem of this code.", "false_positive", &[]),
            finding_entry(4, "It points at the swallowed error.", "matches", &[3]),
        ]
    })
}

fn assessment() -> Assessment {
    Assessment {
        issues: vec![
            (
                IssueOutcome::Found { finding: 0 },
                "Finding 1 says the loop runs one step too far.".into(),
            ),
            (
                IssueOutcome::Found { finding: 1 },
                "Finding 2 names the injection.".into(),
            ),
            (
                IssueOutcome::HalfFound { finding: 3 },
                "Finding 4 points at the swallowed error without saying what it costs.".into(),
            ),
        ],
        findings: vec![
            (
                FindingOutcome::Matches { issues: vec![0] },
                "It names the off-by-one.".into(),
            ),
            (
                FindingOutcome::Matches { issues: vec![1] },
                "It names the injection.".into(),
            ),
            (
                FindingOutcome::FalsePositive,
                "A remark about tests, not a problem of this code.".into(),
            ),
            (
                FindingOutcome::Matches { issues: vec![2] },
                "It points at the swallowed error.".into(),
            ),
        ],
    }
}

/// `valid()` with `edit` made to it.
fn changed(edit: impl FnOnce(&mut Value)) -> Value {
    let mut answer = valid();
    edit(&mut answer);
    answer
}

fn remove(answer: &mut Value, list: &str, at: usize) {
    if let Some(entries) = answer[list].as_array_mut() {
        entries.remove(at);
    }
}

fn repeat(answer: &mut Value, list: &str, at: usize) {
    let copy = answer[list][at].clone();
    if let Some(entries) = answer[list].as_array_mut() {
        entries.push(copy);
    }
}

fn messages(request: &CompletionRequest) -> Vec<(&str, &str)> {
    request
        .input
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { role, content } => Some((role.as_str(), content.as_str())),
            _ => None,
        })
        .collect()
}

/// What the request asking again says after a refusal for `message`.
fn reask(message: &str) -> String {
    format!("That answer could not be used: {message} Answer the same task again, fixing that.")
}

/// The last message of a request, which in a request asking again is the refusal.
fn last_user_message(request: &CompletionRequest) -> String {
    messages(request)
        .last()
        .map(|(_, content)| (*content).to_owned())
        .unwrap_or_default()
}

#[tokio::test]
async fn a_valid_answer_is_the_assessment_in_key_and_finding_order() -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    let graded = grade(&llm, &Scene::new()).await?;
    assert_eq!(graded, assessment());
    assert_eq!(llm.asked().len(), 1, "one call grades a review");
    Ok(())
}

#[tokio::test]
async fn entries_may_come_in_any_order() -> Result<()> {
    let answer = changed(|answer| {
        for list in ["issues", "findings"] {
            if let Some(entries) = answer[list].as_array_mut() {
                entries.reverse();
            }
        }
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    assert_eq!(grade(&llm, &Scene::new()).await?, assessment());
    assert_eq!(llm.asked().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_reason_is_kept_as_the_judge_wrote_it_without_the_space_around_it() -> Result<()> {
    let answer = changed(|answer| {
        answer["issues"][1]["reason"] = json!("  Finding 2 names the injection.\n");
        answer["findings"][2]["reason"] =
            json!("\tA remark about tests, not a problem of this code. ");
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    assert_eq!(grade(&llm, &Scene::new()).await?, assessment());
    Ok(())
}

#[tokio::test]
async fn a_finding_that_names_two_issues_matches_both_and_the_issues_are_listed_once_in_order(
) -> Result<()> {
    let answer = changed(|answer| {
        answer["issues"][1] = issue_entry(
            2,
            "Finding 1 also says the query is built from the name.",
            "found",
            Some(1),
            Some("The loop runs one step too far"),
        );
        answer["findings"][0] = finding_entry(1, "It names both.", "matches", &[2, 1, 2]);
        answer["findings"][1] = finding_entry(2, "It is wrong.", "false_positive", &[]);
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    let graded = grade(&llm, &Scene::new()).await?;
    assert_eq!(graded.issues[1].0, IssueOutcome::Found { finding: 0 });
    assert_eq!(
        graded.findings[0].0,
        FindingOutcome::Matches { issues: vec![0, 1] }
    );
    assert_eq!(graded.findings[1].0, FindingOutcome::FalsePositive);
    assert_eq!(llm.asked().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_quote_is_verbatim_as_the_rationale_check_decides() -> Result<()> {
    for (name, quote) in [
        (
            "whitespace is normalised",
            "indexes  past\nthe end of prices",
        ),
        (
            "it may be the whole text",
            "The loop runs one step too far, so the last pass indexes past the end of prices.",
        ),
        ("it may be padded", "  indexes past the end of prices\n"),
    ] {
        let answer = changed(|answer| answer["issues"][0]["quote"] = json!(quote));
        let llm = ScriptedJudge::answering(vec![answer.to_string()]);
        assert_eq!(grade(&llm, &Scene::new()).await?, assessment(), "{name}");
        assert_eq!(llm.asked().len(), 1, "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn a_half_found_issue_needs_no_quote() -> Result<()> {
    for (name, quote) in [
        ("null", json!(null)),
        ("empty", json!("")),
        ("blank", json!("  \n")),
        ("exact", json!("Nobody will ever know")),
    ] {
        let answer = changed(|answer| answer["issues"][2]["quote"] = quote);
        let llm = ScriptedJudge::answering(vec![answer.to_string()]);
        assert_eq!(grade(&llm, &Scene::new()).await?, assessment(), "{name}");
        assert_eq!(llm.asked().len(), 1, "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn what_a_missed_issue_says_of_a_finding_and_a_quote_is_ignored() -> Result<()> {
    let answer = changed(|answer| {
        answer["issues"][2] = issue_entry(
            3,
            "No finding names it.",
            "missed",
            Some(4),
            Some("This hides failures."),
        );
        answer["findings"][3] = finding_entry(4, "A remark.", "false_positive", &[]);
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    let graded = grade(&llm, &Scene::new()).await?;
    assert_eq!(graded.issues[2].0, IssueOutcome::Missed);
    assert_eq!(graded.findings[3].0, FindingOutcome::FalsePositive);
    assert_eq!(llm.asked().len(), 1);
    Ok(())
}

/// Every way an answer can be refused, with the answer and the words the refusal says. Each is asked again once with
/// the refusal, and the valid answer after it is accepted.
fn refusals() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        (
            "an issue with no entry",
            changed(|answer| remove(answer, "issues", 2)),
            "Issue 3 has no entry: every issue needs exactly one.",
        ),
        (
            "an issue with two entries",
            changed(|answer| repeat(answer, "issues", 0)),
            "Issue 1 has 2 entries: every issue needs exactly one.",
        ),
        (
            "an entry for an issue the key does not have",
            changed(|answer| answer["issues"][2]["issue"] = json!(4)),
            "Issue 4 is not an issue of the key, which has issues 1 to 3. Issue 3 has no entry: every issue needs exactly one.",
        ),
        (
            "an entry for issue zero",
            changed(|answer| answer["issues"][0]["issue"] = json!(0)),
            "Issue 0 is not an issue of the key, which has issues 1 to 3. Issue 1 has no entry: every issue needs exactly one.",
        ),
        (
            "an entry for issue minus one",
            changed(|answer| answer["issues"][0]["issue"] = json!(-1)),
            "Issue -1 is not an issue of the key, which has issues 1 to 3. Issue 1 has no entry: every issue needs exactly one.",
        ),
        (
            "a finding with no entry",
            changed(|answer| remove(answer, "findings", 1)),
            "Finding 2 has no entry: every finding needs exactly one.",
        ),
        (
            "a finding with two entries",
            changed(|answer| repeat(answer, "findings", 2)),
            "Finding 3 has 2 entries: every finding needs exactly one.",
        ),
        (
            "an entry for a finding the review does not have",
            changed(|answer| answer["findings"][3]["finding"] = json!(5)),
            "Finding 5 is not a finding of the review, which has findings 1 to 4. Finding 4 has no entry: every finding needs exactly one.",
        ),
        (
            "an entry for finding zero",
            changed(|answer| answer["findings"][0]["finding"] = json!(0)),
            "Finding 0 is not a finding of the review, which has findings 1 to 4. Finding 1 has no entry: every finding needs exactly one.",
        ),
        (
            "a missing entry among the issues and one among the findings",
            changed(|answer| {
                remove(answer, "issues", 2);
                remove(answer, "findings", 1);
            }),
            "Issue 3 has no entry: every issue needs exactly one. Finding 2 has no entry: every finding needs exactly one.",
        ),
        (
            "an issue without a reason",
            changed(|answer| answer["issues"][1]["reason"] = json!("   ")),
            "Issue 2 has no reason: say why before giving its outcome.",
        ),
        (
            "a finding without a reason",
            changed(|answer| answer["findings"][2]["reason"] = json!("")),
            "Finding 3 has no reason: say why before giving its outcome.",
        ),
        (
            "a found issue that names no finding",
            changed(|answer| answer["issues"][0]["finding"] = json!(null)),
            "Issue 1 is found but names no finding: give the finding that names its cause.",
        ),
        (
            "a half found issue that names no finding",
            changed(|answer| answer["issues"][2]["finding"] = json!(null)),
            "Issue 3 is half found but names no finding: give the finding that points at it.",
        ),
        (
            "an issue found by a finding the review does not have",
            changed(|answer| answer["issues"][0]["finding"] = json!(9)),
            "Issue 1 is found by finding 9, but the review has findings 1 to 4.",
        ),
        (
            "an issue found by finding zero",
            changed(|answer| answer["issues"][0]["finding"] = json!(0)),
            "Issue 1 is found by finding 0, but the review has findings 1 to 4.",
        ),
        (
            "a find with no quote",
            changed(|answer| answer["issues"][0]["quote"] = json!(null)),
            "Issue 1 is found by finding 1 but has no quote: quote the words of finding 1 that name its cause.",
        ),
        (
            "a find with a blank quote",
            changed(|answer| answer["issues"][0]["quote"] = json!(" \n")),
            "Issue 1 is found by finding 1 but has no quote: quote the words of finding 1 that name its cause.",
        ),
        (
            "a quote that is not in the finding",
            changed(|answer| answer["issues"][1]["quote"] = json!("allows SQL injection")),
            "The quote for issue 2, \"allows SQL injection\", is not in the text of finding 2: copy the words that name the cause exactly as finding 2 wrote them.",
        ),
        (
            "a quote in other letters",
            changed(|answer| answer["issues"][0]["quote"] = json!("Indexes past the end of prices")),
            "The quote for issue 1, \"Indexes past the end of prices\", is not in the text of finding 1: copy the words that name the cause exactly as finding 1 wrote them.",
        ),
        (
            "a quote from the code shown beside the finding",
            changed(|answer| answer["issues"][0]["quote"] = json!("range(len(prices) + 1)")),
            "The quote for issue 1, \"range(len(prices) + 1)\", is not in the text of finding 1: copy the words that name the cause exactly as finding 1 wrote them.",
        ),
        (
            "a quote of another finding",
            changed(|answer| answer["issues"][0]["quote"] = json!("lets a crafted name change the query")),
            "The quote for issue 1, \"lets a crafted name change the query\", is not in the text of finding 1: copy the words that name the cause exactly as finding 1 wrote them.",
        ),
        (
            "a half find with a quote that is not in the finding",
            changed(|answer| answer["issues"][2]["quote"] = json!("Invented words")),
            "The quote for issue 3, \"Invented words\", is not in the text of finding 4: copy the words that name the cause exactly as finding 4 wrote them.",
        ),
        (
            "an issue found by a finding that is a false positive",
            changed(|answer| answer["findings"][0] = finding_entry(1, "It is wrong.", "false_positive", &[])),
            "Issue 1 is found by finding 1, but finding 1 does not match issue 1: list issue 1 among the issues finding 1 matches, or give issue 1 another outcome.",
        ),
        (
            "an issue half found by a finding that is a valid extra",
            changed(|answer| answer["findings"][3] = finding_entry(4, "A real problem.", "valid_extra", &[])),
            "Issue 3 is half found by finding 4, but finding 4 does not match issue 3: list issue 3 among the issues finding 4 matches, or give issue 3 another outcome.",
        ),
        (
            "an issue found by a finding that matches another issue",
            changed(|answer| answer["findings"][1] = finding_entry(2, "It names the swallowed error.", "matches", &[3])),
            "Issue 2 is found by finding 2, but finding 2 does not match issue 2: list issue 2 among the issues finding 2 matches, or give issue 2 another outcome.",
        ),
        (
            "a finding that matches no issue",
            changed(|answer| answer["findings"][2] = finding_entry(3, "It matches.", "matches", &[])),
            "Finding 3 matches no issue: list the issues it matches, or call it a valid extra or a false positive.",
        ),
        (
            "a finding that matches without listing issues",
            changed(|answer| answer["findings"][2] = json!({"finding": 3, "reason": "It matches.", "outcome": "matches"})),
            "Finding 3 matches no issue: list the issues it matches, or call it a valid extra or a false positive.",
        ),
        (
            "a finding that matches an issue that is missed",
            changed(|answer| {
                answer["issues"][2] = issue_entry(3, "No finding names it.", "missed", None, None);
            }),
            "Finding 4 matches issue 3, but issue 3 is missed: credit issue 3 to a finding that earns it, or take it out of finding 4's issues.",
        ),
        (
            "a finding that matches an issue the key does not have",
            changed(|answer| answer["findings"][0] = finding_entry(1, "It names two.", "matches", &[1, 6])),
            "Finding 1 matches issue 6, but the key has issues 1 to 3.",
        ),
        (
            "a finding that matches issue zero",
            changed(|answer| answer["findings"][0] = finding_entry(1, "It names two.", "matches", &[1, 0])),
            "Finding 1 matches issue 0, but the key has issues 1 to 3.",
        ),
        (
            "a finding that matches issue minus one",
            changed(|answer| {
                answer["findings"][0] = json!({"finding": 1, "reason": "It names one.", "outcome": "matches", "issues": [-1]});
            }),
            "Finding 1 matches issue -1, but the key has issues 1 to 3.",
        ),
        (
            "a reason missing and a quote missing",
            changed(|answer| {
                answer["issues"][0]["reason"] = json!("");
                answer["issues"][1]["quote"] = json!(null);
            }),
            "Issue 1 has no reason: say why before giving its outcome. Issue 2 is found by finding 2 but has no quote: quote the words of finding 2 that name its cause.",
        ),
        (
            "a quote missing and a finding that does not match the issue it credits",
            changed(|answer| {
                answer["issues"][1]["quote"] = json!(null);
                answer["findings"][3] = finding_entry(4, "A real problem.", "valid_extra", &[]);
            }),
            "Issue 2 is found by finding 2 but has no quote: quote the words of finding 2 that name its cause. Issue 3 is half found by finding 4, but finding 4 does not match issue 3: list issue 3 among the issues finding 4 matches, or give issue 3 another outcome.",
        ),
    ]
}

#[tokio::test]
async fn each_fault_is_refused_with_what_to_change_and_the_next_answer_is_accepted() -> Result<()> {
    let scene = Scene::new();
    for (name, faulty, message) in refusals() {
        let llm = ScriptedJudge::answering(vec![faulty.to_string(), valid().to_string()]);
        let graded = match grade(&llm, &scene).await {
            Ok(graded) => graded,
            Err(error) => panic!("{name}: {error}"),
        };
        assert_eq!(graded, assessment(), "{name}");
        let asked = llm.asked();
        assert_eq!(asked.len(), 2, "{name}: the answer is asked for once more");
        assert_eq!(
            last_user_message(&asked[1]),
            reask(message),
            "{name}: the refusal says what to change"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_is_refused_and_asked_for_again() -> Result<()> {
    for (name, unreadable, said) in [
        (
            "not JSON at all",
            "The grade is mostly right.".to_owned(),
            "The answer could not be read:",
        ),
        (
            "a field missing",
            changed(|answer| {
                if let Some(entry) = answer["issues"][0].as_object_mut() {
                    entry.remove("reason");
                }
            })
            .to_string(),
            "The answer could not be read: missing field `reason`",
        ),
        (
            "an outcome that is not one of the three",
            changed(|answer| answer["issues"][0]["outcome"] = json!("partly")).to_string(),
            "The answer could not be read: unknown variant `partly`",
        ),
        (
            "a number written as text",
            changed(|answer| answer["issues"][0]["issue"] = json!("0")).to_string(),
            "The answer could not be read: invalid type: string \"0\"",
        ),
    ] {
        let llm = ScriptedJudge::answering(vec![unreadable, valid().to_string()]);
        assert_eq!(grade(&llm, &Scene::new()).await?, assessment(), "{name}");
        let asked = llm.asked();
        assert_eq!(asked.len(), 2, "{name}");
        let again = last_user_message(&asked[1]);
        assert!(again.contains(said), "{name}: {again}");
    }
    Ok(())
}

#[tokio::test]
async fn a_second_refusal_is_the_calls_error_with_its_own_reason() {
    let first = changed(|answer| remove(answer, "issues", 2));
    let second = changed(|answer| remove(answer, "findings", 1));
    let llm = ScriptedJudge::answering(vec![first.to_string(), second.to_string()]);
    let error = grade(&llm, &Scene::new()).await.unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason == "Finding 2 has no entry: every finding needs exactly one."),
        "{error:?}"
    );
    assert_eq!(llm.asked().len(), 2, "asked once more, never twice");
}

#[tokio::test]
async fn an_error_that_says_nothing_about_the_answer_is_not_asked_again() {
    let llm = ScriptedJudge::answering(vec![]);
    let error = grade(&llm, &Scene::new()).await.unwrap_err();
    assert!(matches!(error, AppError::Network(_)), "{error:?}");
    assert_eq!(llm.asked().len(), 1);
}

/// The system message and the task of the one request of a single call.
fn the_request(llm: &ScriptedJudge) -> (String, String) {
    let asked = llm.asked();
    let (system, task) = task_messages(&asked[0]);
    (system.to_owned(), task.to_owned())
}

/// Where `needle` first is in `text`, or a failure saying it is not.
fn at(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in the task:\n{text}"))
}

#[tokio::test]
async fn the_request_holds_the_files_the_clean_files_the_key_the_review_and_the_overlaps(
) -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    grade(&llm, &Scene::new()).await?;
    let (system, task) = the_request(&llm);
    assert_eq!(system, JudgeWording::of(&Material::code("python"))?.system);

    let sections = [
        "Drill: Orders (python)",
        "The code as the reviewer saw it, with line numbers:",
        "File orders.py\n1 | def total(prices):\n2 |     result = 0\n3 |     for i in range(len(prices) + 1):\n4 |         result += prices[i]\n5 |     return result\n",
        "8 | def find(db, name):\n9 |     return db.execute(f\"SELECT * FROM orders WHERE name = '{name}'\")",
        "File notify.py\n1 | def send(mail):\n2 |     try:\n3 |         mail.deliver()\n4 |     except Exception:\n5 |         pass",
        "The same code before any issue was planted, with its own line numbers:",
        "3 |     for i in range(len(prices)):\n4 |         result += prices[i]",
        "9 |     return db.execute(\"SELECT * FROM orders WHERE name = ?\", (name,))",
        "File notify.py\n1 | def send(mail):\n2 |     try:\n3 |         mail.deliver()\n4 |     except Exception as error:\n5 |         log.warning(\"delivery failed: %s\", error)",
        "The key: 3 issues, numbered 1 to 3.",
        "Issue 1: Off-by-one error (critical), orders.py line 3",
        "Issue 2: SQL injection (critical), orders.py line 9",
        "Issue 3: Swallowed exception (minor), notify.py lines 4 to 5",
        "The review: 4 findings, numbered 1 to 4, in the order they were made.",
        "Finding 1: orders.py line 3, severity given: major",
        "Finding 2: orders.py line 9\n",
        "Finding 3: no lines selected, severity given: minor",
        "Finding 4: notify.py lines 3 to 5\n",
        "Findings and issues whose lines overlap, a hint and not a verdict:",
        "Give an entry for every issue (1 to 3) and every finding (1 to 4), the reason first.",
    ];
    let mut last = 0;
    for section in sections {
        let found = at(&task, section);
        assert!(found >= last, "{section:?} is out of order");
        last = found;
    }
    Ok(())
}

#[tokio::test]
async fn every_issue_is_shown_with_its_pattern_its_consequence_and_the_lines_it_covers(
) -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    grade(&llm, &Scene::new()).await?;
    let (_, task) = the_request(&llm);
    let definition = |id: &str| {
        pattern(id)
            .map(|known| known.definition)
            .unwrap_or_default()
    };
    for expected in [
        format!(
            "Issue 1: Off-by-one error (critical), orders.py line 3\nPattern: {}\nConsequence: The last pass reads past the end of prices and raises IndexError.\nLines:\n  3 |     for i in range(len(prices) + 1):",
            definition("correctness.off_by_one")
        ),
        format!(
            "Issue 2: SQL injection (critical), orders.py line 9\nPattern: {}\nConsequence: A name with a quote in it changes the query.\nLines:\n  9 |     return db.execute(f\"SELECT * FROM orders WHERE name = '{{name}}'\")",
            definition("security.sql_injection")
        ),
        format!(
            "Issue 3: Swallowed exception (minor), notify.py lines 4 to 5\nPattern: {}\nConsequence: A failed delivery is never noticed.\nLines:\n  4 |     except Exception:\n  5 |         pass",
            definition("errors.swallowed_exception")
        ),
    ] {
        assert!(task.contains(&expected), "{expected}\n\nis not in\n\n{task}");
    }
    Ok(())
}

#[tokio::test]
async fn an_issue_tied_to_the_lesson_shows_its_sentences_after_its_pattern_and_one_without_a_link_shows_none(
) -> Result<()> {
    let mut scene = Scene::new();
    scene.drill.key[0].lesson = Some(LessonLink {
        lesson_id: "lesson-1".into(),
        sentences: vec![
            LessonSentence {
                text: "A loop stops at the length of what it walks.".into(),
                source: None,
            },
            LessonSentence {
                text: "```python\nfor i in range(len(items)):\n    use(items[i])\n```".into(),
                source: Some("Loop notes".into()),
            },
        ],
    });
    scene.drill.key[1].lesson = Some(LessonLink {
        lesson_id: "lesson-1".into(),
        sentences: Vec::new(),
    });
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    grade(&llm, &scene).await?;
    let (_, task) = the_request(&llm);
    let definition = pattern("correctness.off_by_one")
        .map(|known| known.definition)
        .unwrap_or_default();
    let expected = format!(
        "Issue 1: Off-by-one error (critical), orders.py line 3\nPattern: {definition}\nFrom the lesson:\n  A loop stops at the length of what it walks.\n  ```python\n  for i in range(len(items)):\n      use(items[i])\n  ```\nConsequence: The last pass reads past the end of prices and raises IndexError.\nLines:\n"
    );
    assert!(
        task.contains(&expected),
        "{expected}\n\nis not in\n\n{task}"
    );
    assert_eq!(
        task.matches("From the lesson:").count(),
        1,
        "a link with no sentence and an issue with no link show no block: {task}"
    );
    Ok(())
}

#[tokio::test]
async fn every_finding_is_shown_with_its_lines_quoted_its_severity_and_its_text() -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    grade(&llm, &Scene::new()).await?;
    let (_, task) = the_request(&llm);
    for expected in [
        "Finding 1: orders.py line 3, severity given: major\nLines:\n  3 |     for i in range(len(prices) + 1):\nText:\n  The loop runs one step too far, so the last pass indexes past the end of prices.",
        "Finding 2: orders.py line 9\nLines:\n  9 |     return db.execute(f\"SELECT * FROM orders WHERE name = '{name}'\")\nText:\n  Building SQL from name lets a crafted name change the query.",
        "Finding 3: no lines selected, severity given: minor\nText:\n  Nothing here is tested.",
        "Finding 4: notify.py lines 3 to 5\nLines:\n  3 |         mail.deliver()\n  4 |     except Exception:\n  5 |         pass\nText:\n  This hides failures.\n  Nobody will ever know a mail did not go out.",
    ] {
        assert!(task.contains(expected), "{expected}\n\nis not in\n\n{task}");
    }
    Ok(())
}

#[tokio::test]
async fn the_pairs_whose_lines_overlap_are_a_hint_in_the_task() -> Result<()> {
    let scene = Scene::new();
    assert_eq!(
        overlapping(&scene.findings, &scene.drill.key),
        vec![(0, 0), (1, 1), (3, 2)]
    );
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    grade(&llm, &scene).await?;
    let (_, task) = the_request(&llm);
    assert!(task.contains(
        "Findings and issues whose lines overlap, a hint and not a verdict:\nfinding 1 and issue 1\nfinding 2 and issue 2\nfinding 4 and issue 3"
    ));
    assert!(!task.contains("finding 3 and"));

    let mut apart = Scene::new();
    apart.findings = vec![finding(
        Some(span("orders.py", 6, 7)),
        "Two blank lines.",
        None,
    )];
    let llm = ScriptedJudge::answering(vec![json!({"issues": [
            issue_entry(1, "No.", "missed", None, None),
            issue_entry(2, "No.", "missed", None, None),
            issue_entry(3, "No.", "missed", None, None),
        ], "findings": [finding_entry(1, "Not a problem.", "false_positive", &[])]})
    .to_string()]);
    grade(&llm, &apart).await?;
    let (_, task) = the_request(&llm);
    assert!(task.contains("No finding's lines overlap an issue's lines."));
    assert!(!task.contains("a hint and not a verdict"));
    Ok(())
}

#[tokio::test]
async fn line_numbers_are_padded_to_the_width_of_the_files_last_line() -> Result<()> {
    let text: String = (1..=12).map(|number| format!("line {number}\n")).collect();
    let mut scene = Scene::new();
    scene.drill.files = vec![file("long.txt", &text)];
    scene.clean = vec![file("long.txt", &text)];
    scene.drill.key = vec![key_issue(
        "readability.dead_code",
        Severity::Nit,
        span("long.txt", 9, 10),
        "It is never used.",
    )];
    scene.findings = vec![finding(Some(span("long.txt", 9, 10)), "Unused.", None)];
    let answer = json!({
        "issues": [issue_entry(1, "Finding 1 says it is unused.", "half_found", Some(1), None)],
        "findings": [finding_entry(1, "It points at the lines.", "matches", &[1])],
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    grade(&llm, &scene).await?;
    let (_, task) = the_request(&llm);
    assert!(task.contains(" 8 | line 8\n 9 | line 9\n10 | line 10\n11 | line 11"));
    assert!(task.contains("Consequence: It is never used.\nLines:\n   9 | line 9\n  10 | line 10"));
    assert!(task.contains("Finding 1: long.txt lines 9 to 10\nLines:\n   9 | line 9\n  10 | line 10\nText:\n  Unused."));
    Ok(())
}

#[tokio::test]
async fn an_odd_key_and_review_are_still_shown_whole() -> Result<()> {
    let mut scene = Scene::new();
    scene.drill.key.push(key_issue(
        "no.such_pattern",
        Severity::Nit,
        span("orders.py", 2, 2),
        "It is odd.",
    ));
    scene.drill.key[3].origin = Origin::FoundInReview;
    scene.findings = vec![
        finding(
            Some(span("ghost.py", 1, 2)),
            "On a file the drill does not have.",
            None,
        ),
        finding(
            Some(span("orders.py", 40, 41)),
            "Past the end of the file.",
            None,
        ),
        finding(None, "  \n", None),
    ];
    let answer = json!({
        "issues": [
            issue_entry(1, "No.", "missed", None, None),
            issue_entry(2, "No.", "missed", None, None),
            issue_entry(3, "No.", "missed", None, None),
            issue_entry(4, "No.", "missed", None, None),
        ],
        "findings": [
            finding_entry(1, "No such file.", "false_positive", &[]),
            finding_entry(2, "No such lines.", "false_positive", &[]),
            finding_entry(3, "Says nothing.", "false_positive", &[]),
        ]
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    grade(&llm, &scene).await?;
    let (_, task) = the_request(&llm);
    assert!(task.contains("The key: 4 issues, numbered 1 to 4."));
    assert!(task.contains("Issue 4: no.such_pattern (nit), orders.py line 2\nConsequence: It is odd.\nLines:\n  2 |     result = 0"));
    assert!(!task.contains("clean code as well"));
    assert!(task.contains(&format!(
        "Issue 1: Off-by-one error (critical), orders.py line 3\nPattern: {}\nConsequence: The last pass reads past the end of prices and raises IndexError.\nLines:\n  3 |     for i in range(len(prices) + 1):",
        pattern("correctness.off_by_one").map_or("", |known| known.definition)
    )));
    assert!(!task.contains("Issue 4: no.such_pattern (nit), orders.py line 2\nPattern:"));
    assert!(task.contains(
        "Finding 1: ghost.py lines 1 to 2\nLines: ghost.py is not one of the drill's files\nText:"
    ));
    assert!(task.contains(
        "Finding 2: orders.py lines 40 to 41\nLines: none of them is in the file\nText:"
    ));
    assert!(task.contains("Finding 3: no lines selected\nText: (empty)"));
    Ok(())
}

#[tokio::test]
async fn the_answer_is_asked_for_under_the_schema_and_decoded_deterministically() -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    grade(&llm, &Scene::new()).await?;
    let asked = llm.asked();
    assert_eq!(asked[0].json_schema, Some(schema()));
    let sampling = asked[0].sampling.clone().unwrap_or_default();
    assert_eq!((sampling.temperature, sampling.top_k), (Some(0.0), Some(1)));
    Ok(())
}

#[test]
fn the_schema_asks_for_the_reason_before_the_outcome_and_bounds_no_text() {
    let schema = schema();
    for (list, number) in [("issues", "issue"), ("findings", "finding")] {
        let properties = schema["properties"][list]["items"]["properties"]
            .as_object()
            .unwrap();
        let names: Vec<&str> = properties.keys().map(String::as_str).collect();
        assert_eq!(names.first().copied(), Some(number), "{list}: {names:?}");
        let position = |name: &str| names.iter().position(|candidate| *candidate == name);
        assert!(
            position("reason") < position("outcome"),
            "{list}: the reason comes before the outcome in {names:?}"
        );
        assert!(position("reason").is_some());
    }
    let written = schema.to_string();
    for bound in ["maxLength", "maxItems", "minLength", "minItems"] {
        assert!(
            !written.contains(bound),
            "{bound} bounds what the judge may write"
        );
    }
    assert!(!written.contains("\"minimum\":0"), "{written}");
    assert_eq!(
        schema["properties"]["issues"]["items"]["properties"]["finding"]["minimum"],
        1
    );
    assert_eq!(
        schema["properties"]["findings"]["items"]["properties"]["finding"]["minimum"],
        1
    );
    assert_eq!(
        schema["properties"]["issues"]["items"]["properties"]["issue"]["minimum"],
        1
    );
    assert_eq!(
        schema["properties"]["findings"]["items"]["properties"]["issues"]["items"]["minimum"],
        1
    );
    assert_eq!(
        schema["properties"]["issues"]["items"]["properties"]["outcome"]["enum"],
        json!(["found", "half_found", "missed"])
    );
    assert_eq!(
        schema["properties"]["findings"]["items"]["properties"]["outcome"]["enum"],
        json!(["matches", "valid_extra", "false_positive"])
    );
}

#[tokio::test]
async fn a_review_with_no_findings_misses_every_issue_without_asking_the_model() -> Result<()> {
    let mut scene = Scene::new();
    scene.findings = Vec::new();
    let llm = ScriptedJudge::answering(vec![]);
    let graded = grade(&llm, &scene).await?;
    assert_eq!(
        graded,
        Assessment {
            issues: vec![(IssueOutcome::Missed, NO_FINDINGS.to_owned()); 3],
            findings: Vec::new(),
        }
    );
    assert!(llm.sent().is_empty());
    Ok(())
}

#[tokio::test]
async fn a_drill_and_review_the_window_cannot_hold_fail_without_a_call() {
    let llm = ScriptedJudge::answering(vec![valid().to_string()]).with_window(1_200);
    let error = grade(&llm, &Scene::new()).await.unwrap_err();
    assert!(
        matches!(&error, AppError::InvalidConfig(message) if message.starts_with("The drill and the review do not fit in scripted-judge's 1200-token context window.")),
        "{error:?}"
    );
    assert!(llm.sent().is_empty(), "a review is never cut to fit");
}

#[tokio::test]
async fn the_classifier_knob_decides_whether_the_call_reasons_first() -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    grade(&llm, &Scene::new()).await?;
    let expected = if model_call::REASON_FIRST_CLASSIFIERS {
        2
    } else {
        1
    };
    assert_eq!(llm.sent().len(), expected);
    assert_eq!(llm.asked().len(), 1);
    Ok(())
}

#[tokio::test]
async fn the_task_and_the_system_message_number_issues_and_findings_from_one() -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    grade(&llm, &Scene::new()).await?;
    let (system, task) = the_request(&llm);
    assert!(
        system.contains("Issues and findings are numbered from 1."),
        "{system}"
    );
    assert!(task.contains("Issue 1: Off-by-one error"), "{task}");
    assert!(task.contains("Finding 1: orders.py line 3"), "{task}");
    assert!(task.contains("3 issues, numbered 1 to 3"), "{task}");
    assert!(task.contains("4 findings, numbered 1 to 4"), "{task}");
    assert!(
        !task.contains("Issue 0") && !task.contains("Finding 0"),
        "{task}"
    );

    let mut one = Scene::new();
    one.drill.key.truncate(1);
    one.findings.truncate(1);
    let answer = json!({
        "issues": [issue_entry(1, "Finding 1 names it.", "half_found", Some(1), None)],
        "findings": [finding_entry(1, "It points at it.", "matches", &[1])],
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    grade(&llm, &one).await?;
    let (_, task) = the_request(&llm);
    assert!(task.contains("The key: 1 issue, numbered 1."), "{task}");
    assert!(
        task.contains("The review: 1 finding, numbered 1,"),
        "{task}"
    );
    assert!(
        task.contains("every issue (1) and every finding (1)"),
        "{task}"
    );
    Ok(())
}

#[tokio::test]
async fn an_answer_naming_issue_one_and_finding_one_credits_the_first_positions() -> Result<()> {
    let mut one = Scene::new();
    one.drill.key.truncate(1);
    one.findings.truncate(1);
    let answer = json!({
        "issues": [issue_entry(1, "Finding 1 names it.", "found", Some(1), Some("indexes past the end of prices"))],
        "findings": [finding_entry(1, "It names the off-by-one.", "matches", &[1])],
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    let graded = grade(&llm, &one).await?;
    assert_eq!(graded.issues[0].0, IssueOutcome::Found { finding: 0 });
    assert_eq!(
        graded.findings[0].0,
        FindingOutcome::Matches { issues: vec![0] }
    );
    assert_eq!(llm.asked().len(), 1);
    Ok(())
}

#[tokio::test]
async fn an_answer_naming_finding_zero_is_refused_in_one_based_terms() {
    let mut one = Scene::new();
    one.drill.key.truncate(1);
    one.findings.truncate(1);
    let answer = json!({
        "issues": [issue_entry(1, "Finding 0 names it.", "half_found", Some(1), None)],
        "findings": [finding_entry(0, "It points at it.", "matches", &[1])],
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string(), answer.to_string()]);
    let error = grade(&llm, &one).await.unwrap_err();
    assert!(
        matches!(&error, AppError::ModelAnswerUnusable(reason) if reason == "Finding 0 is not a finding of the review, which has finding 1 only. Finding 1 has no entry: every finding needs exactly one."),
        "{error:?}"
    );
}

/// An explanation of a passage with one error planted on its second sentence, and two findings.
fn passage_scene() -> (
    Drill,
    Vec<DrillFile>,
    Vec<Finding>,
    crate::features::learning::drills::build::SourcePassage,
) {
    let planted = "Spacing raised recall by 40%.";
    let drill = Drill {
        id: "passage".into(),
        material: Material::Passage,
        title: "Spacing".into(),
        files: vec![file(
            "explanation.md",
            &format!("Learners spread their study.\n{planted}\n\nThe gain was smaller for known material.\n"),
        )],
        key: vec![KeyIssue {
            proof: Proof::Sourced {
                quote: "recalled 14% more".into(),
                planted: lattice_core::domain::learning::drills::SourceVerdict::Contradicted,
                reason: "The source gives 14%.".into(),
            },
            ..key_issue(
                "fidelity.misquoted_figure",
                Severity::Major,
                span("explanation.md", 2, 2),
                "A reader takes away a gain nearly three times the real one.",
            )
        }],
    };
    let clean = vec![file(
        "explanation.md",
        "Learners spread their study.\nSpacing raised recall by 14%.\n\nThe gain was smaller for known material.\n",
    )];
    let findings = vec![
        finding(
            Some(span("explanation.md", 2, 2)),
            "The study found 14%, not 40%.",
            Some(Severity::Major),
        ),
        finding(None, "Nothing else is wrong.", None),
    ];
    let source = crate::features::learning::drills::build::SourcePassage {
        version_id: "spacing-v1".into(),
        start_byte: 0,
        end_byte: 61,
        title: "Spacing and memory".into(),
        text: "Learners who spread their study recalled 14% more a week later.".into(),
    };
    (drill, clean, findings, source)
}

#[tokio::test]
async fn the_task_for_a_passage_shows_numbered_sentences_and_the_source_passage() -> Result<()> {
    let (drill, clean, findings, source) = passage_scene();
    let answer = json!({
        "issues": [issue_entry(1, "Finding 1 names the figure.", "found", Some(1), Some("found 14%, not 40%"))],
        "findings": [
            finding_entry(1, "It is the figure.", "matches", &[1]),
            finding_entry(2, "A remark.", "false_positive", &[]),
        ],
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    let review = Review {
        drill: &drill,
        clean: &clean,
        source: Some(&source),
        findings: &findings,
    };
    JudgeGrader::new(&llm).assess(&review).await?;
    let (system, task) = the_request(&llm);
    assert_eq!(system, JudgeWording::of(&Material::Passage)?.system);
    let sections = [
        "Drill: Spacing (an explanation of a source passage)",
        "The explanation as the reviewer saw it, one sentence to a numbered line:\n\nFile explanation.md\n1 | Learners spread their study.\n2 | Spacing raised recall by 40%.\n3 | \n4 | The gain was smaller for known material.",
        "The same explanation before any error was planted, with its own numbers:\n\nFile explanation.md\n1 | Learners spread their study.\n2 | Spacing raised recall by 14%.",
        "The source passage the explanation was written from, in Spacing and memory:\n<passage>\nLearners who spread their study recalled 14% more a week later.\n</passage>",
        "The key: 1 issue, numbered 1.",
        "Issue 1: Misquoted figure (major), explanation.md sentence 2",
        "Sentences:\n  2 | Spacing raised recall by 40%.",
        "Finding 1: explanation.md sentence 2, severity given: major",
        "Finding 2: no sentences selected",
        "Findings and issues whose sentences overlap, a hint and not a verdict:\nfinding 1 and issue 1",
    ];
    let mut last = 0;
    for section in sections {
        let found = at(&task, section);
        assert!(found >= last, "{section:?} is out of order");
        last = found;
    }
    assert!(!task.contains("explanation.md line"), "{task}");
    Ok(())
}

#[tokio::test]
async fn the_task_for_code_shows_no_source_even_when_one_is_given() -> Result<()> {
    let scene = Scene::new();
    let source = passage_scene().3;
    let llm = ScriptedJudge::answering(vec![valid().to_string()]);
    let review = Review {
        source: Some(&source),
        ..scene.review()
    };
    JudgeGrader::new(&llm).assess(&review).await?;
    let (_, task) = the_request(&llm);
    assert!(!task.contains("<passage>"), "{task}");
    Ok(())
}

/// An answer for `Scene::unsorted`, which numbers its issues most severe first: issue 1 is the SQL injection, 2 the
/// off-by-one and 3 the swallowed exception.
fn valid_unsorted() -> Value {
    json!({
        "issues": [
            issue_entry(1, "Finding 2 names the injection.", "found", Some(2), Some("lets a crafted name change the query")),
            issue_entry(2, "Finding 1 says the loop runs one step too far.", "found", Some(1), Some("indexes past the end of prices")),
            issue_entry(3, "Finding 4 points at the swallowed error.", "half_found", Some(4), None),
        ],
        "findings": [
            finding_entry(1, "It names the off-by-one.", "matches", &[2]),
            finding_entry(2, "It names the injection.", "matches", &[1]),
            finding_entry(3, "A remark about tests, not a problem of this code.", "false_positive", &[]),
            finding_entry(4, "It points at the swallowed error.", "matches", &[3]),
        ]
    })
}

fn changed_unsorted(edit: impl FnOnce(&mut Value)) -> Value {
    let mut answer = valid_unsorted();
    edit(&mut answer);
    answer
}

#[tokio::test]
async fn the_task_numbers_the_issues_most_severe_first_whatever_the_keys_order() -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid_unsorted().to_string()]);
    grade(&llm, &Scene::unsorted()).await?;
    let (_, task) = the_request(&llm);
    let mut last = 0;
    for heading in [
        "Issue 1: SQL injection (critical), orders.py line 9",
        "Issue 2: Off-by-one error (major), orders.py line 3",
        "Issue 3: Swallowed exception (minor), notify.py lines 4 to 5",
        "The review: 4 findings",
    ] {
        let found = at(&task, heading);
        assert!(found >= last, "{heading:?} is out of order in\n{task}");
        last = found;
    }
    Ok(())
}

#[tokio::test]
async fn the_overlap_hint_names_issues_by_the_numbers_of_the_task() -> Result<()> {
    let scene = Scene::unsorted();
    assert_eq!(
        overlapping(&scene.findings, &scene.drill.key),
        vec![(0, 2), (1, 1), (3, 0)]
    );
    let llm = ScriptedJudge::answering(vec![valid_unsorted().to_string()]);
    grade(&llm, &scene).await?;
    let (_, task) = the_request(&llm);
    assert!(
        task.contains("a hint and not a verdict:\nfinding 1 and issue 2\nfinding 2 and issue 1\nfinding 4 and issue 3"),
        "{task}"
    );
    Ok(())
}

#[tokio::test]
async fn the_answers_issue_numbers_are_mapped_back_to_the_keys_positions() -> Result<()> {
    let llm = ScriptedJudge::answering(vec![valid_unsorted().to_string()]);
    let graded = grade(&llm, &Scene::unsorted()).await?;
    assert_eq!(
        graded,
        Assessment {
            issues: vec![
                (
                    IssueOutcome::HalfFound { finding: 3 },
                    "Finding 4 points at the swallowed error.".into()
                ),
                (
                    IssueOutcome::Found { finding: 1 },
                    "Finding 2 names the injection.".into()
                ),
                (
                    IssueOutcome::Found { finding: 0 },
                    "Finding 1 says the loop runs one step too far.".into()
                ),
            ],
            findings: vec![
                (
                    FindingOutcome::Matches { issues: vec![2] },
                    "It names the off-by-one.".into()
                ),
                (
                    FindingOutcome::Matches { issues: vec![1] },
                    "It names the injection.".into()
                ),
                (
                    FindingOutcome::FalsePositive,
                    "A remark about tests, not a problem of this code.".into()
                ),
                (
                    FindingOutcome::Matches { issues: vec![0] },
                    "It points at the swallowed error.".into()
                ),
            ],
        }
    );
    assert_eq!(llm.asked().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_finding_matching_issues_one_and_three_holds_their_key_positions_sorted() -> Result<()> {
    let answer = changed_unsorted(|answer| {
        answer["issues"][2] = issue_entry(
            3,
            "Finding 2 also points at the swallowed error.",
            "half_found",
            Some(2),
            None,
        );
        answer["findings"][1] = finding_entry(2, "It names two.", "matches", &[3, 1]);
        answer["findings"][3] = finding_entry(4, "A real problem.", "valid_extra", &[]);
    });
    let llm = ScriptedJudge::answering(vec![answer.to_string()]);
    let graded = grade(&llm, &Scene::unsorted()).await?;
    assert_eq!(
        graded.findings[1].0,
        FindingOutcome::Matches { issues: vec![0, 1] }
    );
    assert_eq!(graded.issues[0].0, IssueOutcome::HalfFound { finding: 1 });
    assert_eq!(graded.issues[1].0, IssueOutcome::Found { finding: 1 });
    assert_eq!(graded.issues[2].0, IssueOutcome::Found { finding: 0 });
    assert_eq!(llm.asked().len(), 1);
    Ok(())
}

#[tokio::test]
async fn refusals_name_issues_by_the_numbers_of_the_task() -> Result<()> {
    for (name, faulty, message) in [
        (
            "an issue credited to a finding that matches another",
            changed_unsorted(|answer| {
                answer["findings"][1] = finding_entry(2, "It names the swallowed error.", "matches", &[3]);
            }),
            "Issue 1 is found by finding 2, but finding 2 does not match issue 1: list issue 1 among the issues finding 2 matches, or give issue 1 another outcome.",
        ),
        (
            "a finding matching an issue that is missed",
            changed_unsorted(|answer| {
                answer["issues"][2] = issue_entry(3, "No finding names it.", "missed", None, None);
            }),
            "Finding 4 matches issue 3, but issue 3 is missed: credit issue 3 to a finding that earns it, or take it out of finding 4's issues.",
        ),
        (
            "an issue without an entry",
            changed_unsorted(|answer| remove(answer, "issues", 0)),
            "Issue 1 has no entry: every issue needs exactly one.",
        ),
    ] {
        let llm = ScriptedJudge::answering(vec![faulty.to_string(), valid_unsorted().to_string()]);
        let graded = grade(&llm, &Scene::unsorted()).await;
        assert!(graded.is_ok(), "{name}: {graded:?}");
        let asked = llm.asked();
        assert_eq!(asked.len(), 2, "{name}");
        assert_eq!(last_user_message(&asked[1]), reask(message), "{name}");
    }
    Ok(())
}
