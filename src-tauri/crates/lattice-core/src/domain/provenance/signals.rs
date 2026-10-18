//! What capture recorded about a source: the only thing a standing or
//! ownership judgement ever sees (§4). No page prose is a signal except
//! heading lines and dates found in text, because a page must not be able
//! to talk its way into trust.

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::judgement::{canonical_json, sha256_hex};
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

/// Words a page uses to call itself official, verified or authoritative, in
/// the languages the extractor meets most. Matched whole-word in scripts
/// with word breaks and as substrings in those without. A risk input only
/// (§14): standing never rests on what a page says about itself.
const SELF_DESCRIPTION_TOKENS: [&str; 24] = [
    "official",
    "verified",
    "authoritative",
    "authorized",
    "authorised",
    "officiel",
    "officielle",
    "offiziell",
    "offizielle",
    "oficial",
    "ufficiale",
    "oficjalny",
    "officieel",
    "officiella",
    "официальный",
    "официальная",
    "公式",
    "官方",
    "官网",
    "認証",
    "공식",
    "rasmi",
    "resmi",
    "chính thức",
];

/// Phrases that address a reader or a model rather than describe a page.
/// Metadata describes; text that instructs is an injection attempt (§14),
/// whatever the model makes of it. Lower-case, matched as substrings of the
/// lower-cased field.
const READER_ADDRESSED_PHRASES: [&str; 16] = [
    "ignore previous instructions",
    "ignore all previous",
    "ignore prior instructions",
    "ignore the above",
    "ignore other sources",
    "disregard previous",
    "disregard other sources",
    "note to ai",
    "note to llm",
    "note to the model",
    "attention ai",
    "ai assistants",
    "language model",
    "system prompt",
    "rate this source",
    "rate this page",
];

impl ProvenanceSignals {
    /// The dates capture found; `None` when nothing was extracted.
    pub fn dates(&self) -> Option<&SourceDates> {
        match self {
            Self::Web(page) => Some(&page.dates),
            Self::Document(document) => Some(&document.dates),
            Self::Pasted(pasted) => Some(&pasted.dates),
            Self::NotExtracted { .. } => None,
        }
    }

    /// What the user said about a file or pasted text.
    pub fn declared_origin(&self) -> Option<&DeclaredOrigin> {
        match self {
            Self::Document(document) => document.declared_origin.as_ref(),
            Self::Pasted(pasted) => pasted.declared_origin.as_ref(),
            Self::Web(_) | Self::NotExtracted { .. } => None,
        }
    }

    /// Whether the user added it without saying where it came from (§5
    /// step 9).
    pub fn is_unattributed_user_material(&self) -> bool {
        matches!(self, Self::Document(_) | Self::Pasted(_)) && self.declared_origin().is_none()
    }

    /// The form a standing judgement sees and is keyed by: the signals as
    /// JSON without the outbound links (§12.1), which say where a page
    /// points, not who publishes it.
    pub fn judged_form(&self) -> Value {
        let mut value = serde_json::to_value(self).unwrap_or(Value::Null);
        if let Some(object) = value.as_object_mut() {
            object.remove("links");
        }
        value
    }

    /// sha256 of the canonical judged form: the standing judgement's key.
    pub fn signals_sha256(&self) -> String {
        sha256_hex(&canonical_json(&self.judged_form()))
    }

    /// The metadata fields whose text addresses a reader or a model, by
    /// field name. Non-empty means an injection attempt.
    pub fn reader_addressed_fields(&self) -> Vec<&'static str> {
        let fields: Vec<(&'static str, Option<&str>)> = match self {
            Self::Web(page) => vec![
                ("title", page.title.as_deref()),
                ("site_name", page.site_name.as_deref()),
                ("description", page.description.as_deref()),
                (
                    "publisher",
                    page.publisher.as_ref().map(|p| p.name.as_str()),
                ),
                ("author", page.author.as_ref().map(|a| a.name.as_str())),
                ("generator", page.generator.as_deref()),
                ("license", page.license.as_deref()),
            ],
            Self::Document(document) => vec![("title", document.title.as_deref())]
                .into_iter()
                .chain(
                    document
                        .heading_lines
                        .iter()
                        .map(|line| ("heading_lines", Some(line.as_str()))),
                )
                .collect(),
            Self::Pasted(_) | Self::NotExtracted { .. } => Vec::new(),
        };
        let mut found: Vec<&'static str> = Vec::new();
        for (name, text) in fields {
            if text.is_some_and(addresses_reader) && !found.contains(&name) {
                found.push(name);
            }
        }
        found
    }
}

/// Whether `text` addresses a reader or a model.
pub fn addresses_reader(text: &str) -> bool {
    let lower = text.to_lowercase();
    let collapsed = lower.split_whitespace().collect::<Vec<_>>().join(" ");
    READER_ADDRESSED_PHRASES
        .iter()
        .any(|phrase| collapsed.contains(phrase))
}

/// The self-description tokens in a page's title, site name and
/// description, each once, in the order found.
pub fn self_description<'a>(fields: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut found = Vec::new();
    for field in fields {
        let lower = field.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .collect();
        for token in SELF_DESCRIPTION_TOKENS {
            // Whole words in cased scripts; substrings where words run together.
            let present = if token.chars().all(is_cased) {
                words.contains(&token)
            } else {
                lower.contains(token)
            };
            if present && !found.iter().any(|seen: &String| seen == token) {
                found.push(token.to_owned());
            }
        }
    }
    found
}

/// Letters of scripts that separate words with spaces (cased scripts).
fn is_cased(c: char) -> bool {
    c.is_uppercase() || c.to_uppercase().next() != Some(c)
}

/// Every markdown link `[anchor](http(s)://…)` in `text`, with its byte
/// offsets. An anchor may hold one level of brackets (`[[1]](url)`, the
/// footnote form); an optional link title is dropped.
pub fn markdown_links(text: &str) -> Vec<OutboundLink> {
    let Ok(pattern) =
        Regex::new(r#"\[((?:[^\[\]]|\[[^\[\]]*\])*)\]\((https?://[^\s)]+)(?:\s+"[^"]*")?\)"#)
    else {
        return Vec::new();
    };
    pattern
        .captures_iter(text)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            Some(OutboundLink {
                url: captures.get(2)?.as_str().to_owned(),
                anchor: captures.get(1)?.as_str().to_owned(),
                start_byte: whole.start(),
                end_byte: whole.end(),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "signals_tests.rs"]
mod tests;
