# Integration Tests

Cargo discovers top-level `tests/*.rs` targets and `tests/<name>/main.rs`.
Shared helpers must be declared by one of those targets; the architecture check
rejects orphaned Rust test files. The old uncompiled
integration/helper/migration scaffolding has been removed. Useful link-parser
cases live with the parser's active unit tests; retry, circuit-breaker, tag,
and error behavior is covered by existing unit and integration targets.

## Where integration tests actually live

Each top-level `src-tauri/tests/*.rs` file is its own test target. The main
groups (empty placeholder suites were removed; this table lists compiled targets,
not a claim that each target is comprehensive):

| Area | Targets |
|------|---------|
| Search and retrieval | `hybrid_search_integration_test`, `test_search_enrichment_service_integration`, `hyde_phase2_tests` |
| Indexing and extraction | `indexing_output_test`, `extractor_integration_test`, `test_metadata_extraction_integration`, `test_embedding_persistence_integration`, `article_extraction_demo` |
| Downloads | `download_manager_tests`, `download_progress_tests`, `download_redirect_test`, `download_engine_validation_test`, `test_download_helpers` |
| Commands and plugins | `plugins_tests` (includes `tests/plugins/`) |
| Security and audit | `security_audit_logging_test`, `security_rate_limiting_test`, `native_keyring_backend_test`, `audit_events_test` |
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

## Running

```bash
cd src-tauri

cargo test --lib                                   # unit tests
cargo test --test hybrid_search_integration_test   # one integration target
cargo test --tests                                 # every integration target
cargo test --test learning_studio_evals -- --ignored --nocapture   # opt-in eval
```

The eval targets need a model endpoint; their module docs list the
`LATTICE_EVAL_*` and `LATTICE_LEARNING_EVAL_*` variables.

CI (`.github/workflows/ci.yml`) compiles every target (`cargo check
--all-targets`, `cargo clippy --all-targets`) but only runs
`cargo test --features bindings-export --lib --bin export_bindings --test sidecar_guard_test`,
`cargo test --test security_audit_logging_test --test native_keyring_backend_test`,
and `cargo test --test background_lifecycle_test --test tag_repository_test --test plugins_tests`,
plus two opt-in learning-runtime container tests.
