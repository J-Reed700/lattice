use super::footprint::{Footprint, GgufHeader};
use crate::domain::models::curated::{get_curated_embedding_models, CURATED_RERANKER};

/// Qwen3-Embedding-0.6B and Qwen3-Reranker-0.6B share one cache shape: 28 blocks, 8 KV heads,
/// 128-wide keys and values (the Qwen3-0.6B configuration).
fn qwen3_0_6b(weights_bytes: u64) -> Footprint {
    Footprint::from_header(&GgufHeader {
        weights_bytes,
        full_attention_layers: 28,
        full_attention_kv_heads: 8,
        full_attention_key_length: 128,
        full_attention_value_length: 128,
        sliding_layers: 0,
        sliding_kv_heads: 0,
        sliding_key_length: 0,
        sliding_value_length: 0,
        sliding_window_tokens: 0,
        recurrent_state_bytes: 0,
    })
}

/// One small server the runtime starts beside the chat model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompanionServer {
    pub footprint: Footprint,
    /// The window each slot is opened with.
    pub window_tokens: u64,
    pub slots: u64,
}

/// The embedding model and the reranker that run beside chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Companions {
    pub embedding: CompanionServer,
    pub reranker: CompanionServer,
}

impl Companions {
    /// The two servers' combined memory, each at its own window and slots.
    pub fn bytes(&self) -> u64 {
        [self.embedding, self.reranker]
            .iter()
            .filter(|server| server.slots > 0)
            .map(|server| server.footprint.at(server.window_tokens * server.slots))
            .sum()
    }

    /// No companions: for judging a model alone.
    pub fn none() -> Companions {
        let nothing = CompanionServer {
            footprint: Footprint {
                weights_bytes: 0,
                kv_per_token_bytes: 0,
                windowed_kv_bytes: 0,
                recurrent_state_bytes: 0,
            },
            window_tokens: 0,
            slots: 0,
        };
        Companions {
            embedding: nothing,
            reranker: nothing,
        }
    }

    /// The pair the runtime starts: the curated embedding GGUF at 4 slots of 2,048 tokens
    /// (`SERVER_SLOTS` in `embedding/llama_cpp/launch.rs`; `MAX_PASSAGE_TOKENS` in
    /// `embedding/input_policy.rs`) and the curated reranker at 1 slot of 4,096 tokens
    /// (`RERANK_SLOTS` and `RERANK_WINDOW_TOKENS` in `llama_reranker/launch.rs`).
    pub fn default_pair() -> Companions {
        let embedding_bytes = get_curated_embedding_models()
            .iter()
            .flat_map(|model| model.files.iter())
            .find(|file| file.filename.ends_with(".gguf"))
            .map_or(0, |file| file.size_bytes);
        Companions {
            embedding: CompanionServer {
                footprint: qwen3_0_6b(embedding_bytes),
                window_tokens: 2_048,
                slots: 4,
            },
            reranker: CompanionServer {
                footprint: qwen3_0_6b(CURATED_RERANKER.bytes),
                window_tokens: 4_096,
                slots: 1,
            },
        }
    }
}
