//! The gate's weighing as the ledger records it: a trust assessment fixture.
use super::*;
use lattice_core::domain::learning::claim_review::{SupportedBy, TrustAssessment};
use lattice_core::domain::provenance::*;

pub(super) fn assessment(version: &str, quote: &str) -> TrustAssessment {
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
