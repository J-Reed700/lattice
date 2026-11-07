use super::footprint::{footprint, ServerBuffers};
use super::shape::{ModelShape, PinnedModel};
use crate::domain::models::curated::{
    curated_embedding_pinned, get_curated_embedding_models, CURATED_RERANKER,
};

/// One small server the runtime starts beside the chat model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionServer {
    /// What the device holds of the GGUF.
    pub gpu_weight_bytes: u64,
    pub shape: Option<ModelShape>,
    /// The window each slot is opened with.
    pub window_tokens: u32,
    pub slots: u32,
    pub buffers: ServerBuffers,
}

/// The embedding model and the reranker that run beside chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Companions {
    pub embedding: CompanionServer,
    pub reranker: CompanionServer,
}

impl Companions {
    /// The two servers' combined memory: each priced by `footprint` at its slots times its window.
    pub fn bytes(&self) -> u64 {
        [&self.embedding, &self.reranker]
            .into_iter()
            .map(|server| {
                footprint(
                    server.gpu_weight_bytes,
                    server.shape.as_ref(),
                    server.window_tokens.saturating_mul(server.slots),
                    server.buffers,
                )
                .total()
            })
            .sum()
    }

    /// No companions: for judging a model alone.
    pub fn none() -> Companions {
        let nothing = CompanionServer {
            gpu_weight_bytes: 0,
            shape: None,
            window_tokens: 0,
            slots: 0,
            buffers: ServerBuffers::chat(false),
        };
        Companions {
            embedding: nothing.clone(),
            reranker: nothing,
        }
    }

    /// The pair the runtime starts, from the catalog's pinned files: the embedder at 4 slots of
    /// 2,048 tokens and the reranker at 1 slot of 4,096, each with a micro-batch of its window
    /// (`SERVER_SLOTS` in `embedding/llama_cpp/launch.rs`, `MAX_PASSAGE_TOKENS` in
    /// `embedding/input_policy.rs`, `RERANK_SLOTS` and `RERANK_WINDOW_TOKENS` in
    /// `llama_reranker/launch.rs`, `PoolingServing::new(window, ..)` in the DI modules). Neither
    /// quantizes its cache.
    pub fn default_pair() -> Companions {
        let embedding_bytes = get_curated_embedding_models()
            .iter()
            .flat_map(|model| model.files.iter())
            .find(|file| file.filename.ends_with(".gguf"))
            .map_or(0, |file| file.size_bytes);
        let embedding = curated_embedding_pinned();
        let reranker = CURATED_RERANKER.pinned();
        let server =
            |gpu_weight_bytes: u64, pinned: PinnedModel, window_tokens: u32, slots: u32| {
                CompanionServer {
                    gpu_weight_bytes,
                    shape: Some(pinned.shape),
                    window_tokens,
                    slots,
                    buffers: ServerBuffers {
                        kv_cache_quantization: false,
                        micro_batch: window_tokens,
                    },
                }
            };
        Companions {
            embedding: server(
                embedding_bytes.saturating_sub(embedding.cpu_weight_bytes),
                embedding,
                2_048,
                4,
            ),
            reranker: server(
                CURATED_RERANKER
                    .bytes
                    .saturating_sub(reranker.cpu_weight_bytes),
                reranker,
                4_096,
                1,
            ),
        }
    }
}
