//! Bounded semantic transcript recall, fused with lexical and exact matches.
use super::ConversationRepository;
use crate::application::ports::conversation_memory::{RecallCandidate, RecallCandidates};
use crate::features::embedding::encoding::decode_embedding;
use crate::shared::error::Result;
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
        // prevent comparing vectors from different embedding spaces.
        let scan_limit = (64 * 1024 * 1024 / (vector.len().max(1) * 4)).min(20000);
        let rows = sqlx::query_as::<_, Row>("SELECT m.id AS message_id, m.sequence, m.role, substr(m.content, 1, 2400) AS content, v.embedding FROM conversation_memory_vectors v JOIN conversation_messages m ON m.id=v.message_id AND m.conversation_id=v.conversation_id WHERE m.conversation_id=? AND v.embedding_model=? AND v.dimension=? AND v.content=m.content AND (m.role='user' OR m.status='completed') ORDER BY m.sequence DESC LIMIT ?")
            .bind(conversation).bind(embedder.model_identity()).bind(vector.len() as i64).bind(scan_limit as i64 + 1).fetch_all(&self.pool).await;
        let rows = match rows {
            Ok(rows) => rows,
            Err(_) => {
                lexical.index_error = Some("semantic_index_unavailable".into());
                return Ok(lexical);
            }
        };
        lexical.semantic_ran = true;
        if rows.len() > scan_limit {
            lexical.index_error = Some("semantic_scan_limited".into());
        }
        let mut semantic = Vec::new();
        for row in rows.into_iter().take(scan_limit) {
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
        semantic.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(a.sequence.cmp(&b.sequence))
        });
        semantic.truncate(limit.min(64));
        lexical.candidates = fuse(lexical.candidates, semantic, limit.min(64));
        Ok(lexical)
    }
}

pub(super) fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() || a.iter().chain(b).any(|x| !x.is_finite()) {
        return None;
    }
    let dot: f32 = a.iter().zip(b).map(|(a, b)| a * b).sum();
    let norm = (a.iter().map(|x| x * x).sum::<f32>() * b.iter().map(|x| x * x).sum::<f32>()).sqrt();
    (norm > 0.0 && norm.is_finite()).then_some(dot / norm)
}
fn fuse(
    lexical: Vec<RecallCandidate>,
    semantic: Vec<RecallCandidate>,
    limit: usize,
) -> Vec<RecallCandidate> {
    let mut all: HashMap<String, RecallCandidate> = HashMap::new();
    for branch in [lexical, semantic] {
        for (rank, mut hit) in branch.into_iter().enumerate() {
            let score = 1.0 / (60.0 + rank as f32 + 1.0);
            if let Some(existing) = all.get_mut(&hit.message_id) {
                existing.score += score;
            } else {
                hit.score = score;
                all.insert(hit.message_id.clone(), hit);
            }
        }
    }
    let mut hits: Vec<_> = all.into_values().collect();
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
}
