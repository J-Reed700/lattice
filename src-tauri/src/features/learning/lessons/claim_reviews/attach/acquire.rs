//! Turning what the user attached into a saved source of the course (§5.4):
//! a link read through the web service (with its browser-reader fallback)
//! and frozen, a library document captured, pasted text kept as written, or
//! a passage of a source already saved. Evidence the course already holds
//! is reused, never saved twice.

use lattice_core::application::ports::ReferenceSnapshot;
use lattice_core::domain::learning::claim_review::{Provenance, SourceAddedBy};
use lattice_core::shared::error::{AppError, Result};

use super::failures::AttachFailureDto;
use super::report::SnapshotNoteDto;
use super::{AttachDeps, Evidence};
use crate::features::learning::dto::{
    AddLearningDocumentSourceRequestDto, AddLearningTextSourceRequestDto,
    AddLearningWebSourceRequestDto, LearningSourceKind, LearningSourcePolicy,
};
use crate::features::learning::source_identity::source_request_hash;
use crate::features::learning::source_library::{
    CapturedLearningSource, LearningSourceLibraryRepository, NewLearningSource, SavedSource,
    PASTED_TEXT_EXTRACTION,
};
use crate::features::learning::source_workflows::{captured_page, web_source_url};
use crate::features::learning::sources::capture_document;

/// Where the claim's evidence now lives.
pub(super) struct AcquiredSource {
    pub saved: SavedSource,
    pub provenance: Provenance,
    pub already_saved: bool,
    /// The span the user picked, for a saved passage; otherwise the claim's
    /// passages are retrieved from the whole version.
    pub span: Option<(usize, usize)>,
    pub snapshot: SnapshotNoteDto,
}

pub(super) enum Acquired {
    Saved(AcquiredSource),
    /// The link could not be read; nothing was saved.
    NotSaved(AttachFailureDto),
}

/// IDs for one new source: the capture's own operation, never the claim
/// action's, so a source saved by a first attempt is found by dedupe on a
/// second.
struct NewIds {
    operation_id: String,
    source_id: String,
    version_id: String,
}

impl NewIds {
    fn fresh() -> Self {
        let id = || uuid::Uuid::new_v4().to_string();
        Self {
            operation_id: id(),
            source_id: id(),
            version_id: id(),
        }
    }
}

fn acquired(saved: SavedSource, provenance: Provenance, already_saved: bool) -> AcquiredSource {
    AcquiredSource {
        saved,
        provenance,
        already_saved,
        span: None,
        snapshot: SnapshotNoteDto::NotApplicable,
    }
}

pub(super) async fn acquire(
    deps: &AttachDeps<'_>,
    program_id: &str,
    evidence: &Evidence,
) -> Result<Acquired> {
    let repo = LearningSourceLibraryRepository::new(deps.pool.clone());
    match evidence {
        Evidence::SavedPassage {
            source_version_id,
            start_byte,
            end_byte,
        } => {
            let saved = repo
                .saved_version(program_id, source_version_id)
                .await?
                .ok_or_else(|| {
                    AppError::InvalidInput(
                        "That passage is not in a current version of the course's references."
                            .into(),
                    )
                })?;
            let mut source = acquired(
                saved,
                Provenance::SavedPassage {
                    start_byte: *start_byte,
                    end_byte: *end_byte,
                },
                true,
            );
            source.span = Some((*start_byte, *end_byte));
            Ok(Acquired::Saved(source))
        }
        Evidence::Link { url } => link(deps, &repo, program_id, url).await,
        Evidence::Document { document_id } => document(deps, &repo, program_id, document_id)
            .await
            .map(Acquired::Saved),
        Evidence::Text { title, text } => text_source(&repo, program_id, title, text)
            .await
            .map(Acquired::Saved),
    }
}

async fn link(
    deps: &AttachDeps<'_>,
    repo: &LearningSourceLibraryRepository,
    program_id: &str,
    raw_url: &str,
) -> Result<Acquired> {
    let url = web_source_url(raw_url)?;
    let provenance = Provenance::Link { url: url.clone() };
    let mut source = match repo.saved_web_source(program_id, &url).await? {
        Some(saved) => acquired(saved, provenance, true),
        None => {
            let page = match deps.web.fetch_reference_content(&url).await {
                Ok(page) => page,
                Err(error) => {
                    return Ok(Acquired::NotSaved(AttachFailureDto::from_fetch_error(
                        &url, &error,
                    )))
                }
            };
            if let Some(wall) = AttachFailureDto::from_wall(&url, &page) {
                return Ok(Acquired::NotSaved(wall));
            }
            let captured = match captured_page(&url, page) {
                Ok(captured) => captured,
                Err(error) => {
                    return Ok(Acquired::NotSaved(AttachFailureDto::from_fetch_error(
                        &url, &error,
                    )))
                }
            };
            if let Some(saved) = repo
                .saved_with_digest(
                    program_id,
                    &LearningSourceLibraryRepository::text_digest(&captured.text),
                )
                .await?
            {
                acquired(saved, provenance, true)
            } else {
                let ids = NewIds::fresh();
                let hash = source_request_hash(&AddLearningWebSourceRequestDto {
                    operation_id: ids.operation_id.clone(),
                    source_id: ids.source_id.clone(),
                    version_id: ids.version_id.clone(),
                    program_id: program_id.into(),
                    url: url.clone(),
                    freshness_policy: LearningSourcePolicy::Manual,
                })?;
                let saved = save(
                    repo,
                    program_id,
                    &ids,
                    Capture {
                        kind: LearningSourceKind::Web,
                        operation_kind: "add_web",
                        origin: &url,
                        requested_url: Some(&url),
                        freshness: LearningSourcePolicy::Manual,
                    },
                    captured,
                    hash,
                )
                .await?;
                acquired(saved, provenance, false)
            }
        }
    };
    source.snapshot = freeze(deps, repo, program_id, &source.saved).await?;
    Ok(Acquired::Saved(source))
}

/// The frozen copy of a saved page: kept already, ready now, or queued.
async fn freeze(
    deps: &AttachDeps<'_>,
    repo: &LearningSourceLibraryRepository,
    program_id: &str,
    saved: &SavedSource,
) -> Result<SnapshotNoteDto> {
    if saved.snapshot_hash.is_some() {
        return Ok(SnapshotNoteDto::Ready);
    }
    let Some(url) = saved.url.as_deref() else {
        return Ok(SnapshotNoteDto::NotApplicable);
    };
    Ok(match deps.snapshots.request(url, false).await {
        Ok(ReferenceSnapshot::Ready { sha256 }) => {
            repo.link_snapshot(program_id, &saved.version_id, &sha256)
                .await?;
            SnapshotNoteDto::Ready
        }
        Ok(ReferenceSnapshot::Pending) => SnapshotNoteDto::Pending,
        Ok(ReferenceSnapshot::Failed { reason }) => SnapshotNoteDto::Failed { reason },
        // The text is saved and checked either way; the copy is the extra.
        Err(error) => SnapshotNoteDto::Failed {
            reason: error.to_string(),
        },
    })
}

async fn document(
    deps: &AttachDeps<'_>,
    repo: &LearningSourceLibraryRepository,
    program_id: &str,
    document_id: &str,
) -> Result<AcquiredSource> {
    uuid::Uuid::parse_str(document_id)
        .map_err(|_| AppError::InvalidInput("Invalid document ID".into()))?;
    let provenance = Provenance::Document {
        document_id: document_id.into(),
    };
    if let Some(saved) = repo.saved_document(program_id, document_id).await? {
        return Ok(acquired(saved, provenance, true));
    }
    let captured = capture_document(deps.documents, document_id).await?;
    if let Some(saved) = repo
        .saved_with_digest(
            program_id,
            &LearningSourceLibraryRepository::text_digest(&captured.text),
        )
        .await?
    {
        return Ok(acquired(saved, provenance, true));
    }
    let ids = NewIds::fresh();
    let hash = source_request_hash(&AddLearningDocumentSourceRequestDto {
        operation_id: ids.operation_id.clone(),
        source_id: ids.source_id.clone(),
        version_id: ids.version_id.clone(),
        program_id: program_id.into(),
        document_id: document_id.into(),
    })?;
    let saved = save(
        repo,
        program_id,
        &ids,
        Capture {
            kind: LearningSourceKind::Document,
            operation_kind: "add_document",
            origin: document_id,
            requested_url: None,
            freshness: LearningSourcePolicy::Fixed,
        },
        captured,
        hash,
    )
    .await?;
    Ok(acquired(saved, provenance, false))
}

async fn text_source(
    repo: &LearningSourceLibraryRepository,
    program_id: &str,
    title: &str,
    text: &str,
) -> Result<AcquiredSource> {
    if let Some(saved) = repo
        .saved_with_digest(
            program_id,
            &LearningSourceLibraryRepository::text_digest(text),
        )
        .await?
    {
        return Ok(acquired(saved, Provenance::PastedText, true));
    }
    let ids = NewIds::fresh();
    let hash = source_request_hash(&AddLearningTextSourceRequestDto {
        operation_id: ids.operation_id.clone(),
        source_id: ids.source_id.clone(),
        version_id: ids.version_id.clone(),
        program_id: program_id.into(),
        title: title.into(),
        publisher: None,
        text: text.into(),
    })?;
    let captured = CapturedLearningSource {
        title: title.into(),
        publisher: None,
        requested_url: None,
        resolved_url: None,
        text: text.into(),
        truncated: false,
        extraction_version: PASTED_TEXT_EXTRACTION.into(),
    };
    let saved = save(
        repo,
        program_id,
        &ids,
        Capture {
            kind: LearningSourceKind::Pasted,
            operation_kind: "add_text",
            origin: title,
            requested_url: None,
            freshness: LearningSourcePolicy::Fixed,
        },
        captured,
        hash,
    )
    .await?;
    Ok(acquired(saved, Provenance::PastedText, false))
}

/// How a new source is recorded.
struct Capture<'a> {
    kind: LearningSourceKind,
    operation_kind: &'a str,
    origin: &'a str,
    requested_url: Option<&'a str>,
    freshness: LearningSourcePolicy,
}

/// Saves a new source the user added and reads it back as saved.
async fn save(
    repo: &LearningSourceLibraryRepository,
    program_id: &str,
    ids: &NewIds,
    capture: Capture<'_>,
    captured: CapturedLearningSource,
    hash: String,
) -> Result<SavedSource> {
    repo.preflight_new_source(program_id, &ids.source_id, &ids.version_id)
        .await?;
    repo.add_by(
        NewLearningSource {
            operation_id: &ids.operation_id,
            source_id: &ids.source_id,
            version_id: &ids.version_id,
            program_id,
            kind: capture.kind,
            operation_kind: capture.operation_kind,
            origin: capture.origin,
            requested_url: capture.requested_url,
            freshness: capture.freshness,
        },
        captured,
        hash,
        SourceAddedBy::User,
    )
    .await?;
    repo.saved_version(program_id, &ids.version_id)
        .await?
        .ok_or_else(|| AppError::Database("A saved source could not be read back.".into()))
}
