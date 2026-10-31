//! Embeddings served by the bundled llama-server.
//!
//! [`LlamaEmbeddingService`] is the app's `EmbeddingPort`: it splits text with
//! the model's own tokenizer, prefixes it as the model's family was trained,
//! and sends each batch as one `/v1/embeddings` request. It knows the server
//! only as a [`RunningLlamaServer`]; starting one is the composition
//! root's job, through an [`EmbeddingServerStarter`].

mod client;
mod facts;
mod identity;
mod launch;

use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::application::ports::embedding_port::EmbeddingTextChunk;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::models::downloaded::{DownloadedModel, ModelLocation};
use lattice_core::domain::value_objects::ArtifactIdentity;
use lattice_core::shared::error::{AppError, Result};
use lru::LruCache;

use crate::features::embedding::input_policy::InputPolicy;
use crate::features::embedding::prefixes::EmbeddingPrefixes;
use crate::infrastructure::llama_server::RunningLlamaServer;
use crate::infrastructure::passage_index::build::BATCH_SIZE;

pub use facts::{EmbeddingHeader, EmbeddingModelFacts, Pooling};
pub(crate) use identity::{resolve_prefixes, space_identity};
pub use launch::{EmbeddingLaunch, EmbeddingServerStarter, EmbeddingWork, SERVER_SLOTS};

use client::Failure;

/// Queries remembered per service. A query is embedded on every search and
/// the same few recur; documents are embedded once and never cached.
const QUERY_CACHE_CAPACITY: usize = 256;

/// Passages a request needs to count as indexing: a full batch of the passage
/// index, which off the GPU takes about as long as a chat model takes to load.
const INDEXING_PASSAGES: usize = BATCH_SIZE;

/// How a dead server is brought back: the starter that started it and what
/// it was started for. Only a service that started its own server has one.
struct Restart {
    starter: Arc<dyn EmbeddingServerStarter>,
    launch: EmbeddingLaunch,
    /// Held while one caller restarts, so concurrent failures restart once.
    gate: tokio::sync::Mutex<()>,
}

/// An embedding model on a llama-server.
pub struct LlamaEmbeddingService {
    server: parking_lot::Mutex<Arc<RunningLlamaServer>>,
    restart: Option<Restart>,
    http: reqwest::Client,
    facts: EmbeddingModelFacts,
    policy: InputPolicy,
    prefixes: &'static EmbeddingPrefixes,
    /// One permit per server slot: a batch takes one for its request.
    slots: tokio::sync::Semaphore,
    query_cache: parking_lot::Mutex<LruCache<String, Arc<Vec<f32>>>>,
}

impl LlamaEmbeddingService {
    /// A service over `server`, which something else started and keeps
    /// running. Knows nothing of processes: a dead server is an error.
    pub fn connect(
        server: Arc<RunningLlamaServer>,
        facts: EmbeddingModelFacts,
        policy: InputPolicy,
        prefixes: &'static EmbeddingPrefixes,
    ) -> Self {
        Self {
            server: parking_lot::Mutex::new(server),
            restart: None,
            http: reqwest::Client::new(),
            facts,
            policy,
            prefixes,
            slots: tokio::sync::Semaphore::new(SERVER_SLOTS as usize),
            query_cache: parking_lot::Mutex::new(LruCache::new(
                NonZeroUsize::new(QUERY_CACHE_CAPACITY).unwrap_or(NonZeroUsize::MIN),
            )),
        }
    }

    /// Read `model`'s header, start its server through `starter`, and
    /// connect. `identity` is the SHA-256 of its GGUF; `tokenizer.json`
    /// beside the GGUF is what the splitter counts with.
    pub async fn start(
        starter: Arc<dyn EmbeddingServerStarter>,
        model: &DownloadedModel,
        identity: ArtifactIdentity,
    ) -> Result<Self> {
        let ModelLocation::LocalFile { path: gguf } = model.location() else {
            return Err(AppError::ModelLoadFailed(format!(
                "{} is not a GGUF file; embeddings are served from one",
                model.model_name()
            )));
        };
        Self::start_gguf(
            starter,
            gguf,
            model.model_name(),
            model.model_id(),
            identity,
        )
        .await
    }

    /// [`Self::start`] for the GGUF at `gguf` rather than a model row. `name`
    /// is what errors and logs call it; `model_ref` (a catalog id, repo id or
    /// file name) picks its family's prefixes.
    pub async fn start_gguf(
        starter: Arc<dyn EmbeddingServerStarter>,
        gguf: &Path,
        name: &str,
        model_ref: &str,
        identity: ArtifactIdentity,
    ) -> Result<Self> {
        let header = starter.header(gguf).await.ok_or_else(|| {
            AppError::ModelLoadFailed(format!(
                "The GGUF header of {name} cannot be read: {}",
                gguf.display()
            ))
        })?;
        let file_type = header.file_type;
        let facts = EmbeddingModelFacts::from_header(header, identity, name)?;
        let policy = tokenizer_policy(gguf, &facts).await?;
        let prefixes = resolve_prefixes(model_ref, &facts.architecture);
        let launch = EmbeddingLaunch::new(gguf.to_path_buf(), &facts, starter.accelerated().await);
        tracing::info!(
            model = %facts.display_name,
            architecture = %facts.architecture,
            dimension = facts.dimension,
            pooling = ?facts.pooling,
            file_type,
            window = launch.window_tokens,
            slots = launch.slots,
            gpu = launch.gpu,
            prefixes = prefixes.id,
            "embedding.server.starting"
        );
        let server = starter.start(&launch).await?;
        let mut service = Self::connect(server, facts, policy, prefixes);
        service.slots = tokio::sync::Semaphore::new(launch.slots as usize);
        service.restart = Some(Restart {
            starter,
            launch,
            gate: tokio::sync::Mutex::new(()),
        });
        Ok(service)
    }

    /// What the header and identity say about the model.
    pub fn facts(&self) -> &EmbeddingModelFacts {
        &self.facts
    }

    /// Requests in flight to its server now.
    pub fn in_flight(&self) -> u32 {
        self.current().in_flight()
    }

    /// GPU memory its server holds; `None` on the CPU.
    pub fn gpu_bytes(&self) -> Option<u64> {
        self.current().gpu_bytes()
    }

    fn current(&self) -> Arc<RunningLlamaServer> {
        Arc::clone(&self.server.lock())
    }

    /// Vectors for `inputs`, already prefixed, in input order. A server whose
    /// process died is started again once, for `work`, and the batch sent once
    /// more; an answer from a live server is final.
    async fn embed_inputs(&self, inputs: &[String], work: EmbeddingWork) -> Result<Vec<Vec<f32>>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let _slot = self
            .slots
            .acquire()
            .await
            .map_err(|e| AppError::InternalError(format!("Embedding slots closed: {e}")))?;
        let server = self.current();
        let error = match client::embed(&self.http, &server, inputs).await {
            Ok(embedded) => return self.checked(embedded),
            Err(Failure::Server(error)) => return Err(error),
            Err(Failure::Transport(error)) => error,
        };
        let restart = match &self.restart {
            Some(restart) if !server.alive() => restart,
            _ => return Err(transport_error(&server, &error)),
        };
        tracing::warn!(
            endpoint = server.endpoint(),
            %error,
            "embedding.server.died; starting it again"
        );
        let server = self.restarted(restart, &server, work).await?;
        match client::embed(&self.http, &server, inputs).await {
            Ok(embedded) => self.checked(embedded),
            Err(Failure::Server(error)) => Err(error),
            Err(Failure::Transport(error)) => Err(AppError::EmbeddingFailed {
                reason: format!(
                    "The embedding server was started again after its process died, and failed \
                     again: {}",
                    transport_error(&server, &error)
                ),
            }),
        }
    }

    /// A running server in place of `dead`: the one another caller already
    /// started, or a new one from the starter, which admits it and walks the
    /// sidecar's fallback ladder as any start does.
    async fn restarted(
        &self,
        restart: &Restart,
        dead: &Arc<RunningLlamaServer>,
        work: EmbeddingWork,
    ) -> Result<Arc<RunningLlamaServer>> {
        let _gate = restart.gate.lock().await;
        let current = self.current();
        if !Arc::ptr_eq(&current, dead) && current.alive() {
            return Ok(current);
        }
        let server = restart
            .starter
            .start(&restart.launch.for_work(work))
            .await?;
        *self.server.lock() = Arc::clone(&server);
        Ok(server)
    }

    /// Bring a server that was kept off the GPU onto it for indexing, when the
    /// GPU has room for it by now. One that stays off keeps serving, as does
    /// the one running if the start fails; a dead one is `embed_inputs`' to
    /// bring back.
    async fn move_to_gpu(&self, restart: &Restart) {
        let off_the_gpu = || {
            let server = self.current();
            server.alive() && server.gpu_bytes().is_none()
        };
        if !restart.launch.gpu || !off_the_gpu() {
            return;
        }
        let _gate = restart.gate.lock().await;
        if !off_the_gpu() {
            return;
        }
        let launch = restart.launch.for_work(EmbeddingWork::Indexing);
        match restart.starter.start(&launch).await {
            Ok(server) => *self.server.lock() = server,
            Err(error) => tracing::warn!(%error, "embedding.server.move_to_gpu_failed"),
        }
    }

    /// The vectors of `embedded`, once each is as wide as the header said.
    /// The tokens the server counted are logged for pace diagnostics.
    fn checked(&self, embedded: client::Embedded) -> Result<Vec<Vec<f32>>> {
        let vectors = embedded.vectors;
        tracing::debug!(
            inputs = vectors.len(),
            prompt_tokens = embedded.prompt_tokens,
            "embedding.request"
        );
        if let Some(vector) = vectors.iter().find(|v| v.len() != self.facts.dimension) {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "{}'s header says its vectors have {} dimensions; the server returned {}",
                    self.facts.display_name,
                    self.facts.dimension,
                    vector.len()
                ),
            });
        }
        Ok(vectors)
    }

    fn as_documents(&self, texts: &[String]) -> Result<Vec<String>> {
        texts
            .iter()
            .map(|text| {
                refuse_empty(text)?;
                Ok(self.prefixes.document_input(text).into_owned())
            })
            .collect()
    }
}

/// The splitter for the model at `gguf`, counting with the `tokenizer.json`
/// beside it up to the window each server slot has.
async fn tokenizer_policy(gguf: &Path, facts: &EmbeddingModelFacts) -> Result<InputPolicy> {
    let path = gguf
        .parent()
        .map(|dir| dir.join("tokenizer.json"))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            AppError::ModelLoadFailed(format!(
                "{} has no tokenizer.json beside its GGUF ({}), so its passages cannot be split",
                facts.display_name,
                gguf.display()
            ))
        })?;
    let window = facts.window_tokens() as usize;
    tokio::task::spawn_blocking(move || {
        let tokenizer = crate::infrastructure::ml::tokenizer::load_tokenizer(&path)?;
        InputPolicy::new(tokenizer, window)
    })
    .await
    .map_err(|e| AppError::InternalError(format!("Tokenizer load task failed: {e}")))?
}

fn refuse_empty(text: &str) -> Result<()> {
    if text.is_empty() {
        return Err(AppError::InvalidInput("Cannot embed empty text".into()));
    }
    Ok(())
}

fn transport_error(server: &RunningLlamaServer, error: &reqwest::Error) -> AppError {
    AppError::EmbeddingFailed {
        reason: format!(
            "The embedding server at {} did not answer: {error}",
            server.endpoint()
        ),
    }
}

#[async_trait]
impl EmbeddingPort for LlamaEmbeddingService {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let mut vectors = self
            .embed_inputs(
                &self.as_documents(&[text.to_owned()])?,
                EmbeddingWork::Light,
            )
            .await?;
        vectors.pop().ok_or_else(|| AppError::EmbeddingFailed {
            reason: "The embedding server returned no vector".into(),
        })
    }

    async fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        refuse_empty(text)?;
        let query = self.prefixes.query_input(text).into_owned();
        if let Some(cached) = self.query_cache.lock().get(&query).map(Arc::clone) {
            return Ok(Vec::clone(&cached));
        }
        let vector = self
            .embed_inputs(std::slice::from_ref(&query), EmbeddingWork::Light)
            .await?
            .pop()
            .ok_or_else(|| AppError::EmbeddingFailed {
                reason: "The embedding server returned no vector".into(),
            })?;
        let vector = Arc::new(vector);
        self.query_cache.lock().put(query, Arc::clone(&vector));
        Ok(Vec::clone(&vector))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let inputs = self.as_documents(texts)?;
        if inputs.len() < INDEXING_PASSAGES {
            return self.embed_inputs(&inputs, EmbeddingWork::Light).await;
        }
        if let Some(restart) = &self.restart {
            self.move_to_gpu(restart).await;
        }
        self.embed_inputs(&inputs, EmbeddingWork::Indexing).await
    }

    fn split_text(&self, text: &str, prefix: &str) -> Result<Vec<EmbeddingTextChunk>> {
        // The model's document prefix is part of every passage sent, so the
        // splitter measures it against the window too, outside the chunker's
        // context prefix, in the order `embed_batch` builds.
        if self.prefixes.document.is_empty() {
            return self.policy.split(text, prefix);
        }
        self.policy
            .split(text, &format!("{}{prefix}", self.prefixes.document))
    }

    fn window_parts(&self, text: &str) -> Result<Vec<EmbeddingTextChunk>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }
        let tokens = self.policy.count(&self.prefixes.document_input(text))?;
        if tokens > self.policy.max_tokens {
            return self.split_text(text, "");
        }
        Ok(vec![EmbeddingTextChunk {
            text: text.to_owned(),
            start: 0,
            end: text.len(),
            token_count: tokens,
        }])
    }

    fn model_identity(&self) -> String {
        space_identity(&self.facts.identity, self.prefixes)
    }

    fn dimension(&self) -> usize {
        self.facts.dimension
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(self.current().alive())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod parity_tests;

#[cfg(test)]
mod throughput_bench;
