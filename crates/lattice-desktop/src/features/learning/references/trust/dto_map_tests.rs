//! The override input states only what its scope may set (decision 4): a
//! site sets a standing and never a level; a level, Primary included, comes
//! with a source or a site for a subject class, matched by the class key.

use super::dto_map::{core_scope, override_input, provenance, scope, trust_override};
use crate::features::learning::dto::{
    LearningSiteStanding, LearningSourceRelationKind, LearningSubjectClassDto, LearningTrustLevel,
    LearningTrustOverrideDto, LearningTrustOverrideInputDto, LearningTrustOverrideScopeDto,
};
use lattice_core::domain::provenance::{
    OverrideScope, ProvenanceRecord, RelationEvidence, RelationKind, RelationTarget, SiteStanding,
    SourceRelation, SubjectKey, TrustLevel, TrustOverride,
};
use serde_json::json;

#[test]
fn a_site_override_sets_a_standing_and_nothing_else() {
    let entry = override_input(
        &LearningTrustOverrideInputDto::Site {
            registrable_domain: " Example.ORG ".into(),
            standing: LearningSiteStanding::Institutional,
            note: Some("  ".into()),
        },
        7,
    )
    .expect("valid");
    assert_eq!(
        entry,
        TrustOverride::Site {
            registrable_domain: "example.org".into(),
            standing: SiteStanding::Institutional,
            note: None,
            set_at: 7,
        }
    );
}

#[test]
fn a_site_for_a_subject_class_may_be_primary_and_is_keyed_by_the_class() {
    let entry = override_input(
        &LearningTrustOverrideInputDto::SiteSubject {
            registrable_domain: "learn.microsoft.com".into(),
            class_label: "C#".into(),
            level: LearningTrustLevel::Primary,
            note: Some("Microsoft owns C#".into()),
        },
        1,
    )
    .expect("valid");
    let TrustOverride::SiteSubject { class, level, .. } = &entry else {
        panic!("a site-for-subject override");
    };
    assert_eq!(*level, TrustLevel::Primary);
    assert_eq!(class.label, "C#");
    assert_eq!(class.key, SubjectKey::of("C#"));
    assert_ne!(class.key, SubjectKey::of("C"), "C# is not C");
    assert!(matches!(
        trust_override(&entry),
        LearningTrustOverrideDto::SiteSubject {
            level: LearningTrustLevel::Primary,
            ..
        }
    ));
}

#[test]
fn a_site_for_a_subject_needs_a_class() {
    let refused = override_input(
        &LearningTrustOverrideInputDto::SiteSubject {
            registrable_domain: "developer.mozilla.org".into(),
            class_label: "   ".into(),
            level: LearningTrustLevel::Primary,
            note: None,
        },
        1,
    );
    assert!(refused.is_err());
}

#[test]
fn a_scope_round_trips_and_its_class_key_is_recomputed_from_the_label() {
    let dto = LearningTrustOverrideScopeDto::SiteSubject {
        registrable_domain: "developer.mozilla.org".into(),
        class: LearningSubjectClassDto {
            label: "Web platform APIs".into(),
            key: "a key the page sent".into(),
        },
    };
    let core = core_scope(&dto);
    let OverrideScope::SiteSubject { class, .. } = &core else {
        panic!("a site-for-subject scope");
    };
    assert_eq!(class.key, SubjectKey::of("Web platform APIs"));
    assert!(matches!(
        scope(&core),
        LearningTrustOverrideScopeDto::SiteSubject { class, .. } if class.key == "web platform apis"
    ));
}

#[test]
fn provenance_lists_the_relations_touching_a_version_in_both_directions() {
    let record: ProvenanceRecord = serde_json::from_value(json!({
        "source_version_id": "mirror",
        "program_id": "p",
        "origin": { "key": "web:gdpr-info.eu", "host": "gdpr-info.eu", "registrable_domain": "gdpr-info.eu", "tld_registry": "open" },
        "signals": { "source": "not_extracted", "reason": "a test fake" },
        "signals_sha256": "sha",
        "sketch": vec![0_u32; 128],
        "links": [],
        "extracted_from": "fetch",
        "extraction_version": "v1",
        "extracted_at": 1,
    }))
    .expect("provenance record");
    let edge = |from: &str, to: &str, kind| SourceRelation {
        from: from.into(),
        to: RelationTarget::Version(to.into()),
        kind,
        evidence: RelationEvidence::Text {
            resemblance: 0.9,
            containment: 0.98,
        },
    };
    let relations = [
        edge("mirror", "official", RelationKind::ContainsCopyOf),
        edge("blog", "mirror", RelationKind::Cites),
        edge("blog", "official", RelationKind::Cites),
    ];
    let dto = provenance(&record, &relations);
    assert_eq!(dto.site.as_deref(), Some("gdpr-info.eu"));
    assert_eq!(dto.not_extracted.as_deref(), Some("a test fake"));
    let kinds: Vec<_> = dto.relations.iter().map(|relation| relation.kind).collect();
    assert_eq!(
        kinds,
        vec![
            LearningSourceRelationKind::ContainsCopyOf,
            LearningSourceRelationKind::Cites
        ]
    );
}
