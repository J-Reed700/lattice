#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
//! The judge against a scripted model: what it is asked, which answers it accepts and what it asks again.

use super::contract::{Context, IssueView};
use super::*;
use crate::lessons::structured_call::{is_draft_request, task_messages};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::services::model_contract::shape::Held;
use lattice_core::application::services::model_contract::test_support::samples::schema_of;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::application::services::model_contract::Contract;
use lattice_core::domain::learning::drills::{
    overlapping, pattern, Drill, DrillFile, Edit, Finding, FindingOutcome, KeyIssue, LessonLink,
    LessonSentence, LineSpan, Material, Origin, Proof, Severity,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

/// `model` with a handler for `C` that answers each ask with `answer(task, number of earlier asks)` and a draft step
/// with prose.
fn scripted_on<C: Contract>(
    model: ScriptedModel,
    answer: impl Fn(&str, usize) -> String + Send + Sync + 'static,
) -> ScriptedModel {
    let asked = AtomicUsize::new(0);
    model.on_request::<C>(move |sent, _| {
        if is_draft_request(&sent.request) {
            return reply("The grade is worked through in prose.".to_owned());
        }
        let earlier = asked.fetch_add(1, Ordering::SeqCst);
        reply(answer(first_task(&sent.request), earlier))
    })
}

/// The task the call was first asked: a correction restates it in the history, after it.
fn first_task(request: &CompletionRequest) -> &str {
    request
        .input()
        .iter()
        .find_map(|item| match item {
            CompletionInput::Message { role, content } if role == "user" => Some(content.as_str()),
            _ => None,
        })
        .unwrap_or_default()
}

/// A model that answers the question about the issues with `issues` and the one about the findings no issue claims
/// with `extras`.
fn judge(
    issues: impl Fn(&str, usize) -> String + Send + Sync + 'static,
    extras: impl Fn(&str, usize) -> String + Send + Sync + 'static,
) -> ScriptedModel {
    let model = ScriptedModel::new().named("scripted-judge");
    scripted_on::<GradeExtras>(scripted_on::<GradeReview>(model, issues), extras)
}

/// The asks of `C` the model was sent, a draft step left out.
fn asks<C: Contract>(model: &ScriptedModel) -> Vec<Sent> {
    model
        .sent()
        .into_iter()
        .filter(|sent| sent.call.id == C::ID && !is_draft_request(&sent.request))
        .collect()
}

/// The task of the first ask of `C`.
fn task_of<C: Contract>(model: &ScriptedModel) -> String {
    task_messages(&asks::<C>(model)[0].request).1.to_owned()
}

fn system_of<C: Contract>(model: &ScriptedModel) -> String {
    task_messages(&asks::<C>(model)[0].request).0.to_owned()
}

/// The handles a task shows for `label` ("Issue" or "Finding"), each with the rest of the line it is shown on, in the
/// order shown.
fn shown(task: &str, label: &str) -> Vec<(String, String)> {
    let opening = format!("{label} [");
    let mut found: Vec<(String, String)> = Vec::new();
    for line in task.lines() {
        let Some(rest) = line.strip_prefix(&opening) else {
            continue;
        };
        let Some((handle, head)) = rest.split_once("]: ") else {
            continue;
        };
        if !found.iter().any(|(known, _)| known == handle) {
            found.push((handle.to_owned(), head.to_owned()));
        }
    }
    found
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

async fn grade(llm: &ScriptedModel, scene: &Scene) -> Result<Assessment> {
    JudgeGrader::new(llm).assess(&scene.review()).await
}

const LOOP: &str = "The loop runs one step too far.";
const QUERY: &str = "The query is built from the name.";
const SWALLOWED: &str = "It points at the swallowed error without saying what it costs.";
const REMARK: &str = "A remark about tests, not a problem of this code.";

/// An entry for the issue shown as `of`.
fn judged(of: &str, reason: &str, outcome: &str, finding: &str, about: &[&str]) -> Value {
    json!({"of": of, "reason": reason, "outcome": outcome, "finding": finding, "findings": about})
}

/// An entry for the finding shown as `of`.
fn verdict(of: &str, reason: &str, outcome: &str) -> Value {
    json!({"of": of, "reason": reason, "outcome": outcome})
}

fn entries(list: Vec<Value>) -> String {
    json!({ "entries": list }).to_string()
}

/// The entries of a judge who grades the scenes of this file as `assessment()` does: the issues on lines 3 and 9
/// found by the findings on those lines, the one on notify.py half found by the finding there.
fn issue_entries(task: &str) -> Vec<Value> {
    let findings = shown(task, "Finding");
    let finding = |at: usize| {
        findings
            .get(at)
            .map(|(handle, _)| handle.clone())
            .unwrap_or_default()
    };
    shown(task, "Issue")
        .iter()
        .filter_map(|(handle, head)| {
            if head.contains("orders.py line 3") {
                Some(judged(handle, LOOP, "found", &finding(0), &[]))
            } else if head.contains("orders.py line 9") {
                Some(judged(handle, QUERY, "found", &finding(1), &[]))
            } else if head.contains("notify.py lines 4 to 5") {
                Some(judged(handle, SWALLOWED, "half_found", &finding(3), &[]))
            } else {
                None
            }
        })
        .collect()
}

fn issues_answer(task: &str, _earlier: usize) -> String {
    entries(issue_entries(task))
}

fn extras_answer(task: &str, _earlier: usize) -> String {
    entries(
        shown(task, "Finding")
            .iter()
            .map(|(handle, _)| verdict(handle, REMARK, "false_positive"))
            .collect(),
    )
}

/// A model that grades every scene as `assessment()` does.
fn standard() -> ScriptedModel {
    judge(issues_answer, extras_answer)
}

/// What `standard()` makes of `Scene::new()`.
fn assessment() -> Assessment {
    Assessment {
        issues: vec![
            (IssueOutcome::Found { finding: 0 }, LOOP.into()),
            (IssueOutcome::Found { finding: 1 }, QUERY.into()),
            (IssueOutcome::HalfFound { finding: 3 }, SWALLOWED.into()),
        ],
        findings: vec![
            (FindingOutcome::Matches { issues: vec![0] }, LOOP.into()),
            (FindingOutcome::Matches { issues: vec![1] }, QUERY.into()),
            (FindingOutcome::FalsePositive, REMARK.into()),
            (
                FindingOutcome::Matches { issues: vec![2] },
                SWALLOWED.into(),
            ),
        ],
    }
}

#[tokio::test]
async fn a_valid_answer_is_the_assessment_in_key_and_finding_order() -> Result<()> {
    let llm = standard();
    assert_eq!(grade(&llm, &Scene::new()).await?, assessment());
    assert_eq!(asks::<GradeReview>(&llm).len(), 1);
    assert_eq!(asks::<GradeExtras>(&llm).len(), 1);
    let extras = task_of::<GradeExtras>(&llm);
    assert_eq!(
        shown(&extras, "Finding").len(),
        1,
        "only the finding no issue claims is asked about: {extras}"
    );
    Ok(())
}

#[tokio::test]
async fn the_task_names_nothing_by_number() -> Result<()> {
    let llm = standard();
    grade(&llm, &Scene::new()).await?;
    for task in [task_of::<GradeReview>(&llm), task_of::<GradeExtras>(&llm)] {
        assert!(!task.contains("numbered"), "{task}");
    }
    Ok(())
}

#[tokio::test]
async fn entries_may_come_in_any_order() -> Result<()> {
    let llm = judge(
        |task, _| {
            let mut list = issue_entries(task);
            list.reverse();
            entries(list)
        },
        extras_answer,
    );
    assert_eq!(grade(&llm, &Scene::new()).await?, assessment());
    assert_eq!(asks::<GradeReview>(&llm).len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_reason_is_kept_as_the_judge_wrote_it_without_the_space_around_it() -> Result<()> {
    let llm = judge(
        |task, _| {
            let mut list = issue_entries(task);
            list[1]["reason"] = json!(format!("  {QUERY}\n"));
            entries(list)
        },
        |task, _| {
            let mut list: Vec<Value> = shown(task, "Finding")
                .iter()
                .map(|(handle, _)| verdict(handle, "", "false_positive"))
                .collect();
            list[0]["reason"] = json!(format!("\t{REMARK} "));
            entries(list)
        },
    );
    assert_eq!(grade(&llm, &Scene::new()).await?, assessment());
    Ok(())
}

#[tokio::test]
async fn a_finding_about_two_issues_matches_both_and_the_issues_are_listed_once_in_order(
) -> Result<()> {
    let llm = judge(
        |task, _| {
            let findings = shown(task, "Finding");
            let first = findings[0].0.clone();
            let issues = shown(task, "Issue");
            entries(vec![
                judged(&issues[0].0, LOOP, "found", &first, &[first.as_str()]),
                judged(
                    &issues[1].0,
                    "It also says the query is built from the name.",
                    "found",
                    &first,
                    &[],
                ),
                judged(&issues[2].0, "No finding names it.", "missed", "none", &[]),
            ])
        },
        extras_answer,
    );
    let graded = grade(&llm, &Scene::new()).await?;
    assert_eq!(graded.issues[1].0, IssueOutcome::Found { finding: 0 });
    assert_eq!(
        graded.findings[0].0,
        FindingOutcome::Matches { issues: vec![0, 1] }
    );
    assert_eq!(
        graded.findings[0].1,
        format!("{LOOP} It also says the query is built from the name.")
    );
    for position in [1, 2, 3] {
        assert_eq!(graded.findings[position].0, FindingOutcome::FalsePositive);
    }
    let extras = task_of::<GradeExtras>(&llm);
    assert_eq!(shown(&extras, "Finding").len(), 3, "{extras}");
    Ok(())
}

#[tokio::test]
async fn a_finding_listed_as_about_an_issue_it_does_not_earn_matches_it() -> Result<()> {
    let llm = judge(
        |task, _| {
            let findings = shown(task, "Finding");
            let mut list = issue_entries(task);
            list[2] = judged(
                list[2]["of"].as_str().unwrap_or_default(),
                SWALLOWED,
                "half_found",
                &findings[3].0,
                &[findings[2].0.as_str()],
            );
            entries(list)
        },
        extras_answer,
    );
    let graded = grade(&llm, &Scene::new()).await?;
    assert_eq!(
        graded.findings[2],
        (
            FindingOutcome::Matches { issues: vec![2] },
            SWALLOWED.into()
        )
    );
    assert_eq!(graded.issues[2].0, IssueOutcome::HalfFound { finding: 3 });
    assert!(
        asks::<GradeExtras>(&llm).is_empty(),
        "every finding is about an issue, so nothing is asked about extras"
    );
    Ok(())
}

#[tokio::test]
async fn what_a_missed_issue_says_of_a_finding_is_ignored() -> Result<()> {
    let llm = judge(
        |task, _| {
            let findings = shown(task, "Finding");
            let mut list = issue_entries(task);
            list[2] = judged(
                list[2]["of"].as_str().unwrap_or_default(),
                "No finding names it.",
                "missed",
                &findings[3].0,
                &[],
            );
            entries(list)
        },
        extras_answer,
    );
    let graded = grade(&llm, &Scene::new()).await?;
    assert_eq!(graded.issues[2].0, IssueOutcome::Missed);
    assert_eq!(graded.findings[3].0, FindingOutcome::FalsePositive);
    Ok(())
}

#[tokio::test]
async fn a_finding_no_issue_claims_is_a_valid_extra_or_a_false_positive_as_the_second_question_says(
) -> Result<()> {
    let llm = judge(issues_answer, |task, _| {
        entries(
            shown(task, "Finding")
                .iter()
                .map(|(handle, _)| verdict(handle, "A real problem.", "valid_extra"))
                .collect(),
        )
    });
    let graded = grade(&llm, &Scene::new()).await?;
    assert_eq!(
        graded.findings[2],
        (FindingOutcome::ValidExtra, "A real problem.".into())
    );
    Ok(())
}

/// Every way an issue's entry can be refused, with the words the correction says. The entry of the off-by-one issue is
/// made faulty by `spoil`; it is asked for again alone, with the refusal, and the valid entry after it is accepted.
fn refusals() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "an issue without a reason",
            "This issue has no reason: say why before giving its outcome.",
        ),
        (
            "a found issue that names no finding",
            "This issue is found but names no finding: give the finding that names its cause.",
        ),
        (
            "a half found issue that names no finding",
            "This issue is half found but names no finding: give the finding that points at it.",
        ),
        (
            "a missed issue with findings listed as about it",
            "This issue is missed but findings are listed as about it: credit it to the finding that earns it, or leave findings empty.",
        ),
    ]
}

/// `entry` made faulty the way `name` says, with `finding` the handle of a finding that is shown.
fn spoil(name: &str, entry: &mut Value, finding: &str) {
    match name {
        "an issue without a reason" => entry["reason"] = json!("   "),
        "a found issue that names no finding" => entry["finding"] = json!("none"),
        "a half found issue that names no finding" => {
            entry["outcome"] = json!("half_found");
            entry["finding"] = json!("none");
        }
        "a missed issue with findings listed as about it" => {
            entry["outcome"] = json!("missed");
            entry["finding"] = json!("none");
            entry["findings"] = json!([finding]);
        }
        _ => entry["finding"] = json!("f9"),
    }
}

#[tokio::test]
async fn each_fault_is_refused_for_that_issue_alone_and_the_next_entry_is_accepted() -> Result<()> {
    for (name, message) in refusals() {
        let llm = judge(
            move |task, earlier| {
                let mut list = issue_entries(task);
                if earlier == 0 {
                    let finding = shown(task, "Finding")[1].0.clone();
                    spoil(name, &mut list[0], &finding);
                }
                entries(list)
            },
            extras_answer,
        );
        let graded = grade(&llm, &Scene::new()).await;
        assert_eq!(graded.ok(), Some(assessment()), "{name}");
        let asked = asks::<GradeReview>(&llm);
        assert_eq!(asked.len(), 2, "{name}");
        let again = asked[1].prompt();
        assert!(
            again.contains(message),
            "{name}: {message} is not in {again}"
        );
        assert_eq!(
            shown(&again, "Issue").len(),
            1,
            "{name}: only the refused issue is asked again"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_issue_found_by_something_not_shown_is_asked_again_alone_and_then_told_what_is_wrong(
) -> Result<()> {
    let message = "Your answer pointed at something that is not shown. Point only at what is shown, by the name it is shown under.";
    let llm = judge(
        |task, earlier| {
            let mut list = issue_entries(task);
            if earlier < 2 {
                let finding = shown(task, "Finding")[1].0.clone();
                spoil(
                    "an issue found by something that is not shown",
                    &mut list[0],
                    &finding,
                );
            }
            entries(list)
        },
        extras_answer,
    );
    let graded = grade(&llm, &Scene::new()).await;
    assert_eq!(graded.ok(), Some(assessment()));
    let asked = asks::<GradeReview>(&llm);
    assert_eq!(asked.len(), 3);
    assert_eq!(shown(&asked[1].prompt(), "Issue").len(), 1);
    assert!(!asked[1].prompt().contains(message));
    let again = asked[2].prompt();
    assert!(again.contains(message), "{message} is not in {again}");
    assert_eq!(shown(&again, "Issue").len(), 1);
    Ok(())
}

#[tokio::test]
async fn an_issue_the_answer_leaves_out_is_asked_for_alone_and_the_others_stand() -> Result<()> {
    let llm = judge(
        |task, earlier| {
            let mut list = issue_entries(task);
            if earlier == 0 {
                list.remove(1);
            }
            entries(list)
        },
        extras_answer,
    );
    assert_eq!(grade(&llm, &Scene::new()).await?, assessment());
    let asked = asks::<GradeReview>(&llm);
    assert_eq!(asked.len(), 2);
    let again = shown(&asked[1].prompt(), "Issue");
    assert_eq!(again.len(), 1);
    assert!(again[0].1.contains("orders.py line 9"), "{again:?}");
    assert_eq!(
        shown(&asked[1].prompt(), "Finding").len(),
        4,
        "the review stays whole"
    );
    Ok(())
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_is_asked_for_again() -> Result<()> {
    let llm = judge(
        |task, earlier| {
            if earlier == 0 {
                "These are not entries.".to_owned()
            } else {
                issues_answer(task, earlier)
            }
        },
        extras_answer,
    );
    let scene = Scene::new();
    assert_eq!(grade(&llm, &scene).await?, assessment());
    assert_eq!(asks::<GradeReview>(&llm).len(), 1 + scene.drill.key.len());
    Ok(())
}

#[tokio::test]
async fn an_issue_that_stays_unanswered_fails_the_grade_and_asks_nothing_about_findings() {
    let llm = judge(
        |task, _| {
            let mut list = issue_entries(task);
            list.retain(|entry| entry["reason"] != json!(QUERY));
            entries(list)
        },
        extras_answer,
    );
    let error = grade(&llm, &Scene::new()).await.unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert!(asks::<GradeExtras>(&llm).is_empty());
}

#[tokio::test]
async fn a_finding_that_stays_unanswered_fails_the_grade() {
    let llm = judge(issues_answer, |_, _| entries(Vec::new()));
    let error = grade(&llm, &Scene::new()).await.unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_finding_without_a_reason_is_asked_about_again_alone() -> Result<()> {
    let llm = judge(issues_answer, |task, earlier| {
        let reason = if earlier == 0 { "  " } else { REMARK };
        entries(
            shown(task, "Finding")
                .iter()
                .map(|(handle, _)| verdict(handle, reason, "false_positive"))
                .collect(),
        )
    });
    assert_eq!(grade(&llm, &Scene::new()).await?, assessment());
    let asked = asks::<GradeExtras>(&llm);
    assert_eq!(asked.len(), 2);
    assert!(asked[1]
        .prompt()
        .contains("This finding has no reason: say why before giving its outcome."));
    Ok(())
}

#[tokio::test]
async fn an_error_that_says_nothing_about_the_answer_is_not_asked_again() {
    let llm = ScriptedModel::new()
        .named("scripted-judge")
        .failing_every_request(|_| AppError::Network("the connection dropped".into()));
    let error = grade(&llm, &Scene::new()).await.unwrap_err();
    assert!(matches!(error, AppError::Network(_)), "{error:?}");
    assert!(llm.sent().len() <= 1);
}

#[tokio::test]
async fn a_review_with_no_findings_misses_every_issue_without_asking_the_model() -> Result<()> {
    let mut scene = Scene::new();
    scene.findings = Vec::new();
    let llm = standard();
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
async fn a_drill_and_review_the_window_cannot_hold_fail_with_the_window_too_small() {
    let llm = ScriptedModel::new()
        .named("scripted-judge")
        .window(1_200)
        .on_request::<GradeReview>(|_, _| reply(entries(Vec::new())));
    let error = grade(&llm, &Scene::new()).await.unwrap_err();
    assert!(
        matches!(&error, AppError::WindowTooSmall(message) if message.starts_with("The drill and the review do not fit in scripted-judge's 1200-token context window.")),
        "{error:?}"
    );
}

#[tokio::test]
async fn the_classifier_knob_decides_whether_the_call_reasons_first() -> Result<()> {
    let llm = standard();
    grade(&llm, &Scene::new()).await?;
    let expected = if model_call::REASON_FIRST_CLASSIFIERS {
        2
    } else {
        1
    };
    let sent = llm
        .sent()
        .iter()
        .filter(|sent| sent.call.id == GradeReview::ID)
        .count();
    assert_eq!(sent, expected);
    assert_eq!(asks::<GradeReview>(&llm).len(), 1);
    Ok(())
}

#[tokio::test]
async fn the_answers_are_asked_for_deterministically() -> Result<()> {
    let llm = standard();
    grade(&llm, &Scene::new()).await?;
    for sent in asks::<GradeReview>(&llm)
        .iter()
        .chain(asks::<GradeExtras>(&llm).iter())
    {
        let sampling = sent.request.sampling.unwrap_or_default();
        assert_eq!((sampling.temperature, sampling.top_k), (Some(0.0), Some(1)));
    }
    Ok(())
}

/// Where `needle` first is in `text`, or a failure saying it is not.
fn at(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in the task:\n{text}"))
}

#[tokio::test]
async fn the_request_holds_the_files_the_clean_files_the_key_the_review_and_the_overlaps(
) -> Result<()> {
    let llm = standard();
    grade(&llm, &Scene::new()).await?;
    let wording = JudgeWording::of(&Material::code("python"))?;
    assert_eq!(system_of::<GradeReview>(&llm), wording.system);
    assert_eq!(system_of::<GradeExtras>(&llm), wording.extras);
    let task = task_of::<GradeReview>(&llm);

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
        "The key: 3 issues.",
        "Issue [i1]: Off-by-one error (critical), orders.py line 3",
        "Issue [i2]: SQL injection (critical), orders.py line 9",
        "Issue [i3]: Swallowed exception (minor), notify.py lines 4 to 5",
        "The review: 4 findings, in the order they were made.",
        "Finding [f1]: orders.py line 3, severity given: major",
        "Finding [f2]: orders.py line 9\n",
        "Finding [f3]: no lines selected, severity given: minor",
        "Finding [f4]: notify.py lines 3 to 5\n",
        "Findings and issues whose lines overlap, a hint and not a verdict:",
        "Give an entry for every issue, the reason first.",
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
async fn the_request_about_the_findings_no_issue_claims_holds_the_key_and_only_those_findings(
) -> Result<()> {
    let llm = standard();
    grade(&llm, &Scene::new()).await?;
    let task = task_of::<GradeExtras>(&llm);
    let sections = [
        "Drill: Orders (python)",
        "The code as the reviewer saw it, with line numbers:",
        "The same code before any issue was planted, with its own line numbers:",
        "The key: 3 issues.",
        "Issue: Off-by-one error (critical), orders.py line 3",
        "Issue: SQL injection (critical), orders.py line 9",
        "Issue: Swallowed exception (minor), notify.py lines 4 to 5",
        "The findings about no issue of the key: 1 finding, in the order they were made.",
        "Finding [f1]: no lines selected, severity given: minor\nText:\n  Nothing here is tested.",
        "Give an entry for every finding, the reason first.",
    ];
    let mut last = 0;
    for section in sections {
        let found = at(&task, section);
        assert!(found >= last, "{section:?} is out of order");
        last = found;
    }
    assert!(!task.contains("Finding [f2]"), "{task}");
    for claimed in Scene::new()
        .findings
        .iter()
        .filter(|finding| finding.span.is_some())
    {
        let first_line = claimed.text.lines().next().unwrap_or_default();
        assert!(!task.contains(first_line), "{task}");
    }
    assert!(!task.contains("overlap"), "{task}");
    Ok(())
}

#[tokio::test]
async fn every_issue_is_shown_with_its_pattern_its_consequence_and_the_lines_it_covers(
) -> Result<()> {
    let llm = standard();
    grade(&llm, &Scene::new()).await?;
    let task = task_of::<GradeReview>(&llm);
    let definition = |id: &str| {
        pattern(id)
            .map(|known| known.definition)
            .unwrap_or_default()
    };
    for expected in [
        format!(
            "Issue [i1]: Off-by-one error (critical), orders.py line 3\nPattern: {}\nConsequence: The last pass reads past the end of prices and raises IndexError.\nLines:\n  3 |     for i in range(len(prices) + 1):",
            definition("correctness.off_by_one")
        ),
        format!(
            "Issue [i2]: SQL injection (critical), orders.py line 9\nPattern: {}\nConsequence: A name with a quote in it changes the query.\nLines:\n  9 |     return db.execute(f\"SELECT * FROM orders WHERE name = '{{name}}'\")",
            definition("security.sql_injection")
        ),
        format!(
            "Issue [i3]: Swallowed exception (minor), notify.py lines 4 to 5\nPattern: {}\nConsequence: A failed delivery is never noticed.\nLines:\n  4 |     except Exception:\n  5 |         pass",
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
    let llm = standard();
    grade(&llm, &scene).await?;
    let task = task_of::<GradeReview>(&llm);
    let definition = pattern("correctness.off_by_one")
        .map(|known| known.definition)
        .unwrap_or_default();
    let expected = format!(
        "Issue [i1]: Off-by-one error (critical), orders.py line 3\nPattern: {definition}\nFrom the lesson:\n  A loop stops at the length of what it walks.\n  ```python\n  for i in range(len(items)):\n      use(items[i])\n  ```\nConsequence: The last pass reads past the end of prices and raises IndexError.\nLines:\n"
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
    let llm = standard();
    grade(&llm, &Scene::new()).await?;
    let task = task_of::<GradeReview>(&llm);
    for expected in [
        "Finding [f1]: orders.py line 3, severity given: major\nLines:\n  3 |     for i in range(len(prices) + 1):\nText:\n  The loop runs one step too far, so the last pass indexes past the end of prices.",
        "Finding [f2]: orders.py line 9\nLines:\n  9 |     return db.execute(f\"SELECT * FROM orders WHERE name = '{name}'\")\nText:\n  Building SQL from name lets a crafted name change the query.",
        "Finding [f3]: no lines selected, severity given: minor\nText:\n  Nothing here is tested.",
        "Finding [f4]: notify.py lines 3 to 5\nLines:\n  3 |         mail.deliver()\n  4 |     except Exception:\n  5 |         pass\nText:\n  This hides failures.\n  Nobody will ever know a mail did not go out.",
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
    let llm = standard();
    grade(&llm, &scene).await?;
    let task = task_of::<GradeReview>(&llm);
    assert!(task.contains(
        "Findings and issues whose lines overlap, a hint and not a verdict:\nfinding [f1] and issue [i1]\nfinding [f2] and issue [i2]\nfinding [f4] and issue [i3]"
    ));
    assert!(!task.contains("finding [f3] and"));

    let mut apart = Scene::new();
    apart.findings = vec![finding(
        Some(span("orders.py", 6, 7)),
        "Two blank lines.",
        None,
    )];
    let llm = judge(
        |task, _| {
            entries(
                shown(task, "Issue")
                    .iter()
                    .map(|(handle, _)| judged(handle, "No.", "missed", "none", &[]))
                    .collect(),
            )
        },
        extras_answer,
    );
    grade(&llm, &apart).await?;
    let task = task_of::<GradeReview>(&llm);
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
    let llm = judge(
        |task, _| {
            entries(vec![judged(
                &shown(task, "Issue")[0].0,
                "It points at the lines.",
                "half_found",
                &shown(task, "Finding")[0].0,
                &[],
            )])
        },
        extras_answer,
    );
    grade(&llm, &scene).await?;
    let task = task_of::<GradeReview>(&llm);
    assert!(task.contains(" 8 | line 8\n 9 | line 9\n10 | line 10\n11 | line 11"));
    assert!(task.contains("Consequence: It is never used.\nLines:\n   9 | line 9\n  10 | line 10"));
    assert!(task.contains("Finding [f1]: long.txt lines 9 to 10\nLines:\n   9 | line 9\n  10 | line 10\nText:\n  Unused."));
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
    let llm = judge(
        |task, _| {
            entries(
                shown(task, "Issue")
                    .iter()
                    .map(|(handle, _)| judged(handle, "No.", "missed", "none", &[]))
                    .collect(),
            )
        },
        extras_answer,
    );
    grade(&llm, &scene).await?;
    let task = task_of::<GradeReview>(&llm);
    assert!(task.contains("The key: 4 issues."));
    assert!(task.contains("Issue [i4]: no.such_pattern (nit), orders.py line 2\nConsequence: It is odd.\nLines:\n  2 |     result = 0"));
    assert!(!task.contains("clean code as well"));
    assert!(task.contains(&format!(
        "Issue [i1]: Off-by-one error (critical), orders.py line 3\nPattern: {}\nConsequence: The last pass reads past the end of prices and raises IndexError.\nLines:\n  3 |     for i in range(len(prices) + 1):",
        pattern("correctness.off_by_one").map_or("", |known| known.definition)
    )));
    assert!(!task.contains("Issue [i4]: no.such_pattern (nit), orders.py line 2\nPattern:"));
    assert!(task.contains(
        "Finding [f1]: ghost.py lines 1 to 2\nLines: ghost.py is not one of the drill's files\nText:"
    ));
    assert!(task.contains(
        "Finding [f2]: orders.py lines 40 to 41\nLines: none of them is in the file\nText:"
    ));
    assert!(task.contains("Finding [f3]: no lines selected\nText: (empty)"));
    Ok(())
}

#[tokio::test]
async fn the_task_and_the_instructions_name_issues_and_findings_by_the_handles_shown() -> Result<()>
{
    let llm = standard();
    grade(&llm, &Scene::new()).await?;
    let system = system_of::<GradeReview>(&llm);
    let task = task_of::<GradeReview>(&llm);
    assert!(
        system.contains(
            "Each issue and each finding is named by the handle in square brackets shown with it."
        ),
        "{system}"
    );
    assert!(task.contains("3 issues"), "{task}");
    assert!(task.contains("4 findings"), "{task}");
    assert!(
        !task.contains("Issue 1") && !task.contains("Finding 1"),
        "{task}"
    );

    let mut one = Scene::new();
    one.drill.key.truncate(1);
    one.findings.truncate(1);
    let llm = standard();
    grade(&llm, &one).await?;
    let task = task_of::<GradeReview>(&llm);
    assert!(task.contains("The key: 1 issue."), "{task}");
    assert!(
        task.contains("The review: 1 finding, in the order they were made."),
        "{task}"
    );
    assert!(
        task.contains("Give an entry for every issue, the reason first."),
        "{task}"
    );
    Ok(())
}

/// An explanation of a passage with one error planted on its second sentence, and two findings.
fn passage_scene() -> (
    Drill,
    Vec<DrillFile>,
    Vec<Finding>,
    crate::drills::build::SourcePassage,
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
    let source = crate::drills::build::SourcePassage {
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
    let llm = judge(
        |task, _| {
            entries(vec![judged(
                &shown(task, "Issue")[0].0,
                "It is the figure.",
                "found",
                &shown(task, "Finding")[0].0,
                &[],
            )])
        },
        extras_answer,
    );
    let review = Review {
        drill: &drill,
        clean: &clean,
        source: Some(&source),
        findings: &findings,
    };
    JudgeGrader::new(&llm).assess(&review).await?;
    let wording = JudgeWording::of(&Material::Passage)?;
    assert_eq!(system_of::<GradeReview>(&llm), wording.system);
    assert_eq!(system_of::<GradeExtras>(&llm), wording.extras);
    let task = task_of::<GradeReview>(&llm);
    let sections = [
        "Drill: Spacing (an explanation of a source passage)",
        "The explanation as the reviewer saw it, one sentence to a numbered line:\n\nFile explanation.md\n1 | Learners spread their study.\n2 | Spacing raised recall by 40%.\n3 | \n4 | The gain was smaller for known material.",
        "The same explanation before any error was planted, with its own numbers:\n\nFile explanation.md\n1 | Learners spread their study.\n2 | Spacing raised recall by 14%.",
        "The source passage the explanation was written from, in Spacing and memory:\n<passage>\nLearners who spread their study recalled 14% more a week later.\n</passage>",
        "The key: 1 issue.",
        "Issue [i1]: Misquoted figure (major), explanation.md sentence 2",
        "Sentences:\n  2 | Spacing raised recall by 40%.",
        "Finding [f1]: explanation.md sentence 2, severity given: major",
        "Finding [f2]: no sentences selected",
        "Findings and issues whose sentences overlap, a hint and not a verdict:\nfinding [f1] and issue [i1]",
    ];
    let mut last = 0;
    for section in sections {
        let found = at(&task, section);
        assert!(found >= last, "{section:?} is out of order");
        last = found;
    }
    assert!(!task.contains("explanation.md line"), "{task}");
    let extras = task_of::<GradeExtras>(&llm);
    assert!(
        extras.contains("<passage>"),
        "the second question reads the source too: {extras}"
    );
    Ok(())
}

#[tokio::test]
async fn the_task_for_code_shows_no_source_even_when_one_is_given() -> Result<()> {
    let scene = Scene::new();
    let source = passage_scene().3;
    let llm = standard();
    let review = Review {
        source: Some(&source),
        ..scene.review()
    };
    JudgeGrader::new(&llm).assess(&review).await?;
    for task in [task_of::<GradeReview>(&llm), task_of::<GradeExtras>(&llm)] {
        assert!(!task.contains("<passage>"), "{task}");
    }
    Ok(())
}

/// What `standard()` makes of `Scene::unsorted`, which keeps its issues in the order swallowed exception, SQL
/// injection, off-by-one: the answer names issues in the order they are shown, most severe first, and the assessment
/// holds them in the key's order.
fn assessment_unsorted() -> Assessment {
    Assessment {
        issues: vec![
            (IssueOutcome::HalfFound { finding: 3 }, SWALLOWED.into()),
            (IssueOutcome::Found { finding: 1 }, QUERY.into()),
            (IssueOutcome::Found { finding: 0 }, LOOP.into()),
        ],
        findings: vec![
            (FindingOutcome::Matches { issues: vec![2] }, LOOP.into()),
            (FindingOutcome::Matches { issues: vec![1] }, QUERY.into()),
            (FindingOutcome::FalsePositive, REMARK.into()),
            (
                FindingOutcome::Matches { issues: vec![0] },
                SWALLOWED.into(),
            ),
        ],
    }
}

#[tokio::test]
async fn the_task_shows_the_issues_most_severe_first_whatever_the_keys_order() -> Result<()> {
    let llm = standard();
    grade(&llm, &Scene::unsorted()).await?;
    let task = task_of::<GradeReview>(&llm);
    let mut last = 0;
    for heading in [
        "Issue [i1]: SQL injection (critical), orders.py line 9",
        "Issue [i2]: Off-by-one error (major), orders.py line 3",
        "Issue [i3]: Swallowed exception (minor), notify.py lines 4 to 5",
        "The review: 4 findings",
    ] {
        let found = at(&task, heading);
        assert!(found >= last, "{heading:?} is out of order in\n{task}");
        last = found;
    }
    Ok(())
}

#[tokio::test]
async fn the_overlap_hint_names_issues_by_the_handles_of_the_task() -> Result<()> {
    let scene = Scene::unsorted();
    assert_eq!(
        overlapping(&scene.findings, &scene.drill.key),
        vec![(0, 2), (1, 1), (3, 0)]
    );
    let llm = standard();
    grade(&llm, &scene).await?;
    let task = task_of::<GradeReview>(&llm);
    assert!(
        task.contains("a hint and not a verdict:\nfinding [f1] and issue [i2]\nfinding [f2] and issue [i1]\nfinding [f4] and issue [i3]"),
        "{task}"
    );
    Ok(())
}

#[tokio::test]
async fn the_issues_the_answer_names_are_mapped_back_to_the_keys_positions() -> Result<()> {
    let llm = standard();
    assert_eq!(
        grade(&llm, &Scene::unsorted()).await?,
        assessment_unsorted()
    );
    assert_eq!(asks::<GradeReview>(&llm).len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_finding_about_issues_shown_first_and_third_holds_their_key_positions_sorted(
) -> Result<()> {
    let llm = judge(
        |task, _| {
            let findings = shown(task, "Finding");
            let issues = shown(task, "Issue");
            entries(vec![
                judged(&issues[0].0, QUERY, "found", &findings[1].0, &[]),
                judged(&issues[1].0, LOOP, "found", &findings[0].0, &[]),
                judged(
                    &issues[2].0,
                    "It also points at the swallowed error.",
                    "half_found",
                    &findings[1].0,
                    &[],
                ),
            ])
        },
        |task, _| {
            entries(
                shown(task, "Finding")
                    .iter()
                    .map(|(handle, _)| verdict(handle, "A real problem.", "valid_extra"))
                    .collect(),
            )
        },
    );
    let graded = grade(&llm, &Scene::unsorted()).await?;
    assert_eq!(
        graded.findings[1].0,
        FindingOutcome::Matches { issues: vec![0, 1] }
    );
    assert_eq!(graded.issues[0].0, IssueOutcome::HalfFound { finding: 1 });
    assert_eq!(graded.issues[1].0, IssueOutcome::Found { finding: 1 });
    assert_eq!(graded.issues[2].0, IssueOutcome::Found { finding: 0 });
    assert_eq!(graded.findings[2].0, FindingOutcome::ValidExtra);
    assert_eq!(graded.findings[3].0, FindingOutcome::ValidExtra);
    Ok(())
}

fn the_scene_input(marks: &mut Marks) -> (Context, Vec<IssueView>, Vec<FindingView>) {
    let context =
        Context {
            instructions: marks.told("Grade the review against the key.".to_owned()),
            drill: marks.told("Drill: Harbour log (python)".to_owned()),
            reviewed: marks.told("The code as the reviewer saw it:\n\n1 | tide = 4".to_owned()),
            original: marks
                .told("The same code before any issue was planted:\n\n1 | tide = 3".to_owned()),
            source: Some(marks.told(
                "The source passage: <passage>The tide rises twice a day.</passage>".to_owned(),
            )),
        };
    let issues = vec![IssueView {
        at: marks.reference(100_001_usize),
        head: marks.told("Wrong constant (major), harbour.py line 1".to_owned()),
        rest: marks.told("Consequence: The tide table is off by one.".to_owned()),
    }];
    let findings = vec![FindingView {
        at: marks.reference(100_002_usize),
        head: marks.told("harbour.py line 1, severity given: minor".to_owned()),
        rest: marks.told("Text:\n  The constant looks odd.".to_owned()),
        overlaps: marks.internal(vec![100_001_usize]),
    }];
    (context, issues, findings)
}

#[test]
fn only_what_the_model_must_read_reaches_the_request_about_the_issues() {
    let mut marks = Marks::new();
    let (context, issues, findings) = the_scene_input(&mut marks);
    let input = Input {
        context,
        noun: marks.told("sentences".to_owned()),
        issues,
        findings,
    };
    holds::<GradeReview>(&input, &marks);
}

#[test]
fn only_what_the_model_must_read_reaches_the_request_about_the_findings_no_issue_claims() {
    let mut marks = Marks::new();
    let (context, key, findings) = the_scene_input(&mut marks);
    let input = ExtrasInput {
        context,
        key,
        findings,
    };
    holds::<GradeExtras>(&input, &marks);
}

#[test]
fn the_schemas_point_only_at_what_is_shown() {
    let mut marks = Marks::new();
    let (context, issues, findings) = the_scene_input(&mut marks);
    let input = Input {
        context,
        noun: marks.told("sentences".to_owned()),
        issues,
        findings,
    };
    let schema = schema_of::<GradeReview>(&input, Held::Enums)
        .map(|schema| schema.as_json().to_string())
        .unwrap_or_default();
    assert!(schema.contains("\"i1\""), "{schema}");
    assert!(schema.contains("\"f1\""), "{schema}");
    assert!(schema.contains("\"none\""), "{schema}");
    assert!(!schema.contains("integer"), "{schema}");
}

#[tokio::test]
async fn prompt_snapshot_grade_review() -> Result<()> {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let llm = standard();

    grade(&llm, &Scene::new()).await?;

    let requests: Vec<_> = llm
        .sent()
        .into_iter()
        .filter(|sent| sent.call.id == GradeReview::ID)
        .map(|sent| sent.request)
        .collect();
    snapshot::assert_requests(env!("CARGO_MANIFEST_DIR"), "grade-review", &requests);
    Ok(())
}

#[tokio::test]
async fn prompt_snapshot_grade_extras() -> Result<()> {
    use lattice_core::application::services::model_contract::test_support::snapshot;
    let llm = standard();

    grade(&llm, &Scene::new()).await?;

    let requests: Vec<_> = llm
        .sent()
        .into_iter()
        .filter(|sent| sent.call.id == GradeExtras::ID)
        .map(|sent| sent.request)
        .collect();
    snapshot::assert_requests(env!("CARGO_MANIFEST_DIR"), "grade-extras", &requests);
    Ok(())
}
