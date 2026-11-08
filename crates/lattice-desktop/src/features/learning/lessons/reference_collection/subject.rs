//! What a claim is about, read against the collection's own vocabulary: its
//! distinctive content terms, those no more common across the passages than
//! the median of its own terms, so words the whole subject shares do not
//! count. Terms are looked up as the store's tokenizer keeps words.

use super::*;
use crate::infrastructure::passage_index::indexed_term;
use std::collections::BTreeSet;

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

/// The words of `text` a keyword match should rest on, as the index keeps
/// them: not function words.
pub(super) fn content_terms(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .filter(|word| !FUNCTION_WORDS.contains(&word.as_str()))
        .map(|word| indexed_term(&word))
}

/// How many of `query`'s content terms `text` holds, each counted once,
/// read as the course index reads words (whole words, lower case, Latin
/// diacritics removed): how well a stretch of a source matches a claim.
pub(in crate::features::learning) fn shared_terms(query: &str, text: &str) -> usize {
    let held: HashSet<String> = content_terms(text).collect();
    content_terms(query)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|term| held.contains(term))
        .count()
}

/// The terms that say what one query is about.
#[derive(Debug, Clone, Default)]
pub(in crate::features::learning) struct Subject {
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
    pub(in crate::features::learning) fn undivided(&self) -> bool {
        self.undivided
    }

    /// Whether `term`, as the index keeps words, is one of the distinctive
    /// terms.
    pub(super) fn has(&self, term: &str) -> bool {
        self.distinctive.contains(term)
    }

    /// Whether `text` mentions what the query is about.
    pub(in crate::features::learning) fn mentioned_in(&self, text: &str) -> bool {
        !self.distinctive.is_empty()
            && content_terms(text).any(|term| self.distinctive.contains(&term))
    }
}

impl ReferenceCollection<'_> {
    /// What `query` is about in this collection: its distinctive content
    /// terms, counted across the collection's passages.
    pub(in crate::features::learning) async fn subject_of(&self, query: &str) -> Result<Subject> {
        let terms: Vec<String> = content_terms(query)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if terms.is_empty() {
            return Ok(Subject::default());
        }
        let store = self.store().await?;
        let counts = store.term_counts(&terms).await?;
        let passages = store.counts("").await?.passages_total as usize;
        Ok(Subject::of(counts, passages))
    }
}
