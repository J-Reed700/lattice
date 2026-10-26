# Integration Tests

Cargo discovers top-level `tests/*.rs` targets and `tests/<name>/main.rs`.
Shared helpers must be declared by one of those targets; the architecture check
rejects orphaned Rust test files. The old uncompiled
integration/helper/migration scaffolding has been removed. Useful link-parser
cases live with the parser's active unit tests; retry, circuit-breaker, tag,
and error behavior is covered by existing unit and integration targets.

## Where integration tests actually live

Each top-level `crates/lattice-desktop/tests/*.rs` file is its own test target. The main
groups (empty placeholder suites were removed; this table lists compiled targets,
not a claim that each target is comprehensive):

| Area | Targets |
|------|---------|
| Search and retrieval | `hybrid_search_integration_test`, `test_search_enrichment_service_integration`, `hyde_phase2_tests` |
| Indexing and extraction | `indexing_output_test`, `extractor_integration_test`, `test_metadata_extraction_integration`, `test_embedding_persistence_integration`, `article_extraction_demo` |
| Downloads | `download_manager_tests`, `download_progress_tests`, `download_redirect_test`, `download_engine_validation_test`, `test_download_helpers` |
| Commands and plugins | `plugins_tests` (includes `tests/plugins/`) |
| Security | `native_keyring_backend_test` |
| Lifecycle | `background_lifecycle_test`, `shutdown_tests`, `panic_hook_test`, `sidecar_guard_test` |
| Repositories and services | `tag_repository_test` |
| LLM | `inference_unit_tests` (includes `tests/inference_helpers/`) |
| Live-model evals (all `#[ignore]`) | `conversation_memory_evals`, `learning_studio_evals`, `teaching_course_evals` |

Compiled shared helpers: `tests/common/` (download helpers, `MockHttpClient`)
and `tests/inference_helpers/`. Eval fixtures are JSON under
`tests/fixtures/conversation_memory/`. These JSON fixtures remain inputs to
compiled evaluation targets.

Most behavior is tested by unit tests next to the code (`#[cfg(test)]` modules
and `features/*/tests.rs`), usually against an in-memory SQLite pool with
`sqlx::migrate!("./migrations")` applied.

`npm run test:sql` runs the SQL migration regression suite in `scripts/sql_tests/`
using Python's SQLite driver. It applies the real migrations transactionally
with foreign keys enabled and checks upgrades, document invariants, FTS
maintenance, cascades, rollback, and row identities across VACUUM. A progress
handler compares SQLite execution work across corpus sizes so a return to full
scans fails without relying on machine-dependent latency thresholds. CI runs
this suite alongside the SQL contract check. Native Rust tests still exercise
the application's SQLx connection and migration path.

Rust persistence tests also check that corrupt rows fail explicitly instead of
disappearing from aggregates, numeric metadata survives conversion without
wrapping, and embedding batches validate every vector before writing. Bulk
scope and repository tests count SQL statements and inject failures in later
batches to verify bounded query work and transaction rollback.

## Running

```bash
cd crates/lattice-desktop

cargo test --lib                                   # unit tests
cargo test --lib infrastructure::persistence        # database, mappers and repositories
cargo test --test hybrid_search_integration_test   # one integration target
cargo test --tests                                 # every integration target
cargo test --test learning_studio_evals -- --ignored --nocapture   # opt-in eval
```

The eval targets need a model endpoint; their module docs list the
`LATTICE_EVAL_*` and `LATTICE_LEARNING_EVAL_*` variables.

CI (`.github/workflows/ci.yml`) compiles every target in the workspace (`cargo check
--workspace --all-targets`, `cargo clippy --workspace --all-targets`) and runs
`cargo test --locked --workspace --features lattice-desktop/bindings-export --lib --bins --tests --no-fail-fast`
on every platform in the Rust matrix, plus two opt-in learning-runtime container
tests on Linux. A newly added integration target is automatically part of the
gate. Explicitly ignored live-model and environment-dependent tests remain
opt-in; an ignored test is not evidence that its scenario passed.

`extractor_integration_test` exercises the actual public extractor and content
adapter with temporary files: Unicode content and names, empty files, Markdown
extension variants, exact byte-size limits, unsupported formats, missing files,
directories, corrupt document archives and PDF, and text streaming boundaries.
