//! Content evidence (claim sources design §4.1): what a source's own text
//! and links say about who wrote it and what kind of work it is, read
//! without a model. The standing judgement sees it next to the metadata
//! signals; the level procedure reads its deterministic summary
//! ([`WorkEvidence`]) so a version is levelled the same way with or without
//! its text in hand.
//!
//! Nothing here names a publisher, a domain or a vendor. The only word lists
//! are reader vocabulary: the cue words a byline starts with and the words
//! an about page's address uses.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use url::Url;

use super::origin::{Origin, TldRegistry};
use super::reason::AddedBy;
use super::signals::OutboundLink;

/// Words a byline starts with, lower case, longest first so "written by"
/// wins over "by": English and the languages the design names.
const BYLINE_CUES: [&str; 11] = [
    "written by ",
    "authors: ",
    "author: ",
    "autor: ",
    "author ",
    "autor ",
    "by ",
    "par ",
    "von ",
    "por ",
    "di ",
];

/// Lower-case words that may sit between the capitalised words of a name.
const NAME_PARTICLES: [&str; 13] = [
    "and", "&", "de", "da", "di", "del", "der", "van", "von", "le", "la", "du", "y",
];

/// Address or anchor words of a page where a site says who it is.
const ABOUT_WORDS: [&str; 8] = [
    "about", "author", "authors", "contact", "team", "people", "bio", "imprint",
];

/// Heading words that mark a paper's shape.
const PAPER_HEADINGS: [&str; 3] = ["abstract", "references", "bibliography"];

/// Shortest byline token that can name a person in a domain: the design's
/// "4+ letters", so initials and particles never match.
const NAME_TOKEN_LETTERS: usize = 4;

/// What a source's text and links say about its authorship and its shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentEvidence {
    /// Lines that name an author or publisher ("By …", "Author: …",
    /// "© 2024 …"), one each, deduplicated, in text order.
    pub bylines: Vec<String>,
    /// The registrable domain's first label when a byline's name contains it
    /// or it contains a byline token of four or more letters.
    #[serde(default)]
    pub name_in_domain: Option<String>,
    /// Same-site links whose address or anchor names an about, author,
    /// contact, team, people, bio or imprint page.
    pub about_links: Vec<String>,
    pub citations: CitationProfile,
    pub work: WorkShape,
    /// How the course got the source and, for research, why it was taken.
    pub chosen: Chosen,
    /// Lists were cut from the end so the judgement request fits its
    /// window; the judgement saw less content evidence than the page has.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// What the source's links stand on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationProfile {
    /// Distinct registrable domains linked, other than the site's own.
    pub external_domains: usize,
    /// External links whose registry is restricted (government, education,
    /// military, intergovernmental).
    pub restricted_registry_links: usize,
    /// Distinct DOIs and arXiv identifiers in links.
    pub scholarly_identifiers: usize,
    pub same_site_links: usize,
}

/// The shape of the work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkShape {
    pub words: usize,
    pub headings: usize,
    /// The heading lines, by the rule a library document's
    /// `heading_lines` use (Markdown `#` lines), deduplicated.
    pub heading_lines: Vec<String>,
    /// The page's address is a path of numbered segments (`/08-…/01-…`).
    pub numbered_path: bool,
    pub code_blocks: usize,
    /// A heading named Abstract, References or Bibliography.
    pub has_abstract_or_references: bool,
}

/// How the course obtained the source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chosen {
    pub by: AddedBy,
    /// The research planner's reason for selecting it, when it gave one.
    #[serde(default)]
    pub note: Option<String>,
}

impl Default for Chosen {
    fn default() -> Self {
        Self {
            by: AddedBy::Research,
            note: None,
        }
    }
}

impl ContentEvidence {
    /// The evidence in `text` and its recorded `links`, for a source at
    /// `origin`, obtained as `chosen` says. The page's own address is read
    /// from the origin's canonical URL when capture recorded one.
    pub fn of(text: &str, links: &[OutboundLink], origin: &Origin, chosen: Chosen) -> Self {
        let bylines = bylines(text);
        let name_in_domain = name_in_domain(&bylines, origin);
        Self {
            about_links: about_links(links, origin),
            citations: CitationProfile::of(links, origin),
            work: WorkShape::of(text, origin),
            name_in_domain,
            bylines,
            chosen,
            truncated: false,
        }
    }

    /// Whether any list can still be cut to make the request smaller.
    pub fn can_shrink(&self) -> bool {
        !self.bylines.is_empty()
            || !self.about_links.is_empty()
            || !self.work.heading_lines.is_empty()
    }

    /// Cuts each list to its first half (from the end) and marks the
    /// evidence truncated. The counts and the summary are not changed.
    pub fn shrink(&mut self) {
        for list in [
            &mut self.bylines,
            &mut self.about_links,
            &mut self.work.heading_lines,
        ] {
            list.truncate(list.len() / 2);
        }
        self.truncated = true;
    }
}

impl CitationProfile {
    fn of(links: &[OutboundLink], origin: &Origin) -> Self {
        let mut external = BTreeSet::new();
        let mut scholarly = BTreeSet::new();
        let mut profile = Self::default();
        for link in links {
            if let Some(id) = scholarly_identifier(&link.url) {
                scholarly.insert(id);
            }
            let Ok(target) = Origin::of_url(&link.url) else {
                continue;
            };
            if same_site(&target, origin) {
                profile.same_site_links += 1;
                continue;
            }
            if matches!(target.tld_registry, TldRegistry::Restricted { .. }) {
                profile.restricted_registry_links += 1;
            }
            external.insert(target.key.0);
        }
        profile.external_domains = external.len();
        profile.scholarly_identifiers = scholarly.len();
        profile
    }
}

impl WorkShape {
    fn of(text: &str, origin: &Origin) -> Self {
        let mut heading_lines: Vec<String> = Vec::new();
        let mut headings = 0;
        let mut fences = 0;
        for line in text.lines().map(str::trim) {
            if is_fence(line) {
                fences += 1;
                continue;
            }
            if fences % 2 == 1 || !line.starts_with('#') {
                continue;
            }
            let heading = line.trim_start_matches('#').trim();
            if heading.is_empty() {
                continue;
            }
            headings += 1;
            if !heading_lines.iter().any(|seen| seen == heading) {
                heading_lines.push(heading.to_owned());
            }
        }
        let has_abstract_or_references = heading_lines.iter().any(|heading| {
            let words = letters_lower(heading, true);
            PAPER_HEADINGS.contains(&words.trim())
        });
        Self {
            words: text.split_whitespace().count(),
            headings,
            heading_lines,
            numbered_path: origin.canonical_url.as_deref().is_some_and(numbered_path),
            code_blocks: fences / 2,
            has_abstract_or_references,
        }
    }
}

/// The byline lines of `text`, deduplicated, in text order. Lines inside
/// fenced code are not read.
fn bylines(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut fenced = false;
    for line in text.lines().map(str::trim) {
        if is_fence(line) {
            fenced = !fenced;
            continue;
        }
        let line = line
            .trim_start_matches(['#', '*', '_', '>', '-', ' '])
            .trim_end_matches(['*', '_', ' '])
            .trim();
        if fenced || line.is_empty() || byline_name(line).is_none() {
            continue;
        }
        if !found.iter().any(|seen| seen == line) {
            found.push(line.to_owned());
        }
    }
    found
}

/// The name a byline line gives, or `None` when the line is not a byline.
fn byline_name(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    if line.contains('©') || lower.starts_with("copyright") || lower.starts_with("(c) ") {
        // "© 2024 Jane Doe. All rights reserved.": the words after the
        // mark and the years, up to "all rights".
        let name: Vec<&str> = line
            .split_whitespace()
            .filter(|word| {
                let letters = letters_lower(word, false);
                !(letters.is_empty() || letters == "copyright" || letters == "c")
            })
            .take_while(|word| !word.eq_ignore_ascii_case("all"))
            .collect();
        let name = name.join(" ");
        let name = name.trim_end_matches(['.', ',']);
        return (!name.is_empty()).then(|| name.to_owned());
    }
    let cue = BYLINE_CUES.iter().find(|cue| lower.starts_with(**cue))?;
    let rest = line.get(cue.len()..)?.trim_start();
    let mut name: Vec<&str> = Vec::new();
    for word in rest.split_whitespace() {
        let first = word.chars().next()?;
        if is_separator(first) || !word.chars().any(char::is_alphabetic) {
            break;
        }
        if first.is_uppercase() || (NAME_PARTICLES.contains(&word) && !name.is_empty()) {
            name.push(word);
        } else {
            // A lower-case word that is not a particle: the line is prose.
            return None;
        }
        if word.ends_with([',', ';']) {
            break;
        }
    }
    while name
        .last()
        .is_some_and(|word| NAME_PARTICLES.contains(word))
    {
        name.pop();
    }
    let name = name.join(" ");
    let name = name.trim_end_matches([',', ';']);
    (!name.is_empty()).then(|| name.to_owned())
}

fn is_separator(c: char) -> bool {
    matches!(c, '|' | '·' | '•' | '—' | '–' | '(' | '[' | '/')
}

fn is_fence(line: &str) -> bool {
    line.starts_with("```") || line.starts_with("~~~")
}

/// The registrable domain's first label when a byline names it.
fn name_in_domain(bylines: &[String], origin: &Origin) -> Option<String> {
    let label = origin.registrable_domain.as_deref()?.split('.').next()?;
    let label_letters = letters_lower(label, false);
    if label_letters.is_empty() {
        return None;
    }
    bylines
        .iter()
        .filter_map(|line| byline_name(line))
        .any(|name| {
            letters_lower(&name, false).contains(&label_letters)
                || name.split_whitespace().any(|token| {
                    let token = letters_lower(token, false);
                    token.chars().count() >= NAME_TOKEN_LETTERS && label_letters.contains(&token)
                })
        })
        .then(|| label.to_owned())
}

/// Same-site links whose address or anchor names an about page.
fn about_links(links: &[OutboundLink], origin: &Origin) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for link in links {
        let Ok(target) = Origin::of_url(&link.url) else {
            continue;
        };
        if !same_site(&target, origin) {
            continue;
        }
        let path = Url::parse(&link.url)
            .map(|url| url.path().to_owned())
            .unwrap_or_default();
        let names_about = [path.as_str(), link.anchor.as_str()].iter().any(|text| {
            letters_lower(text, true)
                .split_whitespace()
                .any(|word| ABOUT_WORDS.contains(&word))
        });
        if names_about && !found.contains(&link.url) {
            found.push(link.url.clone());
        }
    }
    found
}

fn same_site(target: &Origin, origin: &Origin) -> bool {
    origin.host.is_some() && target.key == origin.key
}

/// A DOI (`10.NNNN/…`) or an arXiv identifier (`NNNN.NNNNN`) in a link,
/// normalised to lower case.
fn scholarly_identifier(url: &str) -> Option<String> {
    let lower = url.to_lowercase();
    if let Some(at) = lower.find("10.") {
        let rest = &lower[at + 3..];
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        let after = &rest[digits..];
        if digits >= 4 && after.starts_with('/') && after.len() > 1 {
            return Some(format!("doi:{}", &lower[at..]));
        }
    }
    if !lower.contains("arxiv") {
        return None;
    }
    lower
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find(|token| {
            let mut parts = token.split('.');
            matches!(
                (parts.next(), parts.next(), parts.next()),
                (Some(year), Some(number), None)
                    if year.len() == 4 && (number.len() == 4 || number.len() == 5)
            )
        })
        .map(|id| format!("arxiv:{id}"))
}

/// Whether a URL's path has a numbered segment such as `08-tool-use`: a
/// run of digits, a separator, then a letter.
fn numbered_path(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    url.path().split('/').any(|segment| {
        let digits = segment.chars().take_while(char::is_ascii_digit).count();
        let mut rest = segment[digits..].chars();
        digits > 0
            && matches!(rest.next(), Some('-' | '_' | '.'))
            && rest.next().is_some_and(char::is_alphabetic)
    })
}

/// Lower-case letters of `text`; with `spaced`, every other character
/// becomes a space so words stay apart.
fn letters_lower(text: &str, spaced: bool) -> String {
    text.chars()
        .filter_map(|c| {
            if c.is_alphabetic() {
                Some(c.to_lowercase().next().unwrap_or(c))
            } else {
                spaced.then_some(' ')
            }
        })
        .collect()
}

/// The deterministic summary of [`ContentEvidence`] a standing assessment
/// keeps (§4.2): copied from the judgement's input, never from the model's
/// answer, so the level procedure, the source view and the gate read the
/// same facts without the text in hand.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkEvidence {
    pub external_domains: usize,
    pub scholarly_identifiers: usize,
    pub restricted_registry_links: usize,
    pub headings: usize,
    pub has_abstract_or_references: bool,
    pub numbered_path: bool,
    pub bylines: usize,
    pub name_in_domain: bool,
}

impl WorkEvidence {
    pub fn of(content: &ContentEvidence) -> Self {
        Self {
            external_domains: content.citations.external_domains,
            scholarly_identifiers: content.citations.scholarly_identifiers,
            restricted_registry_links: content.citations.restricted_registry_links,
            headings: content.work.headings,
            has_abstract_or_references: content.work.has_abstract_or_references,
            numbered_path: content.work.numbered_path,
            bylines: content.bylines.len(),
            name_in_domain: content.name_in_domain.is_some(),
        }
    }

    /// The work stands on sources: independent origins as many as the
    /// corroboration rule asks for, or one scholarly identifier, or one
    /// restricted-registry link.
    pub fn stands_on_sources(&self) -> bool {
        self.external_domains >= super::sufficiency::CORROBORATING_ORIGINS
            || self.scholarly_identifiers > 0
            || self.restricted_registry_links > 0
    }

    /// The work is sectioned: more than one heading (the least a sectioned
    /// text has), or a paper's abstract or references, or a numbered
    /// chapter path.
    pub fn structured(&self) -> bool {
        self.headings > 1 || self.has_abstract_or_references || self.numbered_path
    }
}

#[cfg(test)]
#[path = "content_evidence_tests.rs"]
mod tests;
