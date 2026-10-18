use super::*;
use crate::domain::provenance::claim_profile::{ClaimSubject, SubjectKind};
use crate::domain::provenance::overturn::{overturn, Overturn};
use crate::domain::provenance::reason::AddedBy;
use crate::domain::provenance::sufficiency::{Bar, CountedSupport, Decision, Missing, PassageKind};

const DAY: i64 = 86_400;
const READ: i64 = 1_790_000_000;

fn profile(kind: ClaimKind, temporal: TemporalScope) -> ClaimProfile {
    ClaimProfile {
        kind,
        subject: Some(ClaimSubject::new(
            "Python",
            SubjectKind::Product,
            vec![],
            vec![],
        )),
        temporal,
        measurement_origin: None,
        attributed_to: None,
        reasons: vec![],
        policy: "p".into(),
        model: "m".into(),
    }
}

fn dated(published: i64) -> SourceDates {
    SourceDates {
        published: Some(published),
        acquired_at: READ,
        ..SourceDates::default()
    }
}

fn trust(level: TrustLevel) -> Trust {
    Trust {
        level,
        reasons: vec![],
        override_: None,
    }
}

fn support(id: &str, level: TrustLevel, published: i64) -> Support {
    Support {
        source_version_id: id.into(),
        origin_cluster: ClusterId(id.into()),
        trust: trust(level),
        dates: dated(published),
        passage: PassageKind::Direct,
        temporal: TemporalValidity::Valid,
        added_by: AddedBy::Research,
        cited_urls: vec![],
        owner_hints: vec![],
    }
}

fn contrary(id: &str, level: TrustLevel, published: i64) -> Contrary {
    Contrary {
        source_version_id: id.into(),
        origin_cluster: ClusterId(id.into()),
        trust: trust(level),
        dates: dated(published),
        temporal: TemporalValidity::Valid,
        quote: "Python 3.14 is the current stable release".into(),
    }
}

fn current() -> ClaimProfile {
    profile(ClaimKind::Specification, TemporalScope::Current)
}

#[test]
fn a_higher_level_contrary_wins_and_a_lower_one_is_only_a_dispute() {
    let supports = [support("blog", TrustLevel::Established, READ - 400 * DAY)];
    let python = contrary("python-org", TrustLevel::Primary, READ - 5 * DAY);
    let resolution = resolve(
        &current(),
        &supports,
        &python,
        std::slice::from_ref(&python),
    );
    assert!(
        matches!(resolution, Resolution::ContraryWins { ref reasons } if reasons[0].code == ReasonCode::OutranksByLevel)
    );

    let primary = [support("python-org", TrustLevel::Primary, READ - 5 * DAY)];
    let forum = contrary("forum", TrustLevel::Community, READ - DAY);
    let Resolution::SupportStands { note: Some(note) } =
        resolve(&current(), &primary, &forum, std::slice::from_ref(&forum))
    else {
        panic!()
    };
    assert_eq!(note.code, ReasonCode::DisputedByLowerStanding);
}

#[test]
fn at_equal_level_the_newer_source_wins_for_changing_subjects_but_not_for_history() {
    let supports = [support("old", TrustLevel::Authoritative, READ - 400 * DAY)];
    let newer = contrary("new", TrustLevel::Authoritative, READ - 10 * DAY);
    assert!(matches!(
        resolve(&current(), &supports, &newer, std::slice::from_ref(&newer)),
        Resolution::ContraryWins { .. }
    ));
    let history = profile(ClaimKind::Historical, TemporalScope::Timeless);
    assert!(matches!(
        resolve(&history, &supports, &newer, std::slice::from_ref(&newer)),
        Resolution::Escalate { .. }
    ));
}

#[test]
fn the_side_placed_at_the_claims_version_wins() {
    let version = profile(
        ClaimKind::VersionBound,
        TemporalScope::Since {
            version: Some("7.2".into()),
            date: None,
        },
    );
    let supports = [support("docs", TrustLevel::Established, READ)];
    let mut unplaced = contrary("blog", TrustLevel::Primary, READ);
    unplaced.temporal = TemporalValidity::CannotPlace {
        reason: "no version".into(),
    };
    let Resolution::SupportStands { note: Some(note) } = resolve(
        &version,
        &supports,
        &unplaced,
        std::slice::from_ref(&unplaced),
    ) else {
        panic!()
    };
    assert_eq!(note.code, ReasonCode::TemporallyPlaced);
}

#[test]
fn independent_origins_outweigh_a_single_community_origin() {
    let history = profile(ClaimKind::Historical, TemporalScope::Timeless);
    let supports = [support("forum", TrustLevel::Community, READ)];
    let a = contrary("a", TrustLevel::Community, READ);
    let b = contrary("b", TrustLevel::Community, READ);
    let both = [a.clone(), b];
    let Resolution::ContraryWins { reasons } = resolve(&history, &supports, &a, &both) else {
        panic!()
    };
    assert_eq!(reasons[0].code, ReasonCode::MoreIndependentOrigins);
}

#[test]
fn distrusted_contrary_is_ignored_and_equal_unplaceable_sides_escalate() {
    let supports = [support("s", TrustLevel::Authoritative, READ)];
    let distrusted = contrary("d", TrustLevel::Distrusted, READ);
    assert!(matches!(
        resolve(
            &current(),
            &supports,
            &distrusted,
            std::slice::from_ref(&distrusted)
        ),
        Resolution::SupportStands {
            note: Some(Reason {
                code: ReasonCode::IgnoredDistrusted,
                ..
            })
        }
    ));
    let same = contrary("c", TrustLevel::Authoritative, READ);
    let resolutions = resolve_all(&current(), &supports, std::slice::from_ref(&same));
    assert!(matches!(resolutions[0].1, Resolution::Escalate { .. }));
    assert!(
        matches!(overturn(&sufficient(), &resolutions), Overturn::Escalated { ref by, .. } if by == "c")
    );
}

fn sufficient() -> Decision {
    Decision::Sufficient {
        met_by: vec!["s".into()],
        label: None,
        reasons: vec![],
    }
}

#[test]
fn overturn_takes_a_verdict_only_for_winning_contrary_evidence_or_a_failed_bar() {
    let disputed = vec![(
        "forum".to_owned(),
        Resolution::SupportStands {
            note: Some(Reason::new(ReasonCode::DisputedByLowerStanding, "x")),
        },
    )];
    assert_eq!(overturn(&sufficient(), &disputed), Overturn::None);
    let won = vec![(
        "python-org".to_owned(),
        Resolution::ContraryWins { reasons: vec![] },
    )];
    assert!(
        matches!(overturn(&sufficient(), &won), Overturn::Contradicted { ref by, .. } if by == "python-org")
    );
    let insufficient = Decision::Insufficient {
        bar: Bar::Primary,
        have: vec![CountedSupport {
            source_version_id: "blog".into(),
            origin_cluster: ClusterId("blog".into()),
            counted: Some(TrustLevel::Established),
        }],
        missing: Missing::NoPrimaryForSubject,
        next: vec![],
        reasons: vec![],
    };
    assert!(matches!(
        overturn(&insufficient, &[]),
        Overturn::InsufficientStanding {
            bar: Bar::Primary,
            ..
        }
    ));
    assert!(
        matches!(overturn(&insufficient, &won), Overturn::Contradicted { .. }),
        "contrary evidence comes first"
    );
}
