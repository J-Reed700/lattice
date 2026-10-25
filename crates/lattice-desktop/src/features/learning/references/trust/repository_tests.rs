//! The trust store round-trips every value the port stores, keys judgements
//! by what they judged, and keeps per-course rows with their course.

use super::test_support::{add_page, article, course, course_on, id, page_signals};
use lattice_core::application::ports::source_trust::{SourceTrustStore, StoredJudgement};
use lattice_core::domain::provenance::{
    AuthorityAssessment, ClaimKind, ClaimProfile, ClaimSubject, ExtractedFrom, JudgementKey,
    LabelSource, Origin, OriginKey, Outcome, OutcomeRecord, OverrideScope, ProvenanceRecord,
    ProvenanceSignals, RelationEvidence, RelationKind, RelationTarget, SiteStanding, Sketch,
    SourceRelation, StandingAssessment, SubjectClass, SubjectKey, SubjectKind, SubjectRelation,
    TemporalScope, TrackRecord, TrustLevel, TrustOverride, SKETCH_VALUES,
};
use lattice_core::shared::error::Result;
use serde_json::json;

fn key(input: &str) -> JudgementKey {
    JudgementKey {
        policy: "test-policy-v1".into(),
        model: "test-model".into(),
        input_sha256: input.into(),
    }
}

fn standing_assessment(signals_sha256: &str) -> StandingAssessment {
    serde_json::from_value(json!({
        "standing": "institutional",
        "publisher": { "name": "Example Tide Office" },
        "publisher_kind": "government",
        "editorial": "editorial",
        "nature": "documentation",
        "flags": [],
        "reasons": [{ "code": "publisher_metadata", "text": "JSON-LD publisher" }],
        "policy": "test-policy-v1",
        "model": "test-model",
        "signals_sha256": signals_sha256,
    }))
    .expect("standing assessment")
}

fn outcome(
    program_id: &str,
    origin: &str,
    sufficient: bool,
    right: bool,
    at: i64,
) -> OutcomeRecord {
    OutcomeRecord {
        id: id(),
        program_id: program_id.into(),
        lesson_id: "lesson".into(),
        claim_id: "claim".into(),
        source_version_id: "version".into(),
        origin_key: OriginKey(origin.into()),
        outcome: Outcome {
            level: TrustLevel::Established,
            kind: ClaimKind::Specification,
            decision_was_sufficient: sufficient,
            right,
            at,
        },
        label_source: LabelSource::UserConfirm,
        policy: "source-trust-v1".into(),
    }
}

#[tokio::test]
async fn provenance_round_trips_and_a_course_lists_its_active_versions_only() -> Result<()> {
    let course = course().await?;
    let url = "https://docs.example.org/round-trip";
    let saved = add_page(
        &course,
        url,
        &article("round", 8),
        page_signals(url, json!({})),
    )
    .await?;
    let stored = course
        .store
        .provenance(&saved.version_id)
        .await?
        .expect("row");

    // A re-extraction replaces the row whole.
    let replaced = ProvenanceRecord {
        signals: ProvenanceSignals::NotExtracted {
            reason: "re-read from the snapshot".into(),
        },
        sketch: Sketch([7; SKETCH_VALUES]),
        extracted_from: ExtractedFrom::Snapshot,
        extraction_version: "page-signals-v2".into(),
        extracted_at: stored.extracted_at + 1,
        ..stored.clone()
    };
    course.store.save_provenance(&replaced).await?;
    assert_eq!(
        course.store.provenance(&saved.version_id).await?,
        Some(replaced)
    );
    assert_eq!(course.store.provenance("no-such-version").await?, None);

    // The course lists its seeded source and the page; a removed source
    // drops out.
    assert_eq!(
        course
            .store
            .program_provenance(&course.program_id)
            .await?
            .len(),
        2
    );
    sqlx::query("UPDATE learning_source_library SET deleted_at=1 WHERE id=?")
        .bind(&saved.source_id)
        .execute(&course.pool)
        .await
        .unwrap();
    let listed = course.store.program_provenance(&course.program_id).await?;
    assert_eq!(listed.len(), 1);
    assert_ne!(listed[0].source_version_id, saved.version_id);
    Ok(())
}

#[tokio::test]
async fn judgements_round_trip_under_their_keys() -> Result<()> {
    let course = course().await?;
    let standing = StoredJudgement {
        key: key("signals-sha"),
        value: standing_assessment("signals-sha"),
        input_tokens: 812,
        output_tokens: 95,
        assessed_at: 1_760_000_000,
    };
    course.store.save_standing(&standing).await?;
    assert_eq!(
        course.store.standing(&standing.key).await?,
        Some(standing.clone())
    );
    let other_model = JudgementKey {
        model: "another-model".into(),
        ..standing.key.clone()
    };
    assert_eq!(course.store.standing(&other_model).await?, None);
    let second = StoredJudgement {
        key: key("second-signals"),
        value: standing_assessment("second-signals"),
        ..standing.clone()
    };
    course.store.save_standing(&second).await?;
    let (batch, reads) =
        crate::tests::common::query_counts::count_reads(course.store.standings(&[
            second.key.clone(),
            other_model,
            standing.key.clone(),
            second.key.clone(),
        ]))
        .await;
    assert_eq!(reads, 1);
    assert_eq!(
        batch?,
        vec![second.value.clone(), standing.value.clone(), second.value]
    );
    let (empty, reads) =
        crate::tests::common::query_counts::count_reads(course.store.standings(&[])).await;
    assert!(empty?.is_empty());
    assert_eq!(reads, 0);

    let origin = Origin::of_url("https://docs.python.org/3/").unwrap().key;
    let subject = SubjectKey::of("Python");
    let authority = StoredJudgement {
        key: key("ownership-input"),
        value: AuthorityAssessment {
            origin_key: origin.clone(),
            subject_key: subject.clone(),
            relation: SubjectRelation::Owner,
            basis: vec![],
            owner_origin_hints: vec!["docs.python.org".into()],
            policy: "test-policy-v1".into(),
            model: "test-model".into(),
        },
        input_tokens: 300,
        output_tokens: 40,
        assessed_at: 1_760_000_001,
    };
    course
        .store
        .save_authority(&course.program_id, &authority)
        .await?;
    let found = course
        .store
        .authority(
            &course.program_id,
            &origin,
            &subject,
            "test-policy-v1",
            "test-model",
        )
        .await?;
    assert_eq!(found, Some(authority));
    let other_course = course_on(&course.pool).await?;
    assert_eq!(
        course
            .store
            .authority(
                &other_course,
                &origin,
                &subject,
                "test-policy-v1",
                "test-model"
            )
            .await?,
        None,
        "ownership is judged per course"
    );

    let profiles: Vec<StoredJudgement<ClaimProfile>> = ["first claim", "second claim"]
        .into_iter()
        .map(|input| StoredJudgement {
            key: key(input),
            value: ClaimProfile {
                kind: ClaimKind::Specification,
                subject: Some(ClaimSubject::new(
                    "Python",
                    SubjectKind::Library,
                    vec![],
                    vec![],
                )),
                temporal: TemporalScope::Current,
                measurement_origin: None,
                attributed_to: None,
                reasons: vec![],
                policy: "test-policy-v1".into(),
                model: "test-model".into(),
            },
            input_tokens: 120,
            output_tokens: 30,
            assessed_at: 1_760_000_002,
        })
        .collect();
    course.store.save_claim_profiles(&profiles).await?;
    for profile in &profiles {
        assert_eq!(
            course.store.claim_profile(&profile.key).await?,
            Some(profile.clone())
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_relation_recorded_again_keeps_one_edge_with_the_new_evidence() -> Result<()> {
    let course = course().await?;
    let url = "https://docs.example.org/cited";
    let saved = add_page(
        &course,
        url,
        &article("cited", 8),
        page_signals(url, json!({})),
    )
    .await?;
    let cites = |anchor: &str| SourceRelation {
        from: saved.version_id.clone(),
        to: RelationTarget::Url("https://spec.example.org/rfc".into()),
        kind: RelationKind::Cites,
        evidence: RelationEvidence::Citation {
            start_byte: 0,
            end_byte: 10,
            anchor: anchor.into(),
        },
    };
    course
        .store
        .save_relations(&course.program_id, &[cites("the RFC")])
        .await?;
    course
        .store
        .save_relations(&course.program_id, &[cites("RFC 9110")])
        .await?;
    assert_eq!(
        course.store.relations(&course.program_id).await?,
        vec![cites("RFC 9110")]
    );
    // A target outside the course's versions is refused.
    let dangling = SourceRelation {
        to: RelationTarget::Version("no-such-version".into()),
        ..cites("x")
    };
    assert!(course
        .store
        .save_relations(&course.program_id, &[dangling])
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
async fn overrides_round_trip_at_each_scope_and_a_course_sees_its_own() -> Result<()> {
    let course = course().await?;
    let url = "https://docs.example.org/overridden";
    let saved = add_page(
        &course,
        url,
        &article("overridden", 8),
        page_signals(url, json!({})),
    )
    .await?;
    let other_course = course_on(&course.pool).await?;
    let source = TrustOverride::Source {
        program_id: course.program_id.clone(),
        source_id: saved.source_id.clone(),
        level: TrustLevel::Primary,
        note: Some("The vendor's own reference".into()),
        set_at: 1_760_000_000,
    };
    let site = TrustOverride::Site {
        registrable_domain: "example.org".into(),
        standing: SiteStanding::Institutional,
        note: None,
        set_at: 1_760_000_001,
    };
    let site_subject = TrustOverride::SiteSubject {
        registrable_domain: "mozilla.org".into(),
        class: SubjectClass::new("Web platform APIs", vec![]),
        level: TrustLevel::Primary,
        note: None,
        set_at: 1_760_000_002,
    };
    for entry in [&source, &site, &site_subject] {
        course.store.set_override(entry).await?;
    }
    let mine = course.store.overrides(&course.program_id).await?.entries;
    assert_eq!(mine.len(), 3);
    for entry in [&source, &site, &site_subject] {
        assert!(mine.contains(entry), "{entry:?}");
    }
    let theirs = course.store.overrides(&other_course).await?.entries;
    assert_eq!(
        theirs.len(),
        2,
        "another course sees only the site overrides"
    );
    assert_eq!(course.store.site_overrides().await?.len(), 2);

    // Setting again replaces; clearing reports whether there was one.
    let distrusted = TrustOverride::Site {
        standing: SiteStanding::Distrusted,
        set_at: 1_760_000_003,
        registrable_domain: "example.org".into(),
        note: Some("Content farm".into()),
    };
    course.store.set_override(&distrusted).await?;
    let sites = course.store.site_overrides().await?;
    assert!(sites.contains(&distrusted) && !sites.contains(&site));
    let scope = OverrideScope::Site {
        registrable_domain: "example.org".into(),
    };
    assert!(course.store.clear_override(&scope).await?);
    assert!(!course.store.clear_override(&scope).await?);
    Ok(())
}

#[tokio::test]
async fn outcomes_list_oldest_first_and_count_into_the_origin_track_record() -> Result<()> {
    let course = course().await?;
    let origin = "web:example.org";
    let held_sufficient = outcome(&course.program_id, origin, true, true, 30);
    let failed_sufficient = outcome(&course.program_id, origin, true, false, 10);
    let held_insufficient = outcome(&course.program_id, origin, false, false, 20);
    let elsewhere = outcome(&course.program_id, "web:other.example", true, true, 5);
    for entry in [
        &held_sufficient,
        &failed_sufficient,
        &held_insufficient,
        &elsewhere,
    ] {
        course.store.record_outcome(entry).await?;
    }
    let listed = course.store.outcomes().await?;
    assert_eq!(
        listed,
        vec![
            elsewhere,
            failed_sufficient,
            held_insufficient,
            held_sufficient
        ]
    );
    assert_eq!(
        course.store.track_record(&OriginKey(origin.into())).await?,
        TrackRecord { right: 2, wrong: 1 }
    );
    assert_eq!(
        course
            .store
            .track_record(&OriginKey("web:unseen.example".into()))
            .await?,
        TrackRecord::default()
    );
    Ok(())
}

#[tokio::test]
async fn site_lists_batch_history_and_count_an_origin_once_across_snapshots() -> Result<()> {
    let course = course().await?;
    let mut origin = None;
    for suffix in ["one", "two"] {
        let url = format!("https://docs.example.org/{suffix}");
        let saved = add_page(
            &course,
            &url,
            &article(suffix, 8),
            page_signals(&url, json!({})),
        )
        .await?;
        origin = Some(
            course
                .store
                .provenance(&saved.version_id)
                .await?
                .unwrap()
                .origin
                .key
                .0,
        );
    }
    let origin = origin.unwrap();
    for (sufficient, right) in [(true, true), (false, false), (true, false)] {
        course
            .store
            .record_outcome(&outcome(&course.program_id, &origin, sufficient, right, 1))
            .await?;
    }
    course
        .store
        .record_outcome(&outcome(
            &course.program_id,
            "web:elsewhere.example.net",
            true,
            true,
            1,
        ))
        .await?;
    for entry in [
        TrustOverride::Site {
            registrable_domain: "example.org".into(),
            standing: SiteStanding::Institutional,
            note: None,
            set_at: 1,
        },
        TrustOverride::SiteSubject {
            registrable_domain: "example.org".into(),
            class: SubjectClass::new("Example APIs", vec![]),
            level: TrustLevel::Primary,
            note: None,
            set_at: 2,
        },
        TrustOverride::Site {
            registrable_domain: "unseen.example".into(),
            standing: SiteStanding::Community,
            note: None,
            set_at: 3,
        },
    ] {
        course.store.set_override(&entry).await?;
    }
    let (listed, reads) = crate::tests::common::query_counts::count_reads(
        super::override_changes::list(&course.store),
    )
    .await;
    assert_eq!(reads, 2);
    let mut counts: Vec<_> = listed?
        .into_iter()
        .map(|site| (site.track_record.right, site.track_record.wrong))
        .collect();
    counts.sort();
    assert_eq!(counts, [(0, 0), (2, 1), (2, 1)]);
    Ok(())
}

#[tokio::test]
async fn a_course_takes_its_trust_rows_with_it_and_leaves_shared_judgements() -> Result<()> {
    let course = course().await?;
    let url = "https://docs.example.org/leaving";
    let saved = add_page(
        &course,
        url,
        &article("leaving", 8),
        page_signals(url, json!({})),
    )
    .await?;
    course
        .store
        .set_override(&TrustOverride::Source {
            program_id: course.program_id.clone(),
            source_id: saved.source_id,
            level: TrustLevel::Established,
            note: None,
            set_at: 1,
        })
        .await?;
    course
        .store
        .record_outcome(&outcome(
            &course.program_id,
            "web:example.org",
            true,
            true,
            1,
        ))
        .await?;
    let standing = StoredJudgement {
        key: key("shared"),
        value: standing_assessment("shared"),
        input_tokens: 1,
        output_tokens: 1,
        assessed_at: 1,
    };
    course.store.save_standing(&standing).await?;
    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(&course.program_id)
        .execute(&course.pool)
        .await
        .unwrap();
    for table in [
        "source_provenance",
        "source_relations",
        "source_trust_overrides",
        "source_trust_outcomes",
    ] {
        let rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&course.pool)
            .await
            .unwrap();
        assert_eq!(rows, 0, "{table}");
    }
    assert_eq!(course.store.standing(&standing.key).await?, Some(standing));
    Ok(())
}
