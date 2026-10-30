//! The embedding model as the activity view lists it: while it loads,
//! everything that embeds waits for it; while it embeds, it holds GPU memory
//! a chat model may need.

use super::llama_cpp::{EmbeddingHeader, EmbeddingLaunch, EmbeddingServerStarter};
use super::runtime::EmbeddingRuntime;
use async_trait::async_trait;
use lattice_core::application::ports::{
    ActivityItem, ActivityResource, ActivitySource, ActivityState,
};
use lattice_core::shared::Result;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};

use crate::infrastructure::llama_server::RunningLlamaServer;

/// The embedding servers this app has started and still holds, by name.
pub(crate) struct StartedServers {
    servers: Mutex<Vec<(Weak<RunningLlamaServer>, String)>>,
    changed: tokio::sync::watch::Sender<u64>,
}

impl Default for StartedServers {
    fn default() -> Self {
        Self {
            servers: Mutex::default(),
            changed: tokio::sync::watch::channel(0).0,
        }
    }
}

impl StartedServers {
    fn note(&self, server: &Arc<RunningLlamaServer>, name: &str) {
        {
            let mut servers = self.servers.lock().unwrap_or_else(|p| p.into_inner());
            servers.retain(|(seen, _)| seen.strong_count() > 0);
            if servers
                .iter()
                .any(|(seen, _)| seen.as_ptr() == Arc::as_ptr(server))
            {
                return;
            }
            servers.push((Arc::downgrade(server), name.to_owned()));
        }
        self.changed
            .send_modify(|count| *count = count.wrapping_add(1));
    }

    fn running(&self) -> Vec<(Arc<RunningLlamaServer>, String)> {
        self.servers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter_map(|(seen, name)| seen.upgrade().map(|server| (server, name.clone())))
            .filter(|(server, _)| server.alive())
            .collect()
    }

    /// Moves when a server is started.
    pub(crate) fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }
}

/// A starter that tells [`StartedServers`] about every server it hands out.
pub(crate) struct WatchedStarter {
    inner: Arc<dyn EmbeddingServerStarter>,
    started: Arc<StartedServers>,
}

impl WatchedStarter {
    pub(crate) fn new(
        inner: Arc<dyn EmbeddingServerStarter>,
        started: Arc<StartedServers>,
    ) -> Self {
        Self { inner, started }
    }
}

#[async_trait]
impl EmbeddingServerStarter for WatchedStarter {
    async fn header(&self, gguf: &Path) -> Option<EmbeddingHeader> {
        self.inner.header(gguf).await
    }

    async fn accelerated(&self) -> bool {
        self.inner.accelerated().await
    }

    async fn start(&self, launch: &EmbeddingLaunch) -> Result<Arc<RunningLlamaServer>> {
        let server = self.inner.start(launch).await?;
        self.started.note(&server, &launch.name);
        Ok(server)
    }
}

pub(crate) struct EmbeddingActivity {
    runtime: Arc<EmbeddingRuntime>,
    servers: Arc<StartedServers>,
}

impl EmbeddingActivity {
    pub(crate) fn new(runtime: Arc<EmbeddingRuntime>, servers: Arc<StartedServers>) -> Self {
        Self { runtime, servers }
    }
}

/// "2 requests embedding · 2.1 GB on the GPU".
fn server_words(in_flight: u32, gpu_bytes: Option<u64>) -> String {
    let requests = if in_flight == 1 {
        "request"
    } else {
        "requests"
    };
    let mut words = format!("{in_flight} {requests} embedding");
    match gpu_bytes {
        Some(bytes) => words.push_str(&format!(
            " · {:.1} GB on the GPU",
            bytes as f64 / 1_000_000_000.0
        )),
        None => words.push_str(" · on the CPU"),
    }
    words
}

#[async_trait]
impl ActivitySource for EmbeddingActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let mut items = Vec::new();
        if self.runtime.is_loading() {
            items.push(
                ActivityItem::new(
                    "embedding.loading",
                    "embedding.loading",
                    "Loading the embedding model",
                    ActivityState::Running,
                )
                .subject("the embedding model")
                .detail("Searches and indexes that embed wait until it is ready"),
            );
        }
        for (server, name) in self.servers.running() {
            let in_flight = server.in_flight();
            if in_flight == 0 {
                continue;
            }
            items.push(
                ActivityItem::new(
                    format!("embedding.server:{}", server.endpoint()),
                    "embedding.server",
                    name.clone(),
                    ActivityState::Running,
                )
                .subject(name)
                .detail(server_words(in_flight, server.gpu_bytes()))
                .uses(ActivityResource::EmbeddingModel),
            );
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_words_name_requests_and_where_the_model_runs() {
        assert_eq!(
            server_words(1, Some(2_100_000_000)),
            "1 request embedding · 2.1 GB on the GPU"
        );
        assert_eq!(server_words(3, None), "3 requests embedding · on the CPU");
    }

    #[test]
    fn started_servers_list_live_servers_once_and_forget_dropped_ones() {
        let started = StartedServers::default();
        let server = Arc::new(RunningLlamaServer::new(
            "http://127.0.0.1:1",
            "token",
            Some(1),
            Arc::new(|| true),
        ));
        started.note(&server, "Qwen3 Embedding 0.6B");
        started.note(&server, "Qwen3 Embedding 0.6B");
        assert_eq!(started.running().len(), 1);
        drop(server);
        assert!(started.running().is_empty());
    }
}
