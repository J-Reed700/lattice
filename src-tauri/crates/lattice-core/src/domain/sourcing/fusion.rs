//! Reciprocal-rank fusion of the finders' ranked lists, with dedupe by
//! canonical key, and the origin-diversity pass that runs after it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};

use super::canonical::{lead_key, CanonicalKey};
use super::descriptor::FinderId;
use super::lead::Lead;
use super::need::Origin;

/// One finder's ranked leads, and the wave it ran in.
#[derive(Debug, Clone, Copy)]
pub struct RankedList<'a> {
    pub finder: &'a FinderId,
    /// Zero-based wave index; earlier waves win ties.
    pub wave: usize,
    pub leads: &'a [Lead],
}

/// The RRF contribution of a 1-based rank.
pub fn rrf_term(k: u32, rank: usize) -> f64 {
    let rank = u32::try_from(rank).unwrap_or(u32::MAX);
    1.0 / (f64::from(k) + f64::from(rank))
}

/// Fuses ranked lists: each lead scores Σ 1/(k + rank) over the lists that
/// hold it (Cormack et al. 2009). Leads that share a canonical key fold into
/// one with the union of `found_by`. Ties break by owner match, then lead
/// kind precedence, then the earliest wave, then the best single rank, then
/// the key, so the order is total and repeatable.
///
/// A lead's rank in a list is its own `found_by` entry for that finder when
/// it has one (a page-2 lead ranks 11, not 1), else its position.
pub fn rrf(lists: &[RankedList<'_>], k: u32, owner: Option<&Origin>) -> Vec<Lead> {
    rrf_scored(lists, k, owner)
        .into_iter()
        .map(|(lead, _)| lead)
        .collect()
}

/// [`rrf`] with each fused lead's score, for logs and tests.
pub fn rrf_scored(lists: &[RankedList<'_>], k: u32, owner: Option<&Origin>) -> Vec<(Lead, f64)> {
    let mut fused: BTreeMap<CanonicalKey, Candidate> = BTreeMap::new();
    for (list_index, list) in lists.iter().enumerate() {
        let mut seen_in_list = HashSet::new();
        for (position, lead) in list.leads.iter().enumerate() {
            let key = lead_key(lead);
            if !seen_in_list.insert(key.clone()) {
                continue; // a finder's own duplicate counts once, at its best rank
            }
            let rank = lead
                .found_by
                .iter()
                .find(|(finder, _)| finder == list.finder)
                .map_or(position + 1, |(_, rank)| *rank);
            let occurrence = Occurrence {
                lead,
                finder: list.finder,
                rank,
                wave: list.wave,
                list_index,
            };
            fused
                .entry(key)
                .or_insert_with(Candidate::default)
                .occurrences
                .push(occurrence);
        }
    }

    let mut scored: Vec<Scored> = fused
        .into_iter()
        .filter_map(|(key, candidate)| candidate.score(key, k, owner))
        .collect();
    scored.sort_by(compare);
    scored
        .into_iter()
        .map(|scored| (scored.lead, scored.score))
        .collect()
}

/// Origin diversity after fusion: the first lead from each host keeps its
/// place and later leads from a host already seen move behind them. A lead
/// on the subject owner's origin is never demoted.
pub fn diversify(leads: Vec<Lead>, owner: Option<&Origin>) -> Vec<Lead> {
    let mut seen_hosts = HashSet::new();
    let mut first = Vec::with_capacity(leads.len());
    let mut overflow = Vec::new();
    for lead in leads {
        let host = lead
            .url
            .host_str()
            .map(|host| host.trim_start_matches("www.").to_ascii_lowercase());
        let new_host = host.is_some_and(|host| seen_hosts.insert(host));
        if new_host || owner_matches(&lead, owner) {
            first.push(lead);
        } else {
            overflow.push(lead);
        }
    }
    first.extend(overflow);
    first
}

/// True when the lead sits on the subject owner's origin.
pub fn owner_matches(lead: &Lead, owner: Option<&Origin>) -> bool {
    owner.is_some_and(|owner| owner.covers(&lead.url))
}

struct Occurrence<'a> {
    lead: &'a Lead,
    finder: &'a FinderId,
    rank: usize,
    wave: usize,
    list_index: usize,
}

#[derive(Default)]
struct Candidate<'a> {
    occurrences: Vec<Occurrence<'a>>,
}

struct Scored {
    lead: Lead,
    score: f64,
    owner_match: bool,
    precedence: u8,
    earliest_wave: usize,
    best_rank: usize,
    key: CanonicalKey,
}

impl Candidate<'_> {
    /// `None` only for a candidate without occurrences, which `rrf` never
    /// builds.
    fn score(mut self, key: CanonicalKey, k: u32, owner: Option<&Origin>) -> Option<Scored> {
        // Sum in ascending rank so equal rank sets give bit-equal scores.
        let mut ranks: Vec<usize> = self.occurrences.iter().map(|o| o.rank).collect();
        ranks.sort_unstable();
        let score = ranks.iter().map(|rank| rrf_term(k, *rank)).sum();

        // The representative: highest kind precedence (a live page over its
        // archived copy), then best rank, then the earliest list.
        self.occurrences.sort_by(|a, b| {
            b.lead
                .kind
                .precedence()
                .cmp(&a.lead.kind.precedence())
                .then(a.rank.cmp(&b.rank))
                .then(a.list_index.cmp(&b.list_index))
        });
        let mut lead = self.occurrences.first()?.lead.clone();
        lead.found_by.clear();

        let mut by_rank: Vec<&Occurrence<'_>> = self.occurrences.iter().collect();
        by_rank.sort_by_key(|o| (o.rank, o.list_index));
        for other in &by_rank {
            fill_missing(&mut lead, other.lead);
        }

        let mut by_list: Vec<&Occurrence<'_>> = self.occurrences.iter().collect();
        by_list.sort_by_key(|o| (o.list_index, o.rank));
        for occurrence in by_list {
            if !lead
                .found_by
                .iter()
                .any(|(finder, _)| finder == occurrence.finder)
            {
                lead.found_by
                    .push((occurrence.finder.clone(), occurrence.rank));
            }
        }

        Some(Scored {
            owner_match: owner_matches(&lead, owner),
            precedence: lead.kind.precedence(),
            earliest_wave: self
                .occurrences
                .iter()
                .map(|o| o.wave)
                .min()
                .unwrap_or(usize::MAX),
            best_rank: ranks.first().copied().unwrap_or(usize::MAX),
            score,
            lead,
            key,
        })
    }
}

fn fill_missing(lead: &mut Lead, other: &Lead) {
    if lead.title.is_none() {
        lead.title.clone_from(&other.title);
    }
    if lead.snippet.is_none() {
        lead.snippet.clone_from(&other.snippet);
    }
    if lead.published.is_none() {
        lead.published = other.published;
    }
    if lead.owner.is_none() {
        lead.owner.clone_from(&other.owner);
    }
    if lead.rel_canonical.is_none() {
        lead.rel_canonical.clone_from(&other.rel_canonical);
    }
    if lead.terms.is_none() {
        lead.terms.clone_from(&other.terms);
    }
}

fn compare(a: &Scored, b: &Scored) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then(b.owner_match.cmp(&a.owner_match))
        .then(b.precedence.cmp(&a.precedence))
        .then(a.earliest_wave.cmp(&b.earliest_wave))
        .then(a.best_rank.cmp(&b.best_rank))
        .then_with(|| a.key.cmp(&b.key))
}

#[cfg(test)]
#[path = "fusion_tests.rs"]
mod tests;
