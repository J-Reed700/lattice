//! A judge's assessment: when it is consistent, which findings overlap which
//! issues, and the score, worked by hand.

use super::*;

fn tally(severity: Severity, found: usize, half_found: usize, total: usize) -> SeverityTally {
    SeverityTally {
        severity,
        found,
        half_found,
        total,
    }
}

/// One issue of each severity, the most severe first, on lines 1, 5, 9 and 13.
fn four_issues() -> Vec<KeyIssue> {
    vec![
        issue(Severity::Critical, 1, 1),
        issue(Severity::Major, 5, 5),
        issue(Severity::Minor, 9, 9),
        issue(Severity::Nit, 13, 13),
    ]
}

#[test]
fn consistent_assessments_are_accepted() {
    let cases = [
        ("nothing at all", 0, 0, assessment(vec![], vec![])),
        (
            "no findings, every issue missed",
            2,
            0,
            assessment(vec![missed(), missed()], vec![]),
        ),
        (
            "found by the only finding",
            1,
            1,
            assessment(vec![found(0)], vec![matches(&[0])]),
        ),
        (
            "half found",
            1,
            1,
            assessment(vec![half_found(0)], vec![matches(&[0])]),
        ),
        (
            "extras and false positives beside a find",
            1,
            3,
            assessment(
                vec![found(0)],
                vec![matches(&[0]), valid_extra(), false_positive()],
            ),
        ),
        (
            "a duplicate finding",
            1,
            2,
            assessment(vec![found(0)], vec![matches(&[0]), matches(&[0])]),
        ),
        (
            "the later of two findings credited",
            1,
            2,
            assessment(vec![found(1)], vec![matches(&[0]), matches(&[0])]),
        ),
        (
            "a finding that names two issues",
            2,
            1,
            assessment(vec![found(0), half_found(0)], vec![matches(&[0, 1])]),
        ),
        (
            "one finding for each issue",
            2,
            2,
            assessment(vec![found(1), found(0)], vec![matches(&[1]), matches(&[0])]),
        ),
    ];
    for (name, key_len, findings_len, assessment) in cases {
        assert_eq!(
            validate(key_len, findings_len, &assessment),
            Ok(()),
            "{name}"
        );
    }
}

#[test]
fn inconsistent_assessments_name_their_fault() {
    use AssessmentFault::*;
    let cases = [
        (
            "too few issue outcomes",
            2,
            0,
            assessment(vec![missed()], vec![]),
            IssueCount {
                expected: 2,
                got: 1,
            },
        ),
        (
            "an issue outcome too many",
            0,
            0,
            assessment(vec![missed()], vec![]),
            IssueCount {
                expected: 0,
                got: 1,
            },
        ),
        (
            "too few finding outcomes",
            0,
            2,
            assessment(vec![], vec![false_positive()]),
            FindingCount {
                expected: 2,
                got: 1,
            },
        ),
        (
            "a finding outcome too many",
            0,
            0,
            assessment(vec![], vec![valid_extra()]),
            FindingCount {
                expected: 0,
                got: 1,
            },
        ),
        (
            "credited to a finding there is not",
            1,
            1,
            assessment(vec![found(3)], vec![matches(&[0])]),
            FindingOutOfRange {
                issue: 0,
                finding: 3,
            },
        ),
        (
            "credited to the finding one past the end",
            1,
            1,
            assessment(vec![half_found(1)], vec![matches(&[0])]),
            FindingOutOfRange {
                issue: 0,
                finding: 1,
            },
        ),
        (
            "a finding that matches an issue there is not",
            1,
            1,
            assessment(vec![missed()], vec![matches(&[1])]),
            IssueOutOfRange {
                finding: 0,
                issue: 1,
            },
        ),
        (
            "one of two matched issues out of range",
            2,
            1,
            assessment(vec![found(0), missed()], vec![matches(&[0, 7])]),
            IssueOutOfRange {
                finding: 0,
                issue: 7,
            },
        ),
        (
            "credited to a false positive",
            1,
            1,
            assessment(vec![found(0)], vec![false_positive()]),
            CreditedWithoutMatch {
                issue: 0,
                finding: 0,
            },
        ),
        (
            "credited to a valid extra",
            1,
            1,
            assessment(vec![half_found(0)], vec![valid_extra()]),
            CreditedWithoutMatch {
                issue: 0,
                finding: 0,
            },
        ),
        (
            "credited to a finding that matches another issue",
            2,
            1,
            assessment(vec![found(0), missed()], vec![matches(&[1])]),
            CreditedWithoutMatch {
                issue: 0,
                finding: 0,
            },
        ),
        (
            "a match of nothing",
            1,
            1,
            assessment(vec![missed()], vec![matches(&[])]),
            MatchesNothing { finding: 0 },
        ),
        (
            "matched but missed",
            1,
            1,
            assessment(vec![missed()], vec![matches(&[0])]),
            MatchedButMissed {
                finding: 0,
                issue: 0,
            },
        ),
        (
            "two matched, one of them missed",
            2,
            1,
            assessment(vec![found(0), missed()], vec![matches(&[0, 1])]),
            MatchedButMissed {
                finding: 0,
                issue: 1,
            },
        ),
    ];
    for (name, key_len, findings_len, assessment, expected) in cases {
        assert_eq!(
            validate(key_len, findings_len, &assessment),
            Err(expected),
            "{name}"
        );
    }
}

#[test]
fn the_first_fault_is_reported_in_the_documented_order() {
    use AssessmentFault::*;
    // The counts come first, issues before findings.
    assert_eq!(
        validate(2, 0, &assessment(vec![found(9)], vec![])),
        Err(IssueCount {
            expected: 2,
            got: 1
        })
    );
    assert_eq!(
        validate(1, 1, &assessment(vec![], vec![])),
        Err(IssueCount {
            expected: 1,
            got: 0
        })
    );
    // Then the indexes: the issues' findings before the findings' issues.
    assert_eq!(
        validate(1, 1, &assessment(vec![found(5)], vec![matches(&[5])])),
        Err(FindingOutOfRange {
            issue: 0,
            finding: 5
        })
    );
    // The indexes come before the credits that do not match.
    assert_eq!(
        validate(
            2,
            1,
            &assessment(vec![found(0), found(4)], vec![false_positive()])
        ),
        Err(FindingOutOfRange {
            issue: 1,
            finding: 4
        })
    );
    // Credits that do not match come before what a finding matches.
    assert_eq!(
        validate(1, 1, &assessment(vec![found(0)], vec![matches(&[])])),
        Err(CreditedWithoutMatch {
            issue: 0,
            finding: 0
        })
    );
}

#[test]
fn a_finding_and_an_issue_are_a_pair_when_their_lines_overlap() {
    let key = [issue(Severity::Major, 4, 6), issue(Severity::Minor, 10, 12)];
    let findings = [
        finding_at(3, 4),
        finding_at(6, 8),
        finding_at(9, 9),
        finding_at(5, 11),
    ];
    assert_eq!(
        overlapping(&findings, &key),
        vec![(0, 0), (1, 0), (3, 0), (3, 1)]
    );
}

#[test]
fn lines_next_to_an_issue_do_not_overlap_it() {
    let key = [issue(Severity::Major, 4, 6)];
    assert!(overlapping(&[finding_at(2, 3), finding_at(7, 9)], &key).is_empty());
}

#[test]
fn a_finding_overlaps_only_issues_in_its_own_file() {
    let key = [issue(Severity::Major, 4, 6)];
    let elsewhere = Finding {
        span: Some(span("other.py", 4, 6)),
        text: "This looks wrong.".to_owned(),
        severity: None,
    };
    assert!(overlapping(&[elsewhere], &key).is_empty());
}

#[test]
fn a_finding_with_no_span_overlaps_nothing() {
    let key = [issue(Severity::Major, 1, 20)];
    let general = Finding {
        span: None,
        text: "The whole design is odd.".to_owned(),
        severity: Some(Severity::Minor),
    };
    assert!(overlapping(&[general], &key).is_empty());
}

#[test]
fn no_findings_and_no_key_overlap_nothing() {
    assert!(overlapping(&[], &four_issues()).is_empty());
    assert!(overlapping(&[finding_at(1, 1)], &[]).is_empty());
}

#[test]
fn a_review_that_found_nothing_scores_nothing_and_has_no_precision() {
    let missed_all = assessment(vec![missed(), missed(), missed(), missed()], vec![]);
    let scored = score(&four_issues(), &[], &missed_all);
    assert_eq!((scored.found, scored.half_found, scored.missed), (0, 0, 4));
    assert_eq!((scored.valid_extras, scored.false_positives), (0, 0));
    assert_eq!(scored.weighted_recall, 0.0);
    assert_eq!(scored.precision, None);
    assert!(!scored.led_with_the_most_severe);
    assert_eq!(
        scored.by_severity,
        vec![
            tally(Severity::Critical, 0, 0, 1),
            tally(Severity::Major, 0, 0, 1),
            tally(Severity::Minor, 0, 0, 1),
            tally(Severity::Nit, 0, 0, 1),
        ]
    );
}

#[test]
fn a_review_that_found_everything_scores_in_full() {
    let findings = [
        finding_at(1, 1),
        finding_at(5, 5),
        finding_at(9, 9),
        finding_at(13, 13),
    ];
    let everything = assessment(
        vec![found(0), found(1), found(2), found(3)],
        vec![matches(&[0]), matches(&[1]), matches(&[2]), matches(&[3])],
    );
    let scored = score(&four_issues(), &findings, &everything);
    assert_eq!((scored.found, scored.half_found, scored.missed), (4, 0, 0));
    assert_eq!(scored.weighted_recall, 1.0);
    assert_eq!(scored.precision, Some(1.0));
    assert!(scored.led_with_the_most_severe);
    assert_eq!(
        scored.by_severity,
        vec![
            tally(Severity::Critical, 1, 0, 1),
            tally(Severity::Major, 1, 0, 1),
            tally(Severity::Minor, 1, 0, 1),
            tally(Severity::Nit, 1, 0, 1),
        ]
    );
}

#[test]
fn finding_everything_but_the_nit_first_does_not_lead_with_the_most_severe() {
    let findings = [
        finding_at(13, 13),
        finding_at(1, 1),
        finding_at(5, 5),
        finding_at(9, 9),
    ];
    let nit_first = assessment(
        vec![found(1), found(2), found(3), found(0)],
        vec![matches(&[3]), matches(&[0]), matches(&[1]), matches(&[2])],
    );
    let scored = score(&four_issues(), &findings, &nit_first);
    assert_eq!(scored.weighted_recall, 1.0);
    assert!(!scored.led_with_the_most_severe);
}

#[test]
fn a_half_found_issue_earns_half_its_weight() {
    let key = [
        issue(Severity::Critical, 1, 1),
        issue(Severity::Minor, 5, 5),
    ];
    let findings = [finding_at(1, 1), finding_at(5, 5)];
    let judged = assessment(
        vec![half_found(0), found(1)],
        vec![matches(&[0]), matches(&[1])],
    );
    let scored = score(&key, &findings, &judged);
    assert_eq!((scored.found, scored.half_found, scored.missed), (1, 1, 0));
    assert_eq!(scored.weighted_recall, 4.0 / 6.0);
    assert_eq!(scored.precision, Some(1.0));
    assert_eq!(
        scored.by_severity,
        vec![
            tally(Severity::Critical, 0, 1, 1),
            tally(Severity::Minor, 1, 0, 1),
        ]
    );
}

#[test]
fn a_missed_issue_weighs_by_its_severity() {
    let key = [issue(Severity::Critical, 1, 1), issue(Severity::Nit, 5, 5)];
    let findings = [finding_at(5, 5)];
    let judged = assessment(vec![missed(), found(0)], vec![matches(&[1])]);
    assert_eq!(score(&key, &findings, &judged).weighted_recall, 1.0 / 5.0);
    let judged = assessment(vec![found(0), missed()], vec![matches(&[0])]);
    assert_eq!(score(&key, &findings, &judged).weighted_recall, 4.0 / 5.0);
}

#[test]
fn a_duplicate_finding_matches_and_counts_toward_precision() {
    let key = [issue(Severity::Major, 5, 5)];
    let findings = [finding_at(5, 5), finding_at(5, 5)];
    let judged = assessment(vec![found(0)], vec![matches(&[0]), matches(&[0])]);
    let scored = score(&key, &findings, &judged);
    assert_eq!((scored.found, scored.missed), (1, 0));
    assert_eq!(scored.weighted_recall, 1.0);
    assert_eq!(scored.precision, Some(1.0));
    assert_eq!((scored.valid_extras, scored.false_positives), (0, 0));
}

#[test]
fn a_valid_extra_counts_toward_precision_but_not_recall() {
    let key = [issue(Severity::Major, 5, 5)];
    let findings = [finding_at(5, 5), finding_at(8, 8)];
    let judged = assessment(vec![found(0)], vec![matches(&[0]), valid_extra()]);
    let scored = score(&key, &findings, &judged);
    assert_eq!(scored.valid_extras, 1);
    assert_eq!(scored.weighted_recall, 1.0);
    assert_eq!(scored.precision, Some(1.0));
}

#[test]
fn a_false_positive_lowers_precision() {
    let key = [issue(Severity::Major, 5, 5)];
    let findings = [finding_at(5, 5), finding_at(8, 8)];
    let judged = assessment(vec![found(0)], vec![matches(&[0]), false_positive()]);
    let scored = score(&key, &findings, &judged);
    assert_eq!(scored.false_positives, 1);
    assert_eq!(scored.weighted_recall, 1.0);
    assert_eq!(scored.precision, Some(0.5));
}

#[test]
fn nothing_but_false_positives_has_a_precision_of_nothing() {
    let key = [issue(Severity::Major, 5, 5)];
    let findings = [finding_at(8, 8), finding_at(9, 9)];
    let judged = assessment(vec![missed()], vec![false_positive(), false_positive()]);
    let scored = score(&key, &findings, &judged);
    assert_eq!(scored.precision, Some(0.0));
    assert_eq!(scored.weighted_recall, 0.0);
}

#[test]
fn a_finding_that_names_two_issues_finds_both() {
    let key = [
        issue(Severity::Critical, 1, 1),
        issue(Severity::Minor, 5, 5),
    ];
    let findings = [finding_at(1, 5)];
    let judged = assessment(vec![found(0), found(0)], vec![matches(&[0, 1])]);
    let scored = score(&key, &findings, &judged);
    assert_eq!((scored.found, scored.missed), (2, 0));
    assert_eq!(scored.weighted_recall, 1.0);
    assert_eq!(scored.precision, Some(1.0));
    assert!(scored.led_with_the_most_severe);
}

#[test]
fn an_empty_key_is_fully_recalled_and_its_findings_are_extras_or_wrong() {
    let findings = [finding_at(1, 1), finding_at(2, 2)];
    let judged = assessment(vec![], vec![valid_extra(), false_positive()]);
    let scored = score(&[], &findings, &judged);
    assert_eq!((scored.found, scored.half_found, scored.missed), (0, 0, 0));
    assert_eq!(scored.weighted_recall, 1.0);
    assert_eq!(scored.precision, Some(0.5));
    assert!(scored.by_severity.is_empty());
    assert!(!scored.led_with_the_most_severe);
}

#[test]
fn an_empty_key_and_no_findings_is_fully_recalled_with_no_precision() {
    let scored = score(&[], &[], &assessment(vec![], vec![]));
    assert_eq!(scored.weighted_recall, 1.0);
    assert_eq!(scored.precision, None);
    assert!(!scored.led_with_the_most_severe);
}

/// Whether the first finding led, for a Minor issue on line 1 and a Critical
/// one on line 5, with a finding on each.
fn leads(judged: Assessment) -> bool {
    let key = [
        issue(Severity::Minor, 1, 1),
        issue(Severity::Critical, 5, 5),
    ];
    let findings = [finding_at(1, 1), finding_at(5, 5)];
    score(&key, &findings, &judged).led_with_the_most_severe
}

#[test]
fn the_first_finding_leads_when_it_found_the_most_severe_issue() {
    assert!(leads(assessment(
        vec![found(1), found(0)],
        vec![matches(&[1]), matches(&[0])]
    )));
}

#[test]
fn the_first_finding_does_not_lead_when_a_later_one_found_the_most_severe_issue() {
    assert!(!leads(assessment(
        vec![found(0), found(1)],
        vec![matches(&[0]), matches(&[1])]
    )));
}

#[test]
fn a_most_severe_issue_that_is_only_half_found_by_the_first_finding_is_not_led_with() {
    assert!(!leads(assessment(
        vec![found(0), half_found(0)],
        vec![matches(&[0, 1]), valid_extra()]
    )));
    assert!(!leads(assessment(
        vec![missed(), half_found(0)],
        vec![matches(&[1]), false_positive()]
    )));
}

#[test]
fn a_first_finding_merely_listed_for_the_most_severe_issue_does_not_lead() {
    assert!(!leads(assessment(
        vec![missed(), found(1)],
        vec![matches(&[1]), matches(&[1])]
    )));
}

#[test]
fn half_finding_a_lesser_issue_does_not_stop_the_first_finding_leading() {
    assert!(leads(assessment(
        vec![half_found(1), found(0)],
        vec![matches(&[1]), matches(&[0])]
    )));
}

#[test]
fn a_first_finding_that_found_nothing_does_not_lead() {
    assert!(!leads(assessment(
        vec![missed(), found(1)],
        vec![false_positive(), matches(&[1])]
    )));
    assert!(!leads(assessment(
        vec![missed(), found(1)],
        vec![valid_extra(), matches(&[1])]
    )));
}

#[test]
fn when_every_issue_has_one_severity_finding_any_of_them_first_leads() {
    let key = [issue(Severity::Major, 1, 1), issue(Severity::Major, 5, 5)];
    let findings = [finding_at(5, 5)];
    let judged = assessment(vec![missed(), found(0)], vec![matches(&[1])]);
    assert!(score(&key, &findings, &judged).led_with_the_most_severe);
}

#[test]
fn the_tallies_list_the_severities_present_most_severe_first() {
    let key = [
        issue(Severity::Nit, 1, 1),
        issue(Severity::Major, 5, 5),
        issue(Severity::Major, 9, 9),
    ];
    let findings = [finding_at(1, 1), finding_at(9, 9)];
    let judged = assessment(
        vec![found(0), missed(), half_found(1)],
        vec![matches(&[0]), matches(&[2])],
    );
    let scored = score(&key, &findings, &judged);
    assert_eq!(
        scored.by_severity,
        vec![
            tally(Severity::Major, 0, 1, 2),
            tally(Severity::Nit, 1, 0, 1),
        ]
    );
}

#[test]
fn an_assessment_that_is_too_short_scores_without_failing() {
    let key = [issue(Severity::Major, 1, 1), issue(Severity::Minor, 5, 5)];
    let findings = [finding_at(1, 1)];
    let scored = score(&key, &findings, &assessment(vec![found(0)], vec![]));
    assert_eq!((scored.found, scored.missed), (1, 1));
    assert_eq!(scored.false_positives, 1);
    assert_eq!(scored.precision, Some(0.0));
}

#[test]
fn an_assessment_is_written_in_snake_case() {
    let judged = assessment(
        vec![found(2), half_found(0), missed()],
        vec![matches(&[0, 1]), valid_extra(), false_positive()],
    );
    let written = serde_json::to_value(&judged).unwrap();
    assert_eq!(
        written["issues"][0][0],
        serde_json::json!({"found": {"finding": 2}})
    );
    assert_eq!(
        written["issues"][1][0],
        serde_json::json!({"half_found": {"finding": 0}})
    );
    assert_eq!(written["issues"][2][0], "missed");
    assert_eq!(
        written["findings"][0][0],
        serde_json::json!({"matches": {"issues": [0, 1]}})
    );
    assert_eq!(written["findings"][1][0], "valid_extra");
    assert_eq!(written["findings"][2][0], "false_positive");
    let read: Assessment = serde_json::from_value(written).unwrap();
    assert_eq!(read, judged);
}

#[test]
fn display_order_is_most_severe_first_and_ties_keep_their_positions() {
    let at = |severity: Severity, line: u32| issue(severity, line, line);
    assert_eq!(display_order(&[]), Vec::<usize>::new());
    assert_eq!(display_order(&four_issues()), vec![0, 1, 2, 3]);
    let key = vec![
        at(Severity::Minor, 1),
        at(Severity::Critical, 2),
        at(Severity::Major, 3),
        at(Severity::Critical, 4),
        at(Severity::Nit, 5),
        at(Severity::Minor, 6),
    ];
    assert_eq!(display_order(&key), vec![1, 3, 2, 0, 5, 4]);
}
