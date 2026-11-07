use super::footprint::ServerBuffers;
use super::shape::ModelShape;

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
        todo!("stage 2")
    }

    /// No companions: for judging a model alone.
    pub fn none() -> Companions {
        todo!("stage 2")
    }

    /// The pair the runtime starts, from the catalog's pinned files, at the windows, slots and
    /// buffers the launches use.
    pub fn default_pair() -> Companions {
        todo!("stage 2")
    }
}
