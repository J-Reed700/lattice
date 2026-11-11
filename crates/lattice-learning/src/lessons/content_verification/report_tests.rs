//! What the stored report keeps of the findings a published edit, put back
//! or removal replaced, and gives back when it is undone.
use super::claim_ledger_tests::pass_report;
use super::*;
use lattice_core::domain::learning::claim_review::ClaimId;
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
