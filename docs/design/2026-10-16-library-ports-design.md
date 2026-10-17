# PR 4 design: library ports into core (2026-10-16)

The baseline is 147 feature-edge refs (main + PR 3). This design brings it to 82 (−65; the plan said −52), split into PRs 4a, 4b and 4c.

**Correction to the plan, §2.** Only `WebServiceTrait` depends on core alone. `TagServiceTrait` names `tags::{entity::Tag, dto::TagWithCountDto, service::DocumentLockGuard}`. `IndexStorageTrait` names `indexing::engine::{chunker::TextChunk, storage::DocumentRecord, error::Result}`.

## Decisions

### Q1. Index generation between search and embedding

| Site | What it is |
|---|---|
| `runtime_index.rs:4` | `EmbeddingIndexBinding`. Embedding declares it (`embedding/loading.rs:15-24`) and search implements it (`runtime_index.rs:157`). It is called inside `EmbeddingLoader`'s model load (`loading.rs:144`) and takes `&sqlx::SqlitePool`. |
| `runtime_index.rs:105` | `generation::prepare`, which re-embeds `text_chunks` into `embedding_generation_vectors`. |
| `runtime_index.rs:108` | `generation::restore`. |
| `persistence.rs:165` | `generation::RestorePage`. |
| `qwen3_reranker.rs:82` | `input_policy::load_tokenizer`. Pure and unrelated to index generation. |

**Ownership.** Search owns the index lifecycle. `generation.rs` (plain SQL) moves to `infrastructure/persistence/repositories/embedding/generation.rs` as free functions with no port. Every caller already holds a pool.

**Port.** Add one port, `ports/embedding_index_binding.rs`:

```rust
async fn bind_first(&self, model: &dyn EmbeddingPort, late_chunking: bool) -> Result<()>
```

`RuntimeVectorIndex::new` takes a `pool` field (`search/di.rs:269`), and `EmbeddingLoader.pool` is deleted (`embedding/di.rs:36`).

**Tokenizer.** `load_tokenizer` moves to `infrastructure/ml/tokenizer.rs`.

### Q2. `USearchVectorIndex`

Make it an infrastructure adapter with no new port. It already implements core's `VectorSearchPort` (`usearch_index/ports.rs:19`). Explorer and summaries only use the open/save methods.

**Move to `infrastructure/vector_index/`:** `usearch_index{.rs,/*}`, `compression.rs`, `rescore_store.rs`, `dimension_metadata.rs`, `IndexConfig` (`manifest.rs:55`) and `engine/vector_ops.rs`. The `SearchServiceTrait` impl (`ports.rs:316`) moves too, once the trait is in core.

**Stays in search:** `manifest`, `persistence` and `runtime_index`.

This is **PR 4b**, which depends on step 5 of 4a.

### Q3. `EmbeddingServiceTrait` vs `EmbeddingPort`

Switch callers to `EmbeddingPort` and delete the trait. The method sets are equivalent:
- `embed_contextualized_chunks` has no callers outside the trait's default and Candle's own impl.
- The `embed_span_chunks` default is equivalent, because every impl embeds `contextualized_content`.
- Both use the same `AppError` result.

**Callers:**
- function_calling: `search_tools.rs:174` (`embed_query`)
- web: `ingestion/service.rs:165-200` and `index.rs:30`

**Behaviour change when no model is loaded:**
- `split_text` now errors with `AiModelsNotInstalled("Load an embedding model before indexing")`. That is the same variant; only the message differs.
- `model_identity` returns "unavailable" instead of "unknown".

**Deletions:**
- `DynamicEmbeddingService`
- the trait impls on Candle and the mocks
- `RemoteEmbeddingService` (no callers; `remote_service.rs:122`)

Delete `RemoteEmbeddingService` in a separate commit.

**Owned `container.rs:20-21`:** change imports only, using the aliases `DynamicEmbedding as DynamicEmbeddingService` and `EmbeddingPort as EmbeddingServiceTrait`. If that isn't possible, fall back to a `pub(crate) use` shim, which costs +1 feature-reexport.

### Q4. `health/di.rs:20`

The health check is fake today: `MockLLMPort::is_ready` returns `Ok(true)`, so the LLM is always reported healthy.

Add a core `DegradedLLMPort` in `ports/mock_llm_port.rs`, next to `MockEmbeddingPort::new_degraded`:
- `is_ready` returns `false`
- `complete` returns `Err(AiModelsNotInstalled)`
- `model_name` returns "unavailable"
- `max_context` is 4096

As a result the `llm` flag flips from true to false, and the overall status is unchanged. This goes in **PR 5**.

### Q5. sqlx row types

Follow the PR 2 precedent:
- `WorkspaceNoteRecord`, `NoteTimestampRecord` and `VaultNoteUpsert<'a>` move to `contracts::notes`, with no `FromRow` and no specta.
- `daily_notes/repository/mod.rs` moves to `infrastructure/persistence/repositories/workspace_notes.rs`. Its 12 `query_as` calls become `query(..).try_map(note_from_row/timestamp_from_row)`, following `references/repository.rs:19,75`.
- `timestamp_from_row` becomes `pub(crate)` for `vault/repository.rs:83-97`.
- `learning/repository/memory.rs:44` calls the infrastructure function `insert_in_transaction` directly, because it has to run inside its own transaction.

## PR 4a: ordered moves (about 45 files)

1. **Embedding callers.** Move callers to `EmbeddingPort` and delete the trait and `DynamicEmbeddingService`. `web::di::build` changes to `(model_dir, embedding: Arc<dyn EmbeddingPort>, store: Arc<dyn IngestedDocumentStorePort>, vector_search, query_cache)` and is built in `interfaces/di/modules/indexing.rs:54`. The mocks implement `EmbeddingPort`.
2. **Tokenizer.** Move `load_tokenizer` to `infrastructure/ml/tokenizer.rs`. Callers: `candle_service/loading.rs:132` and `qwen3_reranker.rs:82`.
3. **`EmbeddingIndexBinding`** goes to core ports (see Q1).
4. **`generation.rs`** goes to infrastructure. Update search's `runtime_index`, `persistence`, `di` and tests, plus `active_models.rs:237`.
5. **Search traits and types.** Move `SearchServiceTrait`, `BM25SearchTrait` and `SparseSearchTrait` to `ports/search_engines.rs`. Move `SearchResult`, `BM25Result` and their `From` impl to `contracts/search/engine.rs`.
   - Keep the `LibrarySearchTrait` shim in `trait_def.rs` for `test_runtime`.
   - function_calling's `hybrid_search` becomes `Arc<dyn LibrarySearchTrait>` and calls `.execute_scoped(req, None, None)`.
6. **Web.**
   - `WebServiceTrait` goes to `ports/web_service.rs`.
   - `REFERENCE_TEXT_EXTRACTION_VERSION` goes to `contracts::web`.
   - `WebArticleDetector` goes to `infrastructure/adapters/fs/web_article.rs`.
7. **Tags.**
   - `Tag` goes to `domain/entities/tag.rs`.
   - `TagWithCountDto` (a specta type) goes to `contracts/tags.rs`.
   - `TagServiceTrait` and `DocumentLockGuard` go to `ports/tag_service.rs`; `DocumentLockGuard::new` becomes `pub`.
   - `TagRepositoryTrait` stays in tags.
8. **Indexing vocabulary.**
   - Import `Embedding` from core.
   - Rename `MODEL_NAME` to `DEFAULT_EMBEDDING_MODEL_NAME`.
   - Move `SpanEmbeddingGroup` to `ports::embedding_port`.
   - Move `PublishedChunk` and `publish_chunks_to_live_index` to `services/live_index.rs`, with the signature `(search: Arc<dyn VectorSearchPort>, query_cache: Option<&dyn SearchResultCachePort>, document_id: &str, chunks: Vec<PublishedChunk>) -> Result<()>`.
9. **New `IngestedDocumentStorePort`** (`ports/ingested_document_store.rs`, implemented by `IndexStorage`), with these methods:
   - `store_document_with_context_for_model(&self, path: &Path, mime_type: &str, chunks: Vec<ContextualizedChunk>, embeddings: Vec<Vec<f32>>, model_identity: &str) -> Result<String>`
   - `chunk_ids_in_order(&self, document_id: &str) -> Result<Vec<String>>`
10. **`SystemStatsDto`** goes from `health/dto.rs:24` to `contracts/system_stats.rs`.
11. **`resolve_vault_root`** goes from `vault/writeback.rs:406` to `shared::fs::roots`. Keep a delegating fn in vault for the owned `settings_side_effects.rs:67`.
12. **New `SettingsReaderPort`** (`ports/settings_reader.rs`, implemented by `GetSettingsUseCase`) with `current_settings(&self) -> Result<SettingsDto>`.
    - Vault's `start_vault_watcher` and `start_vault_writer` take `Arc<dyn SettingsReaderPort>`.
    - `container.rs:359,465` coerces to the port with no text change.

## PR 4a as built

Feature edges 147 → 93 (−54, as estimated). Feature re-exports 17 → 15. `infrastructure-imports-feature` stays 0. Deviations:

- **Step 12 does not coerce.** `Arc::clone(system.get_settings_use_case())` infers `T = dyn SettingsReaderPort` from the expected type and fails (E0308). So `start_vault_writer` and `start_vault_watcher` take `Arc<impl SettingsReaderPort + 'static>`, and `container.rs` is unchanged there. The writer erases the type to `Arc<dyn SettingsReaderPort>` for its worker.
- **Q3: `embed_contextualized_chunks` has one more caller.** The retrieval eval (`examples/retrieval_eval/production.rs`) calls Candle's inherent method. That method stays; only the trait impl is gone. The example's `load_tokenizer` and `SparseSearchTrait` imports moved too.
- **Step 3.** `RuntimeVectorIndex::new(initial, identity, path, compression, persistence, pool, background)`.
- **Step 5.** The `LibrarySearchTrait` shim in `search/trait_def.rs` is now `#[cfg(test)]`, because only `test_runtime.rs` uses it. The impl names the trait by its full path. This is −1 feature-reexport.
- **Step 8.** `embedding/entity.rs` (the `Embedding` compatibility re-export) is deleted, because every caller now imports from core. This is −1 feature-reexport.
- **Step 9.** The web test doubles implement the two-method `IngestedDocumentStorePort` instead of `IndexStorageTrait`. The builder keeps the name `index_storage`.
- **Step 11.** Vault's own callers and the three `resolve_vault_root` tests moved to core as well. The delegating fn in `vault/writeback.rs` remains only for `settings_side_effects.rs`.
- **`RemoteEmbeddingService`.** Deleting it also deletes `EmbeddingServiceTrait` (`trait_def.rs`), because it was the trait's last implementor. Until then, the trait stays alive only for it, so the deletion reviews on its own.

## PR 4b as built

The USearch adapter now lives in `src-tauri/src/infrastructure/vector_index/`: `usearch_index{.rs,/*}` (with the `SearchServiceTrait` and `VectorSearchPort` impls in `usearch_index/ports.rs`), `compression.rs`, `rescore_store.rs`, `dimension_metadata.rs`, `vector_ops.rs` and `vector_ops_test.rs`, all moved with `git mv`. `features/search/engine/vector_search/` keeps `manifest`, `persistence` and `runtime_index`, and no longer re-exports any adapter type; every caller imports from `crate::infrastructure::vector_index` (the retrieval eval from `lattice::infrastructure::vector_index`).

Feature edges 93 → 89 (explorer→search −3, summaries→search −1). Feature re-exports stay 15. `infrastructure-imports-feature` stays 0. Deviations:

- **`manifest_path_for` moved with `IndexConfig`.** The index invalidates the manifest before every save (`usearch_index/persistence.rs`) and `wipe_index_files` deletes it (`dimension_metadata.rs`), so both need the path. Both now sit in a new `vector_index/index_config.rs`; the manifest itself stays in search and imports them.
- **`write_file_atomically` is `pub(crate)`** (was `pub(super)`), because `manifest::write_manifest` now reaches it across modules.

## PR 4c: notes

Move the records and the repository to infrastructure using `try_map` (see Q5), and add `ports::WorkspaceNotesPort` with these methods:
- `get`
- `get_timestamp`
- `upsert_from_vault_if_unqueued`
- `delete_from_vault_if_unqueued`

`capture.rs` stays in daily_notes. It needs `pub(crate) NOTE_COLUMNS` and `insert_in_transaction`.

The owned `port_bindings.rs:77,86,141,157` gets path changes only.

## Estimates

| Pair | Δ | Left |
|---|---|---|
| learning→web | −13 | 0 |
| search→embedding | −7 | 5 (`di.rs`, PR 5) |
| web→indexing | −7 | 0 |
| function_calling→search / embedding / tags / web | −4 / −1 / −1 / −1 | 0 |
| web→embedding | −4 | 0 |
| indexing→embedding | −4 | 2 (`di.rs`, PR 5) |
| file→tags, file→web | −2 each | 0 |
| stats→tags | −2 | 1 (PR 5) |
| stats→health | −1 | 0 |
| vault→settings, backup→vault | −2 each | 0 |
| model_management→embedding | −1 | 4 (PR 6) |

- **4a:** −54, to 93
- **4c:** vault→daily_notes −4, learning→daily_notes −3
- **4b:** explorer→search −3, summaries→search −1
- **Total:** −65, to 82

## Risks and checks

**Owned files.** These get edits in files another session owns, so confirm with that session:
- `container.rs:20-21`: import aliases
- `port_bindings.rs`: path changes

**Implicit coercions** to check with `cargo check`:
- `Arc<HybridSearchUseCase>` to `Arc<dyn LibrarySearchTrait>`
- `Arc<GetSettingsUseCase>` to `Arc<dyn SettingsReaderPort>`
- `Arc<WebService>` to `dyn WebServiceTrait`

**Bindings.** `TagWithCountDto` is the only specta type that moves. Confirm the generated bindings stay byte-identical.

**Name clash.** Core already has `domain::entities::SearchResult`. Keep the moved type at `contracts::search::engine` and never glob-import it.

**New in core.** `publish_chunks_to_live_index` is the first `spawn_blocking` in core's `application` layer. It is allowed, but new.

**Notes `try_map`.** The column order must match `NOTE_COLUMNS`. Run the daily_notes, vault and learning tests.

**Ratchet.** `infrastructure-imports-feature` must stay at 0 after the moves. Regenerate the baseline with `--write-baseline`.

## Split out of PR 4

- **PR 4b:** USearch.
- **PR 4c:** notes.
- **PR 5:**
  - `DegradedLLMPort`
  - `hydrate_workspace_note`
  - learning→vault (the plugin)
  - corpus_shape, indexing and search DI wiring
- **Follow-ups:**
  - delete `IndexStorageTrait`
  - move `MentionRepository` to infrastructure (−1 indexing→mentions)
