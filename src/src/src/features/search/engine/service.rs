use crate::features::search::engine::vector_ops::cosine_similarity_simd;
use rayon::prelude::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub index: usize,

    pub filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub created_at: Option<String>,
    pub content: Option<String>,

    pub file_id: Option<String>,
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    pub file_extension: Option<String>,
    pub file_category: Option<String>,
    pub is_indexed: Option<bool>,

    // Additional fields for search commands
    pub document_id: Option<String>,
    pub snippet: Option<String>,
    pub chunk_index: Option<usize>,
    pub updated_at: Option<String>,
}

pub struct BruteForceSearch {
    embeddings: Vec<Vec<f32>>,
    ids: Vec<String>,
}

impl BruteForceSearch {
    pub fn new(embeddings: Vec<(String, Vec<f32>)>) -> Self {
        let (ids, embeddings): (Vec<_>, Vec<_>) = embeddings.into_iter().unzip();

        Self { embeddings, ids }
    }

    pub fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<SearchResult> {
        if self.embeddings.is_empty() {
            return vec![];
        }

        // OPTIMIZATION: Pre-filter NaN values and compute scores in parallel
        let mut scores: Vec<(usize, f32)> = self
            .embeddings
            .par_iter()
            .enumerate()
            .filter_map(|(idx, emb)| {
                let score = cosine_similarity_simd(query_embedding, emb);
                if score.is_nan() {
                    tracing::warn!("NaN score detected at index {} in brute force search", idx);
                    None
                } else {
                    Some((idx, score))
                }
            })
            .collect();

        let k = top_k.min(scores.len());

        if k == 0 {
            return vec![];
        }

        // OPTIMIZATION: Simplified sorting - NaN values already filtered out
        scores.select_nth_unstable_by(k - 1, |a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });

        scores.truncate(k);
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scores
            .into_iter()
            .filter_map(|(idx, score)| {
                self.ids.get(idx).map(|id| SearchResult {
                    id: id.clone(),
                    score,
                    index: idx,
                    filename: None,
                    mime_type: None,
                    size_bytes: None,
                    created_at: None,
                    content: None,
                    file_id: None,
                    file_path: None,
                    file_name: None,
                    file_extension: None,
                    file_category: None,
                    is_indexed: None,
                    document_id: None,
                    snippet: None,
                    chunk_index: None,
                    updated_at: None,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brute_force_search() {
        let embeddings = vec![
            ("doc1".to_string(), vec![1.0, 0.0, 0.0]),
            ("doc2".to_string(), vec![0.0, 1.0, 0.0]),
            ("doc3".to_string(), vec![0.0, 0.0, 1.0]),
        ];

        let search = BruteForceSearch::new(embeddings);

        let query = vec![1.0, 0.0, 0.0];
        let results = search.search(&query, 2);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "doc1");
        assert!((results[0].score - 1.0).abs() < 1e-5);
    }
}
