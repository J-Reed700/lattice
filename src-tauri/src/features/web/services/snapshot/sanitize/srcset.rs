//! Choosing one image from a `srcset`: parse the candidates, then take the
//! widest one within the size cap, or the densest within the density cap.

use super::*;

/// One `srcset` candidate.
#[derive(Debug, PartialEq)]
struct SrcsetCandidate {
    pub url: String,
    pub width: Option<u32>,
    pub density: Option<f32>,
}

/// Parse `srcset` the way the HTML spec does: a URL runs to whitespace (a
/// trailing comma ends it), descriptors run to a comma outside parentheses.
/// URLs may contain commas (`data:`, image CDNs).
fn parse_srcset(srcset: &str) -> Vec<SrcsetCandidate> {
    let mut candidates = Vec::new();
    let mut rest = srcset;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if rest.is_empty() {
            break;
        }
        let url_end = rest
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let raw_url = &rest[..url_end];
        rest = &rest[url_end..];
        let url = raw_url.trim_end_matches(',');
        let mut descriptors = "";
        if url.len() == raw_url.len() {
            let mut parens = 0usize;
            let end = rest
                .char_indices()
                .find(|&(_, c)| {
                    match c {
                        '(' => parens += 1,
                        ')' => parens = parens.saturating_sub(1),
                        ',' if parens == 0 => return true,
                        _ => {}
                    }
                    false
                })
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            descriptors = &rest[..end];
            rest = &rest[end..];
        }
        candidates.push(srcset_candidate(url, descriptors));
    }
    candidates
}

fn srcset_candidate(url: &str, descriptors: &str) -> SrcsetCandidate {
    let mut candidate = SrcsetCandidate {
        url: url.to_string(),
        width: None,
        density: None,
    };
    for descriptor in descriptors.split_ascii_whitespace() {
        let (number, unit) = descriptor.split_at(descriptor.len().saturating_sub(1));
        match unit {
            "w" => candidate.width = number.parse().ok(),
            "x" => candidate.density = number.parse().ok(),
            _ => {}
        }
    }
    candidate
}

/// The largest `w` ≤ 1600, else the largest `x` ≤ 2 (no descriptor is 1x), else the first.
pub(in super::super) fn choose_srcset(srcset: &str) -> Option<String> {
    let candidates = parse_srcset(srcset);
    let by_width = candidates
        .iter()
        .filter(|c| c.width.is_some_and(|w| w <= SRCSET_MAX_W))
        .max_by_key(|c| c.width);
    let by_density = || {
        candidates
            .iter()
            .filter(|c| c.width.is_none())
            .map(|c| (c, c.density.unwrap_or(1.0)))
            .filter(|(_, x)| *x <= SRCSET_MAX_X)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(c, _)| c)
    };
    by_width
        .or_else(by_density)
        .or(candidates.first())
        .map(|c| c.url.clone())
}
