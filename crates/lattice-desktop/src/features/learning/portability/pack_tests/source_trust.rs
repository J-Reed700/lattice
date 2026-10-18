//! A course pack carries its sources' provenance, the relations between
//! them and its source overrides; a copy gets them under its own ids.

use super::*;
use crate::features::learning::dto::{
    LearningProgramStatus, LearningSourceKind, LearningSourcePolicy,
};
use crate::features::learning::references::trust::{capture::web_capture, SqliteSourceTrustStore};
use crate::features::learning::source_library::NewLearningSource;
use lattice_core::application::contracts::web::{FetchUrlContentOutput, ReferencePage};
use lattice_core::application::ports::source_trust::SourceTrustStore;
use lattice_core::domain::provenance::{
    Origin, RelationKind, RelationTarget, SiteStanding, TrustLevel, TrustOverride,
    PAGE_SIGNALS_VERSION,
};
use serde_json::json;

fn page(url: &str, text: &str) -> ReferencePage {
    let signals = serde_json::from_value(json!({
        "source": "web",
        "origin": Origin::of_url(url).unwrap(),
        "resolved_url": url,
        "publisher": { "name": "Example Tide Office" },
        "dates": { "acquired_at": 1_760_000_000 },
        "extraction_version": PAGE_SIGNALS_VERSION,
    }))
    .unwrap();
    ReferencePage {
        output: FetchUrlContentOutput {
            url: url.into(),
            title: Some("Tide tables".into()),
            content: text.into(),
            content_truncated: false,
            word_count: text.split_whitespace().count(),
            fetch_time_ms: 0.0,
            content_type: None,
            from_cache: false,
            read_via: Default::default(),
        },
        signals,
        fetched_at: 1_760_000_000,
    }
}

#[tokio::test]
async fn a_copy_carries_provenance_relations_and_source_overrides_under_new_ids() -> Result<()> {
    let pool = pool().await?;
    let mut program = fixture();
    program.summary.status = LearningProgramStatus::Active;
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let program_id = program.summary.id.clone();
    let library = LearningSourceLibraryRepository::new(pool.clone());
    let text = (0..30)
        .map(|n| format!("High water on day {n} falls at the hour the tide office tabulates."))
        .collect::<Vec<_>>()
        .join(" ");
    let mut sources = Vec::new();
    for url in [
        "https://www.example.org/tides",
        "https://mirror.example.net/tides",
    ] {
        let (source_id, version_id) = (id(), id());
        library
            .add(
                NewLearningSource {
                    operation_id: &id(),
                    source_id: &source_id,
                    version_id: &version_id,
                    program_id: &program_id,
                    kind: LearningSourceKind::Web,
                    operation_kind: "add_web",
                    origin: url,
                    requested_url: Some(url),
                    freshness: LearningSourcePolicy::Fixed,
                },
                web_capture(page(url, &text), url, "Tide tables".into()),
                id(),
            )
            .await?;
        sources.push((source_id, version_id));
    }
    let store = SqliteSourceTrustStore::new(pool.clone());
    store
        .set_override(&TrustOverride::Source {
            program_id: program_id.clone(),
            source_id: sources[0].0.clone(),
            level: TrustLevel::Primary,
            note: Some("The tide office".into()),
            set_at: 1,
        })
        .await?;
    store
        .set_override(&TrustOverride::Site {
            registrable_domain: "example.net".into(),
            standing: SiteStanding::Community,
            note: None,
            set_at: 2,
        })
        .await?;
    let original = store.program_provenance(&program_id).await?;
    assert_eq!(original.len(), 3, "the seeded source and two pages");

    let workspace = tempfile::tempdir().expect("pack workspace");
    let packs = LearningPackRepository::new(
        pool.clone(),
        workspace.path().join("learning-packs"),
        workspace.path().to_path_buf(),
    );
    let (copy_id, _) = export_copy(&packs, &program_id).await?;

    let copied = store.program_provenance(&copy_id).await?;
    assert_eq!(copied.len(), original.len());
    let original_ids: Vec<&str> = original
        .iter()
        .map(|record| record.source_version_id.as_str())
        .collect();
    for record in &copied {
        assert_eq!(record.program_id, copy_id);
        assert!(!original_ids.contains(&record.source_version_id.as_str()));
    }
    let mut signals: Vec<_> = copied
        .iter()
        .map(|record| record.signals_sha256.clone())
        .collect();
    let mut expected: Vec<_> = original
        .iter()
        .map(|record| record.signals_sha256.clone())
        .collect();
    signals.sort();
    expected.sort();
    assert_eq!(signals, expected, "the signals travel unchanged");

    let relations = store.relations(&copy_id).await?;
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].kind, RelationKind::CopyOf);
    let copied_ids: Vec<&str> = copied
        .iter()
        .map(|record| record.source_version_id.as_str())
        .collect();
    assert!(copied_ids.contains(&relations[0].from.as_str()));
    assert!(
        matches!(&relations[0].to, RelationTarget::Version(id) if copied_ids.contains(&id.as_str()))
    );

    let overrides = store.overrides(&copy_id).await?.entries;
    let source_overrides: Vec<_> = overrides
        .iter()
        .filter(|entry| matches!(entry, TrustOverride::Source { .. }))
        .collect();
    assert_eq!(source_overrides.len(), 1);
    assert!(matches!(
        source_overrides[0],
        TrustOverride::Source { program_id, source_id, level: TrustLevel::Primary, .. }
            if *program_id == copy_id && *source_id != sources[0].0
    ));
    assert_eq!(
        store.site_overrides().await?.len(),
        1,
        "site overrides are not packed"
    );
    Ok(())
}
