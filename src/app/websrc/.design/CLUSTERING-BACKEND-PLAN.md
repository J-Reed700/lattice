# Clustering Backend — Phase 5.3 Plan

**Status:** architecture spec, pre-implementation
**Paired with:** `CORPUS-SHAPE-RESEARCH.md`, `PRODUCT-THESIS.md`
**Scope:** Rust/Tauri backend only. No UI. Prove HDBSCAN + LLM labeling + label stability works on a real vault.

---

## 1. Current state investigation

**USearch.** Lives at `src/app/src/src/features/search/engine/vector_search/usearch_index.rs` as `USearchVectorIndex`. Owns a `usearch::Index` plus a `RwLock<KeyState>` with `id_to_key: HashMap<String, u64>`, `key_to_id`, and `metadata: HashMap<String, VectorMeta { chunk_id, document_id, content }>`. The `usearch` crate (v2.23) exposes neighbor search but **does not expose a "dump all vectors" API** — it's HNSW, not a flat array. However, every vector came from SQLite's `embeddings` table (bytes column, f32 little-endian) via `EmbeddingRepositoryPort::find_all_for_document`. The authoritative source for "all embeddings" is SQLite, not USearch. USearch is the ANN accelerator; SQLite is the ledger.

**Document enumeration.** `documents` table is the source of truth. `list_all_documents` Tauri command at `interfaces/commands/domains/document_list.rs` delegates to `RepositoryPort<Document>::find_all()`. Each doc has an `id`, `file_name`, `file_path`, plus extracted content via chunks.

**LLM invocation.** `Arc<dyn LLMPort>` (`application/ports/llm_port.rs`) is the backend abstraction. `Container::get_or_load_utility_llm()` returns a cheap/fast model for non-chat work — the right hook for cluster labeling (not the expensive chat LLM). `LLMPort::generate(prompt, system, ...) -> Result<String, _>` is the one-shot path we need.

**Clustering crate.** Cargo.toml does **not** currently include `hdbscan` or `petal-clustering`. Add `petal-clustering = "0.8"` — pure Rust, ndarray-native (we already depend on `ndarray = "0.16"`), one-call API, maintained by Einsteinish, used in production ML pipelines. The `hdbscan` crate is viable but has a rougher API and less active maintenance. Either works; petal-clustering wins on the dependency-graph-alignment test.

## 2. Data model

New SQLite tables (migration file, sibling to existing schema — **no changes to documents/embeddings/text_chunks**):

```sql
CREATE TABLE cluster_runs (
    id TEXT PRIMARY KEY,              -- uuid v4
    ran_at TEXT NOT NULL,             -- RFC3339
    doc_count INTEGER NOT NULL,
    cluster_count INTEGER NOT NULL,
    noise_count INTEGER NOT NULL,
    params_hash TEXT NOT NULL,        -- sha256 of (min_cluster_size, min_samples, metric, embedding_model)
    duration_ms INTEGER NOT NULL,
    llm_calls INTEGER NOT NULL        -- for cost tracking
) STRICT;

CREATE TABLE clusters (
    id TEXT PRIMARY KEY,              -- uuid v4
    run_id TEXT NOT NULL REFERENCES cluster_runs(id) ON DELETE CASCADE,
    label TEXT NOT NULL,              -- "Thai dishes", may be fallback "Cluster of 47"
    description TEXT,                 -- one-sentence centroid summary
    member_count INTEGER NOT NULL,
    centroid BLOB NOT NULL,           -- f32-LE packed, same dim as embeddings
    fingerprint TEXT NOT NULL,        -- stability key, see §4
    label_source TEXT NOT NULL,       -- "llm" | "inherited" | "fallback"
    inherited_from_cluster_id TEXT,   -- null unless label_source = "inherited"
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX idx_clusters_run_id ON clusters(run_id);
CREATE INDEX idx_clusters_fingerprint ON clusters(fingerprint);

CREATE TABLE cluster_members (
    cluster_id TEXT NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL,        -- references documents(id) but no FK (read-only overlay)
    membership_probability REAL NOT NULL,  -- HDBSCAN soft membership, 0.0-1.0
    is_representative INTEGER NOT NULL DEFAULT 0,  -- top-5 closest-to-centroid flag
    PRIMARY KEY (cluster_id, document_id)
) STRICT;

CREATE INDEX idx_cluster_members_doc ON cluster_members(document_id);
```

Noise points (HDBSCAN label `-1`) are **not** stored as a cluster. They're derivable: `documents \ union(cluster_members)`. Store `noise_count` on the run for quick reporting.

## 3. Clustering pipeline

Pseudocode — orchestrated by a new `ClusterVaultUseCase` in `features/clustering/use_cases/`:

```
1. load_document_embeddings()
   - For each document in documents table:
     - Fetch all chunk embeddings via embedding_repo.find_all_for_document(doc.id)
     - Pool to single per-document vector: mean-pool across chunks (simpler than max-pool, preserves centroid math)
     - Skip docs with zero embeddings (extraction failed or still pending)
   - Return Vec<(doc_id: String, vec: Vec<f32>)>

2. run_hdbscan()
   - Input: ndarray<f32, Ix2> of shape [n_docs, embedding_dim]
   - petal_clustering::HDbscan { min_cluster_size, min_samples, metric: Euclidean, ... }.fit(&array)
   - Use EUCLIDEAN on L2-NORMALIZED vectors (equivalent to cosine distance; petal-clustering's metric support is cleaner on euclidean)
   - Returns (clusters: Vec<Vec<usize>>, outliers: Vec<usize>) — indices into the input array
   - Starting defaults for ~1000-doc vault: min_cluster_size=5, min_samples=3
     - (Research heuristic is sqrt(N)/2 ≈ 15 for 1000; start smaller to see more structure, tune up if fragmentation)

3. build_cluster_structures()
   - For each cluster_indices in clusters:
     - members = indices mapped back to doc_ids
     - centroid = arithmetic mean of member vectors
     - reps = 5 doc_ids with smallest euclidean distance to centroid
     - membership_probabilities: from HDBSCAN's per-point cluster strength (petal returns this)

4. fingerprint_clusters()  [see §4]
   - For each cluster: compute stability fingerprint

5. reconcile_with_previous_run()  [see §4]
   - Load latest cluster_run's clusters. For each new cluster, try to inherit label from best-matching old cluster.

6. generate_labels_for_new_clusters()
   - For each cluster where label was NOT inherited:
     - Build prompt: titles of 5 reps + first 500 chars of each
     - utility_llm.generate(prompt, system="You name document clusters in 3-5 words...") -> String
     - Parse: first line = label, second line = description (or single JSON blob — decided during implementation)
     - Fallback if label is "miscellaneous", empty, or generic: use "Cluster of N" + first rep title

7. persist()
   - Insert cluster_runs row
   - Insert clusters rows (with inherited_from_cluster_id where applicable)
   - Batch insert cluster_members
   - Single transaction. Commit.

8. return ClusterRunResult { run_id, clusters: Vec<ClusterSummary>, noise_count, duration_ms }
```

All of this is triggered explicitly. Not wired into ingest. Not on a timer. For 5.3 it runs only from a Tauri command or a test.

## 4. Stability / caching — the critical part

This is what Gemini flagged as the flagship risk. The goal: if a cluster of 30 Thai recipes stays mostly the same across runs, "Thai dishes" sticks to it. If it splits into "Thai curries" and "Thai noodles," one of the new clusters inherits "Thai dishes" and the other gets fresh LLM treatment.

**Fingerprint design.** A cluster's fingerprint is a compact signature that's stable under small membership churn:

```
fingerprint(cluster) = sha256(
  "v1|" +
  sorted_doc_ids.join(",") + "|" +   # every member, sorted — detects exact re-match
  bucketed_centroid                   # quantize centroid to low precision
)
```

Where `bucketed_centroid` = the centroid vector, each dim rounded to 2 decimals, then joined. Quantizing makes tiny numerical drift irrelevant while still detecting large movements.

**Exact fingerprint match is rare and cheap.** It only hits on literal re-runs with zero changes. The meaningful case is the near-match, handled by:

**Jaccard similarity matching.** For each new cluster, find the old cluster with the highest `|new ∩ old| / |new ∪ old|`. If ≥ **0.5**, inherit the old label. Pseudo:

```
for new_cluster in new_clusters:
    best = max(old_clusters, key=|new.members ∩ old.members| / |new.members ∪ old.members|)
    overlap = jaccard(new, best)
    if overlap >= 0.5:
        new.label = best.label
        new.inherited_from = best.id
        new.label_source = "inherited"
    else:
        new.needs_llm_labeling = true
```

**Split detection.** A single old cluster may be the best match for two new clusters. That's a split. First-come-first-served: the new cluster with the highest jaccard wins the label; the runner-up falls back to fresh LLM. The winning cluster's metadata records `inherited_from`; the loser does not. (Merges are the inverse: two old clusters feeding one new cluster — that one cluster takes whichever old label has higher jaccard. The other old label is dropped. This is *correct*; a merge means the distinction no longer holds in the data.)

**Threshold (0.5).** Means "more than half the docs are the same." Below that, the cluster has drifted enough that reusing the label would be dishonest. Tunable in 5.3.5 once we see real data.

## 5. Tauri command surface

All commands live in `features/clustering/plugin/commands.rs`. Registered on the plugin. `#[specta::specta]` for TS generation.

- `cluster_vault_debug() -> Result<PathBuf>` — Runs the full pipeline and writes a human-readable JSON report to `~/Documents/recall-cluster-debug-{ISO8601}.json` with: run metadata, per-cluster { label, description, member count, 5 representative file names, inherited-from info }, noise doc count. Returns the path so Josh can open it. **This is the 5.3 milestone.** Not called from UI.
- `cluster_vault_run() -> Result<ClusterRunDto>` — Runs the pipeline and persists. Returns a summary DTO. Not user-facing yet; callable from dev tools.
- `list_clusters() -> Result<Vec<ClusterDto>>` — Returns clusters from the latest `cluster_runs` row. Will power UI in Phase 5.4. Cheap; just reads SQLite.

No unregister/delete commands in 5.3. Re-running creates a new run; old runs can be purged by a migration later.

## 6. Scope boundaries

- **NO UI work** — that's Phase 5.4.
- **NO auto-file / auto-categorization** — clusters are read-only overlay (research risk #7).
- **NO user-visible surfacing** except the debug JSON file.
- **NO changes to ingest pipeline** — clustering is a separate explicit job.
- **NO changes to USearch, documents table, embeddings table, or text_chunks.**
- **NO cross-run diffing UI / visualization** — just the debug JSON output.

## 7. Phasing within 5.3

- **5.3.1** — HDBSCAN integration. Add `petal-clustering` dep, `features/clustering/` skeleton, `load_document_embeddings` + `run_hdbscan` + `build_cluster_structures`. **Test:** synthetic 100-doc fixture (three gaussian blobs in embedding space + scattered noise), assert 3 clusters + noise tail emerge. No labels, no persistence.
- **5.3.2** — LLM labeling. Wire `get_or_load_utility_llm()`, write the prompt, test on one synthetic cluster. **Test:** given 5 docs whose titles/content are all Thai recipes, the returned label contains "Thai" or "recipe" (regex assert; loose).
- **5.3.3** — Fingerprint + caching. Implement jaccard matching, split detection, inheritance. **Three tests:** (a) re-run on identical input preserves all labels and generates zero LLM calls; (b) add 5 docs to a cluster of 30 — label inherits; (c) split a cluster of 60 into two 30s — one inherits, one is freshly labeled.
- **5.3.4** — Debug command + SQLite persistence. Migration file, repository, `cluster_vault_debug` and `cluster_vault_run` commands. **Test:** round-trip through SQLite preserves all fields; debug JSON is valid.
- **5.3.5** — Josh runs it on his actual vault. Eyeballs the output. Tune `min_cluster_size`, `min_samples`, jaccard threshold based on real data. **This is the validation gate** before Phase 5.4 UI work starts.

## 8. Critical risks

1. **Cluster label instability (Gemini's flag).** Mitigated by §4. Must pass the three caching tests in 5.3.3 before shipping.
2. **HDBSCAN doesn't know how many clusters to produce.** On a small or homogeneous vault (100 docs of similar recipes), expect 1–2 clusters + heavy noise. Josh should not read "only 2 clusters" as failure — it's the algorithm being honest. Document this expectation in the debug JSON ("heuristic: min_cluster_size=5 means clusters under 5 docs are reported as noise").
3. **LLM cost.** 30 clusters × 1 utility-LLM call = trivial locally (free, runs on-device). If we ever route to a paid API, cache aggressively; inheritance is the primary mitigation.
4. **Noise percentage.** A vault where HDBSCAN labels >50% as noise is a signal — either `min_cluster_size` is too big, or the corpus is genuinely too heterogeneous for this clustering to be useful. The debug JSON must always report the noise ratio prominently. 30% noise is fine; 70% means clustering isn't earning its place and the user should see the type/source facet instead.
5. **Per-doc embedding = mean-pool of chunk embeddings.** This is a design decision, not a bug: a doc with 20 chunks on wildly different topics will have a centroid that isn't "about" any one thing and will likely land in noise. That's correct behavior for clustering. It's also a real phenomenon in long documents; the feature won't cluster those well, and that's honest.

## 9. Success criteria

Josh eyeballs the debug JSON on his real vault and:

- **Subjective label quality** — ≥70% of cluster labels "feel right" given the representative docs shown. Labels not like "miscellaneous" or "Cluster 4." (Manual judgment; there is no automated quality metric.)
- **Cluster count is scannable** — 8–25 clusters for a 500–2000-doc vault. If 40+, bump `min_cluster_size`; if <5, lower it.
- **Stability** — run twice back-to-back: `llm_calls` on second run is 0. Add 10 docs, run again: `llm_calls` ≤ 2, existing cluster labels preserved.
- **Performance** — < 30s wall time for a 2000-doc vault on Josh's Mac (HDBSCAN itself is sub-second at this size; the LLM calls dominate, and inheritance removes most after the first run).
- **Cost** — first run on a 2000-doc vault: ≤ 30 utility-LLM calls. Subsequent runs on unchanged data: 0.

## 10. What the builder decides, not this plan

- Specific LLM prompt wording (system prompt + user template).
- Exact module decomposition inside `features/clustering/` (use_cases, service, repository split).
- Whether to parallelize LLM calls (`futures::join_all` over clusters).
- Whether to mean-pool or take the "longest chunk's embedding" for per-doc pooling — try mean-pool first.
- Exactly how to represent the debug JSON schema — goal is human readability, not machine consumption.
- Phase 5.4's decision on whether clusters are a FileBrowser `Neighborhoods` tab vs. a sidebar alongside Folders — this plan does not make that call.

---

## Report back

1. **File path:** `/Users/joshreed/Code/Recall/src/app/websrc/.design/CLUSTERING-BACKEND-PLAN.md`
2. **HDBSCAN crate recommendation:** `petal-clustering = "0.8"`. Pure Rust, ndarray-native (aligns with our existing `ndarray = "0.16"` dep), cleaner API than the `hdbscan` crate, maintained. Fall-back to the `hdbscan` crate if petal's Euclidean-only metric proves limiting — unlikely at 768-dim cosine-equivalent on normalized vectors.
3. **What USearch actually gives us:** nothing directly — USearch is HNSW and doesn't expose "dump all vectors." We read embeddings out of the authoritative SQLite `embeddings` table via `EmbeddingRepositoryPort::find_all_for_document` and mean-pool to one vector per doc.
4. **Fingerprint design:** `sha256("v1|" + sorted_member_doc_ids.join(",") + "|" + centroid_rounded_to_2dp)`. Exact fingerprint match covers identical re-runs; near-matches are caught by Jaccard ≥ 0.5 over member-id sets, which is what actually gates label inheritance.
5. **Risk above-and-beyond Gemini's flag:** **per-document embedding pooling collapses topically-heterogeneous documents.** A long journal entry covering five topics will mean-pool to a centroid that's about none of them, land in noise, and appear "invisible" to the user's cluster view. The research doc talks about mixed-corpus collapse across the vault; this is the *intra-document* version. Unavoidable without chunk-level clustering (much more complex); worth documenting so Josh isn't surprised when his messier docs don't get clustered.
6. **What Josh should know before running the debug command:** "First run will be slow-ish — one utility-LLM call per cluster (probably ~15–30). Second run on the same data should take zero LLM calls and produce identical labels. If it doesn't, the fingerprint/inheritance logic is broken and that's a 5.3.3 bug, not a modeling problem." That's the single check that tells him whether stability works.
