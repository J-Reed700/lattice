# Track C: retrieval defaults (2026-10-12)

Full write-up: `evals/retrieval/2026-10-12-RERANKER-FTS-HEADER-FUSION.md`. Data: `evals/retrieval/results/2026-10-12-*`.

## What changed

- **FTS header column.** The new migration `20261012090000_fts_header_column.sql` recreates `chunks_fts` and `chunks_trigram` as `(chunk_id, content, header)` and refills them. `fts_query::HEADER_BM25_WEIGHT = 2.0` is bound by `SqliteTextSearch`, `BM25Search` and `repository_tx::ops`.
- **Reranker download is now Qwen3-Reranker-0.6B; reranking stays off.**
  - `reranker_setup.rs`: only lines 18-28 (`RERANKER_DOWNLOAD_BYTES`, `RERANKER_MODEL_NAME`) and the test that asserts them.
  - `model_manager.rs`: pinned URLs and a new folder `RERANKER_MODEL_DIR` (in `shared/constants.rs`); its tests follow.
  - `search/di.rs`: one line.
- **Compression defaults to full precision** (`settings.rs`: default, comment, test). `SETTINGS_VERSION` 2 moves files still on 256/i8/truncated.
- **Harness flags:** `--fts-header column|concat`, `--fts-header-weight`, `--rerank-depth`.

## Measured

v3 production mode at `--top-k 10`; paired 95% CIs from `compare_rag_eval.py --samples 1000 --seed 2026`.

- **Header.** It was already in BM25, concatenated, so the audit's premise was wrong. Weight 1 is identical to concatenation. At weight 2 the keyword branch's nDCG@5 goes 0.673→0.684 [+0.001, +0.022]; hybrid is flat (-0.0004 [-0.008, +0.007]).
- **Fusion.** The best of 12 on half A (k=10, 0.5/0.5) failed on held-out B: nDCG +0.030 [-0.010, +0.070]. **Constants unchanged.**
- **Compression.** 256-d i8: recall@5 -0.017 [-0.041, 0.000]. The exact scan picks candidates from compressed vectors (`usearch_index/search.rs:87`) and rescores only a 4× window. Full precision costs 41 MB per 10k chunks and 410 MB per 100k.
- **Reranker** against none (recall@5 / nDCG@5):

  | Run | recall@5 | nDCG@5 |
  |---|---|---|
  | Qwen, depth 48 | +0.015 [-0.027, +0.059] | +0.048 [+0.002, +0.091] |
  | Qwen, depth 100 | +0.021 [-0.028, +0.072] | +0.057 [+0.007, +0.106] |
  | MiniLM, depth 48 | 0 | +0.001 |

  At depth 48 Qwen costs 5.6 s per query (against 23 ms) and +3.8 GB. **It stays off:** recall and MRR intervals include 0 at every depth. Depth stays 48.
- **Earlier reranker runs:** at `--top-k 10` they only reranked 30 candidates.
- **Tests:** `cargo test --lib -- search indexing constants settings model_manager schema_validation`: 835 passed, 0 failed. `cargo fmt` is clean, and the Python eval suite passes.

## Not verified

- No `cargo check --all-targets`, per the coordinator's rule.
- The migration has not been run on a real library.
- Some latencies (depth 64 p95, depth 100, MiniLM) were inflated by another agent's llama-server on the GPU.
- The harness uses a different context prefix from the app.
- Nothing was run on a 16 GB machine or live.

## Risks

- Another track may also bump `SETTINGS_VERSION` to 2; merge them.
- The first launch rebuilds the FTS tables.
- `models/reranker/` (MiniLM) and the old `mrl256i8` index files are left orphaned.
- The header weight was chosen and measured on the same queries.
- The harness files are 980 and 910 lines (already over 800 before this track).
- The worktree has a gitignored, empty sidecar placeholder.

## Suggested commit

```
Measure retrieval defaults on v3 and keep only what held up

The context prefix gets its own FTS5 column weighted 2.0, lifting the
keyword branch without moving hybrid ranking. Vectors default to full
precision: 256-d i8 cost 1.7 points of recall because the exact scan
picks candidates from compressed vectors. The reranker download becomes
Qwen3-Reranker-0.6B, which beat MiniLM on nDCG, but reranking stays off:
recall did not move and it costs 5.6 s per query. Fusion constants stay;
the best of a 12-point sweep did not confirm on held-out queries.
```
