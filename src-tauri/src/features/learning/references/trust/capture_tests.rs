//! The capture hook: every path that saves a version writes its publisher,
//! its provenance row and its relations, in the version's transaction.

use super::capture::{
    document_signals, pasted_signals, publisher, DOCUMENT_HEADING_LINES, NO_PRIOR_PROVENANCE,
    PUBLISHER_MAX_CHARS, SEED_WITHOUT_CAPTURE,
};
use super::test_support::{add, add_page, article, course, id, page_signals, text_capture};
use crate::features::learning::dto::{LearningSourceKind, RefreshLearningSourceRequestDto};
use crate::features::learning::portability_dto::{
    DeleteLearningSourceRequestDto, ReimportLearningSourceRequestDto,
};
use lattice_core::application::ports::source_trust::SourceTrustStore;
use lattice_core::application::ports::{LibraryChunk, LibraryDocumentText};
use lattice_core::domain::provenance::{
    CanonicalElsewhere, ExtractedFrom, Origin, ProvenanceSignals, RelationEvidence, RelationKind,
    RelationTarget,
};
use lattice_core::shared::error::Result;
use serde_json::json;

async fn publisher_column(pool: &sqlx::SqlitePool, version_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT publisher FROM learning_source_versions WHERE id=?")
        .bind(version_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[test]
fn the_publisher_is_the_declared_publisher_else_the_site_name_and_never_an_oversized_name() {
    let url = "https://docs.python.org/3/library/asyncio.html";
    let declared = page_signals(
        url,
        json!({ "publisher": { "name": " Python Software Foundation " }, "site_name": "Python docs" }),
    );
    assert_eq!(
        publisher(&declared).as_deref(),
        Some("Python Software Foundation")
    );
    let site_only = page_signals(url, json!({ "site_name": "Python docs" }));
    assert_eq!(publisher(&site_only).as_deref(), Some("Python docs"));
    let oversized = "x".repeat(PUBLISHER_MAX_CHARS + 1);
    let long = page_signals(
        url,
        json!({ "publisher": { "name": oversized }, "site_name": "Python docs" }),
    );
    assert_eq!(publisher(&long).as_deref(), Some("Python docs"));
    assert_eq!(publisher(&page_signals(url, json!({}))), None);
    assert_eq!(publisher(&pasted_signals(Some("Acme"), "text")), None);
}

#[tokio::test]
async fn a_web_capture_writes_publisher_provenance_and_copy_relations() -> Result<()> {
    let course = course().await?;
    let text = article("tidal", 40);
    let original_url = "https://docs.example.org/tides";
    let original = add_page(
        &course,
        original_url,
        &text,
        page_signals(
            original_url,
            json!({ "publisher": { "name": "Example Tide Office" } }),
        ),
    )
    .await?;
    let mirror_url = "https://mirror.example.net/tides";
    let mirror = add_page(
        &course,
        mirror_url,
        &text,
        page_signals(mirror_url, json!({})),
    )
    .await?;
    let other_url = "https://other.example.com/storms";
    add_page(
        &course,
        other_url,
        &article("storm", 40),
        page_signals(other_url, json!({})),
    )
    .await?;

    assert_eq!(
        publisher_column(&course.pool, &original.version_id)
            .await
            .as_deref(),
        Some("Example Tide Office")
    );
    let record = course
        .store
        .provenance(&original.version_id)
        .await?
        .expect("provenance row");
    assert_eq!(record.program_id, course.program_id);
    assert_eq!(record.origin, Origin::of_url(original_url).unwrap());
    assert_eq!(record.extracted_from, ExtractedFrom::Fetch);
    assert_eq!(record.signals_sha256, record.signals.signals_sha256());
    assert!(matches!(record.signals, ProvenanceSignals::Web(_)));

    // The verbatim copy points at the version it copies; the unrelated page
    // relates to nothing.
    let relations = course.store.relations(&course.program_id).await?;
    assert_eq!(relations.len(), 1, "{relations:?}");
    assert_eq!(relations[0].from, mirror.version_id);
    assert_eq!(
        relations[0].to,
        RelationTarget::Version(original.version_id)
    );
    assert_eq!(relations[0].kind, RelationKind::CopyOf);
    Ok(())
}

#[tokio::test]
async fn a_hook_failure_saves_no_part_of_the_version() -> Result<()> {
    let course = course().await?;
    sqlx::query(
        "CREATE TRIGGER refuse_provenance BEFORE INSERT ON source_provenance BEGIN SELECT RAISE(ABORT, 'refused'); END",
    )
    .execute(&course.pool)
    .await
    .unwrap();
    let url = "https://docs.example.org/atomic";
    let failed = add_page(
        &course,
        url,
        &article("atomic", 5),
        page_signals(url, json!({})),
    )
    .await;
    assert!(failed.is_err());
    let versions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_source_versions WHERE program_id=? AND resolved_url=?",
    )
    .bind(&course.program_id)
    .bind(url)
    .fetch_one(&course.pool)
    .await
    .unwrap();
    assert_eq!(versions, 0);
    Ok(())
}

#[tokio::test]
async fn an_archived_copy_and_a_canonical_elsewhere_become_relations() -> Result<()> {
    let course = course().await?;
    let original_url = "https://www.example.org/reports/water";
    let original = add_page(
        &course,
        original_url,
        &article("water", 20),
        page_signals(original_url, json!({})),
    )
    .await?;
    let archive_url =
        "https://web.archive.org/web/20240501123456/https://www.example.org/reports/water";
    let archived = add_page(
        &course,
        archive_url,
        &article("archived water", 25),
        page_signals(archive_url, json!({})),
    )
    .await?;
    let syndicated_url = "https://medium.example.com/@author/post";
    let canonical = "https://author.example.net/post";
    let syndicated = add_page(
        &course,
        syndicated_url,
        &article("syndicated", 20),
        page_signals(
            syndicated_url,
            json!({ "canonical_elsewhere": CanonicalElsewhere::of(canonical).unwrap() }),
        ),
    )
    .await?;

    let relations = course.store.relations(&course.program_id).await?;
    let archive = relations
        .iter()
        .find(|r| r.kind == RelationKind::ArchiveOf)
        .expect("archive relation");
    assert_eq!(archive.from, archived.version_id);
    assert_eq!(
        archive.to,
        RelationTarget::Version(original.version_id.clone())
    );
    assert!(matches!(
        &archive.evidence,
        RelationEvidence::Archive { archive_host, .. } if archive_host == "web.archive.org"
    ));
    let canonical_of = relations
        .iter()
        .find(|r| r.kind == RelationKind::CanonicalOf)
        .expect("canonical relation");
    assert_eq!(canonical_of.from, syndicated.version_id);
    assert_eq!(canonical_of.to, RelationTarget::Url(canonical.into()));
    // The archived copy's origin is the original's.
    let record = course
        .store
        .provenance(&archived.version_id)
        .await?
        .unwrap();
    assert_eq!(record.origin.key, Origin::of_url(original_url).unwrap().key);
    Ok(())
}

#[tokio::test]
async fn documents_and_pasted_text_get_their_own_signals_and_origins() -> Result<()> {
    let course = course().await?;
    let mut text = (0..DOCUMENT_HEADING_LINES + 3)
        .map(|n| format!("# Heading {n}\nBody paragraph {n} that is never a signal."))
        .collect::<Vec<_>>()
        .join("\n");
    text.push_str("\nClosing body text.");
    let document = LibraryDocumentText {
        title: "water-act.pdf".into(),
        file_path: "/library/acts/water-act.pdf".into(),
        mime_type: "application/pdf".into(),
        chunks: vec![LibraryChunk {
            id: "c1".into(),
            text: text.clone(),
        }],
    };
    let signals = document_signals(&document, &text);
    let doc = add(
        &course,
        LearningSourceKind::Document,
        "document-1",
        text_capture("water-act.pdf", &text, signals),
    )
    .await?;
    let record = course.store.provenance(&doc.version_id).await?.unwrap();
    assert_eq!(record.extracted_from, ExtractedFrom::Library);
    assert_eq!(record.origin, Origin::of_file("water-act.pdf"));
    let ProvenanceSignals::Document(signals) = &record.signals else {
        panic!("document signals");
    };
    assert_eq!(signals.mime_type, "application/pdf");
    assert_eq!(signals.heading_lines.len(), DOCUMENT_HEADING_LINES);
    assert!(signals
        .heading_lines
        .iter()
        .all(|line| line.starts_with("Heading")));
    assert_eq!(publisher_column(&course.pool, &doc.version_id).await, None);

    let pasted_text = "A note the user pasted from a newsletter.";
    let mut captured = text_capture(
        "Newsletter",
        pasted_text,
        pasted_signals(Some("Acme Weekly"), pasted_text),
    );
    captured.publisher = Some("Acme Weekly".into());
    let pasted = add(&course, LearningSourceKind::Pasted, "Newsletter", captured).await?;
    let record = course.store.provenance(&pasted.version_id).await?.unwrap();
    assert_eq!(record.origin, Origin::pasted(&pasted.version_id));
    assert_eq!(record.extracted_from, ExtractedFrom::Text);
    assert_eq!(
        record
            .signals
            .declared_origin()
            .and_then(|d| d.description.as_deref()),
        Some("Acme Weekly")
    );
    assert_eq!(
        publisher_column(&course.pool, &pasted.version_id)
            .await
            .as_deref(),
        Some("Acme Weekly")
    );
    Ok(())
}

#[tokio::test]
async fn a_seeded_course_source_records_why_nothing_was_extracted() -> Result<()> {
    let course = course().await?;
    let seeded = course.store.program_provenance(&course.program_id).await?;
    assert_eq!(seeded.len(), 1, "the fixture course seeds one source");
    assert_eq!(
        seeded[0].signals,
        ProvenanceSignals::NotExtracted {
            reason: SEED_WITHOUT_CAPTURE.into()
        }
    );
    assert_eq!(seeded[0].extracted_from, ExtractedFrom::Text);
    Ok(())
}

#[tokio::test]
async fn a_refresh_and_a_reimport_record_provenance_for_their_new_versions() -> Result<()> {
    let course = course().await?;
    let url = "https://docs.example.org/changing";
    let first = add_page(
        &course,
        url,
        &article("first", 10),
        page_signals(url, json!({})),
    )
    .await?;
    let refreshed_signals = page_signals(url, json!({ "site_name": "Example Docs" }));
    let refreshed_page =
        super::test_support::reference_page(url, &article("second", 10), refreshed_signals);
    let title = refreshed_page.output.title.clone().unwrap();
    course
        .library
        .refresh(
            &RefreshLearningSourceRequestDto {
                operation_id: id(),
                program_id: course.program_id.clone(),
                source_id: first.source_id.clone(),
                expected_revision: 0,
            },
            Some(super::capture::web_capture(refreshed_page, url, title)),
            None,
        )
        .await?;
    let pending: String = sqlx::query_scalar(
        "SELECT pending_version_id FROM learning_source_library WHERE program_id=? AND id=?",
    )
    .bind(&course.program_id)
    .bind(&first.source_id)
    .fetch_one(&course.pool)
    .await
    .unwrap();
    assert_eq!(
        publisher_column(&course.pool, &pending).await.as_deref(),
        Some("Example Docs")
    );
    assert!(course.store.provenance(&pending).await?.is_some());

    // A removed source restored with new text keeps its provenance.
    let pasted_text = "Text the user restored by hand.";
    let restored = add(
        &course,
        LearningSourceKind::Pasted,
        "Restored",
        text_capture(
            "Restored",
            pasted_text,
            pasted_signals(Some("Field notes"), pasted_text),
        ),
    )
    .await?;
    course
        .library
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: course.program_id.clone(),
            source_id: restored.source_id.clone(),
            expected_revision: 0,
            reason: "Removed for the test".into(),
        })
        .await?;
    let restored_version = id();
    course
        .library
        .reimport_source(&ReimportLearningSourceRequestDto {
            operation_id: id(),
            program_id: course.program_id.clone(),
            source_id: restored.source_id.clone(),
            version_id: restored_version.clone(),
            expected_revision: 1,
            replacement_text: Some("Corrected text the user restored by hand.".into()),
        })
        .await?;
    let record = course.store.provenance(&restored_version).await?.unwrap();
    let before = course
        .store
        .provenance(&restored.version_id)
        .await?
        .unwrap();
    assert_eq!(record.signals, before.signals);
    assert_ne!(
        record.sketch, before.sketch,
        "the sketch is of the restored text"
    );
    assert_ne!(
        record.signals,
        ProvenanceSignals::NotExtracted {
            reason: NO_PRIOR_PROVENANCE.into()
        }
    );
    Ok(())
}
