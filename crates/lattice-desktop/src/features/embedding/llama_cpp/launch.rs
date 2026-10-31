//! What the adapter asks of the process that serves it, and all it knows
//! about that process once it runs. Starting one is the composition root's
//! job: it owns the sidecar manager this feature may not name.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::error::Result;

use super::facts::{EmbeddingHeader, EmbeddingModelFacts};
use crate::infrastructure::llama_server::RunningLlamaServer;

/// Slots an embedding server opens (`-np`): llama-server's own automatic
/// choice in the pinned build, passed explicitly only so the window behind
/// them (`-c`) can be computed.
pub const SERVER_SLOTS: u32 = 4;

/// What an embedding server is wanted for, which decides what it may take
/// for a place on the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingWork {
    /// A search's query or a few passages: quick anywhere, with a model that
    /// should stay loaded waiting behind it.
    Light,
    /// A batch of passages, slow enough off the GPU to be worth a chat model's
    /// reload.
    Indexing,
}

/// The embedding server to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingLaunch {
    /// The model's GGUF file.
    pub gguf: PathBuf,
    /// The model's name as the user knows it.
    pub name: String,
    /// Offload every layer to the accelerator; none when this machine has
    /// none.
    pub gpu: bool,
    /// Each slot's window, in tokens.
    pub window_tokens: u32,
    /// Sequences the server embeds at once.
    pub slots: u32,
    pub work: EmbeddingWork,
}

impl EmbeddingLaunch {
    /// The server for the model at `gguf` that `facts` describe.
    pub fn new(gguf: PathBuf, facts: &EmbeddingModelFacts, gpu: bool) -> Self {
        Self {
            gguf,
            name: facts.display_name.clone(),
            gpu,
            window_tokens: facts.window_tokens(),
            slots: SERVER_SLOTS,
            work: EmbeddingWork::Light,
        }
    }

    /// The same server, wanted for `work`.
    pub fn for_work(&self, work: EmbeddingWork) -> Self {
        Self {
            work,
            ..self.clone()
        }
    }

    /// The server's whole window (`-c`): llama-server gives each slot an
    /// equal share of it.
    pub fn context_tokens(&self) -> u32 {
        self.slots.saturating_mul(self.window_tokens)
    }
}

/// Starts embedding servers, implemented where the sidecar manager is
/// visible (the composition root) and by fakes in tests.
#[async_trait]
pub trait EmbeddingServerStarter: Send + Sync {
    /// The header of the GGUF at `gguf`; `None` when it cannot be read.
    async fn header(&self, gguf: &Path) -> Option<EmbeddingHeader>;

    /// Whether this machine has an accelerator a server can offload to, by
    /// local device detection.
    async fn accelerated(&self) -> bool;

    /// Start the server `launch` describes, or hand back the one already
    /// running for it, once it answers `/health`.
    async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningLlamaServer>>;
}
