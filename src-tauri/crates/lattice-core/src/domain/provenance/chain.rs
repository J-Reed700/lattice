//! Citation chains (§8): which links in a supporting passage to follow, in
//! what order, to reach the source a secondary page repeats.

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::authority::TrustLevel;
use super::independence::ClusterId;
use super::signals::OutboundLink;

/// What is known about a link's origin for the claim's subject, for
/// ranking: the owner's page first, then a known authoritative one, then
/// the nearest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkStanding {
    Unknown,
    Authoritative,
    Owner,
}

/// One link worth following, with why it ranked where it did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CitationCandidate {
    pub url: String,
    pub anchor: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub standing: LinkStanding,
    /// Bytes between the link and the passage; 0 inside it. A footnote
    /// target counts as 0: the passage names it.
    pub distance: usize,
}

/// The links worth following from a supporting passage (§8), best first:
/// links inside the passage, the nearest link before and after it in the
/// same paragraph, and footnote targets (links anywhere whose anchor is a
/// marker like `[3]` or `[^3]` that the passage carries). Ranked by what is
/// known of the link's origin for the claim's subject (`standing`), then by
/// distance from the passage. `span` is the passage's bytes in `text`.
pub fn citation_candidates(
    text: &str,
    span: (usize, usize),
    links: &[OutboundLink],
    standing: impl Fn(&OutboundLink) -> LinkStanding,
) -> Vec<CitationCandidate> {
    let (start, end) = (span.0.min(text.len()), span.1.min(text.len()));
    let paragraph_start = text
        .get(..start)
        .and_then(|before| before.rfind("\n\n"))
        .map_or(0, |at| at + 2);
    let paragraph_end = text
        .get(end..)
        .and_then(|after| after.find("\n\n"))
        .map_or(text.len(), |at| end + at);
    let mut found: Vec<(&OutboundLink, usize)> = Vec::new();
    for link in links {
        if link.start_byte >= start && link.start_byte < end {
            found.push((link, 0));
        }
    }
    let before = links
        .iter()
        .filter(|link| link.start_byte >= paragraph_start && link.end_byte <= start)
        .max_by_key(|link| link.end_byte);
    if let Some(link) = before {
        found.push((link, start - link.end_byte));
    }
    let after = links
        .iter()
        .filter(|link| link.start_byte >= end && link.end_byte <= paragraph_end)
        .min_by_key(|link| link.start_byte);
    if let Some(link) = after {
        found.push((link, link.start_byte - end));
    }
    let markers = footnote_markers(text.get(start..end).unwrap_or_default());
    for link in links {
        let anchor = link
            .anchor
            .trim_matches(|c: char| c == '[' || c == ']' || c == '^' || c.is_whitespace());
        if !anchor.is_empty() && markers.iter().any(|marker| marker == anchor) {
            found.push((link, 0));
        }
    }

    let mut candidates: Vec<CitationCandidate> = Vec::new();
    for (link, distance) in found {
        match candidates.iter_mut().find(|c| c.url == link.url) {
            Some(existing) if existing.distance > distance => existing.distance = distance,
            Some(_) => {}
            None => candidates.push(CitationCandidate {
                url: link.url.clone(),
                anchor: link.anchor.clone(),
                start_byte: link.start_byte,
                end_byte: link.end_byte,
                standing: standing(link),
                distance,
            }),
        }
    }
    candidates.sort_by(|a, b| {
        b.standing
            .cmp(&a.standing)
            .then_with(|| a.distance.cmp(&b.distance))
            .then_with(|| a.start_byte.cmp(&b.start_byte))
    });
    candidates
}

/// Whether a chain goes on past a fetched target (§8): only from a target
/// below Primary whose origin cluster is new to the claim. A hop that adds
/// no new cluster ends the chain, so chains end without a depth limit.
pub fn follow_further(
    target_level: TrustLevel,
    target_cluster: &ClusterId,
    seen: &[ClusterId],
) -> bool {
    target_level < TrustLevel::Primary && !seen.contains(target_cluster)
}

/// The footnote markers in a passage: `[3]`, `[^3]`, `[a]`.
fn footnote_markers(passage: &str) -> Vec<String> {
    let Ok(pattern) = Regex::new(r"\[\^?([0-9]{1,3}|[a-z])\]") else {
        return Vec::new();
    };
    pattern
        .captures_iter(passage)
        .filter_map(|captures| captures.get(1).map(|m| m.as_str().to_owned()))
        .collect()
}

#[cfg(test)]
#[path = "chain_tests.rs"]
mod tests;
