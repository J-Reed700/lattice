//! Forgetting and recording outcomes change all of them or none.
use super::outcomes;
use super::test_support::{course, id};
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::learning::claim_review::TrustVerdict;
use lattice_core::domain::provenance::{
    ClaimKind, LabelSource, OriginKey, Outcome, OutcomeRecord, TrustLevel,
};
use lattice_core::shared::error::Result;

fn outcome(program_id: &str, claim_id: &str, label_source: LabelSource, at: i64) -> OutcomeRecord {
    OutcomeRecord {
        id: id(),
        program_id: program_id.into(),
        lesson_id: "lesson".into(),
        claim_id: claim_id.into(),
        source_version_id: "version".into(),
        origin_key: OriginKey("web:example.org".into()),
        outcome: Outcome {
            level: TrustLevel::Established,
            kind: ClaimKind::Specification,
            decision_was_sufficient: true,
            right: false,
            at,
        },
        label_source,
        policy: "test-policy-v1".into(),
    }
}

#[tokio::test]
async fn forgetting_a_verdict_keeps_every_outcome_when_one_cannot_be_removed() -> Result<()> {
    let course = course().await?;
    for (label, at) in [
        (LabelSource::UserRemove, 1),
        (LabelSource::VerifierDisagree, 2),
    ] {
        course
            .store
            .record_outcome(&outcome(&course.program_id, "claim", label, at))
            .await?;
    }
    sqlx::query("CREATE TRIGGER refuse_for_test BEFORE DELETE ON source_trust_outcomes WHEN OLD.label_source = 'verifier_disagree' BEGIN SELECT RAISE(ABORT, 'refused for the test'); END")
        .execute(&course.pool)
        .await
        .unwrap();

    let forgotten = outcomes::forget(&course.database, "claim", TrustVerdict::Remove).await;

    assert!(forgotten.is_err());
    assert_eq!(course.store.outcomes().await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn recording_outcomes_stores_none_when_one_is_refused() -> Result<()> {
    let course = course().await?;
    sqlx::query("CREATE TRIGGER refuse_for_test BEFORE INSERT ON source_trust_outcomes WHEN NEW.at = 2 BEGIN SELECT RAISE(ABORT, 'refused for the test'); END")
        .execute(&course.pool)
        .await
        .unwrap();
    let records = [
        outcome(&course.program_id, "claim", LabelSource::UserConfirm, 1),
        outcome(&course.program_id, "claim", LabelSource::UserConfirm, 2),
    ];

    let recorded = course.store.record_outcomes(&records).await;

    assert!(recorded.is_err());
    assert!(course.store.outcomes().await?.is_empty());
    Ok(())
}
