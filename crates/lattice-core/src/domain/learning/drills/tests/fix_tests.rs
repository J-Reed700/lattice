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
        graded_beside: Vec::new(),
    }
}

fn passing() -> SuiteVerdict {
    SuiteVerdict::Passed {
        ran_beside: Vec::new(),
    }
}

fn failing() -> SuiteVerdict {
    SuiteVerdict::Failed {
        output: "AssertionError: the refund is gone".to_owned(),
        ran_beside: Vec::new(),
    }
}

/// A suite that fails on files it ran beside the fixes of the issues at `ran_beside`.
fn failing_beside(ran_beside: &[usize]) -> SuiteVerdict {
    SuiteVerdict::Failed {
        output: "AssertionError: the refund is gone".to_owned(),
        ran_beside: ran_beside.to_vec(),
    }
}

#[test]
fn a_suite_that_passes_leaves_what_the_check_found() {
    let settled = IssueFix::settle(1, check(FixOutcome::Fixed, DecidedBy::Test), &passing());
    assert_eq!(settled.issue, 1);
    assert_eq!(settled.outcome, FixOutcome::Fixed);
    assert_eq!(settled.decided_by, DecidedBy::Test);
    assert_eq!(settled.reason, "The comparison is back to the limit.");
    assert_eq!(settled.output, None);
    assert_eq!(settled.suite, passing());
}

#[test]
fn a_suite_that_passes_beside_fixes_carries_the_issues_it_ran_beside() {
    let beside = SuiteVerdict::Passed {
        ran_beside: vec![0, 2],
    };
    let settled = IssueFix::settle(1, check(FixOutcome::Fixed, DecidedBy::Test), &beside);
    assert_eq!(settled.outcome, FixOutcome::Fixed);
    assert_eq!(settled.reason, "The comparison is back to the limit.");
    assert_eq!(settled.suite, beside);
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
        assert!(
            !settled.reason.contains("you left"),
            "the suite ran beside no fixes: {}",
            settled.reason
        );
        assert_eq!(settled.suite, failing());
    }
}

#[test]
fn a_suite_that_fails_beside_fixes_says_the_issues_the_learner_left_were_fixed_for_it() {
    let beside = failing_beside(&[1, 2]);
    let settled = IssueFix::settle(0, check(FixOutcome::Fixed, DecidedBy::Test), &beside);
    assert_eq!(settled.outcome, FixOutcome::Partly);
    assert_eq!(settled.decided_by, DecidedBy::Suite);
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
    assert!(
        settled
            .reason
            .contains("even with the issues you left fixed"),
        "{}",
        settled.reason
    );
    assert_eq!(
        settled.suite, beside,
        "the verdict says which fixes the suite ran beside"
    );
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
    for suite in [passing(), failing(), failing_beside(&[1])] {
        let settled = IssueFix::settle(0, found.clone(), &suite);
        assert_eq!(
            settled.fallback.as_deref(),
            Some("The test ran out of time.")
        );
    }
}

#[test]
fn the_issues_an_issue_was_graded_beside_are_kept_whatever_the_suite_says() {
    let mut found = check(FixOutcome::Fixed, DecidedBy::Test);
    found.graded_beside = vec![0, 2];
    for suite in [passing(), failing(), failing_beside(&[0, 2])] {
        let settled = IssueFix::settle(1, found.clone(), &suite);
        assert_eq!(settled.graded_beside, [0, 2]);
    }
    let alone = IssueFix::settle(1, check(FixOutcome::Fixed, DecidedBy::Judge), &failing());
    assert!(alone.graded_beside.is_empty());
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
        graded_beside: vec![0, 4],
        suite: failing_beside(&[0, 4]),
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
            "gradedBeside": [0, 4],
            "suite": {
                "status": "failed",
                "output": "AssertionError: the refund is gone",
                "ranBeside": [0, 4]
            }
        })
    );
    let read: IssueFix = serde_json::from_value(written).unwrap();
    assert_eq!(read, result);
}

#[test]
fn a_suite_verdict_names_its_status_and_the_issues_it_ran_beside() {
    let verdicts = [
        (
            SuiteVerdict::Passed {
                ran_beside: vec![1, 3],
            },
            json!({"status": "passed", "ranBeside": [1, 3]}),
        ),
        (passing(), json!({"status": "passed", "ranBeside": []})),
        (
            failing_beside(&[2]),
            json!({
                "status": "failed",
                "output": "AssertionError: the refund is gone",
                "ranBeside": [2]
            }),
        ),
        (
            SuiteVerdict::NotRun {
                why: "No suite.".to_owned(),
            },
            json!({"status": "not_run", "why": "No suite."}),
        ),
    ];
    for (verdict, shown) in verdicts {
        assert_eq!(serde_json::to_value(&verdict).unwrap(), shown);
        let read: SuiteVerdict = serde_json::from_value(shown).unwrap();
        assert_eq!(read, verdict);
    }
}
