#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use lattice_core::domain::provenance::{
    Bar, ClaimKind, ClaimProfile, ClaimSubject, Decision, Missing, NextStep, SubjectKey,
    SubjectKind, TemporalScope,
};

fn profile() -> ClaimProfile {
    ClaimProfile {
        kind: ClaimKind::Specification,
        subject: Some(ClaimSubject {
            name: "OpenAI API".into(),
            kind: SubjectKind::Product,
            aliases: Vec::new(),
            classes: Vec::new(),
            key: SubjectKey::of("OpenAI API"),
        }),
        temporal: TemporalScope::Current,
        measurement_origin: None,
        attributed_to: None,
        reasons: Vec::new(),
        policy: "test".into(),
        model: "test".into(),
    }
}

fn insufficient(next: Vec<NextStep>) -> FindingTrust {
    FindingTrust {
        profile: profile(),
        supports: Vec::new(),
        contrary: Vec::new(),
        decision: Decision::Insufficient {
            bar: Bar::Primary,
            have: Vec::new(),
            missing: Missing::NoPrimaryForSubject,
            next,
            reasons: Vec::new(),
        },
    }
}

fn sufficient() -> FindingTrust {
    FindingTrust {
        profile: profile(),
        supports: Vec::new(),
        contrary: Vec::new(),
        decision: Decision::Sufficient {
            met_by: vec!["docs".into()],
            label: None,
            reasons: Vec::new(),
        },
    }
}

fn finding(statement: &str, verdict: ClaimVerdict, trust: Option<FindingTrust>) -> Finding {
    Finding {
        unit: 0,
        quote: statement.into(),
        statement: statement.into(),
        verdict,
        reason: "The passage states it.".into(),
        evidence: Vec::new(),
        supporting_quote: None,
        resolution: None,
        trust: trust.map(Box::new),
    }
}

const GUIDE: &str = "https://platform.openai.com/docs/guides/structured-outputs";

#[test]
fn only_an_insufficient_or_disputed_support_is_scrutinised() {
    let findings = vec![
        finding("Unweighed.", ClaimVerdict::Supported, None),
        finding(
            "Weighed and enough.",
            ClaimVerdict::Supported,
            Some(sufficient()),
        ),
        finding(
            "Unsupported by the checker.",
            ClaimVerdict::Unsupported,
            None,
        ),
    ];
    assert_eq!(review(&findings, &[]), Scrutiny::default());
    assert!(!findings[0].wants_research());
    assert!(!findings[1].wants_research());
    assert!(findings[2].wants_research());
}

#[test]
fn an_insufficient_support_becomes_a_targeted_gap_and_citations_to_follow() {
    let next = vec![
        NextStep::FollowCitation {
            url: GUIDE.into(),
            from: "blog".into(),
        },
        NextStep::SearchOwner {
            origin_hints: vec!["platform.openai.com".into(), "openai.com".into()],
        },
        NextStep::SearchIndependent,
        NextStep::AskUser,
    ];
    let findings = vec![
        finding(
            "Strict mode follows the schema.",
            ClaimVerdict::Supported,
            Some(insufficient(next.clone())),
        ),
        finding(
            "Strict mode rejects extra keys.",
            ClaimVerdict::Supported,
            Some(insufficient(next)),
        ),
    ];
    assert!(findings.iter().all(Finding::wants_research));
    let scrutiny = review(&findings, &[]);
    assert_eq!(
        scrutiny.follow,
        vec![Follow {
            url: GUIDE.into(),
            from: "blog".into()
        }],
        "each citation is followed once"
    );
    assert_eq!(scrutiny.gaps.len(), 2);
    let gap = &scrutiny.gaps[0];
    assert_eq!(gap["claim"], "Strict mode follows the schema.");
    assert_eq!(gap["kind"], "specification");
    assert_eq!(gap["subject"], "OpenAI API");
    assert_eq!(
        gap["ownerOriginHints"],
        json!(["platform.openai.com", "openai.com"])
    );
    assert_eq!(gap["needsIndependentSource"], true);
    assert!(gap["missingEvidence"]
        .as_str()
        .unwrap()
        .contains("does not come from the subject's owner"));
    assert!(gap.get("needsNewerThan").is_none());
}

#[test]
fn places_in_one_cited_page_are_followed_as_that_page_once() {
    let at = |fragment: &str| NextStep::FollowCitation {
        url: format!("{GUIDE}#{fragment}"),
        from: "blog".into(),
    };
    let findings = vec![
        finding(
            "Strict mode follows the schema.",
            ClaimVerdict::Supported,
            Some(insufficient(vec![at("why-models-need-tools")])),
        ),
        finding(
            "Strict mode rejects extra keys.",
            ClaimVerdict::Supported,
            Some(insufficient(vec![at("exercises"), at("summary")])),
        ),
    ];
    let scrutiny = review(&findings, &[]);
    assert_eq!(
        scrutiny.follow,
        vec![Follow {
            url: format!("{GUIDE}#why-models-need-tools"),
            from: "blog".into()
        }],
        "the page is fetched once, at the best-ranked link"
    );
}

#[test]
fn a_stale_support_asks_for_newer_material() {
    let next = vec![NextStep::FetchNewer {
        newer_primary: "python-org".into(),
    }];
    let scrutiny = review(
        &[finding(
            "The current stable Python is 3.13.",
            ClaimVerdict::Supported,
            Some(insufficient(next)),
        )],
        &[],
    );
    assert!(scrutiny.gaps[0].get("needsNewerThan").is_some());
    assert!(scrutiny.follow.is_empty());
}

/// A saved version with `url`, as research reads the program's sources.
fn saved(id: &str, url: Option<&str>) -> LearningSourceDto {
    LearningSourceDto {
        id: id.into(),
        title: id.into(),
        url: url.map(str::to_owned),
        excerpt: String::new(),
        acquired_at: 0,
    }
}

#[test]
fn an_independent_source_gap_names_the_sites_its_supports_come_from() {
    use lattice_core::domain::provenance::{
        AddedBy, ClusterId, PassageKind, SourceDates, Support, TemporalValidity, Trust, TrustLevel,
    };
    let support = |id: &str| Support {
        source_version_id: id.into(),
        origin_cluster: ClusterId(id.into()),
        trust: Trust {
            level: TrustLevel::Established,
            reasons: Vec::new(),
            override_: None,
        },
        dates: SourceDates::default(),
        passage: PassageKind::Direct,
        temporal: TemporalValidity::Valid,
        added_by: AddedBy::Research,
        cited_urls: Vec::new(),
        owner_hints: Vec::new(),
    };
    let mut trust = insufficient(vec![NextStep::SearchIndependent]);
    trust.supports = vec![support("blog"), support("copy"), support("notes")];
    let sources = [
        saved("blog", Some("https://blog.example.com/strict-mode")),
        // An archived copy of the same site is the same origin.
        saved(
            "copy",
            Some("https://web.archive.org/web/20250101000000/https://www.example.com/strict"),
        ),
        // A file has no web origin to avoid.
        saved("notes", None),
    ];
    let scrutiny = review(
        &[finding(
            "Strict mode follows the schema.",
            ClaimVerdict::Supported,
            Some(trust),
        )],
        &sources,
    );
    let gap = &scrutiny.gaps[0];
    assert_eq!(gap["independentOf"], json!(["example.com"]));

    let excluded = excluded_sites(gap);
    assert!(published_on("https://docs.example.com/guide", &excluded));
    assert!(published_on(
        "https://web.archive.org/web/2024/https://example.com/old",
        &excluded
    ));
    assert!(!published_on("https://independent.org/review", &excluded));
    // Only a gap that asks for independence excludes anything.
    let mut owner_gap = gap.clone();
    owner_gap
        .as_object_mut()
        .unwrap()
        .remove("needsIndependentSource");
    assert!(excluded_sites(&owner_gap).is_empty());
}
