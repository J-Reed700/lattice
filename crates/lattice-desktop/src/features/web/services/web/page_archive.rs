//! The fetch chain's archive steps (design §3.8): the Internet Archive as the
//! last resort for a page nothing else reads, dated reads, and the refusal
//! rule (a refusal is remembered only when the archive had nothing either).

use std::time::Instant;

use chrono::NaiveDate;
use lattice_core::application::contracts::web::{HttpFacts, ReadVia};
use lattice_core::domain::sourcing::unwrap_archive;
use lattice_core::shared::error::{AppError, Result};
use tracing::info;
use url::Url;

use super::super::address_guard::check_url_host;
use super::super::{PageRead, ReadPage, WebService};
use super::{cached_page, served_from_cache, text_len, LiveFailure, PageText};
use crate::features::web::services::page_cache::{ArchiveAnswer, CachedLookup};
use crate::features::web::services::provenance::{PageProvenance, PageReading};
use crate::features::web::services::wayback::{Lookup, ReplayMiss};

/// The archive gave no copy: its answer, for the refusal rule, and a note
/// for the error.
struct ArchiveMiss {
    answer: ArchiveAnswer,
    note: String,
}

impl ArchiveMiss {
    fn nothing(note: impl Into<String>) -> Self {
        Self {
            answer: ArchiveAnswer::NothingArchived,
            note: note.into(),
        }
    }

    fn not_asked(note: impl Into<String>) -> Self {
        Self {
            answer: ArchiveAnswer::NotAsked,
            note: note.into(),
        }
    }

    fn unanswered(note: impl Into<String>) -> Self {
        Self {
            answer: ArchiveAnswer::Unanswered,
            note: note.into(),
        }
    }
}

pub(super) const NOTHING_ARCHIVED: &str = "The Internet Archive holds no readable copy";

impl WebService {
    /// The live page could not be read: ask the archive. The refusal is
    /// remembered only when the archive had nothing either (or was not
    /// asked); a rescued URL is cached as its archived copy instead.
    pub(super) async fn rescue_or_refuse(
        &self,
        url: &str,
        parsed: &Url,
        failure: LiveFailure,
        start: Instant,
        read: PageRead,
    ) -> Result<ReadPage> {
        match self.archive_fallback(parsed, start, read).await {
            Ok(page) => {
                if read == PageRead::Chat {
                    self.page_cache
                        .remember_page(url, &cached_page(&page))
                        .await;
                }
                Ok(page)
            }
            Err(miss) => {
                info!(url, note = %miss.note, "The archive did not rescue the page");
                if let LiveFailure::Status(status) = failure {
                    self.page_cache
                        .remember_refusal(
                            url,
                            status,
                            &format!("HTTP {status}. {}", miss.note),
                            miss.answer,
                        )
                        .await;
                }
                Err(AppError::Network(format!(
                    "{}. {}",
                    failure.describe(),
                    miss.note
                )))
            }
        }
    }

    /// A shell even after the browser: the archive's copy replaces it only
    /// when it holds more text.
    pub(super) async fn thicker_from_archive(
        &self,
        url: &str,
        parsed: &Url,
        live: ReadPage,
        start: Instant,
        read: PageRead,
    ) -> ReadPage {
        match self.archive_fallback(parsed, start, read).await {
            Ok(archived) if text_len(&archived) > text_len(&live) => {
                if read == PageRead::Chat {
                    self.page_cache
                        .remember_page(url, &cached_page(&archived))
                        .await;
                }
                archived
            }
            Ok(_) => live,
            Err(miss) => {
                info!(url, note = %miss.note, "No fuller archived copy of a thin page");
                live
            }
        }
    }

    /// The archive as the last resort for the live page.
    async fn archive_fallback(
        &self,
        parsed: &Url,
        start: Instant,
        read: PageRead,
    ) -> std::result::Result<ReadPage, ArchiveMiss> {
        if self.wayback.is_off() {
            return Err(ArchiveMiss::not_asked("The archive fallback is off"));
        }
        if unwrap_archive(parsed).is_some() {
            return Err(ArchiveMiss::not_asked(
                "The page is an archived copy already",
            ));
        }
        self.read_archived(parsed, None, start, read).await
    }

    /// A dated read: the archive alone. Nothing goes to the live site, but the
    /// archive is told the URL, so it must be one that may leave this machine.
    pub(super) async fn read_dated(
        &self,
        parsed: &Url,
        day: NaiveDate,
        read: PageRead,
    ) -> Result<ReadPage> {
        check_url_host(parsed)?;
        if self.wayback.is_off() {
            return Err(AppError::InvalidInput(
                "A dated read comes from the Internet Archive, and the archive fallback is off"
                    .into(),
            ));
        }
        // Asked of an archive URL, a dated read is of the page it archives.
        let original = unwrap_archive(parsed).map_or_else(|| parsed.clone(), |copy| copy.original);
        self.read_archived(&original, Some(day), Instant::now(), read)
            .await
            .map_err(|miss| {
                AppError::Network(format!(
                    "No archived copy of {original} near {day}. {}",
                    miss.note
                ))
            })
    }

    /// The capture of `original` nearest `as_of` (the latest when `None`),
    /// from the cache or the archive, read through the same extractor as a
    /// live page and labelled with its capture time.
    async fn read_archived(
        &self,
        original: &Url,
        as_of: Option<NaiveDate>,
        start: Instant,
        read: PageRead,
    ) -> std::result::Result<ReadPage, ArchiveMiss> {
        let capture = match self.page_cache.archive_lookup(original, as_of).await {
            Some(CachedLookup::Found(capture)) => capture,
            Some(CachedLookup::NothingArchived) => {
                return Err(ArchiveMiss::nothing(NOTHING_ARCHIVED));
            }
            None => match self.wayback.lookup(original, as_of).await {
                Lookup::Found(capture) => {
                    self.page_cache
                        .remember_archive_lookup(original, as_of, Some(&capture))
                        .await;
                    capture
                }
                Lookup::NothingArchived => {
                    self.page_cache
                        .remember_archive_lookup(original, as_of, None)
                        .await;
                    return Err(ArchiveMiss::nothing(NOTHING_ARCHIVED));
                }
                Lookup::Unanswered(note) => return Err(ArchiveMiss::unanswered(note)),
            },
        };

        if read == PageRead::Chat {
            if let Some(page) = self.page_cache.archived(original, &capture).await {
                info!(url = original.as_str(), captured_at = %capture.captured_at, "Archived copy served from page cache");
                return Ok(served_from_cache(page, start));
            }
        }

        let replayed = match self.wayback.replay(original, &capture).await {
            Ok(replayed) => replayed,
            Err(ReplayMiss::Gone(note)) => {
                // The index named a copy the replay cannot serve.
                self.page_cache
                    .remember_archive_lookup(original, as_of, None)
                    .await;
                return Err(ArchiveMiss::nothing(note));
            }
            Err(ReplayMiss::Unanswered(note)) => return Err(ArchiveMiss::unanswered(note)),
        };
        let unreadable = |error| {
            ArchiveMiss::unanswered(format!("The archived copy could not be read: {error}"))
        };
        let snapshot_url = Url::parse(&replayed.snapshot_url).map_err(unreadable)?;
        let (title, content, metadata, text_form) = self
            .extract(replayed.html, snapshot_url, read)
            .await
            .map_err(unreadable)?;
        if content.trim().is_empty() {
            return Err(ArchiveMiss::nothing(
                "The archived copy has no readable text",
            ));
        }

        info!(
            url = original.as_str(),
            captured_at = %replayed.captured_at,
            "Page read from the Internet Archive"
        );
        // The capture replays the page's own markup, so its metadata is read
        // like a live page's; the snapshot address says it is archived.
        let provenance = PageProvenance::Read(Box::new(PageReading {
            metadata,
            http: HttpFacts {
                // A replay only returns a page the archive answered OK.
                status: reqwest::StatusCode::OK.as_u16(),
                content_type: replayed.content_type.clone(),
                last_modified: None,
                via_browser_reader: false,
            },
            text: text_form,
        }));
        let text = PageText {
            final_url: replayed.snapshot_url.clone(),
            title,
            content,
            content_type: replayed.content_type,
            via: ReadVia::Archive {
                captured_at: replayed.captured_at,
                original: original.to_string(),
            },
        };
        let page = self
            .keep_page(&replayed.snapshot_url, text, (start, read), provenance)
            .await;
        if read == PageRead::Chat {
            self.page_cache
                .remember_archived(original, &capture, &cached_page(&page))
                .await;
        }
        Ok(page)
    }
}
