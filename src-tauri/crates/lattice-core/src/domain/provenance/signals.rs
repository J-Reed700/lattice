//! What capture recorded about a source: the only thing a standing or
//! ownership judgement ever sees (§4). No page prose is a signal except
//! heading lines and dates found in text, because a page must not be able
//! to talk its way into trust.

use serde::{Deserialize, Serialize};

use super::origin::Origin;

/// The extraction version stamped on page signals. Bump it when the reader
/// would record different signals for the same HTML.
pub const PAGE_SIGNALS_VERSION: &str = "page-signals-v1";

/// What capture recorded, by kind of source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ProvenanceSignals {
    /// Boxed: a page carries far more signals than the other kinds.
    Web(Box<PageSignals>),
    Document(DocumentSignals),
    Pasted(PastedSignals),
    /// The fetcher could not extract provenance (a test fake, a fetcher
    /// without a metadata reader). Honest, never silent.
    NotExtracted {
        reason: String,
    },
}

/// Signals of one fetched web page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageSignals {
    pub origin: Origin,
    pub resolved_url: String,
    #[serde(default)]
    pub title: Option<String>,
    /// `og:site_name`, `application-name`.
    #[serde(default)]
    pub site_name: Option<String>,
    /// schema.org `publisher` / JSON-LD `Organization`, `dc.publisher`.
    #[serde(default)]
    pub publisher: Option<NamedEntity>,
    /// `meta author`, `itemprop=author`, JSON-LD `author`, `rel=author`.
    #[serde(default)]
    pub author: Option<NamedEntity>,
    /// JSON-LD `@type` values and `og:type`: data, not trust.
    #[serde(default)]
    pub declared_types: Vec<String>,
    /// `<html lang>`, `og:locale`.
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub dates: SourceDates,
    /// `meta generator`: a wiki, a forum engine, a static site generator.
    #[serde(default)]
    pub generator: Option<String>,
    /// `rel=license` / schema.org `license`.
    #[serde(default)]
    pub license: Option<String>,
    /// Markdown links in the reference text, with byte offsets into it.
    #[serde(default)]
    pub links: Vec<OutboundLink>,
    #[serde(default)]
    pub http: HttpFacts,
    /// Title, site-name or description tokens claiming officiality
    /// (`self_description`). A risk-flag input, never evidence.
    #[serde(default)]
    pub self_description: Vec<String>,
    /// The `meta description`, kept only to be scanned for text addressed to
    /// a reader or a model (an injection attempt); never judged as content.
    #[serde(default)]
    pub description: Option<String>,
    pub extraction_version: String,
}

/// A named publisher or author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedEntity {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    /// The declared type: `Organization`, `Person`, `GovernmentOrganization`.
    #[serde(default)]
    pub kind: Option<String>,
}

/// One markdown link `[anchor](url)` in a source's text; the offsets are
/// the link's bytes in that text, `start_byte..end_byte`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundLink {
    pub url: String,
    pub anchor: String,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// What the HTTP exchange said.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HttpFacts {
    pub status: u16,
    #[serde(default)]
    pub content_type: Option<String>,
    /// `Last-Modified`, unix seconds.
    #[serde(default)]
    pub last_modified: Option<i64>,
    /// The browser reader produced the DOM (the plain fetch was refused).
    #[serde(default)]
    pub via_browser_reader: bool,
}

/// Signals of a library document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSignals {
    pub file_name: String,
    pub mime_type: String,
    #[serde(default)]
    pub page_count: Option<usize>,
    #[serde(default)]
    pub title: Option<String>,
    /// The first heading lines of the extracted text: headings only, never
    /// body text.
    #[serde(default)]
    pub heading_lines: Vec<String>,
    #[serde(default)]
    pub declared_origin: Option<DeclaredOrigin>,
    #[serde(default)]
    pub dates: SourceDates,
}

/// Signals of pasted text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PastedSignals {
    #[serde(default)]
    pub declared_origin: Option<DeclaredOrigin>,
    pub chars: usize,
    #[serde(default)]
    pub dates: SourceDates,
}

/// What the user said about a file or text when attaching it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclaredOrigin {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Every date capture found, unix seconds, with where each came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDates {
    #[serde(default)]
    pub published: Option<i64>,
    #[serde(default)]
    pub modified: Option<i64>,
    #[serde(default)]
    pub archived_at: Option<i64>,
    #[serde(default)]
    pub http_last_modified: Option<i64>,
    pub acquired_at: i64,
    #[serde(default)]
    pub basis: Vec<DateBasis>,
}

/// Where a date came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateBasis {
    ArticlePublishedTime,
    ArticleModifiedTime,
    JsonLdDatePublished,
    JsonLdDateModified,
    MetaDate,
    TimeElement,
    HttpHeader,
    ArchiveTimestamp,
    /// A date written in the text; the quote is the words that carry it.
    InText {
        quote: String,
    },
    /// A field that should have held a date but did not parse; the date was
    /// taken from the other fields.
    Malformed {
        field: String,
    },
}
