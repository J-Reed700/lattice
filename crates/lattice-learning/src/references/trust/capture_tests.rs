//! The capture hook: every path that saves a version writes its publisher,
//! its provenance row and its relations, in the version's transaction.

use super::capture::{
    compare, document_signals, not_extracted, pasted_signals, publisher, record, CapturedVersion,
    VersionText, DOCUMENT_HEADING_LINES, NO_PRIOR_PROVENANCE, PUBLISHER_MAX_CHARS,
    SEED_WITHOUT_CAPTURE,
};
use super::test_support::{
    add, add_page, article, course, course_on_file, id, page_signals, text_capture, Course,
};
use crate::dto::{
    LearningSourceDto, LearningSourceKind, LearningSourcePolicy, RefreshLearningSourceRequestDto,
};
use crate::portability_dto::{DeleteLearningSourceRequestDto, ReimportLearningSourceRequestDto};
use crate::repository::LearningRepository;
use crate::source_library::{CapturedLearningSource, NewLearningSource};
use crate::sources::InitialReference;
use crate::tests::fixture;
use lattice_core::application::ports::source_trust::SourceTrustStore;
use lattice_core::application::ports::{LibraryChunk, LibraryDocumentText};
use lattice_core::domain::learning::claim_review::SourceAddedBy;
use lattice_core::domain::provenance::{
    CanonicalElsewhere, ExtractedFrom, Origin, ProvenanceSignals, RelationEvidence, RelationKind,
    RelationTarget,
};
use lattice_core::shared::error::Result;
use serde_json::json;
use std::time::{Duration, Instant};

async fn publisher_column(pool: &sqlx::SqlitePool, version_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT publisher FROM learning_source_versions WHERE id=?")
        .bind(version_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Text that repeats none of `seed`'s neighbours' wording.
fn distinct_words(seed: usize, words: usize) -> String {
    (0..words)
        .map(|word| format!("s{seed}w{word}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Saves `count` sources straight into the tables, without the comparison
/// the test is about.
async fn seed_sources(course: &Course, count: usize, words: usize) {
    for seed in 0..count {
        let (source, version) = (id(), id());
        let text = distinct_words(seed, words);
        sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,created_at,updated_at,added_by) VALUES(?,?,'pasted','seed',NULL,'fixed',?,NULL,0,1,1,'user')")
            .bind(&source).bind(&course.program_id).bind(&version)
            .execute(&course.pool).await.unwrap();
        sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,1,'Seed',?,'',?,?,0,'test',1)")
            .bind(&version).bind(&course.program_id).bind(&source).bind(&text).bind(&version).bind(words as i64)
            .execute(&course.pool).await.unwrap();
    }
}

/// How long the slowest of a stream of writes waited, while `work` ran.
async fn longest_write_during(
    pool: &sqlx::SqlitePool,
    program_id: &str,
    work: impl std::future::Future<Output = Result<()>> + Send + 'static,
) -> Duration {
    let work = tokio::spawn(work);
    let mut longest = Duration::ZERO;
    while !work.is_finished() {
        let started = Instant::now();
        sqlx::query("UPDATE learning_programs SET status=status WHERE id=?")
            .bind(program_id)
            .execute(pool)
            .await
            .unwrap();
        longest = longest.max(started.elapsed());
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    work.await.unwrap().unwrap();
    longest
}

/// Runs `save` while writes go on, and fails if one had to wait out most of
/// it: comparing a source's text with the others takes seconds on a real
/// course, and a transaction that long keeps every other write out.
async fn assert_writes_go_on_during(
    course: &Course,
    save: impl std::future::Future<Output = Result<()>> + Send + 'static,
) {
    let started = Instant::now();
    let longest = longest_write_during(&course.pool, &course.program_id, save).await;
    let saving = started.elapsed();
    assert!(
        longest < saving / 4,
        "a write waited {longest:?} of the {saving:?} the save took"
    );
}

const SOURCES: usize = 40;
const WORDS: usize = 6_000;

/// A lesson job failed with "database is locked" because another job was
/// saving the pages it read: each save held the write lock while it compared
/// the page with every other source's full text, and a course's sources made
/// that seconds long, past the five seconds every other write waits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saving_a_source_does_not_hold_the_database_while_comparing_it_with_the_others(
) -> Result<()> {
    let (course, _file) = course_on_file().await?;
    seed_sources(&course, SOURCES, WORDS).await;
    let page = distinct_words(1_000, WORDS);
    let source = text_capture("A new page", &page, pasted_signals(None, &page));
    let library = course.library.clone();
    let program_id = course.program_id.clone();
    assert_writes_go_on_during(&course, async move {
        let (source_id, version_id) = (id(), id());
        library
            .add_by(
                NewLearningSource {
                    operation_id: &id(),
                    source_id: &source_id,
                    version_id: &version_id,
                    program_id: &program_id,
                    operation_kind: "add_test",
                    origin: "pasted",
                    requested_url: None,
                    freshness: LearningSourcePolicy::Fixed,
                    kind: LearningSourceKind::Pasted,
                },
                source,
                id(),
                SourceAddedBy::Research,
            )
            .await
    })
    .await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refreshing_a_source_does_not_hold_the_database_while_comparing_it_with_the_others(
) -> Result<()> {
    let (course, _file) = course_on_file().await?;
    seed_sources(&course, SOURCES, WORDS).await;
    let url = "https://docs.example.org/changing";
    let saved = add_page(
        &course,
        url,
        "The first version.",
        page_signals(url, json!({})),
    )
    .await?;
    let page = super::test_support::reference_page(
        url,
        &distinct_words(1_000, WORDS),
        page_signals(url, json!({})),
    );
    let title = page.output.title.clone().unwrap();
    let library = course.library.clone();
    let request = RefreshLearningSourceRequestDto {
        operation_id: id(),
        program_id: course.program_id.clone(),
        source_id: saved.source_id,
        expected_revision: 0,
    };
    assert_writes_go_on_during(&course, async move {
        let captured = super::capture::web_capture(page, url, title);
        library.refresh(&request, Some(captured), None).await
    })
    .await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reimporting_a_source_does_not_hold_the_database_while_comparing_it_with_the_others(
) -> Result<()> {
    let (course, _file) = course_on_file().await?;
    seed_sources(&course, SOURCES, WORDS).await;
    let text = "Text the user restored by hand.";
    let saved = add(
        &course,
        LearningSourceKind::Pasted,
        "Restored",
        text_capture("Restored", text, pasted_signals(None, text)),
    )
    .await?;
    course
        .library
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: course.program_id.clone(),
            source_id: saved.source_id.clone(),
            expected_revision: 0,
            reason: "Removed for the test".into(),
        })
        .await?;
    let library = course.library.clone();
    let request = ReimportLearningSourceRequestDto {
        operation_id: id(),
        program_id: course.program_id.clone(),
        source_id: saved.source_id,
        version_id: id(),
        expected_revision: 1,
        replacement_text: Some(distinct_words(1_000, WORDS)),
    };
    assert_writes_go_on_during(
        &course,
        async move { library.reimport_source(&request).await },
    )
    .await;
    Ok(())
}

/// A course's first sources are saved in one transaction, each compared with
/// the ones before it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creating_a_course_does_not_hold_the_database_while_comparing_its_sources() -> Result<()> {
    let (course, _file) = course_on_file().await?;
    let mut program = fixture();
    program.sources = (0..SOURCES / 2)
        .map(|seed| LearningSourceDto {
            id: id(),
            title: format!("Page {seed}"),
            url: Some(format!("https://example.org/{seed}")),
            excerpt: distinct_words(seed, WORDS),
            acquired_at: 1,
        })
        .collect();
    let references: Vec<_> = program
        .sources
        .iter()
        .map(|source| InitialReference {
            source_id: source.id.clone(),
            origin: source.url.clone().unwrap(),
            captured: CapturedLearningSource {
                title: source.title.clone(),
                publisher: None,
                requested_url: source.url.clone(),
                resolved_url: source.url.clone(),
                text: source.excerpt.clone(),
                truncated: false,
                extraction_version: "fixture_full_text".into(),
                signals: not_extracted("test fixture"),
            },
            found: false,
        })
        .collect();
    let repository = LearningRepository::new(course.database.clone());
    assert_writes_go_on_during(&course, async move {
        repository
            .create_with_references(&program, &references)
            .await
    })
    .await;
    Ok(())
}

/// A version never exists without its relations: a source saved after the
/// comparison ran is still compared, and one removed since is not related.
#[tokio::test]
async fn a_source_saved_after_the_comparison_is_related_and_one_removed_since_is_not() -> Result<()>
{
    let course = course().await?;
    let text = article("tidal", 40);
    let removed_url = "https://removed.example.org/tides";
    let removed = add_page(
        &course,
        removed_url,
        &text,
        page_signals(removed_url, json!({})),
    )
    .await?;
    let (source_id, version_id) = (id(), id());
    let comparison = compare(
        &course.database,
        &course.program_id,
        VersionText {
            source_id: &source_id,
            version_id: &version_id,
            text: &text,
        },
    )
    .await?;
    course
        .library
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: course.program_id.clone(),
            source_id: removed.source_id.clone(),
            expected_revision: 0,
            reason: "Removed for the test".into(),
        })
        .await?;
    let saved_url = "https://mirror.example.net/tides";
    let saved_since = add_page(
        &course,
        saved_url,
        &text,
        page_signals(saved_url, json!({})),
    )
    .await?;

    let mut tx = course.database.begin_write().await.unwrap();
    sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,created_at,updated_at,added_by) VALUES(?,?,'pasted','seed',NULL,'fixed',?,NULL,0,1,1,'user')")
        .bind(&source_id).bind(&course.program_id).bind(&version_id)
        .execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,1,'Seed',?,'',?,1,0,'test',1)")
        .bind(&version_id).bind(&course.program_id).bind(&source_id).bind(&text).bind(&version_id)
        .execute(&mut *tx).await.unwrap();
    record(
        &mut tx,
        &CapturedVersion {
            program_id: &course.program_id,
            source_id: &source_id,
            version_id: &version_id,
            kind: &LearningSourceKind::Pasted,
            title: "Seed",
            resolved_url: None,
            text: &text,
            acquired_at_ms: 1_000,
        },
        &pasted_signals(None, &text),
        &comparison,
    )
    .await?;
    tx.commit().await.unwrap();

    let relations = course.store.relations(&course.program_id).await?;
    let saved: Vec<_> = relations.iter().filter(|r| r.from == version_id).collect();
    assert_eq!(saved.len(), 1, "{relations:?}");
    assert_eq!(saved[0].kind, RelationKind::CopyOf);
    assert_eq!(saved[0].to, RelationTarget::Version(saved_since.version_id));
    Ok(())
}

#[tokio::test]
async fn sources_saved_together_are_compared_with_each_other() -> Result<()> {
    let course = course().await?;
    let mut program = fixture();
    program.sources = (0..2)
        .map(|seed| LearningSourceDto {
            id: id(),
            title: format!("Page {seed}"),
            url: None,
            excerpt: article("tidal", 40),
            acquired_at: 1,
        })
        .collect();
    LearningRepository::new(course.database.clone())
        .create(&program)
        .await?;
    let relations = course.store.relations(&program.summary.id).await?;
    assert_eq!(relations.len(), 1, "{relations:?}");
    assert_eq!(relations[0].kind, RelationKind::CopyOf);
    assert_eq!(relations[0].from, program.sources[1].id);
    assert_eq!(
        relations[0].to,
        RelationTarget::Version(program.sources[0].id.clone())
    );
    Ok(())
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
