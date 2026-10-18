//! `ChatImageService`: the [`ChatImagesPort`] implementation. Finds
//! candidates, fetches and validates them, writes the row (leased to the
//! turn's request) and then the file, and attaches or releases them as the
//! turn ends (§7.6).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use reqwest::header::HeaderMap;
use sqlx::SqlitePool;

use lattice_core::application::contracts::images::{
    ChatImageDto, FindImageInput, FindImageOutput, FoundImage, ImageOrigin, NO_PICTURE_NOTE,
};
use lattice_core::application::ports::chat_images::{
    ChatImagesPort, CopiedMessage, ImageTurnScope,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::background::BackgroundTasks;

use super::commons::{page_candidate, page_title, parse_search, search_url, Candidate};
use super::fetch::{
    browser_headers, commons_headers, fetch_image, FetchedImage, ImageNetwork, ImageRejected,
    IMAGE_ACCEPT,
};
use super::gc::{ChatImageGc, ImageSweepReport};
use super::repository::{ImageRepository, ImageRow, NewImageRow};
use super::store::{new_id, ImageStore};
use crate::features::web::services::web::{BoundedFetchError, MAX_PAGE_BODY_BYTES};

const PAGE_ACCEPT: &str = "text/html,application/xhtml+xml";

pub struct ChatImageService {
    repository: ImageRepository,
    store: ImageStore,
    network: Arc<dyn ImageNetwork>,
    gc: ChatImageGc,
    background: Arc<BackgroundTasks>,
}

impl ChatImageService {
    pub fn new(
        pool: SqlitePool,
        root: PathBuf,
        network: Arc<dyn ImageNetwork>,
        background: Arc<BackgroundTasks>,
    ) -> Self {
        let repository = ImageRepository::new(pool);
        let store = ImageStore::new(root);
        Self {
            gc: ChatImageGc::new(store.clone(), repository.clone()),
            repository,
            store,
            network,
            background,
        }
    }

    pub(crate) fn repository(&self) -> &ImageRepository {
        &self.repository
    }

    pub(crate) fn store(&self) -> &ImageStore {
        &self.store
    }

    /// The startup sweep: leases no message took, except those of
    /// `kept_requests` (research turns that resume), and unclaimed files.
    pub async fn sweep(&self, kept_requests: &HashSet<String>) -> Result<ImageSweepReport> {
        self.gc.sweep(kept_requests).await
    }

    /// Runs [`Self::sweep`] on the shared background tasks, keeping the
    /// requests `kept_requests` names. When those cannot be read the sweep is
    /// skipped: deleting a resumable turn's pictures cannot be undone, and
    /// the next start sweeps again.
    pub fn sweep_in_background(
        self: &Arc<Self>,
        reason: &'static str,
        kept_requests: impl std::future::Future<Output = Result<HashSet<String>>> + Send + 'static,
    ) {
        let service = Arc::clone(self);
        self.background.spawn(async move {
            let kept = match kept_requests.await {
                Ok(kept) => kept,
                Err(error) => {
                    tracing::warn!(%error, reason, "Skipped the chat image sweep: could not read which turns resume");
                    return;
                }
            };
            match service.sweep(&kept).await {
                Ok(report) => tracing::info!(
                    reason,
                    kept_requests = kept.len(),
                    removed_leases = report.removed_leases,
                    removed_orphans = report.removed_orphans,
                    retained = report.retained,
                    bytes_freed = report.bytes_freed,
                    "Swept chat images"
                ),
                Err(error) => tracing::warn!(%error, reason, "Failed to sweep chat images"),
            }
        });
    }

    /// Search results from Wikimedia Commons, or the page's own picture.
    async fn candidates(
        &self,
        query: &str,
        from_page_url: Option<&str>,
        limit: usize,
        deadline: Option<Instant>,
    ) -> std::result::Result<Vec<Candidate>, String> {
        if let Some(page_url) = from_page_url {
            let body = self
                .network
                .get(
                    page_url,
                    browser_headers(None, Some(PAGE_ACCEPT)),
                    MAX_PAGE_BODY_BYTES,
                    deadline,
                )
                .await
                .map_err(|error| format!("could not read {page_url}: {error}"))?;
            let html = String::from_utf8_lossy(&body.bytes);
            let title = page_title(&html);
            return Ok(page_candidate(&html, &body.final_url, title)
                .into_iter()
                .collect());
        }
        let body = self
            .network
            .get(
                &search_url(query, limit),
                commons_headers("application/json"),
                MAX_PAGE_BODY_BYTES,
                deadline,
            )
            .await
            .map_err(|error: BoundedFetchError| {
                format!("could not search Wikimedia Commons: {error}")
            })?;
        let json: serde_json::Value = serde_json::from_slice(&body.bytes)
            .map_err(|error| format!("Wikimedia Commons answered with invalid JSON: {error}"))?;
        Ok(parse_search(&json))
    }

    fn headers_for(candidate: &Candidate) -> HeaderMap {
        match candidate.origin {
            ImageOrigin::WikimediaCommons => commons_headers(IMAGE_ACCEPT),
            // The page is the referrer a browser would send; sites that
            // refuse hotlinks look for it.
            ImageOrigin::Page => browser_headers(Some(&candidate.source_url), Some(IMAGE_ACCEPT)),
        }
    }

    /// Writes the row (leased to the request) and then the file, so a row
    /// always exists before its file and the sweep never takes a file a
    /// turn is still writing.
    async fn store_fetched(
        &self,
        scope: &ImageTurnScope,
        candidate: &Candidate,
        fetched: FetchedImage,
        fallback_caption: &str,
    ) -> Result<ImageRow> {
        let image = fetched.image;
        let id = new_id();
        let caption = if candidate.caption.trim().is_empty() {
            fallback_caption.trim().to_string()
        } else {
            candidate.caption.clone()
        };
        let row = NewImageRow {
            id: id.clone(),
            conversation_id: scope.conversation_id.clone(),
            request_id: scope.request_id.clone(),
            ext: image.ext.to_string(),
            mime: image.mime.to_string(),
            sha256: image.sha256.clone(),
            byte_size: image.bytes.len() as u64,
            width: image.width,
            height: image.height,
            caption,
            source_url: candidate.source_url.clone(),
            source_title: candidate.source_title.clone(),
            image_url: candidate.image_url.clone(),
            origin: candidate.origin,
            license: candidate.license.clone(),
            attribution: candidate.attribution.clone(),
        };
        self.repository.insert(&row).await?;
        if let Err(error) = self.store.write(&id, image.ext, image.bytes).await {
            let _ = self.repository.delete_ids(std::slice::from_ref(&id)).await;
            return Err(error);
        }
        self.repository
            .get(&id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("chat image {id} vanished after insert")))
    }

    /// Deletes the rows, then their files. A file left behind by a failed
    /// removal has no row, so the next startup sweep takes it.
    async fn release_rows(&self, rows: Vec<ImageRow>) -> Result<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        self.repository.delete_ids(&ids).await?;
        for row in rows {
            self.store.remove(&row.id, &row.ext).await;
        }
        Ok(())
    }
}

/// The model's view of a stored row, with markdown it can paste.
fn found(row: &ImageRow) -> FoundImage {
    let alt: String = row
        .caption
        .chars()
        .map(|c| match c {
            '[' => '(',
            ']' => ')',
            '\n' | '\r' => ' ',
            c => c,
        })
        .collect();
    FoundImage {
        r#ref: row.image_ref.clone(),
        caption: row.caption.clone(),
        source_title: row.source_title.clone(),
        source_url: row.source_url.clone(),
        license: row.license.clone(),
        width: u32::try_from(row.width).unwrap_or(0),
        height: u32::try_from(row.height).unwrap_or(0),
        markdown: format!("![{}]({})", alt.trim(), row.image_ref),
    }
}

/// What the model is told when fewer pictures were stored than it asked for.
fn shortfall_note(
    stored: usize,
    limit: usize,
    searched: &str,
    candidates: usize,
    rejected: &[ImageRejected],
    out_of_time: bool,
) -> Option<String> {
    if stored >= limit {
        return None;
    }
    let mut parts = Vec::new();
    if candidates == 0 {
        parts.push(format!("No pictures found for {searched}."));
    } else {
        parts.push(format!(
            "Stored {stored} of the {limit} pictures asked for."
        ));
    }
    if !rejected.is_empty() {
        let reasons: Vec<String> = rejected.iter().map(ToString::to_string).collect();
        parts.push(format!(
            "Skipped {}: {}.",
            rejected.len(),
            reasons.join("; ")
        ));
    }
    if out_of_time {
        parts.push("The turn ran out of time before more could be fetched.".to_string());
    }
    Some(parts.join(" "))
}

#[async_trait]
impl ChatImagesPort for ChatImageService {
    async fn find_and_store(
        &self,
        scope: &ImageTurnScope,
        input: FindImageInput,
        deadline: Option<Instant>,
    ) -> Result<(FindImageOutput, Vec<ChatImageDto>)> {
        let limit = input.clamped_limit();
        let query = input.query.trim().to_string();
        let from_page = input
            .from_page_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty());
        if query.is_empty() && from_page.is_none() {
            return Err(AppError::InvalidInput(
                "find_image needs a query or a from_page_url".to_string(),
            ));
        }
        let searched = from_page.map_or_else(|| format!("\"{query}\""), str::to_string);

        let candidates = match self.candidates(&query, from_page, limit, deadline).await {
            Ok(candidates) => candidates,
            Err(reason) => {
                tracing::info!(%reason, "find_image stored no picture");
                let output = FindImageOutput {
                    images: Vec::new(),
                    note: Some(NO_PICTURE_NOTE.to_string()),
                };
                return Ok((output, Vec::new()));
            }
        };

        // Pictures this turn already holds are handed back under their ref
        // rather than downloaded and stored a second time.
        let held = self.repository.unattached(&scope.request_id).await?;
        let mut rows: Vec<ImageRow> = Vec::new();
        let mut rejected = Vec::new();
        let mut out_of_time = false;
        for candidate in &candidates {
            if rows.len() >= limit {
                break;
            }
            if let Some(row) = held.iter().find(|row| row.image_url == candidate.image_url) {
                if !rows.iter().any(|kept| kept.id == row.id) {
                    rows.push(row.clone());
                }
                continue;
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                out_of_time = true;
                break;
            }
            let headers = Self::headers_for(candidate);
            match fetch_image(
                self.network.as_ref(),
                &candidate.image_url,
                headers,
                deadline,
            )
            .await
            {
                Ok(fetched) => match self.store_fetched(scope, candidate, fetched, &query).await {
                    Ok(row) => rows.push(row),
                    Err(error) => {
                        tracing::warn!(%error, url = %candidate.image_url, "Could not store a chat image");
                        rejected.push(ImageRejected::Network(error.to_string()));
                    }
                },
                Err(ImageRejected::Deadline) => {
                    out_of_time = true;
                    break;
                }
                Err(reason) => {
                    tracing::debug!(%reason, url = %candidate.image_url, "Skipped a chat image candidate");
                    rejected.push(reason);
                }
            }
        }

        let mut note = shortfall_note(
            rows.len(),
            limit,
            &searched,
            candidates.len(),
            &rejected,
            out_of_time,
        );
        if rows.is_empty() {
            tracing::info!(detail = ?note, "find_image stored no picture");
            note = Some(NO_PICTURE_NOTE.to_string());
        }
        let output = FindImageOutput {
            images: rows.iter().map(found).collect(),
            note,
        };
        Ok((output, rows.iter().map(ImageRow::to_dto).collect()))
    }

    async fn attach_to_message(
        &self,
        request_id: &str,
        message_id: &str,
        referenced_refs: &[String],
    ) -> Result<Vec<ChatImageDto>> {
        let attached = self
            .repository
            .attach(request_id, message_id, referenced_refs)
            .await?;
        let unreferenced = self.repository.unattached(request_id).await?;
        self.release_rows(unreferenced).await?;
        Ok(attached.iter().map(ImageRow::to_dto).collect())
    }

    async fn release_request(&self, request_id: &str) -> Result<()> {
        let rows = self.repository.unattached(request_id).await?;
        self.release_rows(rows).await
    }

    async fn release_message(&self, message_id: &str) -> Result<()> {
        let rows = self.repository.for_message(message_id).await?;
        self.release_rows(rows).await
    }

    async fn remove_unclaimed_files(&self) -> Result<u64> {
        Ok(self.gc.remove_unclaimed_files().await?.removed_orphans)
    }

    async fn copy_for_messages(
        &self,
        conversation_id: &str,
        copies: &[CopiedMessage],
    ) -> Result<Vec<(String, Vec<ChatImageDto>)>> {
        let mut copied = Vec::with_capacity(copies.len());
        for message in copies {
            // Attached at once, so this request never holds a lease; it
            // only keeps each copy's ref unique.
            let request_id = format!("copy:{}", message.message_id);
            let mut images = Vec::new();
            for source_id in &message.image_ids {
                let id = new_id();
                // The row before its file, as the sweep expects.
                let Some(row) = self
                    .repository
                    .insert_copy(
                        source_id,
                        &id,
                        conversation_id,
                        &request_id,
                        &message.message_id,
                    )
                    .await?
                else {
                    continue;
                };
                if let Err(error) = self.store.copy(source_id, &id, &row.ext).await {
                    tracing::warn!(%error, image_id = source_id.as_str(), "Could not copy a chat image");
                    self.repository
                        .delete_ids(std::slice::from_ref(&id))
                        .await?;
                    continue;
                }
                images.push(row.to_dto());
            }
            copied.push((message.message_id.clone(), images));
        }
        Ok(copied)
    }

    async fn release_conversation(&self, conversation_id: &str) -> Result<()> {
        let rows = self.repository.for_conversation(conversation_id).await?;
        self.release_rows(rows).await
    }

    async fn list_for_message(&self, message_id: &str) -> Result<Vec<ChatImageDto>> {
        let rows = self.repository.for_message(message_id).await?;
        Ok(rows.iter().map(ImageRow::to_dto).collect())
    }
}
