//! What the ledger records of the gate's weighing when a later pass sees the
//! same failure again.
use super::claim_ledger_tests::{pass_report, passage_finding};
use super::*;
use lattice_core::domain::learning::claim_review::{ClaimEvent, SupportedBy, TrustAssessment};
use lattice_core::domain::provenance::*;

fn assessment(version: &str, quote: &str) -> TrustAssessment {
    let profile = ClaimProfile {
        kind: ClaimKind::Definition,
        subject: None,
        temporal: TemporalScope::Timeless,
        measurement_origin: None,
        attributed_to: None,
        reasons: vec![],
        policy: "claim-profile-v1".into(),
        model: "fixture".into(),
    };
    let supports = vec![Support {
        source_version_id: version.into(),
        origin_cluster: ClusterId(version.into()),
        trust: Trust {
            level: TrustLevel::Unknown,
            reasons: vec![],
            override_: None,
        },
        dates: SourceDates::default(),
        passage: PassageKind::Direct,
        temporal: TemporalValidity::Valid,
        added_by: AddedBy::Research,
        cited_urls: vec![],
        owner_hints: vec![],
    }];
    TrustAssessment {
        decision: decide(
            &profile,
            &supports,
            &[],
            &EffectiveBars::unmoved(
                &StaticBars::table(),
                &lattice_core::domain::provenance::POLICY,
            ),
            &Available::default(),
        ),
        profile,
        supports,
        contrary: vec![],
        supported: SupportedBy {
            source_version_id: version.into(),
            supporting_quote: quote.into(),
        },
        policy: TRUST_POLICY_VERSION.into(),
        available: None,
    }
}

#[tokio::test]
async fn an_unchanged_failure_records_that_standing_reach_has_run_without_rechecking() -> Result<()>
{
    let model = Model::new();
    let text = "A mineral contains alpha.";
    let lesson = json!({"blocks":[{"kind":"explanation","body":text}],"questions":[]});
    let mut ledger = claim_ledger::ClaimLedger::open(&model, "{}").await?;
    let trust = assessment("source-1", text);
    let mut finding = passage_finding(text, text, true, text);
    finding.trust = Some(Box::new(provenance_gate::FindingTrust {
        profile: trust.profile,
        supports: trust.supports,
        contrary: vec![],
        decision: trust.decision,
        available: None,
    }));
    ledger
        .observe(&lesson, &pass_report(vec![finding.clone()]))
        .await?;
    let before = ledger.entries()[0].clone();
    finding.trust.as_mut().unwrap().available = Some(Available::default());
    ledger
        .observe(&lesson, &pass_report(vec![finding.clone()]))
        .await?;
    let after = &ledger.entries()[0];
    assert_eq!(before.state(), after.state());
    assert_eq!(before.basis(), after.basis());
    assert_eq!(after.trust().unwrap().available, Some(Available::default()));
    assert_eq!(after.events().len(), before.events().len() + 1);
    assert!(matches!(
        after.events().last().unwrap().event,
        ClaimEvent::StandingAssessed { .. }
    ));
    let count = after.events().len();
    ledger.observe(&lesson, &pass_report(vec![finding])).await?;
    assert_eq!(
        ledger.entries()[0].events().len(),
        count,
        "an unchanged current assessment adds nothing"
    );
    Ok(())
}
