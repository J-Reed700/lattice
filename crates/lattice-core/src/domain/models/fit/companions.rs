use super::footprint::Footprint;

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
        todo!("stage 2")
    }

    /// No companions: for judging a model alone.
    pub fn none() -> Companions {
        todo!("stage 2")
    }
}
