//! The trust view shows what the gate would decide on: an unjudged
//! publisher says so, a judged one is levelled with its reasons, overrides
//! fix the level and name their scope, a claim reads its profile and bar,
//! and the override menu counts the waiting claims an override reopens.

use super::source_view::{view, NOT_ASSESSED};
use super::test_support::{add_page, article, course, id, page_signals, Course, Saved};
use crate::features::learning::dto::{
    GetLearningSourceTrustRequestDto, LearningSourceStanding, LearningTrustClaimRequestDto,
    LearningTrustLevel, LearningTrustOverrideDto, LearningTrustOverrideScopeDto,
};
use lattice_core::application::ports::source_trust::{SourceTrustStore, StoredJudgement};
use lattice_core::application::services::source_trust::{claim_profile_key, standing_key};
use lattice_core::domain::provenance::{
    ClaimProfile, SiteStanding, StandingAssessment, TrustLevel, TrustOverride,
};
use lattice_core::shared::error::Result;
use serde_json::json;

const MODEL: &str = "test-model";

/// The saved source's trust: the course's own seeded source is listed too,
/// and first.
fn of_source(course: &Course, saved: &Saved) -> GetLearningSourceTrustRequestDto {
    GetLearningSourceTrustRequestDto {
        source_ids: vec![saved.source_id.clone()],
        ..request(course, None)
    }
}

fn request(
    course: &Course,
    claim: Option<LearningTrustClaimRequestDto>,
) -> GetLearningSourceTrustRequestDto {
    GetLearningSourceTrustRequestDto {
        program_id: course.program_id.clone(),
        source_ids: Vec::new(),
        claim,
        include_impact: false,
    }
}

async fn page(course: &Course, url: &str) -> Result<Saved> {
    add_page(
        course,
        url,
        &article(&id(), 8),
        page_signals(url, json!({})),
    )
    .await
}

/// An institutional, editorial publisher of documentation: Authoritative
/// for any subject it does not own (§5 step 6).
async fn judge_institutional(course: &Course, saved: &Saved) -> Result<()> {
    let record = course
        .store
        .provenance(&saved.version_id)
        .await?
        .expect("capture records provenance");
    let value: StandingAssessment = serde_json::from_value(json!({
        "standing": "institutional",
        "publisher": { "name": "Example Docs" },
        "publisher_kind": "vendor",
        "editorial": "editorial",
        "nature": "documentation",
        "flags": [],
        "reasons": [{ "code": "publisher_metadata", "text": "JSON-LD publisher: Example Docs" }],
        "policy": "source-standing-v1",
        "model": MODEL,
        "signals_sha256": record.signals_sha256,
    }))
    .expect("standing assessment");
    course
        .store
        .save_standing(&StoredJudgement {
            key: standing_key(&record.signals, MODEL),
            value,
            input_tokens: 1,
            output_tokens: 1,
            assessed_at: 1,
        })
        .await
}

#[tokio::test]
async fn an_unjudged_publisher_reads_as_unknown_and_says_why() -> Result<()> {
    let course = course().await?;
    let saved = page(&course, "https://docs.example.org/a").await?;
    let trust = view(&course.store, &of_source(&course, &saved)).await?;
    let entry = &trust.sources[0];
    assert_eq!(entry.source_id, saved.source_id);
    assert_eq!(entry.trust.level, LearningTrustLevel::Unknown);
    assert_eq!(entry.trust.reasons[0].code, NOT_ASSESSED);
    assert_eq!(entry.standing, None);
    let provenance = entry.provenance.as_ref().expect("provenance");
    assert_eq!(provenance.site.as_deref(), Some("example.org"));
    Ok(())
}

#[tokio::test]
async fn a_judged_publisher_is_levelled_with_its_reasons() -> Result<()> {
    let course = course().await?;
    let saved = page(&course, "https://docs.example.org/a").await?;
    judge_institutional(&course, &saved).await?;
    let trust = view(&course.store, &of_source(&course, &saved)).await?;
    let entry = &trust.sources[0];
    assert_eq!(entry.trust.level, LearningTrustLevel::Authoritative);
    assert_eq!(entry.trust.subject, None);
    assert!(!entry.trust.reasons.is_empty());
    assert_eq!(entry.standing, Some(LearningSourceStanding::Institutional));
    assert_eq!(
        entry.standing_reasons[0].text,
        "JSON-LD publisher: Example Docs"
    );
    assert_eq!(entry.trust.override_scope, None);
    Ok(())
}

#[tokio::test]
async fn a_source_override_fixes_the_level_and_names_its_scope() -> Result<()> {
    let course = course().await?;
    let saved = page(&course, "https://docs.example.org/a").await?;
    judge_institutional(&course, &saved).await?;
    course
        .store
        .set_override(&TrustOverride::Source {
            program_id: course.program_id.clone(),
            source_id: saved.source_id.clone(),
            level: TrustLevel::Primary,
            note: Some("The vendor's own reference".into()),
            set_at: 1,
        })
        .await?;
    let trust = view(&course.store, &of_source(&course, &saved)).await?;
    let entry = &trust.sources[0];
    assert_eq!(entry.trust.level, LearningTrustLevel::Primary);
    assert_eq!(
        entry.trust.override_scope,
        Some(LearningTrustOverrideScopeDto::Source {
            program_id: course.program_id.clone(),
            source_id: saved.source_id.clone(),
        })
    );
    assert!(matches!(
        entry.overrides.as_slice(),
        [LearningTrustOverrideDto::Source {
            level: LearningTrustLevel::Primary,
            ..
        }]
    ));
    Ok(())
}

#[tokio::test]
async fn a_site_override_is_listed_on_its_sites_sources_only() -> Result<()> {
    let course = course().await?;
    page(&course, "https://docs.example.org/a").await?;
    page(&course, "https://elsewhere.example.net/b").await?;
    course
        .store
        .set_override(&TrustOverride::Site {
            registrable_domain: "example.org".into(),
            standing: SiteStanding::Community,
            note: None,
            set_at: 1,
        })
        .await?;
    let trust = view(&course.store, &request(&course, None)).await?;
    let listed: Vec<usize> = trust
        .sources
        .iter()
        .map(|entry| entry.overrides.len())
        .collect();
    assert_eq!(listed.iter().sum::<usize>(), 1);
    Ok(())
}

#[tokio::test]
async fn a_claim_reads_its_profile_its_supports_and_the_bar_for_its_kind() -> Result<()> {
    let course = course().await?;
    let saved = page(&course, "https://docs.example.org/a").await?;
    judge_institutional(&course, &saved).await?;
    let statement = "With strict mode on, the response matches the schema.";
    let claim = LearningTrustClaimRequestDto {
        statement: statement.into(),
        source_version_ids: vec![saved.version_id.clone(), saved.version_id.clone()],
    };
    let unprofiled = view(&course.store, &request(&course, Some(claim.clone()))).await?;
    let unprofiled = unprofiled.claim.expect("claim asked for");
    assert_eq!(unprofiled.profile, None);
    assert_eq!(unprofiled.bar, None);
    assert_eq!(unprofiled.supports.len(), 1, "a version is levelled once");

    let profile: ClaimProfile = serde_json::from_value(json!({
        "kind": "specification",
        "subject": { "name": "Example API", "kind": "product", "aliases": [], "classes": ["web APIs"], "key": "example api" },
        "temporal": { "scope": "current" },
        "policy": "claim-profile-v1",
        "model": MODEL,
    }))
    .expect("claim profile");
    course
        .store
        .save_claim_profiles(&[StoredJudgement {
            key: claim_profile_key(statement, &[], MODEL),
            value: profile,
            input_tokens: 1,
            output_tokens: 1,
            assessed_at: 1,
        }])
        .await?;
    let profiled = view(&course.store, &request(&course, Some(claim))).await?;
    let profiled = profiled.claim.expect("claim asked for");
    let subject = profiled
        .profile
        .as_ref()
        .and_then(|profile| profile.subject.as_ref());
    assert_eq!(
        subject.map(|subject| subject.classes.clone()),
        Some(vec!["web APIs".to_owned()])
    );
    assert_eq!(
        profiled.supports[0].trust.subject.as_deref(),
        Some("Example API")
    );
    let bar = profiled.bar.expect("a profiled claim has a bar");
    assert_eq!(bar.claim_kind, "specification");
    assert_eq!(bar.static_lowest, LearningTrustLevel::Authoritative);
    Ok(())
}
