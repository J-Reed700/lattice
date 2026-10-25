//! Held courses made before standing reach and checker changes must catch up
//! once, without asking the user to manufacture a claim action.
use super::claim_ledger_tests::{pass_report, passage_finding};
use super::claim_review_flow_tests::{Supporting, Waiting};
use super::*;
use crate::features::learning::lessons::claim_reviews::repository::ClaimReviewRepository;
use lattice_core::application::ports::{
    source_trust::{SourceTrustStore, StoredJudgement},
    ClaimReviewStore,
};
use lattice_core::application::services::source_trust::{standing_key, STANDING_POLICY};
use lattice_core::domain::learning::claim_review::{
    Actor, Check, CheckResult, Checker, ClaimEvent, EventProvenance, EvidenceFingerprint,
    GateVerdict, SupportedBy, TrustAssessment,
};
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

async fn hold_with_assessment(course: &Waiting, trust: TrustAssessment) -> Result<()> {
    let store = ClaimReviewRepository::new(course.pool.clone());
    let ledger = store.load(&course.lesson_id).await?;
    let entry = ledger
        .claims
        .iter()
        .find(|entry| entry.state().awaits_user())
        .unwrap();
    let provenance = EventProvenance {
        actor: Actor::Machine,
        at: 0,
        job_id: Some(course.job_id.clone()),
        app_version: "fixture".into(),
        checker_model: Some(ledger.review.as_ref().unwrap().checker_model.clone()),
    };
    let GateVerdict::Fails(kind) = trust.verdict() else {
        panic!("insufficient fixture")
    };
    let revision = store
        .append(
            &course.lesson_id,
            entry.claim_id(),
            provenance.clone(),
            ClaimEvent::Checked(Check {
                checker: Checker::Local {
                    model: ledger.review.as_ref().unwrap().checker_model.clone(),
                },
                result: CheckResult::Failed { kind },
                reason: "Source standing".into(),
                fingerprint: EvidenceFingerprint("legacy-standing".into()),
                evidence: vec![],
                window_tokens: 32768,
            }),
            ledger.revision,
        )
        .await?;
    store
        .append(
            &course.lesson_id,
            entry.claim_id(),
            provenance,
            ClaimEvent::StandingAssessed {
                trust: Box::new(trust),
                reason: "Source standing".into(),
                cause: None,
            },
            revision,
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_legacy_standing_hold_can_resume_without_a_model_change_or_user_action() -> Result<()> {
    let course = Waiting::new(&Supporting::new()).await?;
    hold_with_assessment(&course, assessment("legacy-source", "A source states it.")).await?;
    assert!(
        course
            .service()
            .review(&course.program_id, &course.lesson_id)
            .await?
            .unwrap()
            .changed_since_stop
    );
    let next = course.resume().await?;
    assert_eq!(
        next.status,
        crate::features::learning::curriculum::LearningGenerationJobStatus::Pending
    );
    Ok(())
}

#[tokio::test]
async fn a_missing_current_source_assessment_unlocks_resume_but_a_fresh_hold_does_not() -> Result<()>
{
    use crate::features::learning::references::trust::{
        judgements::content_evidence, SqliteSourceTrustStore,
    };
    let course = Waiting::new(&Supporting::new()).await?;
    let store = SqliteSourceTrustStore::new(course.pool.clone());
    let record = store
        .program_provenance(&course.program_id)
        .await?
        .remove(0);
    let mut trust = assessment(&record.source_version_id, "A source states it.");
    trust.available = Some(Available::default());
    hold_with_assessment(&course, trust).await?;
    assert!(
        course
            .service()
            .changed_since_stop(&course.lesson_id)
            .await?
    );
    let content = content_evidence(&course.pool, &course.program_id, &record)
        .await?
        .unwrap();
    let key = standing_key(&record.signals, &content, "fixture");
    store.save_standing(&StoredJudgement {
        value: serde_json::from_value(json!({"standing":"unknown","publisher":null,"publisher_kind":"unknown","editorial":"none","nature":"reference","flags":[],"reasons":[],"policy":STANDING_POLICY,"model":"fixture","signals_sha256":key.input_sha256}))?,
        key, input_tokens:0, output_tokens:0, assessed_at:1,
    }).await?;
    assert!(
        !course
            .service()
            .changed_since_stop(&course.lesson_id)
            .await?,
        "a fully processed failure is not an endless retry"
    );
    assert!(course.resume().await.is_err());
    Ok(())
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
