//! Bounded semantic transcript recall, fused with lexical and exact matches.
use super::ConversationRepository;
use lattice_core::application::contracts::search::{ReciprocalRankFusion, WeightedRanking};
use lattice_core::application::ports::conversation_memory::{RecallCandidate, RecallCandidates};
use lattice_core::shared::constants::{
    DEFAULT_KEYWORD_FUSION_WEIGHT, DEFAULT_RRF_K, DEFAULT_VECTOR_FUSION_WEIGHT,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::embedding_encoding::decode_embedding;
use std::collections::HashMap;

impl ConversationRepository {
    pub(super) async fn search_memory_hybrid(
        &self,
        conversation: &str,
        query: &str,
        exact: &[String],
        limit: usize,
    ) -> Result<RecallCandidates> {
        let mut lexical = self
            .search_memory_source_messages(conversation, query, exact, limit)
            .await?;
        let Some(embedder) = &self.memory_embedding else {
            return Ok(lexical);
        };
        let query: String = query.chars().take(2048).collect();
        let vector = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            embedder.embed_query(&query),
        )
        .await
        {
            Ok(Ok(vector)) if !vector.is_empty() && vector.iter().all(|v| v.is_finite()) => vector,
            _ => {
                lexical.index_error = Some("semantic_query_unavailable".into());
                return Ok(lexical);
            }
        };
        #[derive(sqlx::FromRow)]
        struct Row {
            message_id: String,
            sequence: i64,
            role: String,
            content: String,
            embedding: Vec<u8>,
        }
        // Hydrate only live, byte-identical sources. Model identity and dimension
        // prevent comparing vectors from different embedding spaces. Every
        // vector is scored, newest first, a page at a time: a page is sized so
        // its vectors stay within a fixed amount of memory, and only the best
        // `limit` hits are carried between pages.
        let page = (SCAN_PAGE_BYTES / (vector.len().max(1) * 4)).max(1);
        let mut before = i64::MAX;
        let mut semantic = Vec::new();
        loop {
            let rows = sqlx::query_as::<_, Row>("SELECT m.id AS message_id, m.sequence, m.role, substr(m.content, 1, 2400) AS content, v.embedding FROM conversation_memory_vectors v JOIN conversation_messages m ON m.id=v.message_id AND m.conversation_id=v.conversation_id WHERE m.conversation_id=? AND v.embedding_model=? AND v.dimension=? AND v.content=m.content AND (m.role='user' OR m.status='completed') AND m.sequence < ? ORDER BY m.sequence DESC LIMIT ?")
                .bind(conversation).bind(embedder.model_identity()).bind(vector.len() as i64).bind(before).bind(page as i64).fetch_all(&self.pool).await;
            let rows = match rows {
                Ok(rows) => rows,
                Err(_) => {
                    lexical.index_error = Some("semantic_index_unavailable".into());
                    return Ok(lexical);
                }
            };
            lexical.semantic_ran = true;
            let read = rows.len();
            for row in rows {
                before = before.min(row.sequence);
                let Ok(stored) = decode_embedding(&row.embedding) else {
                    continue;
                };
                let Some(score) = cosine(&vector, &stored) else {
                    continue;
                };
                if score <= 0.0 {
                    continue;
                }
                semantic.push(RecallCandidate {
                    message_id: row.message_id,
                    sequence: row.sequence,
                    role: row.role.parse()?,
                    excerpt: row.content,
                    score,
                    exact_identifier: false,
                });
            }
            keep_best(&mut semantic, limit);
            if read < page {
                break;
            }
        }
        lexical.candidates = fuse(lexical.candidates, semantic, limit);
        Ok(lexical)
    }
}

/// Memory one page of stored vectors may occupy while it is scored.
const SCAN_PAGE_BYTES: usize = 64 * 1024 * 1024;

/// The best `limit` hits, best first, ties broken by sequence.
fn keep_best(hits: &mut Vec<RecallCandidate>, limit: usize) {
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.sequence.cmp(&b.sequence))
    });
    hits.truncate(limit);
}

pub(super) fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() || a.iter().chain(b).any(|x| !x.is_finite()) {
        return None;
    }
    let dot: f32 = a.iter().zip(b).map(|(a, b)| a * b).sum();
    let norm = (a.iter().map(|x| x * x).sum::<f32>() * b.iter().map(|x| x * x).sum::<f32>()).sqrt();
    (norm > 0.0 && norm.is_finite()).then_some(dot / norm)
}

/// Lexical and semantic recall fused by the app's one reciprocal rank fusion,
/// at the library search's constant, with semantic carrying the vector weight
/// and lexical the keyword weight. Exact-identifier hits still come first;
/// equal scores fall back to transcript order. A message both modes found
/// keeps its lexical record, which is the one that can carry
/// `exact_identifier`.
fn fuse(
    lexical: Vec<RecallCandidate>,
    semantic: Vec<RecallCandidate>,
    limit: usize,
) -> Vec<RecallCandidate> {
    let ids = |branch: &[RecallCandidate]| -> Vec<String> {
        branch.iter().map(|hit| hit.message_id.clone()).collect()
    };
    let rankings = vec![
        WeightedRanking::new(DEFAULT_KEYWORD_FUSION_WEIGHT, ids(&lexical)),
        WeightedRanking::new(DEFAULT_VECTOR_FUSION_WEIGHT, ids(&semantic)),
    ];
    let mut records: HashMap<String, RecallCandidate> = HashMap::new();
    for hit in lexical.into_iter().chain(semantic) {
        records.entry(hit.message_id.clone()).or_insert(hit);
    }
    let mut hits: Vec<RecallCandidate> = ReciprocalRankFusion::new(DEFAULT_RRF_K)
        .fuse_ranked(rankings)
        .into_iter()
        .filter_map(|fused| {
            let mut hit = records.remove(&fused.id)?;
            hit.score = fused.score;
            Some(hit)
        })
        .collect();
    hits.sort_by(|a, b| {
        b.exact_identifier
            .cmp(&a.exact_identifier)
            .then(b.score.total_cmp(&a.score))
            .then(a.sequence.cmp(&b.sequence))
    });
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incompatible_or_corrupt_vectors_are_not_evidence() {
        assert_eq!(cosine(&[1.0], &[1.0, 0.0]), None);
        assert_eq!(cosine(&[0.0], &[0.0]), None);
        assert_eq!(cosine(&[f32::NAN], &[1.0]), None);
        assert_eq!(cosine(&[1.0, 0.0], &[1.0, 0.0]), Some(1.0));
    }

    fn hit(id: &str, sequence: i64, exact_identifier: bool) -> RecallCandidate {
        RecallCandidate {
            message_id: id.into(),
            sequence,
            role: lattice_core::domain::conversation::memory::SourceRole::User,
            excerpt: id.into(),
            score: 0.0,
            exact_identifier,
        }
    }

    /// Recall fuses at the library search's constant and weights: a message only
    /// semantic recall found, first, beats one both modes ranked seventh. An
    /// exact-identifier hit still leads whatever its fused score.
    #[test]
    fn recall_fuses_at_the_shared_constant_and_weights() {
        let mut lexical = vec![hit("exact", 50, true)];
        lexical.extend((1..=5).map(|i| hit(&format!("lex{i}"), i, false)));
        lexical.push(hit("both", 7, false));
        let mut semantic = vec![hit("meaning", 40, false)];
        semantic.extend((1..=5).map(|i| hit(&format!("sem{i}"), 10 + i, false)));
        semantic.push(hit("both", 7, false));
        // Found by both modes, it keeps the lexical record and its flag.
        semantic.push(hit("exact", 50, false));

        let fused = fuse(lexical, semantic, 20);
        let order: Vec<&str> = fused.iter().map(|h| h.message_id.as_str()).collect();
        assert_eq!(order[..2], ["exact", "meaning"], "{order:?}");
        assert!(fused[0].exact_identifier);
        let position = |id: &str| order.iter().position(|m| *m == id);
        assert!(position("meaning") < position("both"), "{order:?}");
        let both = fused.iter().find(|h| h.message_id == "both").unwrap();
        assert!((both.score - 1.0 / 17.0).abs() < 1e-6);
        assert_eq!(fused.iter().filter(|h| h.message_id == "both").count(), 1);
    }
}
