//! The result of grading a learner's fix: what a suite that fails does to an issue its check calls fixed, and the one
//! form a result is stored and shown in.

use serde_json::json;

use super::*;

fn check(outcome: FixOutcome, decided_by: DecidedBy) -> IssueCheck {
    IssueCheck {
        outcome,
        decided_by,
        reason: "The comparison is back to the limit.".to_owned(),
        output: None,
        fallback: None,
    }
}

fn failing() -> SuiteVerdict {
    SuiteVerdict::Failed {
        output: "AssertionError: the refund is gone".to_owned(),
    }
}

#[test]
fn a_suite_that_passes_leaves_what_the_check_found() {
    let settled = IssueFix::settle(
        1,
        check(FixOutcome::Fixed, DecidedBy::Test),
        &SuiteVerdict::Passed,
    );
    assert_eq!(settled.issue, 1);
    assert_eq!(settled.outcome, FixOutcome::Fixed);
    assert_eq!(settled.decided_by, DecidedBy::Test);
    assert_eq!(settled.reason, "The comparison is back to the limit.");
    assert_eq!(settled.output, None);
    assert_eq!(settled.suite, SuiteVerdict::Passed);
}

#[test]
fn a_suite_that_did_not_run_leaves_what_the_check_found() {
    let not_run = SuiteVerdict::NotRun {
        why: "There is no runtime for this language.".to_owned(),
    };
    let settled = IssueFix::settle(0, check(FixOutcome::Fixed, DecidedBy::Judge), &not_run);
    assert_eq!(settled.outcome, FixOutcome::Fixed);
    assert_eq!(settled.decided_by, DecidedBy::Judge);
    assert_eq!(settled.suite, not_run);
}

#[test]
fn a_suite_that_fails_makes_a_fixed_issue_partly_fixed_and_says_why() {
    for decided_by in [DecidedBy::Test, DecidedBy::Judge] {
        let settled = IssueFix::settle(0, check(FixOutcome::Fixed, decided_by), &failing());
        assert_eq!(settled.outcome, FixOutcome::Partly, "{decided_by:?}");
        assert_eq!(settled.decided_by, DecidedBy::Suite, "{decided_by:?}");
        assert_eq!(
            settled.output.as_deref(),
            Some("AssertionError: the refund is gone")
        );
        assert!(
            settled
                .reason
                .starts_with("The comparison is back to the limit."),
            "the check's own reason is kept: {}",
            settled.reason
        );
        assert!(
            settled.reason.contains("hidden suite fails"),
            "{}",
            settled.reason
        );
        assert_eq!(settled.suite, failing());
    }
}

#[test]
fn a_suite_that_fails_leaves_an_issue_that_is_not_fully_fixed_as_its_check_found_it() {
    for outcome in [FixOutcome::Partly, FixOutcome::NotFixed] {
        let mut found = check(outcome, DecidedBy::Test);
        found.output = Some("the test's output".to_owned());
        let settled = IssueFix::settle(2, found, &failing());
        assert_eq!(settled.outcome, outcome);
        assert_eq!(settled.decided_by, DecidedBy::Test);
        assert_eq!(settled.output.as_deref(), Some("the test's output"));
        assert_eq!(
            settled.suite,
            failing(),
            "{outcome:?} still carries the broken suite"
        );
    }
}

#[test]
fn a_fallback_is_kept_whatever_the_suite_says() {
    let mut found = check(FixOutcome::Fixed, DecidedBy::Judge);
    found.fallback = Some("The test ran out of time.".to_owned());
    for suite in [SuiteVerdict::Passed, failing()] {
        let settled = IssueFix::settle(0, found.clone(), &suite);
        assert_eq!(
            settled.fallback.as_deref(),
            Some("The test ran out of time.")
        );
    }
}

#[test]
fn a_result_is_stored_and_shown_in_one_form() {
    let result = IssueFix {
        issue: 3,
        outcome: FixOutcome::NotFixed,
        decided_by: DecidedBy::Test,
        reason: "The test still fails.".to_owned(),
        output: Some("AssertionError".to_owned()),
        fallback: None,
        suite: failing(),
    };
    let written = serde_json::to_value(&result).unwrap();
    assert_eq!(
        written,
        json!({
            "issue": 3,
            "outcome": "not_fixed",
            "decidedBy": "test",
            "reason": "The test still fails.",
            "output": "AssertionError",
            "fallback": null,
            "suite": {"status": "failed", "output": "AssertionError: the refund is gone"}
        })
    );
    let read: IssueFix = serde_json::from_value(written).unwrap();
    assert_eq!(read, result);
}

#[test]
fn a_suite_verdict_names_its_status() {
    assert_eq!(
        serde_json::to_value(SuiteVerdict::Passed).unwrap(),
        json!({"status": "passed"})
    );
    assert_eq!(
        serde_json::to_value(SuiteVerdict::NotRun {
            why: "No suite.".to_owned()
        })
        .unwrap(),
        json!({"status": "not_run", "why": "No suite."})
    );
}
