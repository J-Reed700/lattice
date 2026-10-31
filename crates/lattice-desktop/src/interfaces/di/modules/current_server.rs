//! A bundled server outside the pool of LLMs, as their admission sees it:
//! what it holds, whether it is busy, and how to stop it.

use std::sync::{Arc, Weak};

use crate::features::llm::engine::sidecar_manager::SidecarHandle;
use crate::features::llm::instances::IdleServer;
use crate::infrastructure::llama_server::RunningLlamaServer;

/// The server of one role running now: the embedding server, or the
/// reranking server.
pub(crate) struct CurrentServer {
    /// What logs call it before it has a model's name.
    role: &'static str,
    current: parking_lot::Mutex<Option<Current>>,
}

struct Current {
    name: String,
    server: Weak<RunningLlamaServer>,
    handle: Weak<SidecarHandle>,
}

impl CurrentServer {
    pub(crate) fn new(role: &'static str) -> Self {
        Self {
            role,
            current: parking_lot::Mutex::new(None),
        }
    }

    /// What it is, for messages.
    pub(super) fn role(&self) -> &'static str {
        self.role
    }

    pub(super) fn record(
        &self,
        name: String,
        server: &Arc<RunningLlamaServer>,
        handle: &Arc<SidecarHandle>,
    ) {
        *self.current.lock() = Some(Current {
            name,
            server: Arc::downgrade(server),
            handle: Arc::downgrade(handle),
        });
    }

    fn running(&self) -> Option<(String, Arc<RunningLlamaServer>, Arc<SidecarHandle>)> {
        let current = self.current.lock();
        let current = current.as_ref()?;
        let handle = current
            .handle
            .upgrade()
            .filter(|handle| handle.is_running())?;
        Some((current.name.clone(), current.server.upgrade()?, handle))
    }
}

impl IdleServer for CurrentServer {
    fn name(&self) -> String {
        self.running()
            .map_or_else(|| self.role.to_owned(), |(name, ..)| name)
    }

    fn idle_gpu_bytes(&self) -> Option<u64> {
        let (_, server, _) = self.running()?;
        if server.in_flight() > 0 {
            return None;
        }
        server.gpu_bytes().filter(|bytes| *bytes > 0)
    }

    fn stop(&self) {
        if let Some((name, _, handle)) = self.running() {
            tracing::info!(model = %name, role = self.role, "Stopping an idle server to make room");
            handle.stop();
        }
    }
}
