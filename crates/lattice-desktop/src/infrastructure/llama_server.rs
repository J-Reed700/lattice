//! A bundled llama-server that answers one feature's requests: where it
//! listens, the key it wants, what it holds on the GPU, and whether its
//! process is still up. Starting one is the composition root's job; the
//! embedding and reranking adapters only hold the handle.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// A running llama-server.
pub struct RunningLlamaServer {
    endpoint: String,
    /// Sent as the bearer token on every request. Never logged.
    api_token: String,
    gpu_bytes: Option<u64>,
    alive: Arc<dyn Fn() -> bool + Send + Sync>,
    /// Requests sent to it and not yet answered, from any client.
    in_flight: AtomicU32,
}

impl RunningLlamaServer {
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

    pub(crate) fn api_token(&self) -> &str {
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
    pub(crate) fn request(&self) -> InFlight<'_> {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        InFlight(&self.in_flight)
    }
}

/// One request in flight to a server.
pub(crate) struct InFlight<'a>(&'a AtomicU32);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
