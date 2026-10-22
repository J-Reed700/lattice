use super::*;
use crate::domain::provenance::calibration::{effective_bars, Calibration, Outcome};
use crate::domain::provenance::claim_profile::{ClaimSubject, SubjectKind};
use crate::domain::provenance::policy::POLICY;
use crate::domain::provenance::temporal::TemporalValidity;

fn profile(kind: ClaimKind) -> ClaimProfile {
    ClaimProfile {
        kind,
        subject: Some(ClaimSubject::new(
            "OpenAI API",
            SubjectKind::Product,
            vec![],
            vec![],
        )),
        temporal: TemporalScope::Current,
        measurement_origin: None,
        attributed_to: None,
        reasons: vec![],
        policy: "claim-profile-v1".into(),
        model: "m".into(),
    }
}

fn support(id: &str, cluster: &str, level: TrustLevel) -> Support {
    Support {
        source_version_id: id.into(),
        origin_cluster: ClusterId(cluster.into()),
        trust: Trust {
            level,
            reasons: vec![],
            override_: None,
        },
        dates: SourceDates::default(),
        passage: PassageKind::Direct,
        temporal: TemporalValidity::Valid,
        added_by: AddedBy::Research,
        cited_urls: vec![],
        owner_hints: vec![],
    }
}

fn testimonial(mut support: Support, of: &str) -> Support {
    support.passage = PassageKind::Testimonial {
        attributed_to: of.into(),
        borne_out: None,
    };
    support
}

fn static_bars() -> EffectiveBars {
    EffectiveBars::unmoved(&StaticBars::table(), &POLICY)
}

fn decide_static(profile: &ClaimProfile, supports: &[Support]) -> Decision {
    decide(
        profile,
        supports,
        &[],
        &static_bars(),
        &Available::default(),
    )
}

#[test]
fn a_primary_meets_a_specification_and_an_established_blog_alone_does_not() {
    let decision = decide_static(
        &profile(ClaimKind::Specification),
        &[
            support("blog", "blog", TrustLevel::Established),
            support("docs", "docs", TrustLevel::Primary),
        ],
    );
    let Decision::Sufficient { met_by, label, .. } = decision else {
        panic!("{decision:?}")
    };
    assert_eq!(met_by, vec!["docs".to_string()]);
    assert_eq!(label, None);

    let mut blog = support("blog", "blog", TrustLevel::Established);
    blog.cited_urls = vec!["https://platform.openai.com/docs/guides/structured-outputs".into()];
    blog.owner_hints = vec!["platform.openai.com".into(), "openai.com".into()];
    let decision = decide_static(&profile(ClaimKind::Specification), &[blog]);
    let Decision::Insufficient {
        bar,
        missing,
        next,
        have,
        reasons,
    } = decision
    else {
        panic!()
    };
    assert_eq!(bar, Bar::Primary);
    assert_eq!(missing, Missing::NoPrimaryForSubject);
    assert_eq!(have[0].counted, Some(TrustLevel::Established));
    assert_eq!(
        next,
        vec![
            NextStep::FollowCitation {
                url: "https://platform.openai.com/docs/guides/structured-outputs".into(),
                from: "blog".into()
            },
            NextStep::SearchOwner {
                origin_hints: vec!["platform.openai.com".into(), "openai.com".into()]
            },
            NextStep::SearchIndependent,
            NextStep::AskUser,
        ]
    );
    assert!(reasons.last().unwrap().text.starts_with(
        "Specification claim about OpenAI API: supported only by established sources"
    ));
}

#[test]
fn an_authoritative_source_is_secondary_and_a_primary_contradiction_blocks_it() {
    let supports = [support("mdn", "mdn", TrustLevel::Authoritative)];
    let Decision::Sufficient { label, .. } =
        decide_static(&profile(ClaimKind::Specification), &supports)
    else {
        panic!()
    };
    assert_eq!(label, Some(Label::Secondary));
    let contrary = Contrary {
        source_version_id: "spec".into(),
        origin_cluster: ClusterId("spec".into()),
        trust: Trust {
            level: TrustLevel::Primary,
            reasons: vec![],
            override_: None,
        },
        dates: SourceDates::default(),
        temporal: TemporalValidity::Valid,
        quote: "q".into(),
    };
    let decision = decide(
        &profile(ClaimKind::Specification),
        &supports,
        &[contrary],
        &static_bars(),
        &Available::default(),
    );
    let Decision::Insufficient { reasons, .. } = decision else {
        panic!()
    };
    assert!(reasons
        .iter()
        .any(|r| r.code == ReasonCode::PrimaryContradicts));
}

#[test]
fn two_independent_established_origins_corroborate_but_two_pages_of_one_origin_do_not() {
    let corroborated = decide_static(
        &profile(ClaimKind::Specification),
        &[
            support("a", "a", TrustLevel::Established),
            support("b", "b", TrustLevel::Established),
        ],
    );
    assert!(matches!(
        corroborated,
        Decision::Sufficient {
            label: Some(Label::Secondary),
            ..
        }
    ));
    let one_origin = decide_static(
        &profile(ClaimKind::Historical),
        &[
            support("a", "a", TrustLevel::Established),
            support("b", "a", TrustLevel::Established),
        ],
    );
    let Decision::Insufficient { missing, .. } = one_origin else {
        panic!()
    };
    assert_eq!(missing, Missing::NoIndependentCorroboration { have: 1 });
}

#[test]
fn a_testimonial_counts_as_community_until_its_chain_is_followed() {
    let healthline = testimonial(support("healthline", "h", TrustLevel::Authoritative), "WHO");
    let decision = decide_static(&profile(ClaimKind::Practice), &[healthline]);
    let Decision::Insufficient { missing, have, .. } = decision else {
        panic!()
    };
    assert_eq!(missing, Missing::OnlyTestimonial);
    assert_eq!(have[0].counted, Some(TrustLevel::Community));
}

#[test]
fn practice_is_labelled_and_two_community_origins_are_consensus() {
    let Decision::Sufficient { label, .. } = decide_static(
        &profile(ClaimKind::Practice),
        &[support("a", "a", TrustLevel::Established)],
    ) else {
        panic!()
    };
    assert_eq!(label, Some(Label::Practice));
    let Decision::Sufficient { label, .. } = decide_static(
        &profile(ClaimKind::Definition),
        &[
            support("a", "a", TrustLevel::Community),
            support("b", "b", TrustLevel::Community),
        ],
    ) else {
        panic!()
    };
    assert_eq!(label, Some(Label::CommunityConsensus));
}

#[test]
fn a_measurement_needs_its_origin_or_two_reports_naming_it() {
    let mut measurement = profile(ClaimKind::Measurement);
    measurement.measurement_origin = Some("USDA".into());
    let one = [testimonial(
        support("se", "se", TrustLevel::Authoritative),
        "USDA",
    )];
    let Decision::Insufficient { missing, bar, .. } = decide_static(&measurement, &one) else {
        panic!()
    };
    assert_eq!((bar, missing), (Bar::Origin, Missing::NoMeasurementOrigin));
}

#[test]
fn reported_measurements_match_the_origin_by_key() {
    let mut measurement = profile(ClaimKind::Measurement);
    measurement.measurement_origin = Some("USDA".into());
    let two = [
        testimonial(support("se", "se", TrustLevel::Authoritative), "USDA"),
        testimonial(support("nyt", "nyt", TrustLevel::Established), "usda"),
    ];
    let Decision::Sufficient { label, .. } = decide_static(&measurement, &two) else {
        panic!()
    };
    assert_eq!(
        label,
        Some(Label::ReportedBy {
            attributing_to: "USDA".into()
        })
    );
    let other = [
        testimonial(support("se", "se", TrustLevel::Authoritative), "USDA"),
        testimonial(support("nyt", "nyt", TrustLevel::Established), "FDA"),
    ];
    assert!(matches!(
        decide_static(&measurement, &other),
        Decision::Insufficient { .. }
    ));
}

#[test]
fn the_users_own_evidence_is_sufficient_and_says_so() {
    let mut pasted = support("pasted", "pasted", TrustLevel::Unknown);
    pasted.added_by = AddedBy::User;
    let Decision::Sufficient { reasons, label, .. } =
        decide_static(&profile(ClaimKind::Specification), &[pasted])
    else {
        panic!()
    };
    assert_eq!(label, None);
    assert_eq!(reasons[0].code, ReasonCode::AddedByYou);
}

#[test]
fn distrusted_and_misdated_supports_are_excluded_with_reasons() {
    let mut old = support("old", "old", TrustLevel::Primary);
    old.temporal = TemporalValidity::Invalid {
        reason: "dated before".into(),
    };
    let decision = decide_static(
        &profile(ClaimKind::Specification),
        &[support("bad", "bad", TrustLevel::Distrusted), old],
    );
    let Decision::Insufficient {
        have,
        reasons,
        missing,
        ..
    } = decision
    else {
        panic!()
    };
    assert!(have.iter().all(|h| h.counted.is_none()));
    let codes: Vec<ReasonCode> = reasons.iter().map(|r| r.code).collect();
    assert!(
        codes.contains(&ReasonCode::ExcludedDistrusted)
            && codes.contains(&ReasonCode::ExcludedInvalidDate)
    );
    assert_eq!(missing, Missing::NoPrimaryForSubject);
}

#[test]
fn undated_supports_count_lower_for_current_claims_and_stale_ones_fetch_newer_first() {
    let mut undated = support("u", "u", TrustLevel::Authoritative);
    undated.temporal = TemporalValidity::Undated {
        reason: "undated".into(),
    };
    let Decision::Insufficient { have, .. } =
        decide_static(&profile(ClaimKind::Specification), &[undated])
    else {
        panic!()
    };
    assert_eq!(have[0].counted, Some(TrustLevel::Established));

    let mut stale = support("s", "s", TrustLevel::Established);
    stale.temporal = TemporalValidity::Stale {
        newer_primary: "python-org".into(),
    };
    let Decision::Insufficient { next, missing, .. } =
        decide_static(&profile(ClaimKind::Specification), &[stale])
    else {
        panic!()
    };
    assert_eq!(
        next[0],
        NextStep::FetchNewer {
            newer_primary: "python-org".into()
        }
    );
    assert_eq!(
        missing,
        Missing::OnlyStale {
            newer_primary: "python-org".into()
        }
    );
}

#[test]
fn version_claims_exclude_supports_that_cannot_place_the_version() {
    let mut blog = support("b", "b", TrustLevel::Primary);
    blog.temporal = TemporalValidity::CannotPlace {
        reason: "no version".into(),
    };
    let decision = decide_static(&profile(ClaimKind::VersionBound), &[blog.clone()]);
    assert!(matches!(decision, Decision::Insufficient { .. }));
    assert!(matches!(
        decide_static(&profile(ClaimKind::Definition), &[blog]),
        Decision::Sufficient { .. }
    ));
}

#[test]
fn a_calibrated_bar_accepts_what_the_users_verdicts_showed_reliable() {
    let outcomes: Vec<Outcome> = (0..40)
        .map(|i| Outcome {
            level: TrustLevel::Established,
            kind: ClaimKind::Specification,
            decision_was_sufficient: false,
            right: false,
            at: i,
        })
        .collect();
    let bars = effective_bars(
        &StaticBars::table(),
        &Calibration::from_outcomes(&outcomes),
        &POLICY,
    );
    let decision = decide(
        &profile(ClaimKind::Specification),
        &[support("blog", "blog", TrustLevel::Established)],
        &[],
        &bars,
        &Available::default(),
    );
    let Decision::Sufficient { reasons, label, .. } = decision else {
        panic!("{decision:?}")
    };
    assert_eq!(label, Some(Label::Secondary));
    assert!(reasons.iter().any(|r| r.code == ReasonCode::CalibratedBar));
}

#[test]
fn opinions_need_an_identified_source_and_are_attributed() {
    let Decision::Sufficient { label, .. } = decide_static(
        &profile(ClaimKind::Opinion),
        &[support("col", "col", TrustLevel::Established)],
    ) else {
        panic!()
    };
    assert_eq!(label, Some(Label::Attributed));
    let Decision::Insufficient { missing, bar, .. } = decide_static(
        &profile(ClaimKind::Opinion),
        &[support("f", "f", TrustLevel::Community)],
    ) else {
        panic!()
    };
    assert_eq!(
        (bar, missing),
        (Bar::Attributed, Missing::NoIdentifiedSource)
    );
}

// Best available (claim sources design §6.1) -----------------------------------------------

fn available(ceiling: TrustLevel, considered: usize) -> Available {
    Available {
        ceiling: Some(ceiling),
        considered,
    }
}

fn decide_available(
    profile: &ClaimProfile,
    supports: &[Support],
    contrary: &[Contrary],
    available: &Available,
) -> Decision {
    decide(profile, supports, contrary, &static_bars(), available)
}

fn contrary_at(id: &str, level: TrustLevel) -> Contrary {
    Contrary {
        source_version_id: id.into(),
        origin_cluster: ClusterId(id.into()),
        trust: Trust {
            level,
            reasons: vec![],
            override_: None,
        },
        dates: SourceDates::default(),
        temporal: TemporalValidity::Valid,
        quote: "q".into(),
    }
}

#[test]
fn the_courses_strongest_source_on_a_subject_meets_the_bar_as_best_available() {
    // A kind whose static bar is above Established, so neither the bar nor
    // corroboration settles it and the best-available rule decides.
    let decision = decide_available(
        &profile(ClaimKind::Specification),
        &[support("book", "book", TrustLevel::Established)],
        &[],
        &available(TrustLevel::Established, 3),
    );
    let Decision::Sufficient { label, reasons, .. } = decision else {
        panic!("{decision:?}")
    };
    assert_eq!(
        label,
        Some(Label::BestAvailable {
            level: TrustLevel::Established
        })
    );
    assert!(reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::MetAsBestAvailable
            && reason.text.contains("3 sources considered")));
}

#[test]
fn nothing_considered_decides_as_the_static_bars_alone() {
    let decision = decide_available(
        &profile(ClaimKind::Specification),
        &[support("book", "book", TrustLevel::Established)],
        &[],
        &Available::default(),
    );
    assert!(matches!(
        decision,
        Decision::Insufficient {
            missing: Missing::NoPrimaryForSubject,
            ..
        }
    ));
}

#[test]
fn a_stronger_silent_source_blocks_best_available() {
    let decision = decide_available(
        &profile(ClaimKind::Specification),
        &[support("book", "book", TrustLevel::Established)],
        &[],
        &available(TrustLevel::Authoritative, 3),
    );
    let Decision::Insufficient {
        missing, reasons, ..
    } = decision
    else {
        panic!("{decision:?}")
    };
    assert_eq!(
        missing,
        Missing::StrongerSourceSilent {
            level: TrustLevel::Authoritative
        }
    );
    assert!(reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::StrongerSourceSilent));
}

#[test]
fn two_independent_community_origins_are_best_available_but_one_is_not() {
    let spec = profile(ClaimKind::Specification);
    let community = available(TrustLevel::Community, 4);
    let two = decide_available(
        &spec,
        &[
            support("a", "a", TrustLevel::Community),
            support("b", "b", TrustLevel::Community),
        ],
        &[],
        &community,
    );
    assert!(matches!(
        two,
        Decision::Sufficient {
            label: Some(Label::BestAvailable {
                level: TrustLevel::Community
            }),
            ..
        }
    ));
    let one = decide_available(
        &spec,
        &[support("a", "a", TrustLevel::Community)],
        &[],
        &community,
    );
    assert!(matches!(one, Decision::Insufficient { .. }));
    let one_cluster = decide_available(
        &spec,
        &[
            support("a", "same", TrustLevel::Community),
            support("b", "same", TrustLevel::Community),
        ],
        &[],
        &community,
    );
    assert!(matches!(one_cluster, Decision::Insufficient { .. }));
}

#[test]
fn a_contrary_at_the_supports_level_blocks_best_available() {
    let decision = decide_available(
        &profile(ClaimKind::Specification),
        &[support("book", "book", TrustLevel::Established)],
        &[contrary_at("other", TrustLevel::Established)],
        &available(TrustLevel::Established, 2),
    );
    assert!(matches!(decision, Decision::Insufficient { .. }));
}

#[test]
fn a_measurement_is_never_best_available() {
    let decision = decide_available(
        &profile(ClaimKind::Measurement),
        &[support("book", "book", TrustLevel::Established)],
        &[],
        &available(TrustLevel::Established, 2),
    );
    assert!(matches!(decision, Decision::Insufficient { .. }));
}

#[test]
fn best_available_in_a_biology_course() {
    let species = |kind| ClaimProfile {
        subject: Some(ClaimSubject::new(
            "Quercus robur",
            SubjectKind::Other,
            vec!["English oak".into()],
            vec!["trees".into()],
        )),
        ..profile(kind)
    };
    let museum = support("museum", "museum", TrustLevel::Established);
    let field_guide = support("guide", "guide", TrustLevel::Established);
    for kind in [ClaimKind::Historical, ClaimKind::General] {
        let decision = decide_available(
            &species(kind),
            &[museum.clone(), field_guide.clone()],
            &[],
            &available(TrustLevel::Established, 2),
        );
        assert!(
            matches!(decision, Decision::Sufficient { .. }),
            "{kind:?}: {decision:?}"
        );
    }
    // A nomenclature code's rule, with the code's publisher in the course
    // (ceiling Primary) and only the field guide stating it.
    let decision = decide_available(
        &species(ClaimKind::Specification),
        &[field_guide],
        &[],
        &available(TrustLevel::Primary, 3),
    );
    assert!(matches!(
        decision,
        Decision::Insufficient {
            missing: Missing::StrongerSourceSilent {
                level: TrustLevel::Primary
            },
            ..
        }
    ));
}
