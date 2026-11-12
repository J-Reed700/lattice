//! What the stored report keeps of the findings a published edit, put back
//! or removal replaced, and gives back when it is undone.
use super::claim_ledger_tests::pass_report;
use super::resolution::Resolution;
use super::*;
use lattice_core::domain::learning::claim_review::{ClaimId, UnverifiedReason};
use std::collections::HashSet;

const OLD: &str = "Taken out by the machine.";
const PUT: &str = "You put this sentence back.";

fn finding(claim: &str, reason: &str) -> Finding {
    Finding {
        unit: 0,
        quote: "X.".into(),
        statement: "X.".into(),
        verdict: ClaimVerdict::Unsupported,
        reason: reason.into(),
        evidence: Vec::new(),
        supporting_quote: None,
        resolution: None,
        claim_id: Some(claim.into()),
        trust: None,
        label: None,
        class: None,
        two_pass: Default::default(),
    }
}

fn reasons(report: &LessonVerificationReport, claim: &str) -> Vec<String> {
    report
        .findings
        .iter()
        .filter(|finding| finding.claim_id.as_deref() == Some(claim))
        .map(|finding| finding.reason.clone())
        .collect()
}

fn put_back(report: &mut LessonVerificationReport, claim: &ClaimId) {
    report.written_back(&report::WrittenBack {
        unit: 0,
        old_body: "A.",
        range: (0, 0),
        new_body: "A. X.",
        replaced: &HashSet::new(),
        claim,
        sentence: "X.",
        statement: "X.",
        reason: PUT,
        at: 1,
    });
}

fn remove(report: &mut LessonVerificationReport, claim: &ClaimId) {
    let removed = HashSet::from([claim.0.clone()]);
    report.taken_out(&report::TakenOut {
        unit: 0,
        old_body: "A. X.",
        range: (3, 5),
        new_body: "A.",
        removed: &removed,
        claim,
    });
}

#[test]
fn undoing_the_removal_of_a_sentence_put_back_brings_back_the_put_back_finding_only() {
    let x = ClaimId("x".into());
    let claims = HashSet::from([x.0.clone()]);
    let mut report = pass_report(vec![finding("x", OLD)]);
    put_back(&mut report, &x);
    assert_eq!(reasons(&report, "x"), [PUT]);
    remove(&mut report, &x);
    assert!(reasons(&report, "x").is_empty());

    report.taken_back(0, "A. X.", &claims);

    assert_eq!(reasons(&report, "x"), [PUT]);

    report.written_out(0, "A.", &claims);

    assert_eq!(reasons(&report, "x"), [OLD]);
}

#[test]
fn a_statement_sharing_a_sentence_with_one_published_not_verified_counts_as_not_verified() {
    let mut unverified = finding("a", "Not supported.");
    unverified.resolution = Some(Resolution::Unverified {
        reason: UnverifiedReason::Unsupported,
    });
    let mut supported = finding("b", "Supported.");
    supported.verdict = ClaimVerdict::Supported;
    let report = pass_report(vec![unverified, supported]);

    assert_eq!(report.unverified_count(), 2);
}

fn later_section_report() -> LessonVerificationReport {
    let mut later = finding("later", "In a later section.");
    later.unit = 1;
    pass_report(vec![later])
}

#[test]
fn a_rewrite_that_takes_no_finding_puts_the_users_sentences_before_later_sections() {
    let mut report = later_section_report();
    let written = ClaimId("written".into());

    report.rewritten(&report::Rewritten {
        unit: 0,
        new_title: "Title",
        new_body: "New sentence.",
        ended: &HashSet::new(),
        written: &[("New sentence.", &written)],
        at: 1,
    });

    assert_eq!(report.findings[0].claim_id.as_deref(), Some("written"));
}

#[test]
fn a_sentence_written_over_nothing_lands_before_later_sections() {
    let mut report = later_section_report();
    let successor = ClaimId("written".into());

    report.written_over(&report::WrittenOver {
        unit: 0,
        old_body: "A.",
        range: (0, 0),
        new_body: "A. New.",
        replaced: &HashSet::new(),
        after: "New.",
        successor: &successor,
        at: 1,
    });

    assert_eq!(report.findings[0].claim_id.as_deref(), Some("written"));
}

#[test]
fn an_edit_of_an_empty_range_takes_no_finding() {
    let mut unclaimed = finding("unused", "Stays.");
    unclaimed.claim_id = None;
    unclaimed.quote = "Alpha beta.".into();
    let mut report = pass_report(vec![unclaimed]);
    let successor = ClaimId("written".into());

    report.written_over(&report::WrittenOver {
        unit: 0,
        old_body: "Alpha beta. Next.",
        range: (3, 3),
        new_body: "Alpha beta. Next.",
        replaced: &HashSet::new(),
        after: "New.",
        successor: &successor,
        at: 1,
    });

    assert!(report
        .findings
        .iter()
        .any(|finding| finding.reason == "Stays."));
}
