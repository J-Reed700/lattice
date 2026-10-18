//! Courses and captures for the trust tests, saved through the same
//! repository paths the app uses.

use super::capture::{publisher, web_capture};
use super::repository::SqliteSourceTrustStore;
use crate::features::learning::dto::{
    LearningProgramStatus, LearningSourceKind, LearningSourcePolicy,
};
use crate::features::learning::repository::LearningRepository;
use crate::features::learning::source_library::{
    CapturedLearningSource, LearningSourceLibraryRepository, NewLearningSource,
};
use crate::features::learning::tests::{fixture, pool};
use lattice_core::application::contracts::web::{FetchUrlContentOutput, ReferencePage};
use lattice_core::domain::provenance::{Origin, ProvenanceSignals, PAGE_SIGNALS_VERSION};
use lattice_core::shared::error::Result;
use serde_json::{json, Value};
use sqlx::SqlitePool;

pub(super) fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// An active course on a migrated database, with its store and library.
pub(super) struct Course {
    pub pool: SqlitePool,
    pub program_id: String,
    pub library: LearningSourceLibraryRepository,
    pub store: SqliteSourceTrustStore,
}

pub(super) async fn course() -> Result<Course> {
    let pool = pool().await?;
    let program_id = course_on(&pool).await?;
    Ok(Course {
        library: LearningSourceLibraryRepository::new(pool.clone()),
        store: SqliteSourceTrustStore::new(pool.clone()),
        program_id,
        pool,
    })
}

/// Another active course on the same database: its seeded fixture source
/// is saved through `insert_source`, the initial-reference capture path.
pub(super) async fn course_on(pool: &SqlitePool) -> Result<String> {
    let mut program = fixture();
    program.summary.status = LearningProgramStatus::Active;
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    Ok(program.summary.id)
}

/// Page signals as the web reader emits them, built from JSON so a field
/// added later with a default does not break the tests.
pub(super) fn page_signals(url: &str, extra: Value) -> ProvenanceSignals {
    let mut page = json!({
        "source": "web",
        "origin": Origin::of_url(url).expect("test URL"),
        "resolved_url": url,
        "dates": { "acquired_at": 1_760_000_000 },
        "extraction_version": PAGE_SIGNALS_VERSION,
    });
    if let (Some(page), Some(extra)) = (page.as_object_mut(), extra.as_object()) {
        page.extend(extra.clone());
    }
    serde_json::from_value(page).expect("page signals")
}

/// A fetched reference page, as `fetch_reference_page` returns it.
pub(super) fn reference_page(url: &str, text: &str, signals: ProvenanceSignals) -> ReferencePage {
    ReferencePage {
        output: FetchUrlContentOutput {
            url: url.into(),
            title: Some(format!("Page at {url}")),
            content: text.into(),
            content_truncated: false,
            word_count: text.split_whitespace().count(),
            fetch_time_ms: 0.0,
            content_type: Some("text/html".into()),
            from_cache: false,
        },
        signals,
        fetched_at: 1_760_000_000,
    }
}

/// The ids a saved source was given.
pub(super) struct Saved {
    pub source_id: String,
    pub version_id: String,
}

/// Saves a web page as a user-added source, through the capture a link
/// takes (`web_capture`, then the library's `add`).
pub(super) async fn add_page(
    course: &Course,
    url: &str,
    text: &str,
    signals: ProvenanceSignals,
) -> Result<Saved> {
    let title = format!("Page at {url}");
    let captured = web_capture(reference_page(url, text, signals), url, title);
    add(course, LearningSourceKind::Web, url, captured).await
}

/// Saves any capture as a new source of `kind`.
pub(super) async fn add(
    course: &Course,
    kind: LearningSourceKind,
    origin: &str,
    captured: CapturedLearningSource,
) -> Result<Saved> {
    let saved = Saved {
        source_id: id(),
        version_id: id(),
    };
    let requested_url = captured.requested_url.clone();
    course
        .library
        .add(
            NewLearningSource {
                operation_id: &id(),
                source_id: &saved.source_id,
                version_id: &saved.version_id,
                program_id: &course.program_id,
                operation_kind: "add_test",
                origin,
                requested_url: requested_url.as_deref(),
                // Web sources can be refreshed; documents and pasted text
                // are fixed.
                freshness: if kind == LearningSourceKind::Web {
                    LearningSourcePolicy::Manual
                } else {
                    LearningSourcePolicy::Fixed
                },
                kind,
            },
            captured,
            id(),
        )
        .await?;
    Ok(saved)
}

/// A capture of plain text with the given signals and no URL.
pub(super) fn text_capture(
    title: &str,
    text: &str,
    signals: ProvenanceSignals,
) -> CapturedLearningSource {
    CapturedLearningSource {
        title: title.into(),
        publisher: publisher(&signals),
        requested_url: None,
        resolved_url: None,
        text: text.into(),
        truncated: false,
        extraction_version: "trust_test_v1".into(),
        signals,
    }
}

/// Text of `words` words, every one carrying `seed`, so texts with different
/// seeds share no shingle and are never related by copy detection.
pub(super) fn article(seed: &str, words: usize) -> String {
    let seed: String = seed.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    (0..words)
        .map(|n| format!("{seed}{n}"))
        .collect::<Vec<_>>()
        .join(" ")
}
