//! What the adapter asks of the process that serves it, and all it knows
//! about that process once it runs. Starting one is the composition root's
//! job: it owns the sidecar manager this feature may not name.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::error::Result;

use super::facts::{EmbeddingHeader, EmbeddingModelFacts};

/// Slots an embedding server opens (`-np`): llama-server's own automatic
/// choice in the pinned build, passed explicitly only so the window behind
/// them (`-c`) can be computed.
pub const SERVER_SLOTS: u32 = 4;

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
        }
    }

    /// The server's whole window (`-c`): llama-server gives each slot an
    /// equal share of it.
    pub fn context_tokens(&self) -> u32 {
        self.slots.saturating_mul(self.window_tokens)
    }
}

/// A running embedding server: where it listens, the key it wants, what it
/// holds on the GPU, and whether its process is still up.
pub struct RunningEmbeddingServer {
    endpoint: String,
    /// Sent as the bearer token on every request. Never logged.
    api_token: String,
    gpu_bytes: Option<u64>,
    alive: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Requests sent to it and not yet answered, from any client.
    in_flight: AtomicU32,
}

impl RunningEmbeddingServer {
    /// The server at `endpoint`. `alive` reads its process's state, and
    /// whoever holds what it captures keeps the process running.
    pub fn new(
        endpoint: impl Into<String>,
        api_token: impl Into<String>,
        gpu_bytes: Option<u64>,
        alive: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            api_token: api_token.into(),
            gpu_bytes,
            alive,
            in_flight: AtomicU32::new(0),
        }
    }

    /// Base URL, e.g. `http://127.0.0.1:53412`.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub(super) fn api_token(&self) -> &str {
        &self.api_token
    }

    /// GPU memory the server holds; `None` on the CPU.
    pub fn gpu_bytes(&self) -> Option<u64> {
        self.gpu_bytes
    }

    /// Whether its process is still running.
    pub fn alive(&self) -> bool {
        (self.alive)()
    }

    /// Requests in flight to it now.
    pub fn in_flight(&self) -> u32 {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Count one request in flight until the guard drops.
    pub(super) fn request(&self) -> InFlight<'_> {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        InFlight(&self.in_flight)
    }
}

/// One request in flight to a server.
pub(super) struct InFlight<'a>(&'a AtomicU32);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
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
    async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningEmbeddingServer>>;
}
