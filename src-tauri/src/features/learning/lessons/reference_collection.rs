//! Retrieval over a program's immutable source snapshots.
//!
//! A snapshot captured from a library document is ranked by the library
//! itself: its chunks, vectors and hybrid fusion, confined to the program's
//! documents. Web pages and pasted text have no library copy, so they are
//! ranked by keyword alone. Every hit is mapped back onto the snapshot's
//! bytes, so evidence is always an exact substring of what was captured.
use crate::application::ports::{LibraryChunk, LibraryPassagesPort};
use crate::features::learning::dto::LearningSourceDto;
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

const CHUNK_CHARS: usize = 1200;
const STEP_CHARS: usize = 900;
/// Reciprocal-rank constant for merging library and keyword rankings.
const RANK_K: usize = 60;

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
        for (source, value) in sources.iter().enumerate() {
            if ranked_by_library.contains(&source) {
                continue;
            }
            for (start, end) in spans(&value.excerpt) {
                let counts = terms(&value.excerpt[start..end]);
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
    /// only; they are never a factual-correctness confidence.
    pub async fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<ReferencePassage>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 50);
        let mut ranked: Vec<(Span, f64, &'static str)> = Vec::new();
        if let Some(library) = self.library.filter(|_| !self.library_documents.is_empty()) {
            let hits = library
                .search(query, &self.library_documents, limit * 3)
                .await?;
            let spans = hits
                .iter()
                .filter_map(|hit| self.library_spans.get(&hit.chunk_id).copied());
            for (rank, span) in spans.enumerate() {
                ranked.push((span, 1.0 / (RANK_K + rank + 1) as f64, "hybrid"));
            }
        }
        for (rank, span) in self
            .keyword_ranking(query)
            .into_iter()
            .take(100)
            .enumerate()
        {
            ranked.push((span, 1.0 / (RANK_K + rank + 1) as f64, "lexical_fallback"));
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
            let mut start = chunk.start.saturating_sub(300);
            while !source.excerpt.is_char_boundary(start) {
                start += 1;
            }
            let mut end = (chunk.end + 300).min(source.excerpt.len());
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

    /// BM25 over the keyword passages, best first.
    fn keyword_ranking(&self, query: &str) -> Vec<Span> {
        let query_terms = terms(query);
        let count = self.chunks.len() as f64;
        let mut scored: Vec<_> = self
            .chunks
            .iter()
            .enumerate()
            .filter_map(|(index, chunk)| {
                let score: f64 = query_terms
                    .keys()
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

    /// Verification needs nearby conditions as well as the matching paragraph.
    /// Keep retrieval ranking unchanged, then restore surrounding source bytes
    /// and remove overlapping windows so repeated hits do not crowd out sources.
    pub(in crate::features::learning) async fn retrieve_for_verification(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ReferencePassage>> {
        self.verification_context(self.retrieve(query, limit).await?)
    }

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
    pub fn catalog(&self, llm: &dyn crate::application::ports::LLMPort) -> serde_json::Value {
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
                let title: String = line.chars().take(120).collect();
                let cost = llm.count_tokens(&serde_json::json!(title).to_string()) + 1;
                if used + base + cost > budget || headings.len() >= 40 {
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

    pub async fn author_sources(&self, query: &str) -> Result<Vec<LearningSourceDto>> {
        let passages = self.retrieve(query, 12).await?;
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for passage in &passages {
            if !seen.insert(&passage.source_id) {
                continue;
            }
            if let Some(source) = self.sources.iter().find(|s| s.id == passage.source_id) {
                let mut source = source.clone();
                source.excerpt = passages
                    .iter()
                    .filter(|p| p.source_id == source.id)
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                result.push(source);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
pub(in crate::features::learning) mod tests;
