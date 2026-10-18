//! The provenance signals of a fetched page (§4): who says they published
//! it, who wrote it, when, where it really lives, and what it links to.
//!
//! Signals come from the page's markup and its URL, parsed once when the
//! page is read: `metadata` (head tags, OpenGraph, Dublin Core, microdata),
//! `json_ld` (schema.org) and `dates`. The origin, archive unwrapping, the
//! reference text's links (with byte offsets) and the officiality tokens
//! are core's rules (`domain::provenance`), applied here
//! to what the page declared. No request is made for any of it (§15), and no
//! value is ever read as an instruction (§14).

mod dates;
mod json_ld;
mod metadata;

#[cfg(test)]
mod parts_tests;
#[cfg(test)]
mod tests;

use lattice_core::application::contracts::web::{
    DateBasis, HttpFacts, Origin, PageSignals, ProvenanceSignals, SourceDates, PAGE_SIGNALS_VERSION,
};
use lattice_core::domain::provenance::{markdown_links, self_description, CanonicalLink};
use tracing::debug;

pub(crate) use dates::parse_http_date;
pub(crate) use metadata::{read_metadata, PageMetadata};

/// Why a page served from the page cache has no signals: the cache keeps a
/// page's text, not its markup.
pub(crate) const FROM_PAGE_CACHE: &str =
    "served from the page cache, which keeps a page's text and not its markup";

/// Why a page the browser read for a conversation has no signals: that read
/// reports paragraph text only.
pub(crate) const BROWSER_TEXT_ONLY: &str =
    "read through the browser's text report, which carries no page markup";

/// How a read wrote the page's text, which decides whether its links can be
/// read back from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextForm {
    /// The reference reader's markdown: links are `[anchor](url)` and the
    /// page's own brackets are escaped.
    Markdown,
    /// Paragraph text: any bracket in it is the page's own prose.
    Plain,
}

/// What one read of a page found in its markup and its HTTP exchange.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PageReading {
    pub metadata: PageMetadata,
    pub http: HttpFacts,
    pub text: TextForm,
}

/// What a read knows about where a page came from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PageProvenance {
    Read(Box<PageReading>),
    /// The read had no markup to look at; the reason says why.
    NotRead(&'static str),
}

impl PageProvenance {
    /// The signals of the page read at `resolved_url`, whose text is `text`,
    /// acquired at `fetched_at` (unix seconds).
    pub(crate) fn into_signals(
        self,
        resolved_url: &str,
        text: &str,
        fetched_at: i64,
    ) -> ProvenanceSignals {
        match self {
            Self::Read(reading) => page_signals(*reading, resolved_url, text, fetched_at),
            Self::NotRead(reason) => ProvenanceSignals::NotExtracted {
                reason: reason.to_string(),
            },
        }
    }
}

fn page_signals(
    reading: PageReading,
    resolved_url: &str,
    text: &str,
    fetched_at: i64,
) -> ProvenanceSignals {
    let mut origin = match Origin::of_url(resolved_url) {
        Ok(origin) => origin,
        Err(error) => {
            return ProvenanceSignals::NotExtracted {
                reason: error.to_string(),
            }
        }
    };
    let PageReading {
        metadata,
        http,
        text: form,
    } = reading;
    if let Some(canonical) = declared_canonical(&metadata, &origin) {
        if let CanonicalLink::CrossOrigin { url } = origin.apply_canonical(&canonical) {
            // The signals have no field for another origin's canonical: the
            // origin stays the page's own (§7), and the declaration is not
            // carried further (as-built notes, P2).
            debug!(
                resolved_url,
                canonical = url.as_str(),
                "Cross-origin canonical not recorded"
            );
        }
    }
    let dates = source_dates(&metadata, &origin, &http, fetched_at);
    let links = match form {
        TextForm::Markdown => markdown_links(text),
        TextForm::Plain => Vec::new(),
    };
    let self_description = self_description(
        [&metadata.title, &metadata.site_name, &metadata.description]
            .into_iter()
            .flatten()
            .map(String::as_str),
    );
    ProvenanceSignals::Web(Box::new(PageSignals {
        origin,
        resolved_url: resolved_url.to_string(),
        title: metadata.title,
        site_name: metadata.site_name,
        publisher: metadata.publisher,
        author: metadata.author,
        declared_types: metadata.declared_types,
        language: metadata.language,
        dates,
        generator: metadata.generator,
        license: metadata.license,
        links,
        http,
        self_description,
        description: metadata.description,
        extraction_version: PAGE_SIGNALS_VERSION.to_string(),
    }))
}

/// The canonical the page declared (`rel=canonical`, else `og:url`), as the
/// page meant it. An archived page's canonical is rewritten into the
/// archive's address space, so a wrapper canonical stands for the page it
/// wraps, and the archive's own address for itself names nothing.
fn declared_canonical(metadata: &PageMetadata, origin: &Origin) -> Option<String> {
    let declared = metadata.canonical.as_ref().or(metadata.og_url.as_ref())?;
    let Some(archived) = &origin.archived else {
        return Some(declared.clone());
    };
    match Origin::of_url(declared).ok()?.archived {
        Some(inner) => Some(inner.original_url),
        None => {
            let host = url::Url::parse(declared)
                .ok()?
                .host_str()?
                .to_ascii_lowercase();
            (host != archived.archive_host).then(|| declared.clone())
        }
    }
}

fn source_dates(
    metadata: &PageMetadata,
    origin: &Origin,
    http: &HttpFacts,
    fetched_at: i64,
) -> SourceDates {
    let mut basis = Vec::new();
    let mut take = |chosen: &Option<(i64, DateBasis)>| {
        chosen.as_ref().map(|(at, from)| {
            basis.push(from.clone());
            *at
        })
    };
    let published = take(&metadata.published.at);
    let modified = take(&metadata.modified.at);
    let archived_at = origin
        .archived
        .as_ref()
        .and_then(|archived| archived.archived_at);
    if archived_at.is_some() {
        basis.push(DateBasis::ArchiveTimestamp);
    }
    if http.last_modified.is_some() {
        basis.push(DateBasis::HttpHeader);
    }
    basis.extend(
        metadata
            .published
            .malformed
            .iter()
            .chain(&metadata.modified.malformed)
            .map(|field| DateBasis::Malformed {
                field: field.clone(),
            }),
    );
    SourceDates {
        published,
        modified,
        archived_at,
        http_last_modified: http.last_modified,
        acquired_at: fetched_at,
        basis,
    }
}
