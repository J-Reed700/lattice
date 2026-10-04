//! Retrieval over immutable course snapshots. Uses the existing saved-source
//! vector table and embedding port, keeping course versions out of global search.
use super::dto::LearningSourceDto;
use crate::application::ports::EmbeddingPort;
use crate::shared::error::{AppError, Result};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};

const CHUNK_CHARS: usize = 1200;
const STEP_CHARS: usize = 900;
const INDEX_VERSION: &str = "course-passages-v2";
const MAX_COLLECTION_CHARS: usize = 20_000_000;

#[derive(Clone, Debug, Serialize)]
pub struct ReferencePassage {
    /// Immutable source version ID, also used by lesson citations.
    pub source_id: String,
    pub text: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub retrieval_kind: String,
    pub score: f64,
}
struct Chunk {
    source: usize,
    start: usize,
    end: usize,
    terms: HashMap<String, usize>,
    length: usize,
    vector: Option<Vec<f32>>,
}

pub struct ReferenceCollection<'a> {
    pub sources: Vec<LearningSourceDto>,
    chunks: Vec<Chunk>,
    embedder: Option<&'a dyn EmbeddingPort>,
    frequencies: HashMap<String, usize>,
    average_length: f64,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
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
    let mut boundaries: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
    boundaries.push(text.len());
    let mut result = Vec::new();
    let mut offset = 0;
    while offset + 1 < boundaries.len() {
        let end = (offset + CHUNK_CHARS).min(boundaries.len() - 1);
        if let (Some(&start_byte), Some(&end_byte)) = (boundaries.get(offset), boundaries.get(end))
        {
            result.push((start_byte, end_byte));
        } else {
            break;
        }
        if end == boundaries.len() - 1 {
            break;
        }
        offset += STEP_CHARS;
    }
    result
}
fn valid_vector(vector: &[f32], dimension: usize) -> bool {
    vector.len() == dimension
        && dimension > 0
        && vector.iter().all(|v| v.is_finite())
        && vector.iter().any(|v| *v != 0.0)
}
fn cosine(a: &[f32], b: &[f32]) -> f64 {
    let dot: f64 = a.iter().zip(b).map(|(a, b)| *a as f64 * *b as f64).sum();
    let norm = |v: &[f32]| v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    dot / (norm(a) * norm(b))
}

impl<'a> ReferenceCollection<'a> {
    pub fn lexical(sources: &[LearningSourceDto]) -> Result<Self> {
        if sources
            .iter()
            .map(|s| s.excerpt.chars().count())
            .sum::<usize>()
            > MAX_COLLECTION_CHARS
        {
            return Err(invalid("This course exceeds the MVP reference limit of 20 million characters. Split the materials into smaller courses."));
        }
        let mut chunks = Vec::new();
        let mut frequencies = HashMap::new();
        for (source, value) in sources.iter().enumerate() {
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
                    vector: None,
                });
            }
        }
        let average_length = (chunks.iter().map(|c| c.length).sum::<usize>() as f64
            / chunks.len().max(1) as f64)
            .max(1.0);
        Ok(Self {
            sources: sources.to_vec(),
            chunks,
            embedder: None,
            frequencies,
            average_length,
        })
    }

    /// Completed versions are reused on retry. A version is replaced atomically;
    /// cancellation cannot mark a partially embedded version as complete. Model
    /// identity and chunk layout are checked, including after model switches.
    pub async fn load(
        pool: &SqlitePool,
        program_id: &str,
        sources: &[LearningSourceDto],
        embedder: Option<&'a dyn EmbeddingPort>,
    ) -> Result<Self> {
        let mut collection = Self::lexical(sources)?;
        let Some(embedder) = embedder else {
            return Ok(collection);
        };
        if !embedder.is_ready().await? {
            return Ok(collection);
        }
        let identity = format!("{INDEX_VERSION}:{}", embedder.model_identity());
        for (source_index, source) in sources.iter().enumerate() {
            let chunks: Vec<_> = collection
                .chunks
                .iter_mut()
                .filter(|c| c.source == source_index)
                .collect();
            let saved = sqlx::query("SELECT chunk_text,start_byte,end_byte,embedding_json FROM learning_source_retrieval_index WHERE program_id=? AND source_version_id=? AND embedding_model=? ORDER BY chunk_ordinal")
                .bind(program_id).bind(&source.id).bind(&identity).fetch_all(pool).await.map_err(db)?;
            let cached: Option<Vec<Vec<f32>>> = if saved.len() == chunks.len() {
                saved
                    .iter()
                    .zip(&chunks)
                    .map(|(row, chunk)| {
                        if row.get::<i64, _>("start_byte") != chunk.start as i64
                            || row.get::<i64, _>("end_byte") != chunk.end as i64
                            || row.get::<String, _>("chunk_text")
                                != source.excerpt[chunk.start..chunk.end]
                        {
                            return None;
                        }
                        let raw: Option<String> = row.get("embedding_json");
                        let vector: Vec<f32> = serde_json::from_str(raw.as_deref()?).ok()?;
                        valid_vector(&vector, embedder.dimension()).then_some(vector)
                    })
                    .collect()
            } else {
                None
            };
            let vectors = if let Some(cached) = cached {
                cached
            } else {
                let mut vectors = Vec::new();
                // Bound inference batches; never embed only a document's prefix.
                for batch in chunks.chunks(16) {
                    let texts: Vec<_> = batch
                        .iter()
                        .map(|c| source.excerpt[c.start..c.end].to_owned())
                        .collect();
                    let generated = embedder.embed_batch(&texts).await?;
                    if generated.len() != texts.len()
                        || generated
                            .iter()
                            .any(|v| !valid_vector(v, embedder.dimension()))
                    {
                        return Err(invalid("The embedding model returned incomplete or invalid reference vectors. Retry indexing the sources."));
                    }
                    vectors.extend(generated);
                }
                let mut tx = pool.begin().await.map_err(db)?;
                sqlx::query("DELETE FROM learning_source_retrieval_index WHERE program_id=? AND source_version_id=?").bind(program_id).bind(&source.id).execute(&mut *tx).await.map_err(db)?;
                for (ordinal, (chunk, vector)) in chunks.iter().zip(&vectors).enumerate() {
                    sqlx::query("INSERT INTO learning_source_retrieval_index(program_id,source_version_id,chunk_ordinal,chunk_text,start_byte,end_byte,embedding_model,embedding_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
                        .bind(program_id).bind(&source.id).bind(ordinal as i64).bind(&source.excerpt[chunk.start..chunk.end]).bind(chunk.start as i64).bind(chunk.end as i64).bind(&identity).bind(serde_json::to_string(vector)?).bind(chrono::Utc::now().timestamp_millis()).execute(&mut *tx).await.map_err(db)?;
                }
                tx.commit().await.map_err(db)?;
                vectors
            };
            for (chunk, vector) in chunks.into_iter().zip(vectors) {
                chunk.vector = Some(vector);
            }
        }
        collection.embedder = Some(embedder);
        Ok(collection)
    }

    pub fn mode(&self) -> &'static str {
        if self.embedder.is_some() {
            "hybrid"
        } else {
            "lexical_fallback"
        }
    }
    pub fn embedding_model(&self) -> Option<String> {
        self.embedder.map(EmbeddingPort::model_identity)
    }

    /// Keyword BM25 and semantic rankings contribute equally through RRF. Scores
    /// rank evidence only; they are never a factual-correctness confidence.
    pub async fn retrieve(&self, query: &str, limit: usize) -> Result<Vec<ReferencePassage>> {
        if self.chunks.is_empty() || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let query_terms = terms(query);
        let count = self.chunks.len() as f64;
        let mut lexical: Vec<_> = self
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
        lexical.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut scores: HashMap<usize, f64> = HashMap::new();
        for (rank, (index, _)) in lexical.into_iter().take(100).enumerate() {
            *scores.entry(index).or_default() += 1.0 / (60 + rank + 1) as f64;
        }
        if let Some(embedder) = self.embedder {
            let vector = embedder.embed_query(query).await?;
            if !valid_vector(&vector, embedder.dimension()) {
                return Err(invalid("Reference query embedding is invalid."));
            }
            let mut semantic: Vec<_> = self
                .chunks
                .iter()
                .enumerate()
                .filter_map(|(index, c)| c.vector.as_ref().map(|v| (index, cosine(&vector, v))))
                .filter(|(_, score)| *score > 0.0)
                .collect();
            semantic.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            for (rank, (index, _)) in semantic.into_iter().take(100).enumerate() {
                *scores.entry(index).or_default() += 1.0 / (60 + rank + 1) as f64;
            }
        }
        let mut ranked: Vec<_> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut selected: Vec<ReferencePassage> = Vec::new();
        for (index, score) in ranked {
            let chunk = self
                .chunks
                .get(index)
                .ok_or_else(|| invalid("Reference index contains an unknown passage."))?;
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
                retrieval_kind: self.mode().into(),
                score,
            });
            if selected.len() >= limit.clamp(1, 50) {
                break;
            }
        }
        Ok(selected)
    }

    /// A bounded structural view is supplied alongside passages, so the writer
    /// can see which books/sections exist without treating it as factual evidence.
    pub fn catalog(&self) -> Vec<serde_json::Value> {
        let mut heading_budget = 4_000_usize;
        self.sources.iter().map(|source| {
            let mut headings = Vec::new();
            let mut omitted = false;
            for line in source.excerpt.lines().filter(|line| line.trim_start().starts_with('#')) {
                let heading: String = line.chars().take(120).collect();
                let count = heading.chars().count();
                if count > heading_budget || headings.len() >= 40 { omitted = true; continue; }
                heading_budget -= count;
                headings.push(heading);
            }
            serde_json::json!({"title":source.title.chars().take(90).collect::<String>(),"versionId":source.id,"characters":source.excerpt.chars().count(),"headings":headings,"additionalHeadingsOmitted":omitted})
        }).collect()
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
mod tests;
