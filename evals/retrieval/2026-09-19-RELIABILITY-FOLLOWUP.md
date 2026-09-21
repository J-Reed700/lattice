# Reliability follow-up (2026-09-19)

This follow-up addresses the gaps identified in the modernization review.
It does not certify the entire application or all hardware configurations.

## Changes

- Startup compares SQLite's vector count with the loaded index and checks
  generation membership during content hydration. Pending publications that
  add or swap chunks therefore cannot be silently accepted after a crash.
- A save invalidates its old manifest before replacing the graph or keymap.
  Its vector count is captured under the snapshot lock. Background and shutdown
  flushes on the same persistence controller are serialized. The source count
  and write counter are read in one SQLite statement.
- Manifest version 2 rejects snapshots certified by the old logic. Existing
  installations rebuild the graph once from saved embeddings; this does not
  require downloading a model or re-embedding the library.
- First-run chat model tiers use system RAM, without adding GPU memory whose
  dedicated/shared status the probe cannot establish. This includes AMD APUs;
  CUDA/ROCm vendor labels are no longer treated as proof of separate memory.
- Regression coverage now exercises the actual panic hook in a subprocess,
  rollback success and failure with preserved bytes, and orphan cleanup when a
  model row is unreadable or the database is unavailable.
- The modernization results record now reports the small v2 ranking regression
  and explicitly limits its quality and performance claims to the measured fixture.

## Verification

- Full Rust library suite: **3,248 passed, 0 failed, 54 ignored**.
- Two additional opt-in tests: **2 passed**.
- Repository barrier, Rust layer boundaries, and SQL contracts: passed.
- `cargo clippy --lib --tests -- -D warnings`: passed.
- Formatting of the changed Rust files and `git diff --check`: passed.
- Repo-wide `cargo fmt --check` still reports formatting in concurrent,
  unrelated edits to `features/web/services/page_cache.rs` and `web.rs`.
  Those files were left untouched by this follow-up.

The real-model check uses existing Qwen3-Embedding-0.6B artifacts and actual
repository prose (`EVALUATION_PROTOCOL.md`). Production chunk preparation
produced eight chunks. A fresh, migrated SQLite database populated the lexical
index. Full-precision and MRL-256/i8 indexes both preserved five ranked results,
their scores, and their hydrated text after reopening disk-backed resources.
All state was temporary; no user library was reset. This is a backend component
lifecycle check, not a desktop installation or GUI import acceptance test.

The scale check builds 20,000 64-dimensional synthetic vectors, verifies exact
rankings, then adds the 20,001st vector without changing the production search
threshold. Graph recall@10 was **1.0000 on 32 separate synthetic queries**.
This checks the real threshold and a repeatable regression floor; it does not
establish recall on arbitrary real libraries. Debug-build timings are not used
as production latency evidence.

Re-run the opt-in checks with a local model directory:

```sh
cd src-tauri
LATTICE_TEST_EMBEDDING_MODEL=/path/to/qwen3 \
  cargo test --lib -- real_model_import_search_and_disk_restart_preserve_results \
  recall_across_the_production_exact_to_graph_boundary \
  --ignored --nocapture --test-threads=1
```

No Windows/Linux execution, packaged-app acceptance run, or representative
real-vault quality evaluation was performed in this follow-up. The older
long-context quality regression remains recorded; retrieval weights were not
retuned to hide it.
