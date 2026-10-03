# Integration Tests

**Status (2026-10-02): the files in this directory are not compiled.** Cargo
only builds `tests/*.rs` files as test targets (and `tests/<dir>/main.rs`), and
no target declares `mod integration;`. So `test_end_to_end_indexing.rs`,
`test_search_integration.rs`, `test_tag_integration.rs`, `ipc_commands_test.rs`,
`link_parser_integration_test.rs` and `tag_generator_integration_test.rs` never
run, and `cargo test --test test_search_integration` fails with "no test target
named". They also depend on `tests/helpers/`, which is in the same state (see
its README). Treat them as reference material until someone ports them.

## Where integration tests actually live

Each top-level `src-tauri/tests/*.rs` file is its own test target. The main
groups:

| Area | Targets |
|------|---------|
| Search and retrieval | `hybrid_search_integration_test`, `search_comprehensive_tests`, `test_search_enrichment_service_integration`, `hyde_phase2_tests` |
| Indexing and extraction | `indexing_comprehensive_tests`, `indexing_output_test`, `extractor_integration_test`, `test_metadata_extraction_integration`, `test_embedding_persistence_integration`, `article_extraction_demo` |
| Web ingestion | `test_web_ingestion_service_integration` |
| Downloads | `download_manager_tests`, `download_progress_tests`, `download_redirect_test`, `download_engine_validation_test`, `test_download_helpers` |
| Commands and plugins | `commands_tests`, `plugins_tests` (includes `tests/plugins/`) |
| Security and audit | `security_audit_logging_test`, `security_rate_limiting_test`, `security_and_repositories_tests`, `native_keyring_backend_test`, `audit_events_test` |
| Lifecycle | `shutdown_tests`, `panic_hook_test`, `sidecar_guard_test` |
| Repositories and services | `tag_repository_test`, `services_comprehensive_tests`, `mock_unit_of_work_examples` |
| LLM | `inference_unit_tests` (includes `tests/inference_helpers/`) |
| Live-model evals (all `#[ignore]`) | `conversation_memory_evals`, `learning_studio_evals` |

Compiled shared helpers: `tests/common/` (download helpers, `MockHttpClient`)
and `tests/inference_helpers/`. Eval fixtures are JSON under
`tests/fixtures/conversation_memory/`. `tests/fixtures/*.rs`, `tests/mocks/`,
`tests/patterns/` and `tests/migration/` are not part of any target either.

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
`cargo test --features bindings-export --lib --bin export_bindings --test sidecar_guard_test`
and `cargo test --test security_audit_logging_test --test native_keyring_backend_test`,
plus two opt-in learning-runtime container tests.
