//! The capture hook (§4, §18.1-2). Every path that saves a source version
//! calls [`record`] inside the version's own transaction: it writes what
//! capture read about the version (its provenance row) and how the version
//! relates to the course's other sources, so a version never exists without
//! them. The signal builders here give each kind of source its signals, and
//! [`publisher`] is the name the library's `publisher` column shows.

use super::repository::unix_seconds;
use super::rows::{from_json, write_provenance, write_relations};
use crate::features::learning::dto::LearningSourceKind;
use crate::features::learning::persistence::db;
use crate::features::learning::source_library::CapturedLearningSource;
use lattice_core::application::contracts::web::{ReferencePage, REFERENCE_TEXT_EXTRACTION_VERSION};
use lattice_core::application::ports::LibraryDocumentText;
use lattice_core::domain::provenance::{
    markdown_links, page_relations, shingles, DeclaredOrigin, DocumentSignals, ExtractedFrom,
    Origin, PastedSignals, ProvenanceRecord, ProvenanceSignals, RelationEvidence, RelationKind,
    RelationTarget, ShingleSet, Sketch, SourceDates, SourceRelation, POLICY,
};
use lattice_core::shared::error::Result;
use sqlx::{Row, Sqlite, SqliteConnection, Transaction};

/// The extraction version of a library document's signals.
pub const DOCUMENT_SIGNALS_VERSION: &str = "document-signals-v1";
/// The extraction version of pasted text's signals.
pub const PASTED_SIGNALS_VERSION: &str = "pasted-signals-v1";
/// The extraction version of a row whose signals were not extracted: only
/// the saved text was read (links and shingles).
pub const TEXT_ONLY_EXTRACTION: &str = "text-only-v1";

/// The library's limit on the `publisher` column (the source library's
/// capture validation). A longer declared name is not shown as the
/// publisher; the signals keep it whole for the standing judgement, which
/// flags oversized metadata.
pub const PUBLISHER_MAX_CHARS: usize = 180;

/// How many heading lines of a document its signals keep. A document names
/// itself and its issuer in its first headings (title page, issuing body,
/// document number); further headings are its outline, which is content,
/// and the standing judgement must not read content (§14).
pub const DOCUMENT_HEADING_LINES: usize = 8;

/// The library clock is unix milliseconds; the trust tables keep seconds.
const MILLIS_PER_SECOND: i64 = 1_000;

/// Why a version has no extracted signals, by the path that saved it.
pub const SEED_WITHOUT_CAPTURE: &str =
    "the course plan named this source without capturing a page or document";
pub const NO_PRIOR_PROVENANCE: &str = "the version this restores had no provenance recorded";

/// The name the library shows as a version's publisher: the publisher the
/// page's metadata declares, else its site name, else none. Only web pages
/// declare one; a document's or pasted text's publisher is what the user
/// said, which the capture path passes through itself.
pub fn publisher(signals: &ProvenanceSignals) -> Option<String> {
    let ProvenanceSignals::Web(page) = signals else {
        return None;
    };
    [
        page.publisher.as_ref().map(|entity| entity.name.as_str()),
        page.site_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|name| !name.is_empty() && name.chars().count() <= PUBLISHER_MAX_CHARS)
    .map(str::to_owned)
}

/// A fetched reference page as a capture: its text, the publisher its
/// signals name, and the signals themselves.
pub fn web_capture(
    page: ReferencePage,
    requested_url: &str,
    title: String,
) -> CapturedLearningSource {
    CapturedLearningSource {
        title,
        publisher: publisher(&page.signals),
        requested_url: Some(requested_url.to_owned()),
        resolved_url: Some(page.output.url),
        text: page.output.content,
        truncated: page.output.content_truncated,
        extraction_version: REFERENCE_TEXT_EXTRACTION_VERSION.into(),
        signals: page.signals,
    }
}

/// A library document's signals: its file name, type and title, and its
/// first heading lines (headings only, never body text).
pub fn document_signals(document: &LibraryDocumentText, text: &str) -> ProvenanceSignals {
    let file_name = std::path::Path::new(&document.file_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&document.title)
        .to_owned();
    let heading_lines = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .map(|line| line.trim_start_matches('#').trim())
        .filter(|line| !line.is_empty())
        .take(DOCUMENT_HEADING_LINES)
        .map(str::to_owned)
        .collect();
    ProvenanceSignals::Document(DocumentSignals {
        file_name,
        mime_type: document.mime_type.clone(),
        page_count: None,
        title: Some(document.title.clone()),
        heading_lines,
        declared_origin: None,
        dates: dated_now(),
    })
}

/// Pasted text's signals: its length and what the user said about where it
/// came from (the publisher they typed, if any).
pub fn pasted_signals(declared_publisher: Option<&str>, text: &str) -> ProvenanceSignals {
    let declared_origin = declared_publisher
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| DeclaredOrigin {
            url: None,
            description: Some(name.to_owned()),
        });
    ProvenanceSignals::Pasted(PastedSignals {
        declared_origin,
        chars: text.chars().count(),
        dates: dated_now(),
    })
}

/// Signals for a version whose provenance nobody read, with the reason.
pub fn not_extracted(reason: &str) -> ProvenanceSignals {
    ProvenanceSignals::NotExtracted {
        reason: reason.to_owned(),
    }
}

fn dated_now() -> SourceDates {
    SourceDates {
        acquired_at: unix_seconds(),
        ..SourceDates::default()
    }
}

/// The version a capture path has just inserted, as the hook needs it.
#[derive(Debug, Clone, Copy)]
pub struct CapturedVersion<'a> {
    pub program_id: &'a str,
    pub source_id: &'a str,
    pub version_id: &'a str,
    pub kind: &'a LearningSourceKind,
    pub title: &'a str,
    pub resolved_url: Option<&'a str>,
    /// The text as saved (`full_text`): links and shingles index it.
    pub text: &'a str,
    /// The version's `acquired_at`, unix milliseconds (the library clock).
    pub acquired_at_ms: i64,
}

/// What the hook wrote.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub record: ProvenanceRecord,
    pub relations: Vec<SourceRelation>,
}

/// Writes a newly inserted version's provenance row and its relations to
/// the course's other sources, in the version's transaction.
pub async fn record(
    tx: &mut Transaction<'_, Sqlite>,
    version: &CapturedVersion<'_>,
    signals: &ProvenanceSignals,
) -> Result<Recorded> {
    let text_shingles = shingles(version.text, &POLICY);
    let record = ProvenanceRecord {
        source_version_id: version.version_id.to_owned(),
        program_id: version.program_id.to_owned(),
        origin: origin_of(version, signals),
        signals: signals.clone(),
        signals_sha256: signals.signals_sha256(),
        sketch: Sketch::of(&text_shingles),
        links: markdown_links(version.text),
        extracted_from: extracted_from(signals),
        extraction_version: extraction_version(signals),
        extracted_at: version.acquired_at_ms.div_euclid(MILLIS_PER_SECOND),
    };
    write_provenance(&mut **tx, &record).await?;
    let relations = relate(tx, version, &record, &text_shingles).await?;
    write_relations(
        &mut **tx,
        version.program_id,
        &relations,
        record.extracted_at,
    )
    .await?;
    Ok(Recorded { record, relations })
}

/// The signals a version's earlier version recorded, for a version that
/// restores it with the same provenance (a re-import).
pub async fn carried_forward(
    conn: &mut SqliteConnection,
    program_id: &str,
    version_id: &str,
) -> Result<ProvenanceSignals> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT signals_json FROM source_provenance WHERE program_id=? AND source_version_id=?",
    )
    .bind(program_id)
    .bind(version_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(db)?;
    match stored {
        Some(json) => from_json(&json, "signals"),
        None => Ok(not_extracted(NO_PRIOR_PROVENANCE)),
    }
}

/// The publishing origin: the page's own for a web page; a file's name or
/// the version itself otherwise. Without extracted signals the saved URL
/// still names the origin; with no URL either, nothing identifies a
/// publisher, so a document is its file and anything else its own version.
fn origin_of(version: &CapturedVersion<'_>, signals: &ProvenanceSignals) -> Origin {
    match signals {
        ProvenanceSignals::Web(page) => page.origin.clone(),
        ProvenanceSignals::Document(document) => Origin::of_file(&document.file_name),
        ProvenanceSignals::Pasted(_) => Origin::pasted(version.version_id),
        ProvenanceSignals::NotExtracted { .. } => version
            .resolved_url
            .and_then(|url| Origin::of_url(url).ok())
            .unwrap_or_else(|| match version.kind {
                LearningSourceKind::Document => Origin::of_file(version.title),
                LearningSourceKind::Web | LearningSourceKind::Pasted => {
                    Origin::pasted(version.version_id)
                }
            }),
    }
}

fn extracted_from(signals: &ProvenanceSignals) -> ExtractedFrom {
    match signals {
        ProvenanceSignals::Web(page) if page.http.via_browser_reader => {
            ExtractedFrom::BrowserReader
        }
        ProvenanceSignals::Web(_) => ExtractedFrom::Fetch,
        ProvenanceSignals::Document(_) => ExtractedFrom::Library,
        ProvenanceSignals::Pasted(_) | ProvenanceSignals::NotExtracted { .. } => {
            ExtractedFrom::Text
        }
    }
}

fn extraction_version(signals: &ProvenanceSignals) -> String {
    match signals {
        ProvenanceSignals::Web(page) => page.extraction_version.clone(),
        ProvenanceSignals::Document(_) => DOCUMENT_SIGNALS_VERSION.into(),
        ProvenanceSignals::Pasted(_) => PASTED_SIGNALS_VERSION.into(),
        ProvenanceSignals::NotExtracted { .. } => TEXT_ONLY_EXTRACTION.into(),
    }
}

/// Page-level relations (§7): copies against the course's other active
/// sources, the archive a wrapper URL unwrapped to, and a canonical on
/// another site. A target the course has captured is linked by version.
async fn relate(
    tx: &mut Transaction<'_, Sqlite>,
    version: &CapturedVersion<'_>,
    record: &ProvenanceRecord,
    text_shingles: &ShingleSet,
) -> Result<Vec<SourceRelation>> {
    let others = sqlx::query(
        "SELECT v.id, v.full_text FROM learning_source_library l JOIN learning_source_versions v ON v.program_id=l.program_id AND v.id=l.active_version_id WHERE l.program_id=? AND l.deleted_at IS NULL AND l.id<>? ORDER BY v.id",
    )
    .bind(version.program_id)
    .bind(version.source_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(db)?;
    let sets: Vec<(String, ShingleSet)> = others
        .iter()
        .map(|row| {
            let id: String = row.get("id");
            let text: String = row.get("full_text");
            (id, shingles(&text, &POLICY))
        })
        .collect();
    let existing: Vec<(&str, &ShingleSet)> =
        sets.iter().map(|(id, set)| (id.as_str(), set)).collect();
    let mut relations = page_relations(version.version_id, text_shingles, &existing, &POLICY);
    if let Some(archived) = &record.origin.archived {
        relations.push(SourceRelation {
            from: version.version_id.to_owned(),
            to: target(tx, version, &archived.original_url).await?,
            kind: RelationKind::ArchiveOf,
            evidence: RelationEvidence::Archive {
                archive_host: archived.archive_host.clone(),
                archived_at: archived.archived_at,
            },
        });
    }
    if let ProvenanceSignals::Web(page) = &record.signals {
        if let Some(canonical) = &page.canonical_elsewhere {
            relations.push(SourceRelation {
                from: version.version_id.to_owned(),
                to: target(tx, version, &canonical.url).await?,
                kind: RelationKind::CanonicalOf,
                evidence: RelationEvidence::Canonical {
                    canonical_url: canonical.url.clone(),
                },
            });
        }
    }
    Ok(relations)
}

/// The course's captured version of `url` from another source, else the URL.
async fn target(
    tx: &mut Transaction<'_, Sqlite>,
    version: &CapturedVersion<'_>,
    url: &str,
) -> Result<RelationTarget> {
    let captured: Option<String> = sqlx::query_scalar(
        "SELECT v.id FROM learning_source_library l JOIN learning_source_versions v ON v.program_id=l.program_id AND v.id=l.active_version_id WHERE l.program_id=? AND l.deleted_at IS NULL AND l.id<>? AND (v.resolved_url=? OR v.requested_url=?) ORDER BY v.acquired_at, v.id LIMIT 1",
    )
    .bind(version.program_id)
    .bind(version.source_id)
    .bind(url)
    .bind(url)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db)?;
    Ok(captured.map_or_else(
        || RelationTarget::Url(url.to_owned()),
        RelationTarget::Version,
    ))
}
