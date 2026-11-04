//! What the reranking adapter asks of the server that serves it. Starting one
//! is the composition root's job: it owns the sidecar manager and the GPU
//! admission this feature may not name.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::error::Result;

use crate::infrastructure::llama_server::RunningLlamaServer;

/// Slots a reranking server opens (`-np`). One query's passages are judged
/// one after another; a second slot would double the cache for little.
pub const RERANK_SLOTS: u32 = 1;

/// Each slot's window, in tokens: one query, one passage and the model's
/// prompt around them. The client cuts both to fit it.
pub const RERANK_WINDOW_TOKENS: u32 = 4096;

/// The reranking server to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RerankLaunch {
    /// The model's GGUF file.
    pub gguf: PathBuf,
    /// What logs call it.
    pub name: String,
    /// Offload every layer when this machine has an accelerator. The starter
    /// still runs it on the CPU when it does not fit beside the models already
    /// holding GPU memory.
    pub gpu: bool,
    /// Each slot's window, in tokens.
    pub window_tokens: u32,
    /// Passages the server judges at once.
    pub slots: u32,
}

impl RerankLaunch {
    /// The server for the reranker at `gguf`.
    pub fn new(gguf: PathBuf, gpu: bool) -> Self {
        let name = gguf.file_name().map_or_else(
            || "reranking server".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        Self {
            gguf,
            name,
            gpu,
            window_tokens: RERANK_WINDOW_TOKENS,
            slots: RERANK_SLOTS,
        }
    }

    /// The server's whole window (`-c`), shared equally by its slots.
    pub fn context_tokens(&self) -> u32 {
        self.slots.saturating_mul(self.window_tokens)
    }
}

/// Starts reranking servers, implemented where the sidecar manager is
/// visible (the composition root) and by fakes in tests.
#[async_trait]
pub trait RerankServerStarter: Send + Sync {
    /// Whether this machine has an accelerator a server can offload to.
    async fn accelerated(&self) -> bool;

    /// Start the server `launch` describes, or hand back the one already
    /// running for it, once it answers `/health`. Nothing is unloaded to make
    /// room for it: a launch that asks for the GPU and does not fit runs on
    /// the CPU.
    async fn start(&self, launch: &RerankLaunch) -> Result<Arc<RunningLlamaServer>>;
}
