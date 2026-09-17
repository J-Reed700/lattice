# Embedding artifact identity: hash on activation, never on launch

Status: implemented (2026-09-16, uncommitted). See section 7 for where the build
differs from this brief.
Branch: `architecture-refactor`. Crate: `src-tauri/`. Frontend untouched.

## 1. Problem

Launching Lattice with the Qwen3-Embedding-0.6B model active hangs for ~45 s
in `tauri dev`, then aborts with SIGABRT.

Root cause, verified:

- `artifact_identity()` in `src-tauri/src/features/embedding/candle_service.rs`
  (line ~1159) streams every model artifact through SHA-256, including the
  1.19 GB `model.safetensors`, to produce the string that names the vector
  space (`sha256:<hex>`). That string decides which `usearch-<id>.usearch`
  file is opened and which rows of `embedding_generation_vectors` are restored.
- It runs synchronously on the async runtime, **twice per launch**: once in
  `build_with_compression` (`src-tauri/src/features/search/di.rs` ~line 108)
  and again inside `CandleEmbeddingService::new` (candle_service.rs ~line 310)
  when the model is loaded.
- `initialize_app_async` (`src-tauri/src/infrastructure/setup/app.rs` ~line 197)
  wraps container construction in `tokio::time::timeout(30 s)`. A tokio timeout
  cannot preempt blocking code, so it fires only after the hash returns, then
  rejects the already-built container. The `Err` propagates out of Tauri's
  `setup` hook, which runs inside a macOS callback that cannot unwind, hence the
  crash report.
- Unoptimized `sha2` in the dev profile makes the hash ~20x slower than
  release. `[profile.dev.package.sha2] opt-level = 3` is already in
  `src-tauri/Cargo.toml` and stays. It is a dev-ergonomics fix, not the fix:
  release still reads 1.2 GB from disk on every launch.

## 2. Design

Same rule as library blobs (`docs/design/2026-09-16-library-blob-lifecycle.md`):
**content is hashed when it is written, and the hash is stored next to the row
that owns it. Reads never rehash.**

- Model artifacts in a Lattice-owned model directory are immutable after
  download. Their identity is a fact about the download, not something to
  rediscover at every launch.
- The identity is **established when a model becomes the active embedding
  model** and persisted on its `models` row. Both activation paths (user
  activation and the download saga's first-download auto-activation) go
  through one repository method that refuses to activate a local model
  without an identity. The compiler enforces the invariant; startup trusts it.
- Startup reads the identity from the row. `CandleEmbeddingService` is
  constructed **with** an identity; it no longer computes one.
- Re-verification of files stays where it already lives: the explicit verify
  path in `src-tauri/src/features/llm/use_cases/download_model.rs`
  (`verify_model_files`). If a user replaces files by hand they re-activate the
  model. Not a launch concern.
- The 30 s blanket deadline is deleted. Startup failure shows the dialog and
  exits non-zero without a panic crossing Tauri's setup callback. Any remaining
  blocking work in container construction moves to `spawn_blocking`.
- No compatibility shims, no mtime/size caches, no fallback hashing at
  startup. Pre-release rule applies (`docs/design/...`, memory
  `no-legacy-support`): schema change goes into the single squashed
  migration; the local `lattice.db` is deleted.

Explicitly rejected: a metadata-validated identity cache (a second, weaker
source of truth with a stale-cache failure mode).

## 3. Shared API (both agents code against this exactly)

### 3.1 Domain value object

New file `src-tauri/src/domain/value_objects/artifact_identity.rs`, exported
from `src-tauri/src/domain/value_objects/mod.rs` (follow the existing
`sparse_embedding` export pattern).

```rust
/// Content identity of an embedding model's artifacts: `sha256:` + 64 lowercase hex.
/// Produced once, when a model is activated for embedding, and stored on its row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArtifactIdentity(String);

impl ArtifactIdentity {
    /// Parse a stored value. Rejects anything that is not `sha256:` + 64 lowercase hex.
    pub fn parse(value: &str) -> Result<Self, ArtifactIdentityError>;
    /// Build from a freshly finalized digest.
    pub fn from_digest(digest: &[u8; 32]) -> Self;
    pub fn as_str(&self) -> &str;
}
impl std::fmt::Display for ArtifactIdentity { /* as_str */ }

#[derive(Debug, thiserror::Error)]
#[error("malformed artifact identity: {0}")]
pub struct ArtifactIdentityError(String);
```

The domain type does no I/O. Computation lives in the embedding feature.

Do not reuse `domain::value_objects::Checksum`. That type is a bare 64-hex
digest of one document's bytes, accepts mixed case, and has an unchecked
`From<String>`. The artifact identity is a digest over a structured input
(prefix plus several files), carries the `sha256:` scheme, and must stay
byte-identical to the strings already baked into index filenames.

### 3.2 Computation and establishment

New file `src-tauri/src/features/embedding/artifact_identity.rs`, declared in
`src-tauri/src/features/embedding/mod.rs`.

```rust
/// Stream the artifacts under `dir` through SHA-256. Byte-for-byte the same
/// input as today's `candle_service::artifact_identity` (prefix
/// `lattice-embedding-input-v2`, same file list, same `absent` markers, pickle
/// folded only when present), so existing tests and index filenames keep
/// their values. Blocking; call through `compute_in_background` from async code.
pub fn compute(dir: &Path) -> Result<ArtifactIdentity>;

/// `spawn_blocking` wrapper around `compute`.
pub async fn compute_in_background(dir: PathBuf) -> Result<ArtifactIdentity>;

/// The identity a model must carry to be activated for embedding.
/// Remote models carry none. Local models reuse a stored identity or compute
/// one now, off the runtime thread.
pub async fn establish(model: &DownloadedModel) -> Result<Option<ArtifactIdentity>>;
```

`fold_file_contents` and `artifact_identity` move out of candle_service.rs
into this module (keep the 64 KiB streaming buffer). The two existing tests
`artifact_identity_for_safetensors_models_is_unchanged` and
`artifact_identity_covers_the_pickle_when_it_is_the_only_weights_file` move
with them, renamed to `compute_...`.

### 3.3 Domain model and repository

`src-tauri/src/domain/downloaded_model.rs`:

- `DownloadedModel` gains `embedding_artifact_identity: Option<ArtifactIdentity>`.
- `from_db(...)` gains the parameter (last position). `new` / `new_with_catalog`
  start it as `None`.
- Accessor `pub fn embedding_artifact_identity(&self) -> Option<&ArtifactIdentity>`.

`src-tauri/src/features/download/downloaded_model_repository.rs`:

- `MODEL_SELECT_BASE` selects `m.embedding_artifact_identity`;
  `DownloadedModelRecord` gains `embedding_artifact_identity: Option<String>`;
  the record-to-domain mapping parses it with `ArtifactIdentity::parse` and
  fails the read on a malformed value (it is our own write; corruption is an
  error, not something to paper over).
- `save()` persists the column in both the INSERT and the `ON CONFLICT` update.
- Signature change:

```rust
/// Activate `model_id` for embedding. `identity` is required for local
/// models; `None` is legal only for remote locations. The guard is in the
/// statement itself so the invariant holds even under concurrent callers.
pub async fn set_active_embedding_model(
    &self,
    model_id: &str,
    identity: Option<&ArtifactIdentity>,
) -> Result<()>;
```

Statement shape (one UPDATE):

```sql
UPDATE models
SET is_active_for_embedding = 1,
    embedding_artifact_identity = COALESCE(?2, embedding_artifact_identity),
    updated_at = CURRENT_TIMESTAMP
WHERE model_id = ?1
  AND (storage_kind = 'remote_ollama'
       OR COALESCE(?2, embedding_artifact_identity) IS NOT NULL)
```

Zero rows affected: distinguish "model not found" (`NotFound`) from "local
model without identity" (`InvalidState`) with a follow-up existence check.
`clear_active_embedding_model` does **not** clear the identity; it belongs to
the artifact, not to the activation.

If the statement stays a `sqlx::query!` macro, regenerate the offline cache
with `src-tauri/scripts/update-sqlx-cache.sh`. Using a runtime `sqlx::query`
(as `MODEL_SELECT_BASE` already does) is also acceptable.

### 3.4 Migration

`src-tauri/migrations/20260916000000_init_schema.sql`, table `models`
(line ~613): add

```sql
    embedding_artifact_identity TEXT
        CHECK (embedding_artifact_identity IS NULL
               OR embedding_artifact_identity GLOB 'sha256:[0-9a-f]*'),
```

No new migration file. Update `scripts/check-sql-contracts.py` fixtures only
if it enumerates `models` columns (it does not today).

### 3.5 Candle service

`src-tauri/src/features/embedding/candle_service.rs`:

```rust
impl CandleEmbeddingService {
    /// Open the artifacts under `dir` and label the service with `identity`.
    /// Does not hash anything. `identity` comes from the model's row.
    pub fn open(dir: impl AsRef<Path>, identity: ArtifactIdentity) -> Result<Self>;

    /// Compute the identity and open, for directories that have no row:
    /// env-configured structure models, eval tooling, tests.
    /// Never call this from a launch or activation path.
    pub fn open_unregistered(dir: impl AsRef<Path>) -> Result<Self>;
}
```

`new` is removed. The struct field `identity: String` becomes
`identity: ArtifactIdentity`; `model_identity()` still returns
`strategy_identity(self.identity.as_str(), self.strategy)`, so the public
identity string is unchanged.

## 4. Work split

Three agents, disjoint file ownership. **Agent A lands step A1 before B and C
start**, because both compile against the new domain type and accessor.
A1 is ~40 lines; after it, A, B and C run in parallel.

### Agent A: identity type, schema, persistence, activation

Owns: `src-tauri/src/domain/value_objects/{mod.rs,artifact_identity.rs}`,
`src-tauri/src/domain/downloaded_model.rs`,
`src-tauri/migrations/20260916000000_init_schema.sql`,
`src-tauri/src/features/download/downloaded_model_repository.rs`,
`src-tauri/src/features/download/saga.rs`,
`src-tauri/src/features/model_management/use_cases/set_active_embedding_model.rs`,
`src-tauri/src/features/embedding/artifact_identity.rs` (new; `mod.rs` line
added here too), `src-tauri/.sqlx/` if regenerated, plus any test fixtures
that call `DownloadedModel::from_db` or `set_active_embedding_model`.

- A1. Domain type (3.1), `DownloadedModel` field + accessor + `from_db` arg,
  migration column (3.4). Push nothing; just make `cargo check -j 2` pass.
  Announce completion so B and C can start.
- A2. `features/embedding/artifact_identity.rs` (3.2): **copy** `compute`
  (today's `artifact_identity`) and `fold_file_contents` here from
  candle_service.rs together with their two tests. Do not touch
  candle_service.rs; Agent B owns it and deletes the originals in B1.
- A3. Repository (3.3): SELECT, record, mapping, `save`, new
  `set_active_embedding_model` signature and guard.
- A4. `SetActiveEmbeddingModelUseCase::execute` returns
  `Result<Option<ArtifactIdentity>>`: after the existing download/type checks,
  `let identity = artifact_identity::establish(&model).await?;` then
  `set_active_embedding_model(model_id, identity.as_ref())`.
- A5. Saga auto-activation (`saga.rs` ~line 313): reload the freshly saved
  `DownloadedModel`, call `establish`, pass the result to the repository.
  This runs after the completion transaction has committed, never inside it.
- A6. Tests:
  - `ArtifactIdentity::parse` accepts a valid value, rejects uppercase hex,
    wrong length, missing prefix.
  - Repository: activating a local model with `None` fails with
    `InvalidState` and leaves `is_active_for_embedding = 0`; with `Some`
    persists and round-trips through `get_active_embedding_model`; remote
    model with `None` succeeds; re-activating with `None` after an identity
    is stored succeeds and keeps the stored value; `clear_active_embedding_model`
    keeps the identity.
  - Use case: activation stores an identity equal to `compute(dir)`;
    activating again after modifying a file in the directory does **not**
    change the stored identity (documents the immutability contract).
  - Saga: the existing auto-activation test asserts the stored identity is
    `Some` for a local embedding model.

### Agent B: consumers stop hashing

Owns: `src-tauri/src/features/embedding/candle_service.rs`,
`src-tauri/src/infrastructure/embedding_loading.rs`,
`src-tauri/src/features/search/di.rs`,
`src-tauri/src/features/model_management/commands_extra.rs`,
`src-tauri/src/features/embedding/generator.rs`,
`src-tauri/src/features/indexing/use_cases/embedding_input.rs`,
`src-tauri/src/features/conversation/chat/retrieval/corpus_plan/tests.rs`,
`src-tauri/examples/measure_artifact_identity.rs` (delete; scratch file from
an earlier session), `docs/RUST_ARCHITECTURE.md`.

- B1. `CandleEmbeddingService::open` / `open_unregistered` (3.5). Delete
  `new`, `artifact_identity`, `fold_file_contents` and their tests from
  candle_service.rs once A2 has landed the copies.
- B2. `embedding_loading.rs`: take the identity from
  `active_model.embedding_artifact_identity()`. A local active model with
  `None` is an invariant violation: return
  `ModelLoadFailed("Embedding model has no recorded identity. Activate it again in Settings.")`.
  Keep the `expected_identity` comparison (it still catches "active model
  changed since the index was opened"). Run `CandleEmbeddingService::open`
  inside `tokio::task::spawn_blocking` (mmap + Metal init are blocking).
- B3. `search/di.rs::build_with_compression`: `identity` comes from the row,
  no `artifact_identity(&dir)` call. Keep the file-presence filter as a
  sanity check only if it costs nothing (it is three `is_file` calls; fine).
  The backfill branch uses `open(&dir, identity)`. Wrap
  `ensure_index_layout_match`, `open_or_create_with_compression` and
  `rebuild_from_embeddings` in `spawn_blocking`.
- B4. `commands_extra.rs` set-active command (~line 1010): call the use case
  first, take the returned identity, then `open(&dir, identity)` for
  `generation::prepare`. Delete the pre-use-case `CandleEmbeddingService::new`.
- B5. `generator.rs`, `embedding_input.rs`, `corpus_plan/tests.rs`: these
  load directories from environment variables or test fixtures with no row.
  Use `open_unregistered`.
- B6. `docs/RUST_ARCHITECTURE.md`, "Runtime ownership": add one bullet:
  "Embedding artifact identity is established when a model is activated and
  stored on its `models` row. Launch and model load read it; nothing on the
  launch path hashes model files."
- B7. Tests: `open` labels the service with the identity it was given (assert
  `model_identity()` for `ChunkFirst` equals the input); a unit test on the
  row-to-identity mapping in `search/di.rs` if it is factored into a
  function (it should be: `fn index_identity(active: Option<&DownloadedModel>, strategy) -> Option<String>`).

### Agent C: startup path

Owns: `src-tauri/src/infrastructure/setup/app.rs`,
`src-tauri/src/infrastructure/setup/mod.rs`, `src-tauri/src/main.rs`.

- C1. Delete the `tokio::time::timeout(Duration::from_secs(30), ...)` wrapper
  around container construction in `initialize_app_async`. Keep the inner
  future and the `Ok/Err` handling. Remove the "timed out after 30 seconds"
  branch and the now-unused `Duration` import if any.
- C2. Make initialization failure terminate cleanly. Requirement: a failed
  init shows the existing error dialog, logs at error level, and the process
  exits non-zero **without** a panic crossing the `setup` callback and
  **without** a macOS crash report. Preferred shape: `initialize_app` returns
  `Ok(())` after calling `app.handle().exit(1)` on failure, and `main.rs`
  no longer `?`s it. Verify `renderer_shutdown::defer` in `main.rs` does not
  swallow that exit before the window exists; if it does, fall back to
  `std::process::exit(1)` after flushing tracing. Prove it by temporarily
  forcing a failure (for example an unwritable app directory) and confirming
  no new file appears in `~/Library/Application Support/tech.lattice.app/crashes`.
- C3. Do not add a replacement deadline. Long required work is reported by
  logging, not aborted.
- C4. Note in the doc's "Known follow-ups" below anything else you find
  blocking the runtime thread in `initialize_app_async` that is outside your
  files; do not fix it.

## 5. Verification (each agent for its own slice, then once overall)

Another session builds concurrently on this machine; use `-j 2` for cargo.

```sh
cd src-tauri
cargo fmt --all -- --check
cargo clippy -j 2 --all-targets -- -D warnings
cargo test -j 2 --lib
cargo run --bin export_bindings -- --check
cd ..
bash scripts/check-rust-layer-boundaries.sh
bash scripts/check-repository-barrier.sh
cargo test --locked --manifest-path scripts/rust-architecture-check/Cargo.toml
python3 scripts/check-sql-contracts.py
```

Crate lints deny `unwrap`, `expect`, `panic`, and slice indexing outside
tests. Bindings must not change (no DTO exposes the identity).

Manual acceptance, after `rm ~/Library/Application\ Support/tech.lattice.app/lattice.db`:

1. `npm run tauri:dev`, download or re-register Qwen3-Embedding-0.6B and
   activate it. Activation logs one identity computation.
2. Quit and relaunch. Startup log shows no hashing, "USearch vector index
   ready" within a few seconds of "Initializing DI Container", and the index
   file name is `usearch-sha256-<same hex as before>...`.
3. `grep -rn "artifact_identity(\|compute(" src-tauri/src/infrastructure/setup src-tauri/src/interfaces/di src-tauri/src/features/search/di.rs` returns nothing.

## 6. Out of scope, recorded as known follow-ups

- `generation::prepare` in `search/di.rs` runs embedding inference at startup
  to backfill missing chunk vectors. With the deadline gone it no longer
  crashes, but it can still make first launch after a model switch slow. It
  belongs in a background job with UI progress.
- `model_files.checksum_sha256` is NULL for Hugging Face downloads; the
  adapter does not capture the LFS `oid`. Download-time verification is a
  separate improvement.
- `EmbeddingGenerator` (`generator.rs`) and the `LATTICE_STRUCTURE_MODEL`
  path still compute identities on first use; they are tools, not launch
  paths.

Blocking work still on the launch thread (found in C4, not fixed).
`initialize_app` runs `initialize_app_async` under `block_on` on the main
thread, so every item below delays the first window. Paths are under
`src-tauri/src/`.

- `SystemInfoAdapter::new` (`infrastructure/system_info_adapter.rs:56`) calls
  `sysinfo::System::new_all()`, a full scan of processes, disks, networks and
  CPUs. It runs twice per launch, from `interfaces/di/modules.rs:1084` and
  `features/llm/di.rs:66`, and nothing at startup reads either result. Make it
  lazy.
- `generation::restore` (`features/embedding/generation.rs:82`, loop at
  `:93-98`), called from `search/di.rs` (~lines 160 and 190), loads every
  stored vector with its chunk text, then SHA-256es each text and decodes each
  vector inline. The cost grows
  with the library. It belongs in `spawn_blocking` next to the index rebuild.
- `summaries::di::register` (`features/summaries/di.rs:111`) reaches
  `shared_index` (`:51-59`), which calls the synchronous
  `USearchVectorIndex::open_or_create` while holding a `std::sync::Mutex`.
  This only happens when `summary_index_enabled` is on.
- `Container::refresh_allowed_roots` (`interfaces/di/container.rs:411`)
  canonicalizes the vault, every indexed path and every watch folder
  (`infrastructure/security/file_access_config.rs:287`). This is usually cheap,
  but it can stall on an unmounted or network volume.
- `setup_tokenizer` (`infrastructure/setup/embedding.rs:15`), called from
  `features/web/di.rs:48`, parses `<app_data>/models/tokenizer.json`
  synchronously. The file exists only in the old ONNX layout, so the loader
  can probably go too.
- Not on the main thread, but still synchronous I/O on a tokio worker: the
  spawned `reconcile_orphaned_files`
  (`infrastructure/services/startup_reconciliation.rs:167-212`) uses
  `read_dir`, a recursive size walk and `remove_dir_all` on model folders that
  can be several GB.

Found while proving C2 (not fixed):

- `crash::set_crashes_directory` (`infrastructure/crash/mod.rs:79`) has no
  effect. `install_panic_hook` has already filled the same `OnceLock`s with
  `<cwd>/crashes`, so panic reports go to `src-tauri/crashes` under
  `tauri dev`, not to `<app_data>/crashes`.
- `init_regular_tracing` (`infrastructure/observability/tracing.rs:155-156`)
  calls `mem::forget` on the non-blocking file writer's `WorkerGuard`, and
  `shutdown_tracing` flushes only OTEL. Log lines still queued at
  `process::exit` can be lost. The startup-failure path writes its error line
  before the blocking dialog, so that line reaches the file.
- `tracing_appender::rolling::daily` panics if it cannot create the log
  directory. `setup_tracing` runs inside the `setup` hook, so an unwritable
  `<data_local_dir>/lattice/logs` would still abort with a crash report.

Dead code found while building (not deleted):

- The pool-based `SqliteModelRepository` in
  `features/model_management/repository_tx/implementation.rs` has no callers,
  and `infrastructure/persistence/repositories/sqlite_model_repository.rs`
  (which only re-exports it) is not declared in any `mod.rs`.
- `TrackDownloadUseCase` has no callers.

## 7. As built: deviations from this brief

- `SetActiveEmbeddingModelUseCase` has `establish` (checks plus identity, no
  write), `activate(model_id, identity)` and `execute` (both). The set-active
  command calls `establish`, opens the model, runs `generation::prepare`, then
  `activate`, so a failed prepare still leaves the previous model active and
  the files are hashed once. The brief's B4 order (activate, then prepare)
  would have switched models before their vectors existed.
- The saga passes the model it just saved (identity `None`) to `establish`
  instead of reloading it. `save()` writes the column on conflict, so a
  re-download clears the old identity and the next activation recomputes it.
- Schema: the column CHECK also requires `typeof = 'text'` and a 71-byte,
  all-hex value, so it admits exactly what `ArtifactIdentity::parse` accepts.
  A table CHECK adds `is_active_for_embedding = 0 OR storage_kind =
  'remote_ollama' OR embedding_artifact_identity IS NOT NULL`, so the
  invariant holds for every write, not only the guarded UPDATE.
- `ArtifactIdentity` also derives serde (`try_from`/`into` `String`) because
  `DownloadedModel` does.
- Removed paths that could activate a local model without an identity:
  `DownloadedModel::set_active_for_embedding` and the unused embedding
  activation methods in `features/model_management/repository_tx/`.
- Removed the ONNX-era `setup_embedding_service` / `initialize_embedding_layer`
  (it called `CandleEmbeddingService::new` through the `EmbeddingService`
  alias with a file path and could never succeed).
- Startup failure exits with `std::process::exit(1)` after the dialog and
  `graceful_shutdown`. `AppHandle::exit(1)` reports status 0 and would run
  the `ExitRequested` handler, which panics on renderer-shutdown state that
  only a successful startup registers. Verified by running the binary with
  a forced directory failure and a forced database failure: dialog shown,
  status 1, no crash report.
- `examples/retrieval_eval/main.rs` also moved to `open_unregistered`.
