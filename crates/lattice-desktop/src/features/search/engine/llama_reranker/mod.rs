//! The cross-encoder search orders its shortlist with, served by the bundled
//! llama-server's `/v1/rerank`.
//!
//! [`LlamaReranker`] starts its server on first use through a
//! [`RerankServerStarter`] and starts it again when its process is gone. A
//! model that is not downloaded, a server that would not start, or one too
//! slow to answer in time leaves search in its first-stage order; each is
//! logged once.

mod client;
mod launch;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};

use super::reranker::{RerankResult, Reranker};
use crate::infrastructure::llama_server::RunningLlamaServer;
use client::Failure;

pub use launch::{RerankLaunch, RerankServerStarter, RERANK_SLOTS, RERANK_WINDOW_TOKENS};

/// How long after a server fails to start search waits before trying again.
const START_RETRY_DELAY: Duration = Duration::from_secs(5 * 60);

/// How long after a request runs out of time search waits before asking
/// again: a server this slow will not be quicker on the next turn.
const TOO_SLOW_RETRY_DELAY: Duration = Duration::from_secs(60 * 60);

/// The longest one query's passages may take to score. A GPU takes seconds;
/// past this a turn gains less from the order than it loses waiting for it.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Where the server comes from: the GGUF it serves and who starts it.
struct Serving {
    gguf: PathBuf,
    starter: Arc<dyn RerankServerStarter>,
}

/// A reranker on a llama-server.
pub struct LlamaReranker {
    serving: parking_lot::RwLock<Serving>,
    http: reqwest::Client,
    /// The server answering now. Held across a start, so concurrent searches
    /// start one server between them.
    server: tokio::sync::Mutex<Option<Arc<RunningLlamaServer>>>,
    /// No request is tried before this: a start failed, or one ran out of
    /// time.
    resting_until: parking_lot::Mutex<Option<Instant>>,
    request_timeout: Duration,
    told_missing: AtomicBool,
    told_failed: AtomicBool,
    told_slow: AtomicBool,
}

impl std::fmt::Debug for LlamaReranker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaReranker")
            .field("gguf", &self.serving.read().gguf)
            .finish_non_exhaustive()
    }
}

impl LlamaReranker {
    /// The reranker in `gguf`, served by servers `starter` starts. Nothing
    /// starts until the first query.
    pub fn new(gguf: impl Into<PathBuf>, starter: Arc<dyn RerankServerStarter>) -> Self {
        Self {
            serving: parking_lot::RwLock::new(Serving {
                gguf: gguf.into(),
                starter,
            }),
            http: reqwest::Client::new(),
            server: tokio::sync::Mutex::new(None),
            resting_until: parking_lot::Mutex::new(None),
            request_timeout: REQUEST_TIMEOUT,
            told_missing: AtomicBool::new(false),
            told_failed: AtomicBool::new(false),
            told_slow: AtomicBool::new(false),
        }
    }

    /// Serve the reranker in `gguf` through `starter` from the next query on,
    /// as tools that run without the app do.
    pub async fn serve_from(
        &self,
        gguf: impl Into<PathBuf>,
        starter: Arc<dyn RerankServerStarter>,
    ) {
        let mut server = self.server.lock().await;
        *self.serving.write() = Serving {
            gguf: gguf.into(),
            starter,
        };
        *server = None;
        *self.resting_until.lock() = None;
    }

    /// The GGUF it serves.
    pub fn gguf(&self) -> PathBuf {
        self.serving.read().gguf.clone()
    }

    /// The running server, started now when there is none or its process is
    /// gone.
    async fn server(&self) -> Result<Arc<RunningLlamaServer>> {
        let mut current = self.server.lock().await;
        if let Some(server) = current.as_ref().filter(|server| server.alive()) {
            return Ok(Arc::clone(server));
        }
        let (gguf, starter) = {
            let serving = self.serving.read();
            (serving.gguf.clone(), Arc::clone(&serving.starter))
        };
        let launch = RerankLaunch::new(gguf, starter.accelerated().await);
        match starter.start(&launch).await {
            Ok(server) => {
                self.told_failed.store(false, Ordering::SeqCst);
                tracing::info!(
                    model = %launch.name,
                    gpu_bytes = ?server.gpu_bytes(),
                    "reranking.server.started"
                );
                *current = Some(Arc::clone(&server));
                Ok(server)
            }
            Err(error) => {
                self.rest_for(START_RETRY_DELAY);
                if self.told_failed.swap(true, Ordering::SeqCst) {
                    tracing::debug!(%error, "reranking.server.failed");
                } else {
                    tracing::warn!(
                        %error,
                        retry_after_secs = START_RETRY_DELAY.as_secs(),
                        "reranking.server.failed; search keeps its first-stage order"
                    );
                }
                Err(error)
            }
        }
    }

    fn rest_for(&self, delay: Duration) {
        *self.resting_until.lock() = Some(Instant::now() + delay);
    }

    fn resting(&self) -> bool {
        self.resting_until
            .lock()
            .is_some_and(|until| Instant::now() < until)
    }

    /// Score `documents` on `server`, within the request bound.
    async fn ask(
        &self,
        server: &RunningLlamaServer,
        query: &str,
        documents: &[String],
    ) -> std::result::Result<Vec<RerankResult>, Failure> {
        client::rerank(&self.http, server, query, documents, self.request_timeout).await
    }
}

fn transport_error(server: &RunningLlamaServer, error: &reqwest::Error) -> AppError {
    AppError::Other(format!(
        "The reranking server at {} did not answer: {error}",
        server.endpoint()
    ))
}

#[async_trait]
impl Reranker for LlamaReranker {
    fn is_available(&self) -> bool {
        let gguf = self.gguf();
        if !gguf.is_file() {
            if !self.told_missing.swap(true, Ordering::SeqCst) {
                tracing::info!(
                    path = %gguf.display(),
                    "reranking.model.missing; search keeps its first-stage order until it is \
                     downloaded"
                );
            }
            return false;
        }
        !self.resting()
    }

    /// One score per document, by its index; `top_k` is ignored, since the
    /// fusion needs every candidate's rank.
    async fn rerank(
        &self,
        query: &str,
        documents: Vec<String>,
        _top_k: usize,
    ) -> Result<Vec<RerankResult>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        let (query, documents) = client::fitted(query, documents, RERANK_WINDOW_TOKENS);
        let server = self.server().await?;
        let error = match self.ask(&server, &query, &documents).await {
            Ok(results) => return Ok(results),
            Err(Failure::Server(error)) => return Err(error),
            Err(Failure::Transport(error)) => error,
        };
        if error.is_timeout() {
            self.rest_for(TOO_SLOW_RETRY_DELAY);
            if !self.told_slow.swap(true, Ordering::SeqCst) {
                tracing::warn!(
                    timeout_secs = self.request_timeout.as_secs(),
                    passages = documents.len(),
                    "reranking.too_slow; search keeps its first-stage order"
                );
            }
            // Its last holder gone, it stops holding memory, and the next try
            // places it afresh: the GPU may have room by then.
            *self.server.lock().await = None;
        }
        if server.alive() {
            return Err(transport_error(&server, &error));
        }
        tracing::warn!(
            endpoint = server.endpoint(),
            %error,
            "reranking.server.died; starting it again"
        );
        let server = self.server().await?;
        self.ask(&server, &query, &documents)
            .await
            .map_err(|failure| match failure {
                Failure::Server(error) => error,
                Failure::Transport(error) => transport_error(&server, &error),
            })
    }
}

#[cfg(test)]
mod tests;
