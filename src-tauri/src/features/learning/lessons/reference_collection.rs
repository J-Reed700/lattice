//! Retrieval over a program's immutable source snapshots.
//!
//! A snapshot captured from a library document is ranked by the library
//! itself: its chunks, vectors and hybrid fusion, confined to the program's
//! documents. Web pages and pasted text have no library copy, so they are
//! ranked by keyword alone. Every hit is mapped back onto the snapshot's
//! bytes, so evidence is always an exact substring of what was captured.
use crate::features::learning::dto::LearningSourceDto;
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use lattice_core::application::ports::{LibraryChunk, LibraryPassagesPort};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

const CHUNK_CHARS: usize = 1200;
/// How far `retrieve` widens a ranked chunk on each side, toward paragraph
/// edges.
const WIDEN_BYTES: usize = 300;
/// About the longest passage `retrieve` returns: one chunk widened on both
/// sides. Callers size retrieval to a token budget with it.
pub(in crate::features::learning) const PASSAGE_CHARS: usize = CHUNK_CHARS + 2 * WIDEN_BYTES;
const STEP_CHARS: usize = 900;
/// Reciprocal-rank constant for merging library and keyword rankings.
const RANK_K: usize = 60;
/// How many passages a filled retrieval asks for first; it pages deeper,
/// doubling, while everything found still fits.
const FIRST_PAGE: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferencePassage {
    /// Immutable source version ID, also used by lesson citations.
    pub source_id: String,
    pub text: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub retrieval_kind: String,
    pub score: f64,
}

/// A keyword-ranked passage of a snapshot the library cannot rank.
struct Chunk {
    source: usize,
    start: usize,
    end: usize,
    terms: HashMap<String, usize>,
    length: usize,
}

/// Where one library chunk sits inside a captured snapshot.
#[derive(Clone, Copy)]
struct Span {
    source: usize,
    start: usize,
    end: usize,
}

pub struct ReferenceCollection<'a> {
    pub sources: Vec<LearningSourceDto>,
    chunks: Vec<Chunk>,
    frequencies: HashMap<String, usize>,
    /// How many passages of the whole collection, library-ranked snapshots
    /// included, contain each term.
    subject_frequencies: HashMap<String, usize>,
    /// How many passages the whole collection has.
    subject_passages: usize,
    average_length: f64,
    library: Option<&'a dyn LibraryPassagesPort>,
    /// Library chunk ID to its bytes in a snapshot.
    library_spans: HashMap<String, Span>,
    /// The library documents behind the snapshots the library ranks.
    library_documents: HashSet<String>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
/// The longest prefix of `passages` that `fits` accepts; `fits` holds for
/// every shorter prefix of one it accepts.
fn longest_fitting_prefix(
    passages: &[ReferencePassage],
    fits: &impl Fn(&[ReferencePassage]) -> bool,
) -> usize {
    let (mut low, mut high) = (0, passages.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        if fits(passages.get(..middle).unwrap_or_default()) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

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

/// The words of `text` a keyword match should rest on: not function words.
fn content_terms(text: &str) -> impl Iterator<Item = String> + '_ {
    terms_iter(text).filter(|word| !FUNCTION_WORDS.contains(&word.as_str()))
}

fn terms_iter(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
}

fn terms(text: &str) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for word in text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
    {
        *counts.entry(word.to_lowercase()).or_default() += 1;
    }
    counts
}
fn spans(text: &str) -> Vec<(usize, usize)> {
    // Scan one passage at a time. Precomputing every character boundary would
    // allocate one machine word per character across the entire source.
    let mut result = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = text.len();
        let mut next = text.len();
        for (ordinal, (offset, _)) in text[start..].char_indices().enumerate() {
            if ordinal == STEP_CHARS {
                next = start + offset;
            }
            if ordinal == CHUNK_CHARS {
                end = start + offset;
                break;
            }
        }
        result.push((start, end));
        if end == text.len() {
            break;
        }
        start = next;
    }
    result
}

/// Locate each library chunk, in order, inside the snapshot captured from
/// it. `None` when any chunk is missing: the library document changed after
/// the capture, so the library can no longer rank this snapshot.
fn locate_chunks(text: &str, chunks: &[LibraryChunk]) -> Option<Vec<(String, usize, usize)>> {
    let mut cursor = 0;
    let mut located = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let needle = chunk.text.trim();
        if needle.is_empty() {
            continue;
        }
        let start = cursor + text.get(cursor..)?.find(needle)?;
        let end = start + needle.len();
        located.push((chunk.id.clone(), start, end));
        cursor = end;
    }
    (!located.is_empty()).then_some(located)
}

impl<'a> ReferenceCollection<'a> {
    /// Keyword ranking over every source, with no library.
    pub fn lexical(sources: &[LearningSourceDto]) -> Result<Self> {
        Ok(Self::keyword(sources, &HashSet::new()))
    }

    /// Keyword passages for every source not in `ranked_by_library`.
    fn keyword(sources: &[LearningSourceDto], ranked_by_library: &HashSet<usize>) -> Self {
        let mut chunks = Vec::new();
        let mut frequencies = HashMap::new();
        let mut subject_frequencies: HashMap<String, usize> = HashMap::new();
        let mut subject_passages = 0;
        for (source, value) in sources.iter().enumerate() {
            for (start, end) in spans(&value.excerpt) {
                subject_passages += 1;
                let counts = terms(&value.excerpt[start..end]);
                for term in counts.keys() {
                    *subject_frequencies.entry(term.clone()).or_default() += 1;
                }
                // The library ranks its own snapshots; only the rest are
                // keyword passages.
                if ranked_by_library.contains(&source) {
                    continue;
                }
                for term in counts.keys() {
                    *frequencies.entry(term.clone()).or_default() += 1;
                }
                chunks.push(Chunk {
                    source,
                    start,
                    end,
                    length: counts.values().sum(),
                    terms: counts,
                });
            }
        }
        let average_length = (chunks.iter().map(|c| c.length).sum::<usize>() as f64
            / chunks.len().max(1) as f64)
            .max(1.0);
        Self {
            sources: sources.to_vec(),
            chunks,
            frequencies,
            subject_frequencies,
            subject_passages,
            average_length,
            library: None,
            library_spans: HashMap::new(),
            library_documents: HashSet::new(),
        }
    }

    /// Rank snapshots captured from library documents through the library,
    /// and the rest by keyword. A document the library no longer holds as it
    /// was captured falls back to keywords for that snapshot only.
    pub async fn load(
        pool: &SqlitePool,
        program_id: &str,
        sources: &[LearningSourceDto],
        library: Option<&'a dyn LibraryPassagesPort>,
    ) -> Result<Self> {
        let Some(library) = library else {
            return Self::lexical(sources);
        };
        let documents = LearningSourceLibraryRepository::new(pool.clone())
            .library_documents(program_id)
            .await?;
        let mut library_spans = HashMap::new();
        let mut library_documents = HashSet::new();
        let mut ranked_by_library = HashSet::new();
        for (index, source) in sources.iter().enumerate() {
            let Some(document_id) = documents.get(&source.id) else {
                continue;
            };
            // The snapshot itself is always searchable, so a document the
            // library cannot serve right now (mid-import, unreadable) is
            // ranked by keyword instead of failing the whole collection.
            let document = match library.document_text(document_id).await {
                Ok(document) => document,
                Err(error) => {
                    tracing::warn!(%document_id, %error, "Library cannot serve a reference; ranking it by keyword");
                    None
                }
            };
            let Some(located) =
                document.and_then(|document| locate_chunks(&source.excerpt, &document.chunks))
            else {
                continue;
            };
            for (chunk_id, start, end) in located {
                library_spans.insert(
                    chunk_id,
                    Span {
                        source: index,
                        start,
                        end,
                    },
                );
            }
            library_documents.insert(document_id.clone());
            ranked_by_library.insert(index);
        }
        let mut collection = Self::keyword(sources, &ranked_by_library);
        collection.library = Some(library);
        collection.library_spans = library_spans;
        collection.library_documents = library_documents;
        Ok(collection)
    }

    /// `hybrid` once the library ranks at least one snapshot.
    pub fn mode(&self) -> &'static str {
        if self.ranks_through_library() {
            "hybrid"
        } else {
            "lexical_fallback"
        }
    }
    pub fn embedding_model(&self) -> Option<String> {
        self.library
            .filter(|_| self.ranks_through_library())
            .map(LibraryPassagesPort::model_identity)
    }
    fn ranks_through_library(&self) -> bool {
        self.library.is_some() && !self.library_documents.is_empty()
    }

    pub(in crate::features::learning) async fn reload(
        &self,
        pool: &SqlitePool,
        program_id: &str,
        sources: &[LearningSourceDto],
    ) -> Result<Self> {
        Self::load(pool, program_id, sources, self.library).await
    }

    /// Library hits and keyword hits come from disjoint sources, so they are
    /// merged by rank, each list contributing equally. Scores rank evidence
    /// only; they are never a factual-correctness confidence. The library is
    /// asked for more hits while overlaps keep `limit` from being reached.
    pub async fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<ReferencePassage>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.max(1);
        let keyword = self.keyword_ranking(query);
        let mut fetch = limit;
        loop {
            let mut library_spans = Vec::new();
            let mut exhausted = true;
            if let Some(library) = self.library.filter(|_| !self.library_documents.is_empty()) {
                let hits = library
                    .search(query, &self.library_documents, fetch)
                    .await?;
                exhausted = hits.len() < fetch;
                library_spans.extend(
                    hits.iter()
                        .filter_map(|hit| self.library_spans.get(&hit.chunk_id).copied()),
                );
            }
            let selected = self.merge(&library_spans, &keyword, limit)?;
            if selected.len() >= limit || exhausted {
                return Ok(selected);
            }
            fetch = fetch.saturating_mul(2);
        }
    }

    /// Merge library and keyword rankings by rank and widen each hit to
    /// paragraph edges, each source byte at most once, up to `limit`.
    fn merge(
        &self,
        library: &[Span],
        keyword: &[Span],
        limit: usize,
    ) -> Result<Vec<ReferencePassage>> {
        let mut ranked: Vec<(Span, f64, &'static str)> = Vec::new();
        for (rank, span) in library.iter().enumerate() {
            ranked.push((*span, 1.0 / (RANK_K + rank + 1) as f64, "hybrid"));
        }
        for (rank, span) in keyword.iter().enumerate() {
            ranked.push((*span, 1.0 / (RANK_K + rank + 1) as f64, "lexical_fallback"));
        }
        // Equal ranks keep library hits ahead; the sort is stable.
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut selected: Vec<ReferencePassage> = Vec::new();
        for (chunk, score, retrieval_kind) in ranked {
            let source = self
                .sources
                .get(chunk.source)
                .ok_or_else(|| invalid("Reference index contains an unknown source."))?;
            // Expand to adjacent paragraph boundaries where practical. Evidence
            // is always an exact substring, including significant whitespace.
            let mut start = chunk.start.saturating_sub(WIDEN_BYTES);
            while !source.excerpt.is_char_boundary(start) {
                start += 1;
            }
            let mut end = (chunk.end + WIDEN_BYTES).min(source.excerpt.len());
            while !source.excerpt.is_char_boundary(end) {
                end -= 1;
            }
            if let Some(i) = source.excerpt[start..chunk.start].find('\n') {
                start += i + 1;
            }
            if let Some(i) = source.excerpt[chunk.end..end].rfind('\n') {
                end = chunk.end + i;
            }
            if selected
                .iter()
                .any(|p| p.source_id == source.id && p.start_byte < end && start < p.end_byte)
            {
                continue;
            }
            selected.push(ReferencePassage {
                source_id: source.id.clone(),
                text: source.excerpt[start..end].to_owned(),
                start_byte: start,
                end_byte: end,
                retrieval_kind: retrieval_kind.into(),
                score,
            });
            if selected.len() >= limit {
                break;
            }
        }
        Ok(selected)
    }

    /// Passages for `query`, best first, for as long as `fits` accepts them:
    /// the caller's window decides how many, not a count. Retrieval pages
    /// deeper while every passage found so far still fits.
    pub(in crate::features::learning) async fn retrieve_within(
        &self,
        query: &str,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<Vec<ReferencePassage>> {
        self.fill(query, fits).await
    }

    /// The next page of verification evidence for `query`: ranked passages,
    /// each widened to its surrounding source text (see
    /// `verification_context`), that overlap none of `shown`, best first,
    /// while `fits` accepts them. Also returns the ranked hits the page was
    /// built from, so a quote can be located inside a widened passage.
    pub(in crate::features::learning) async fn verification_page(
        &self,
        query: &str,
        shown: &[ReferencePassage],
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<(Vec<ReferencePassage>, Vec<ReferencePassage>)> {
        self.verification_page_within(query, shown, |_| true, fits)
            .await
    }

    /// [`Self::verification_page`] built only from the ranked hits `within`
    /// accepts, in their ranked order.
    pub(in crate::features::learning) async fn verification_page_within(
        &self,
        query: &str,
        shown: &[ReferencePassage],
        within: impl Fn(&ReferencePassage) -> bool,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<(Vec<ReferencePassage>, Vec<ReferencePassage>)> {
        let mut depth = FIRST_PAGE;
        loop {
            let hits = self.retrieve(query, depth).await?;
            let exhausted = hits.len() < depth;
            let mut page: Vec<_> = self
                .verification_context(hits.iter().filter(|hit| within(hit)).cloned().collect())?
                .into_iter()
                .filter(|passage| {
                    !shown.iter().any(|seen| {
                        seen.source_id == passage.source_id
                            && seen.start_byte < passage.end_byte
                            && passage.start_byte < seen.end_byte
                    })
                })
                .collect();
            let fitting = longest_fitting_prefix(&page, &fits);
            if fitting < page.len() || exhausted {
                page.truncate(fitting);
                return Ok((page, hits));
            }
            depth = depth.saturating_mul(2);
        }
    }

    /// Whether `text` mentions what `query` is about: one of the query's
    /// distinctive content terms, those no more common across this
    /// collection's passages than the median of the query's own content
    /// terms. Words the whole subject shares (in a .NET collection, "net" or
    /// "core") do not count, so a walk through deeper evidence ends once only
    /// the subject's common vocabulary is left. Vector neighbours alone never
    /// count. Before the median is taken, two kinds of query term are left
    /// out because no page can tell anything by them: a term in no passage of
    /// the collection, such as a name a lesson invented for its example (no
    /// page can contain it, so it neither keeps a walk going nor lowers the
    /// bar the real terms must meet), and a term in every passage.
    pub(in crate::features::learning) fn mentions_subject_of(
        &self,
        query: &str,
        text: &str,
    ) -> bool {
        let mut wanted: Vec<(usize, String)> = content_terms(query)
            .filter_map(|term| {
                let frequency = self.subject_frequencies.get(&term).copied().unwrap_or(0);
                (frequency > 0 && frequency < self.subject_passages).then_some((frequency, term))
            })
            .collect();
        wanted.sort();
        wanted.dedup();
        let Some(&(median, _)) = wanted.get(wanted.len().saturating_sub(1) / 2) else {
            return false;
        };
        let distinctive: HashSet<&str> = wanted
            .iter()
            .filter(|(frequency, _)| *frequency <= median)
            .map(|(_, term)| term.as_str())
            .collect();
        content_terms(text).any(|term| distinctive.contains(term.as_str()))
    }

    async fn fill(
        &self,
        query: &str,
        fits: impl Fn(&[ReferencePassage]) -> bool,
    ) -> Result<Vec<ReferencePassage>> {
        let mut depth = FIRST_PAGE;
        loop {
            let found = self.retrieve(query, depth).await?;
            let exhausted = found.len() < depth;
            let mut passages = found;
            let fitting = longest_fitting_prefix(&passages, &fits);
            if fitting < passages.len() || exhausted {
                passages.truncate(fitting);
                return Ok(passages);
            }
            depth = depth.saturating_mul(2);
        }
    }

    /// BM25 over the keyword passages, best first. Terms are summed in a
    /// fixed order, so equal inputs always rank alike: a map's iteration
    /// order would change the floating-point sums, and near-tied passages
    /// would swap places between runs.
    fn keyword_ranking(&self, query: &str) -> Vec<Span> {
        let query_terms: std::collections::BTreeSet<String> = terms(query).into_keys().collect();
        let count = self.chunks.len() as f64;
        let mut scored: Vec<_> = self
            .chunks
            .iter()
            .enumerate()
            .filter_map(|(index, chunk)| {
                let score: f64 = query_terms
                    .iter()
                    .map(|term| {
                        let tf = *chunk.terms.get(term).unwrap_or(&0) as f64;
                        let df = *self.frequencies.get(term).unwrap_or(&0) as f64;
                        let idf = (1.0 + (count - df + 0.5) / (df + 0.5)).ln();
                        idf * tf * 2.2
                            / (tf + 1.2 * (0.25 + 0.75 * chunk.length as f64 / self.average_length))
                    })
                    .sum();
                (score > 0.0).then_some((index, score))
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored
            .into_iter()
            .filter_map(|(index, _)| self.chunks.get(index))
            .map(|chunk| Span {
                source: chunk.source,
                start: chunk.start,
                end: chunk.end,
            })
            .collect()
    }

    /// The source passages `quotes` copy, in quote order: each quote's span
    /// in the first snapshot that contains it, exactly or apart from
    /// typography and spacing, widened as verification widens a retrieved
    /// passage. A quote no snapshot contains is skipped.
    pub(in crate::features::learning) fn cited_passages(
        &self,
        quotes: &[&str],
    ) -> Result<Vec<ReferencePassage>> {
        let span = |source: &LearningSourceDto, start: usize, end: usize| ReferencePassage {
            source_id: source.id.clone(),
            text: source
                .excerpt
                .get(start..end)
                .unwrap_or_default()
                .to_owned(),
            start_byte: start,
            end_byte: end,
            retrieval_kind: "cited".into(),
            score: 0.0,
        };
        let mut found = Vec::new();
        for quote in quotes
            .iter()
            .map(|quote| quote.trim())
            .filter(|q| !q.is_empty())
        {
            let exact = self.sources.iter().find_map(|source| {
                let start = source.excerpt.find(quote)?;
                Some(span(source, start, start + quote.len()))
            });
            let restored = || {
                self.sources.iter().find_map(|source| {
                    let copied = crate::features::learning::outline_citations::exact_source_quote(
                        &source.excerpt,
                        quote,
                    )?;
                    let start = source.excerpt.find(copied)?;
                    Some(span(source, start, start + copied.len()))
                })
            };
            found.extend(exact.or_else(restored));
        }
        self.verification_context(found)
    }

    /// Verification needs nearby conditions as well as the matching paragraph.
    /// Keep retrieval ranking unchanged, then restore surrounding source bytes
    /// and remove overlapping windows so repeated hits do not crowd out sources.
    fn verification_context(
        &self,
        passages: Vec<ReferencePassage>,
    ) -> Result<Vec<ReferencePassage>> {
        const SURROUNDING_CHARS: usize = 1700;
        let mut selected: Vec<ReferencePassage> = Vec::new();
        for mut passage in passages {
            let source = self
                .sources
                .iter()
                .find(|source| source.id == passage.source_id)
                .ok_or_else(|| invalid("Unknown verification source."))?;
            let text = &source.excerpt;
            if text.get(passage.start_byte..passage.end_byte) != Some(passage.text.as_str()) {
                return Err(invalid("Verification passage does not match its source."));
            }
            let mut start = text[..passage.start_byte]
                .char_indices()
                .rev()
                .nth(SURROUNDING_CHARS - 1)
                .map_or(0, |(index, _)| index);
            let mut end = text[passage.end_byte..]
                .char_indices()
                .nth(SURROUNDING_CHARS)
                .map_or(text.len(), |(index, _)| passage.end_byte + index);
            if let Some(index) = text[start..passage.start_byte]
                .find('\n')
                .filter(|index| *index <= 300)
            {
                // Preserve the start of a short source instead of trimming its
                // first line merely because the entire prefix fits.
                if start != 0 {
                    start += index + 1;
                }
            }
            if let Some(index) = text[passage.end_byte..end]
                .rfind('\n')
                .filter(|index| end - (passage.end_byte + *index) <= 300)
            {
                if end != text.len() {
                    end = passage.end_byte + index;
                }
            }
            if let Some(previous) = selected.iter_mut().find(|previous| {
                previous.source_id == passage.source_id
                    && previous.start_byte < end
                    && start < previous.end_byte
            }) {
                // Retain both original hits when their expanded windows only
                // partly overlap; dropping the later hit could lose a caveat.
                previous.start_byte = previous.start_byte.min(start);
                previous.end_byte = previous.end_byte.max(end);
                previous.text = text[previous.start_byte..previous.end_byte].to_owned();
                continue;
            }
            passage.start_byte = start;
            passage.end_byte = end;
            passage.text = text[start..end].to_owned();
            selected.push(passage);
        }
        Ok(selected)
    }

    /// A bounded structural view is supplied alongside passages, so the writer
    /// can see which books/sections exist without treating it as factual evidence.
    pub fn catalog(
        &self,
        llm: &dyn lattice_core::application::ports::LLMPort,
    ) -> serde_json::Value {
        let budget = llm.max_context_tokens() / 16;
        let mut entries = Vec::new();
        let mut used = 64;
        for source in &self.sources {
            let mut entry = serde_json::json!({
                "title":source.title,"versionId":source.id,
                "characters":source.excerpt.chars().count(),
                "headings":[],"additionalHeadingsOmitted":false,
            });
            let base = llm.count_tokens(&entry.to_string()) + 1;
            if used + base > budget {
                continue;
            }
            let mut headings = Vec::new();
            let mut omitted = false;
            for line in source
                .excerpt
                .lines()
                .filter(|line| line.trim_start().starts_with('#'))
            {
                // Whole headings, as many as the catalog's share of the window holds.
                let title = line.trim().to_owned();
                let cost = llm.count_tokens(&serde_json::json!(title).to_string()) + 1;
                if used + base + cost > budget {
                    omitted = true;
                    continue;
                }
                used += cost;
                headings.push(title);
            }
            used += base;
            if let Some(fields) = entry.as_object_mut() {
                fields.insert("headings".into(), serde_json::json!(headings));
                fields.insert(
                    "additionalHeadingsOmitted".into(),
                    serde_json::json!(omitted),
                );
            }
            entries.push(entry);
        }
        serde_json::json!({"totalSources":self.sources.len(),
            "omittedFromThisPrompt":self.sources.len()-entries.len(),"sources":entries,
            "scope":"A partial catalog for orientation. Retrieval searches all saved sources."})
    }
}

mod snapshots;
pub(in crate::features::learning) use snapshots::{snapshot_passages, snapshot_room};

#[cfg(test)]
pub(in crate::features::learning) mod tests;
