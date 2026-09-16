//! Run-clustering orchestration.
//!
//! 1. Load all documents + mean-pool their chunk embeddings.
//! 2. HDBSCAN.
//! 3. For each new cluster: try to inherit a label from the previous run
//!    (exact fingerprint → Jaccard ≥ threshold → LLM).
//! 4. Persist the run + clusters + members in a single transaction.
//!
//! Not wired into ingest. Runs only from the Tauri debug commands or from
//! tests.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::application::ports::{DocumentRepositoryPort, EmbeddingRepositoryPort, LLMPort};
use crate::features::corpus_shape::clustering::{
    cluster as hdbscan_cluster, mean_pool, ClusteringInput, ClusteringParams, RawCluster,
    MAX_CLUSTERING_DOCS,
};
use crate::features::corpus_shape::entity::{Cluster, ClusterMember, ClusterRun, LabelSource};
use crate::features::corpus_shape::fingerprint::{
    fingerprint, jaccard, JACCARD_INHERITANCE_THRESHOLD,
};
use crate::features::corpus_shape::labeling::{
    label_cluster, ClusterLabel, RepresentativeDoc, REP_CONTENT_PREVIEW_CHARS,
};
use crate::features::corpus_shape::repository::ClusterRepositoryPort;
use crate::shared::error::{AppError, Result};

/// Port that yields "give me the title + short content preview for doc X."
/// Kept generic so tests can fake it without dragging the whole chunking
/// pipeline in.
#[async_trait::async_trait]
pub trait DocumentContentPreviewPort: Send + Sync {
    async fn preview(&self, document_id: &str) -> Result<Option<RepresentativeDoc>>;
}

/// One step of a rebuild, for the rail's progress line. Tauri-free so the use
/// case stays testable; `cluster_vault_run` adapts it to an event.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ClusterProgress {
    /// "loading" | "clustering" | "labeling" | "saving"
    pub phase: String,
    pub current: i64,
    pub total: i64,
}

pub type ProgressSink = Arc<dyn Fn(ClusterProgress) + Send + Sync>;

/// Result surface for the use case. Primarily for the debug command.
#[derive(Debug, Clone)]
pub struct RunClusteringOutcome {
    pub run: ClusterRun,
    pub clusters: Vec<Cluster>,
    pub noise_doc_ids: Vec<String>,
}

/// Orchestrates the clustering pipeline end-to-end.
pub struct RunClusteringUseCase {
    document_repo: Arc<dyn DocumentRepositoryPort>,
    embedding_repo: Arc<dyn EmbeddingRepositoryPort>,
    cluster_repo: Arc<dyn ClusterRepositoryPort>,
    content_preview: Arc<dyn DocumentContentPreviewPort>,
    llm: Option<Arc<dyn LLMPort>>,
    params: ClusteringParams,
    progress: Option<ProgressSink>,
}

impl RunClusteringUseCase {
    pub fn new(
        document_repo: Arc<dyn DocumentRepositoryPort>,
        embedding_repo: Arc<dyn EmbeddingRepositoryPort>,
        cluster_repo: Arc<dyn ClusterRepositoryPort>,
        content_preview: Arc<dyn DocumentContentPreviewPort>,
        llm: Option<Arc<dyn LLMPort>>,
    ) -> Self {
        Self {
            document_repo,
            embedding_repo,
            cluster_repo,
            content_preview,
            llm,
            params: ClusteringParams::default(),
            progress: None,
        }
    }

    /// Attach a progress sink. Emission is decoration — a failure to deliver a
    /// step never fails the run.
    pub fn with_progress(mut self, sink: ProgressSink) -> Self {
        self.progress = Some(sink);
        self
    }

    fn emit(&self, phase: &str, current: usize, total: usize) {
        if let Some(sink) = &self.progress {
            sink(ClusterProgress {
                phase: phase.to_string(),
                current: current as i64,
                total: total as i64,
            });
        }
    }

    /// Override clustering parameters (currently only used by tests; Phase
    /// 5.3.5 will wire a settings surface).
    pub fn with_params(mut self, params: ClusteringParams) -> Self {
        self.params = params;
        self
    }

    /// Execute the full pipeline: load → cluster → label (inheriting where
    /// possible) → persist → return.
    pub async fn execute(&self) -> Result<RunClusteringOutcome> {
        let started = Instant::now();

        // 1. Load document-level embeddings. Newest first, capped: the pure
        //    stage is O(n²·d), so an unbounded vault would wedge the run.
        let documents = self
            .document_repo
            .find_all_paginated(MAX_CLUSTERING_DOCS)
            .await
            .map_err(|e| AppError::Database(format!("corpus_shape: list documents: {}", e)))?;

        self.emit("loading", 0, documents.len());
        let mut inputs: Vec<ClusteringInput> = Vec::new();
        for (index, doc) in documents.iter().enumerate() {
            if index > 0 && index % 200 == 0 {
                self.emit("loading", index, documents.len());
            }
            let doc_id = doc.id().as_str().to_string();
            let chunk_pairs = self.embedding_repo.find_by_document_id(&doc_id).await?;
            if chunk_pairs.is_empty() {
                continue;
            }
            let vectors: Vec<Vec<f32>> = chunk_pairs.into_iter().map(|(_, v)| v).collect();
            if let Some(pooled) = mean_pool(&vectors) {
                inputs.push(ClusteringInput {
                    doc_id,
                    vector: pooled,
                });
            }
        }

        let doc_count = inputs.len();
        tracing::info!(
            doc_count,
            total_documents = documents.len(),
            "corpus_shape: loaded per-doc embeddings"
        );

        // 2. Cluster. The pure stage is CPU-bound, so it goes off the async
        //    runtime rather than stalling every other task on the executor.
        self.emit("clustering", 0, doc_count);
        let params = self.params;
        let clustering_output =
            tokio::task::spawn_blocking(move || hdbscan_cluster(inputs, params))
                .await
                .map_err(|e| AppError::InternalError(format!("clustering task: {}", e)))??;

        // 3. Load previous run for label inheritance.
        let prev_run = self.cluster_repo.get_latest_run().await?;
        let prev_clusters = match &prev_run {
            Some(run) => self.cluster_repo.get_clusters_for_run(&run.id).await?,
            None => Vec::new(),
        };

        // Pre-compute prev fingerprint → cluster index for O(1) exact lookup.
        let prev_fingerprint_map: std::collections::HashMap<&str, usize> = prev_clusters
            .iter()
            .enumerate()
            .map(|(i, c)| (c.fingerprint.as_str(), i))
            .collect();

        // Track which previous-cluster indices have been claimed so a split
        // doesn't produce two copies of the same label.
        let mut claimed_prev: HashSet<usize> = HashSet::new();

        // 4. Label + fingerprint each new cluster.
        let run_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let mut new_clusters: Vec<(Cluster, Vec<ClusterMember>)> = Vec::new();
        let mut llm_call_count: i64 = 0;

        let cluster_total = clustering_output.clusters.len();
        for (index, raw) in clustering_output.clusters.iter().enumerate() {
            self.emit("labeling", index + 1, cluster_total);
            let fp = fingerprint(&raw.member_doc_ids, &raw.centroid);
            let label_outcome = self
                .resolve_label(
                    raw,
                    &fp,
                    &prev_clusters,
                    &prev_fingerprint_map,
                    &mut claimed_prev,
                )
                .await?;

            if matches!(label_outcome.source, LabelSource::Llm) {
                llm_call_count += 1;
            }

            let cluster_id = Uuid::new_v4().to_string();
            let cluster_entity = Cluster {
                id: cluster_id.clone(),
                run_id: run_id.clone(),
                label: label_outcome.label.label,
                description: label_outcome.label.description,
                member_doc_ids: raw.member_doc_ids.clone(),
                centroid: raw.centroid.clone(),
                fingerprint: fp,
                label_source: label_outcome.source,
                inherited_from_cluster_id: label_outcome.inherited_from,
                created_at: now,
            };

            let reps: HashSet<&String> = raw.representatives.iter().collect();
            let members: Vec<ClusterMember> = raw
                .member_doc_ids
                .iter()
                .zip(raw.membership_probabilities.iter())
                .map(|(id, prob)| ClusterMember {
                    document_id: id.clone(),
                    membership_probability: *prob,
                    is_representative: reps.contains(id),
                })
                .collect();

            new_clusters.push((cluster_entity, members));
        }

        let duration_ms = started.elapsed().as_millis().min(i64::MAX as u128) as i64;

        let params_hash = params_hash(&self.params);
        let run = ClusterRun {
            id: run_id.clone(),
            ran_at: now,
            doc_count: doc_count as i64,
            cluster_count: new_clusters.len() as i64,
            noise_count: clustering_output.noise_doc_ids.len() as i64,
            params_hash,
            duration_ms,
            llm_calls: llm_call_count,
        };

        // 5. Persist.
        self.emit("saving", 0, new_clusters.len());
        self.cluster_repo.save_run(&run, &new_clusters).await?;

        // 6. Return.
        let clusters_out: Vec<Cluster> = new_clusters.into_iter().map(|(c, _)| c).collect();

        Ok(RunClusteringOutcome {
            run,
            clusters: clusters_out,
            noise_doc_ids: clustering_output.noise_doc_ids,
        })
    }

    async fn resolve_label(
        &self,
        raw: &RawCluster,
        new_fingerprint: &str,
        prev_clusters: &[Cluster],
        prev_fingerprint_map: &std::collections::HashMap<&str, usize>,
        claimed_prev: &mut HashSet<usize>,
    ) -> Result<LabelOutcome> {
        // Exact fingerprint match — skip LLM entirely.
        if let Some(&idx) = prev_fingerprint_map.get(new_fingerprint) {
            if !claimed_prev.contains(&idx) {
                if let Some(prev) = prev_clusters.get(idx) {
                    claimed_prev.insert(idx);
                    return Ok(LabelOutcome {
                        label: ClusterLabel {
                            label: prev.label.clone(),
                            description: prev.description.clone(),
                        },
                        source: LabelSource::InheritedExact,
                        inherited_from: Some(prev.id.clone()),
                    });
                }
            }
        }

        // Jaccard match — highest-overlap unclaimed prev cluster wins,
        // provided overlap ≥ threshold.
        let mut best: Option<(usize, f32)> = None;
        for (i, prev) in prev_clusters.iter().enumerate() {
            if claimed_prev.contains(&i) {
                continue;
            }
            let j = jaccard(&raw.member_doc_ids, &prev.member_doc_ids);
            match best {
                Some((_, bj)) if j <= bj => {}
                _ => best = Some((i, j)),
            }
        }
        if let Some((idx, j)) = best {
            if j >= JACCARD_INHERITANCE_THRESHOLD {
                if let Some(prev) = prev_clusters.get(idx) {
                    claimed_prev.insert(idx);
                    return Ok(LabelOutcome {
                        label: ClusterLabel {
                            label: prev.label.clone(),
                            description: prev.description.clone(),
                        },
                        source: LabelSource::InheritedJaccard,
                        inherited_from: Some(prev.id.clone()),
                    });
                }
            }
        }

        // Fresh LLM call (or fallback if we don't have one).
        let mut reps: Vec<RepresentativeDoc> = Vec::new();
        for rep_id in &raw.representatives {
            if let Some(doc) = self.content_preview.preview(rep_id).await? {
                reps.push(doc);
            }
        }

        let Some(llm) = self.llm.as_ref() else {
            let hint = reps.first().map(|r| r.title.clone());
            return Ok(LabelOutcome {
                label: crate::features::corpus_shape::labeling::fallback_label(
                    raw.member_doc_ids.len(),
                    hint,
                ),
                source: LabelSource::Fallback,
                inherited_from: None,
            });
        };

        let label = label_cluster(llm, &reps, raw.member_doc_ids.len()).await?;
        // If labeling fell back inside (LLM failed / parse failed), we still
        // mark the source as "Llm" because an attempt was made — the
        // fallback path surfaces as a "Cluster of N" label, which is visible
        // to Josh.
        Ok(LabelOutcome {
            label,
            source: LabelSource::Llm,
            inherited_from: None,
        })
    }
}

struct LabelOutcome {
    label: ClusterLabel,
    source: LabelSource,
    inherited_from: Option<String>,
}

fn params_hash(params: &ClusteringParams) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"v1|");
    hasher.update(params.min_cluster_size.to_le_bytes());
    hasher.update(b"|");
    hasher.update(params.min_samples.to_le_bytes());
    // Truncate for readability.
    let digest = hasher.finalize();
    let full = hex::encode(digest);
    full.get(..16).unwrap_or(full.as_str()).to_string()
}

// Preview length constant is also exposed so callers using a custom content
// port can match the prompt's preview size.
pub const DOCUMENT_CONTENT_PREVIEW_CHARS: usize = REP_CONTENT_PREVIEW_CHARS;
