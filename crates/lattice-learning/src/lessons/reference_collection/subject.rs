//! What a claim is about, read against the collection's own vocabulary: its
//! distinctive content terms, those no more common across the passages than
//! the median of its own terms, so words the whole subject shares do not
//! count. Terms are looked up as the store's tokenizer keeps words.

use super::*;
use lattice_index::passage_index::{indexed_term, query_terms};
use std::collections::{BTreeSet, HashMap};

/// Function words that match nearly every passage; never a content term.
const FUNCTION_WORDS: &[&str] = &[
    "a", "about", "after", "all", "also", "an", "and", "any", "are", "as", "at", "be", "been",
    "before", "but", "by", "can", "could", "did", "do", "does", "each", "for", "from", "had",
    "has", "have", "how", "if", "in", "into", "is", "it", "its", "may", "more", "most", "must",
    "no", "not", "of", "on", "only", "or", "other", "should", "so", "some", "such", "than", "that",
    "the", "their", "them", "then", "there", "these", "they", "this", "those", "to", "under", "up",
    "use", "used", "uses", "using", "was", "we", "were", "what", "when", "where", "which", "while",
    "who", "will", "with", "without", "would", "you", "your",
];

/// What FTS5 gives a term whose inverse document frequency is not above zero.
const MIN_WEIGHT: f64 = 1e-6;

/// Minimum fraction of a long query's total IDF weight a passage must carry to
/// stay above the relevance cutoff.
const MIN_LONG_QUERY_IDF_SHARE: f64 = 0.10;
/// Single high-specificity IDF share that keeps a passage even when few total
/// query terms match.
const HIGH_IDF_SHARE_BYPASS: f64 = 0.35;

/// The words of `text` a keyword match should rest on, as the index keeps
/// them: not function words.
pub(super) fn content_terms(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .filter(|word| !FUNCTION_WORDS.contains(&word.as_str()))
        .map(|word| indexed_term(&word))
}

/// `query` with pure function words and one-character tokens removed while
/// preserving original word order, so bounded FTS queries keep domain terms.
pub(crate) fn content_query(query: &str) -> String {
    let mut words = Vec::new();
    for word in query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|word| word.chars().count() >= 2)
    {
        let lower = word.to_lowercase();
        if !FUNCTION_WORDS.contains(&lower.as_str()) {
            words.push(word);
        }
    }
    words.join(" ")
}

/// How many of `query`'s content terms `text` holds, each counted once,
/// read as the course index reads words (whole words, lower case, Latin
/// diacritics removed): how well a stretch of a source matches a claim.
pub(crate) fn shared_terms(query: &str, text: &str) -> usize {
    let held: HashSet<String> = content_terms(text).collect();
    content_terms(query)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|term| held.contains(term))
        .count()
}

/// The terms that say what one query is about.
#[derive(Debug, Clone, Default)]
pub(crate) struct Subject {
    distinctive: HashSet<String>,
    /// The query's terms are in the collection, but each is in every
    /// passage: nothing tells one passage from another.
    undivided: bool,
}

impl Subject {
    /// The distinctive terms among `counts` (each term's passage count) in a
    /// collection of `passages`. Before the median is taken, two kinds of
    /// term are left out because no page can tell anything by them: a term
    /// in no passage, such as a name a lesson invented for its example (no
    /// page can contain it, so it neither keeps a walk going nor lowers the
    /// bar the real terms must meet), and a term in every passage.
    fn of(counts: impl IntoIterator<Item = (String, usize)>, passages: usize) -> Self {
        let counts: Vec<(String, usize)> = counts.into_iter().collect();
        let mut wanted: Vec<(usize, String)> = counts
            .iter()
            .filter(|(_, count)| *count > 0 && *count < passages)
            .map(|(term, count)| (*count, term.clone()))
            .collect();
        wanted.sort();
        let Some(&(median, _)) = wanted.get(wanted.len().saturating_sub(1) / 2) else {
            return Self {
                distinctive: HashSet::new(),
                undivided: counts.iter().any(|(_, count)| *count > 0),
            };
        };
        Self {
            distinctive: wanted
                .into_iter()
                .filter(|(count, _)| *count <= median)
                .map(|(_, term)| term)
                .collect(),
            undivided: false,
        }
    }

    /// Whether the query's terms are in the collection but each is in every
    /// passage, as in a course of one short source: no passage is more on the
    /// subject than another, so the subject sets no list's end and the whole
    /// ranking is on it (see `claim_list`).
    pub(crate) fn undivided(&self) -> bool {
        self.undivided
    }

    /// Whether `term`, as the index keeps words, is one of the distinctive
    /// terms.
    pub(super) fn has(&self, term: &str) -> bool {
        self.distinctive.contains(term)
    }

    /// Whether `text` mentions what the query is about.
    pub(crate) fn mentioned_in(&self, text: &str) -> bool {
        !self.distinctive.is_empty()
            && content_terms(text).any(|term| self.distinctive.contains(&term))
    }
}

/// BM25 inverse document frequency for a term present in `holding` out of
/// `passages` total passages, floored at `MIN_WEIGHT`.
fn idf_weight(holding: usize, passages: f64) -> f64 {
    let holding = f64::from(u32::try_from(holding).unwrap_or(u32::MAX));
    let idf = ((passages - holding + 0.5) / (holding + 0.5)).ln();
    if idf > 0.0 {
        idf
    } else {
        MIN_WEIGHT
    }
}

/// Unique indexed content terms in `query`, sorted deterministically.
fn distinct_content_terms(query: &str) -> Vec<String> {
    content_terms(query)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

impl ReferenceCollection<'_> {
    async fn term_counts_and_total(
        &self,
        terms: &[String],
    ) -> Result<(HashMap<String, usize>, usize)> {
        let store = self.store().await?;
        let counts = store.term_counts(terms).await?;
        let passages = store.counts("").await?.passages_total as usize;
        Ok((counts, passages))
    }

    /// The weight of each of `query`'s terms in this collection, as the index keeps words: its `bm25` inverse document
    /// frequency over the collection's passages, floored at a small positive number as FTS5 does. A term in no
    /// passage has none.
    pub(crate) async fn term_weights(&self, query: &str) -> Result<HashMap<String, f64>> {
        let terms: Vec<String> = query_terms(query)
            .iter()
            .flat_map(|term| term.split(|c: char| !c.is_alphanumeric()))
            .filter(|word| !word.is_empty())
            .map(indexed_term)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if terms.is_empty() {
            return Ok(HashMap::new());
        }
        let (counts, passages) = self.term_counts_and_total(&terms).await?;
        let total = f64::from(u32::try_from(passages).unwrap_or(u32::MAX));
        Ok(counts
            .into_iter()
            .filter(|(_, holding)| *holding > 0)
            .map(|(term, holding)| (term, idf_weight(holding, total)))
            .collect())
    }

    /// What `query` is about in this collection: its distinctive content
    /// terms, counted across the collection's passages.
    pub(crate) async fn subject_of(&self, query: &str) -> Result<Subject> {
        let terms = distinct_content_terms(query);
        if terms.is_empty() {
            return Ok(Subject::default());
        }
        let (counts, passages) = self.term_counts_and_total(&terms).await?;
        Ok(Subject::of(counts, passages))
    }

    /// Relevance filter for [`ReferenceCollection::retrieve_relevant_within`]:
    /// combines `Subject` for short queries (up to 6 indexed content terms) with
    /// IDF-weighted multi-term coverage for longer multi-clause queries so
    /// generic prose words neither drop core topic terms nor admit noisy
    /// single-word matches.
    pub(super) async fn relevance_of(&self, query: &str) -> Result<Relevance> {
        let terms = distinct_content_terms(query);
        if terms.is_empty() {
            return Ok(Relevance::default());
        }
        let (counts, passages) = self.term_counts_and_total(&terms).await?;
        let subject = Subject::of(counts.clone(), passages);
        let total = f64::from(u32::try_from(passages).unwrap_or(u32::MAX));
        let weights: HashMap<String, f64> = counts
            .into_iter()
            .filter(|(_, count)| *count > 0 && *count < passages)
            .map(|(term, count)| (term, idf_weight(count, total)))
            .collect();
        let total_weight: f64 = weights.values().sum();
        Ok(Relevance {
            subject,
            weights,
            total_weight,
        })
    }
}

/// Collection-calibrated lexical relevance gate for retrieved passages.
#[derive(Debug, Clone, Default)]
pub(super) struct Relevance {
    subject: Subject,
    weights: HashMap<String, f64>,
    total_weight: f64,
}

impl Relevance {
    pub(super) fn matches(&self, text: &str) -> bool {
        if self.subject.undivided() {
            return true;
        }
        if self.weights.is_empty() {
            return false;
        }
        if self.weights.len() <= 6 {
            return self.subject.mentioned_in(text);
        }
        let held: HashSet<String> = content_terms(text).collect();
        let mut hits = 0usize;
        let mut matched_weight = 0.0_f64;
        for (term, weight) in &self.weights {
            if held.contains(term) {
                hits += 1;
                matched_weight += *weight;
            }
        }
        let share = if self.total_weight > 0.0 {
            matched_weight / self.total_weight
        } else {
            0.0
        };
        let min_hits = if self.weights.len() <= 14 { 2 } else { 3 };
        (hits >= min_hits && share >= MIN_LONG_QUERY_IDF_SHARE) || share >= HIGH_IDF_SHARE_BYPASS
    }
}
