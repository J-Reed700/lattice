//! A lead: a URL a finder says may hold the source. Leads are not evidence; a
//! lead becomes a source only through fetch, extraction and provenance.

use chrono::{DateTime, Utc};
use url::Url;

use super::descriptor::FinderId;
use super::need::{Identifier, Origin};
use super::outcome::FinderOutcome;

/// One URL a finder returned, with what the finder knows about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Lead {
    pub url: Url,
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub published: Option<DateTime<Utc>>,
    pub kind: LeadKind,
    /// The origin the finder asserts owns the subject (a registry homepage, a
    /// Wikidata official website), for provenance's subject ownership.
    pub owner: Option<Origin>,
    /// The page's declared `rel=canonical`, when the finder saw it (an API
    /// that returns it, or a page already read). Dedupe folds the lead into
    /// its canonical target.
    pub rel_canonical: Option<Url>,
    /// Reuse terms the citation must honour, when the finder knows them.
    pub terms: Option<Terms>,
    /// Each finder that returned this lead and its 1-based rank in that
    /// finder's list. A finder fills its own entry; fusion unions them.
    pub found_by: Vec<(FinderId, usize)>,
}

impl Lead {
    /// A bare page lead found by `finder` at `rank`, for finders and tests
    /// that know nothing else yet.
    pub fn page(url: Url, finder: FinderId, rank: usize) -> Self {
        Self {
            url,
            title: None,
            snippet: None,
            published: None,
            kind: LeadKind::Page,
            owner: None,
            rel_canonical: None,
            terms: None,
            found_by: vec![(finder, rank)],
        }
    }
}

/// What a lead points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeadKind {
    Page,
    /// The subject owner's site (a registry homepage, a Wikidata P856).
    OfficialSite,
    /// A document with an exact identifier (a DOI landing page, an RFC).
    Document(Identifier),
    Repository,
    /// A link found on `page`, with where it sits when known.
    ReferenceOf {
        page: Url,
        place: Option<LinkPlace>,
    },
    /// A dated archive copy of `original`.
    ArchivedCopy {
        original: Url,
        captured_at: Option<DateTime<Utc>>,
    },
}

impl LeadKind {
    /// Tie-break precedence when fused scores are equal: a document beats an
    /// official site beats a repository beats a page or reference beats an
    /// archived copy (the design's `Document > OfficialSite > Page >
    /// ArchivedCopy`, with `Repository` and `ReferenceOf` placed beside their
    /// nearest kin).
    pub fn precedence(&self) -> u8 {
        match self {
            LeadKind::Document(_) => 5,
            LeadKind::OfficialSite => 4,
            LeadKind::Repository => 3,
            LeadKind::Page | LeadKind::ReferenceOf { .. } => 2,
            LeadKind::ArchivedCopy { .. } => 1,
        }
    }
}

/// Where a link sits on the page that makes it (`follow_links`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPlace {
    pub anchor: String,
    /// The sentence the link sits in.
    pub context: String,
    /// Byte offset of the link in the page's extracted text.
    pub offset: usize,
}

/// Reuse terms a lead carries, so the citation honours them visibly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terms {
    /// CC BY-SA content (Stack Exchange): link the post and name the author.
    CcBySa {
        post_url: Url,
        author: Option<String>,
    },
    /// Free for non-commercial use only (Marginalia).
    NonCommercial,
    /// Only metadata may be shown (ISO catalogue records).
    MetadataOnly,
}

/// Which part of the near text or span `leads_from_page` returns links for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Near {
    /// A short quote from the page; links in and around its paragraph first.
    Quote(String),
    /// A byte range of the page's extracted text.
    Bytes { start: usize, end: usize },
}

/// Who resolved a subject's owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OwnerSource {
    Wikidata,
    Registry,
    GitHub,
}

impl OwnerSource {
    pub fn as_str(self) -> &'static str {
        match self {
            OwnerSource::Wikidata => "wikidata",
            OwnerSource::Registry => "registry",
            OwnerSource::GitHub => "github",
        }
    }
}

/// The origins that own a subject, and how that was learned.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnerResolution {
    /// Owner origins, most authoritative first; empty when none was found.
    pub origins: Vec<Origin>,
    pub resolved_by: Option<OwnerSource>,
    /// Every finder asked, with its result, so a miss says what was tried.
    pub outcomes: Vec<FinderOutcome>,
}
