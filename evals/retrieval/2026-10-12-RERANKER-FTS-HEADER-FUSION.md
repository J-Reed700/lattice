# Reranker, header column, fusion and compression on v3 (2026-10-12)

Four retrieval defaults from the 2026-10-12 LLM-stack audit (items 3, 4, 6 and 11),
each measured on `synthetic-library-v3.json` in production mode and decided by a
paired bootstrap.

| Question | Decision | Evidence (paired 95% CI, 1000 resamples) |
|---|---|---|
| Swap the reranker to Qwen3-Reranker-0.6B and turn it on? | Swap yes, keep it off | Depth 48: nDCG@5 +0.048 [+0.002, +0.091] but recall@5 +0.015 [-0.027, +0.059]; 5.6 s/query and +3.8 GB |
| Give the context prefix its own FTS5 column? | Yes, `bm25()` weight 2.0 | Keyword branch nDCG@5 +0.011 [+0.001, +0.022]; hybrid unchanged |
| Retune RRF `k` and branch weights? | No, keep `k = 10`, 0.7 / 0.3 | Best of 12 on half A did not confirm on half B: nDCG@5 +0.030 [-0.010, +0.070] |
| Keep 256-d i8 vectors as the default? | No, full precision | 256-d i8 costs recall@5 -0.017 [-0.041, 0.000], nDCG@5 -0.015 [-0.035, +0.001] |

These are synthetic-fixture retrieval measurements on one machine. They are not
answer accuracy and not evidence about real vaults.

## Setup

- Fixture: v3, 379 documents, 194 queries, 173 scored (21 unanswerable).
- Embedding: Qwen3-Embedding-0.6B from `~/.cache/lattice/models/qwen3-embedding-0.6b`,
  Candle on Metal, chunk-first, 1,145 chunks.
- Machine: Apple M3 Max, 36 GB. Other agents were building on it during some runs;
  latencies are indicative, not benchmarks.
- Harness: `retrieval_eval --mode production --top-k 10 --compression none --sparse off`
  (the 2026-09-19 recipe), built with an `evalopt` profile (`inherits = "dev"`,
  `opt-level = 3`, no debug assertions) because `--release` refuses the placeholder
  sidecar a track worktree has. Every arm of every comparison ran on the same binary.
- Embedding cache: `RETRIEVAL_EVAL_EMBED_CACHE`, filled by the first run. Nothing in
  this study changes embedded text, so every run indexes identical vectors.
- Each run was its own process, one at a time, so no model stayed resident between runs.
- Scoring: `scripts/rag_eval.py --k 5`. Comparisons: `scripts/compare_rag_eval.py
  --k 5 --samples 1000 --seed 2026` (per-query paired differences, query-cluster
  bootstrap, percentile 95% interval).
- "Lexical only" is the BM25 branch the fused run recorded (`branch_ranked_ids.bm25`),
  scored as a ranking of its own.

New harness flags, all production-only:

- `--fts-header column|concat` and `--fts-header-weight W`: where the context prefix
  sits in the keyword index and how `bm25()` weighs it. `concat` rebuilds the index as
  it was before this change. Its baseline reproduces the 2026-10-09 run exactly
  (recall@5 0.7755, nDCG@5 0.6904, MRR@5 0.6978, passage recall 0.7630).
- `--rerank-depth N`: how many fused candidates the cross-encoder scores, with the fused
  pool widened to at least `N`. Without it, `--top-k 10` retrieves a pool of 30
  (`RerankOptions::candidate_pool` is 3 × `top_k`, at most 64), so the "chat's candidate
  cap (48)" the README describes was never reached: every earlier reranked v3 run
  reranked 30 candidates. Chat itself retrieves at least 48 (`kb_search_min_limit` 16 × 3).

## 1. The context prefix in the keyword index

The audit said the prefix (heading tail, title, file name, …) is embedded but not in
`chunks_fts`. That is not so. The insert trigger has always indexed
`COALESCE(contextualized_content, content)`, and every production write path stores
`contextualized_content` as prefix + body (`ChunkMapper::to_model`). The prefix was in
BM25, concatenated with the body.

What concatenation could not do is weigh it apart. FTS5's `bm25()` adds
column-weighted term counts and normalises by the length of the whole row, so a
separate `header` column at weight 1.0 ranks exactly as the concatenated column did,
and any other weight is a pure re-weighting of prefix words. The measurement agrees:
the column at 1.0 matches `concat` to four decimals on every metric.

`chunks_fts` and `chunks_trigram` are now `(chunk_id UNINDEXED, content, header)`,
created by `migrations/20261012090000_fts_header_column.sql`, which drops and refills
both tables from `text_chunks`.

| `header` weight | hybrid recall@5 | hybrid nDCG@5 | hybrid MRR@5 | keyword recall@5 | keyword nDCG@5 | keyword MRR@5 |
|---|---|---|---|---|---|---|
| concat (before) | 0.7755 | 0.6904 | 0.6978 | 0.7004 | 0.6729 | 0.6941 |
| 0 (prefix ignored) | 0.7775 | 0.6809 | 0.6848 | 0.6628 | 0.6080 | 0.6112 |
| 0.25 | 0.7697 | 0.6790 | 0.6846 | 0.6715 | 0.6359 | 0.6461 |
| 0.5 | 0.7746 | 0.6889 | 0.6956 | 0.6811 | 0.6516 | 0.6662 |
| 1 | 0.7755 | 0.6904 | 0.6978 | 0.7004 | 0.6729 | 0.6941 |
| 1.5 | 0.7784 | 0.6894 | 0.6961 | 0.7081 | 0.6836 | 0.7083 |
| **2** | **0.7784** | **0.6900** | **0.6971** | **0.7081** | **0.6835** | **0.7083** |
| 3 | 0.7784 | 0.6885 | 0.6952 | 0.7091 | 0.6880 | 0.7121 |
| 4 | 0.7784 | 0.6878 | 0.6942 | 0.7062 | 0.6860 | 0.7121 |
| 6 | 0.7784 | 0.6882 | 0.6952 | 0.7062 | 0.6889 | 0.7160 |

Paired against `concat`:

| Comparison | recall@5 | nDCG@5 | MRR@5 |
|---|---|---|---|
| weight 2, keyword only | +0.0077 [0.0000, +0.0183] | +0.0106 [+0.0013, +0.0221] | +0.0142 [+0.0034, +0.0278] |
| weight 2, hybrid | +0.0029 [0.0000, +0.0087] | -0.0004 [-0.0084, +0.0071] | -0.0007 [-0.0116, +0.0096] |
| weight 0, keyword only | -0.0376 [-0.0684, -0.0135] | -0.0649 [-0.0922, -0.0404] | -0.0829 [-0.1172, -0.0528] |

**Decision: weight 2.0** (`fts_query::HEADER_BM25_WEIGHT`), the middle of a flat
keyword plateau from 1.5 to 6. Every lexical reader binds it: chat's keyword branch
(`SqliteTextSearch`), direct search (`BM25Search`) and the transactional
`search_bm25`. It helps the keyword branch on its own, which is what direct keyword
search returns, and it does not move the fused hybrid ranking. By dimension (keyword
branch, nDCG@5) the gains are multi-hop composition +0.071, buried needle +0.048,
long-context tail facts +0.042, graded relevance +0.036; near-duplicate distractors
lose 0.015. Hybrid "short fragment notes" (16 queries) loses 0.023.

The weight was chosen and measured on the same queries, so the interval is somewhat
optimistic. The harness writes the old chunker's `[Document: … | Section: …]` prefix,
while the app writes `heading tail | title | edition | file name | description`; both
carry the title and heading, but the weight was not measured on the app's exact prefix.

## 2. Fusion

RRF `k` ∈ {10, 30, 60} × vector weight ∈ {0.5, 0.6, 0.7, 0.8} (keyword = 1 − vector),
on the header-weight-2 index. The queries were split 50/50 by a fixed seed
(`results/2026-10-12-FUSION-split.json`: sorted ids, `random.Random(20261012).shuffle`;
A has 85 answerable queries, B 88). The selection rule, fixed before scoring, was the
highest nDCG@5 on A with recall@5 as tie-break.

| k | vector / keyword | A nDCG@5 | A recall@5 | A MRR@5 | B nDCG@5 | B recall@5 | B MRR@5 | all nDCG@5 | all recall@5 |
|---|---|---|---|---|---|---|---|---|---|
| 10 | 0.5 / 0.5 | **0.7161** | 0.7824 | 0.7259 | 0.7292 | 0.7992 | 0.7384 | 0.7227 | 0.7909 |
| 10 | 0.6 / 0.4 | 0.6994 | 0.7745 | 0.7092 | 0.7147 | 0.7936 | 0.7227 | 0.7072 | 0.7842 |
| 10 | 0.7 / 0.3 (current) | 0.6800 | 0.7804 | 0.6837 | 0.6996 | 0.7765 | 0.7100 | 0.6900 | 0.7784 |
| 10 | 0.8 / 0.2 | 0.6609 | 0.7569 | 0.6694 | 0.6817 | 0.7746 | 0.6962 | 0.6715 | 0.7659 |
| 30 | 0.5 / 0.5 | 0.7005 | 0.7588 | 0.7127 | 0.7061 | 0.7708 | 0.7172 | 0.7034 | 0.7649 |
| 30 | 0.6 / 0.4 | 0.6891 | 0.7627 | 0.6994 | 0.7027 | 0.7595 | 0.7182 | 0.6960 | 0.7611 |
| 30 | 0.7 / 0.3 | 0.6805 | 0.7510 | 0.6955 | 0.6906 | 0.7670 | 0.7038 | 0.6856 | 0.7592 |
| 30 | 0.8 / 0.2 | 0.6708 | 0.7529 | 0.6857 | 0.6833 | 0.7727 | 0.6987 | 0.6772 | 0.7630 |
| 60 | 0.5 / 0.5 | 0.6998 | 0.7588 | 0.7118 | 0.7061 | 0.7708 | 0.7172 | 0.7030 | 0.7649 |
| 60 | 0.6 / 0.4 | 0.6849 | 0.7549 | 0.6971 | 0.6984 | 0.7595 | 0.7125 | 0.6918 | 0.7572 |
| 60 | 0.7 / 0.3 | 0.6718 | 0.7392 | 0.6876 | 0.6829 | 0.7557 | 0.6958 | 0.6774 | 0.7476 |
| 60 | 0.8 / 0.2 | 0.6700 | 0.7529 | 0.6847 | 0.6798 | 0.7670 | 0.6943 | 0.6750 | 0.7601 |

`k = 10`, 0.5 / 0.5 won on A. On held-out B, paired against the current constants:

| | recall@5 | nDCG@5 | MRR@5 |
|---|---|---|---|
| B (88 queries) | +0.0227 [-0.0398, +0.0795] | +0.0296 [-0.0098, +0.0695] | +0.0284 [-0.0119, +0.0697] |
| A (selection half) | +0.0020 [-0.0353, +0.0373] | +0.0360 [+0.0009, +0.0730] | +0.0422 [-0.0010, +0.0884] |

**Decision: no change.** B's intervals include zero, so `shared/constants.rs` keeps
`k = 10` and 0.7 / 0.3. The direction is the same on both halves, and the trade is
uneven by dimension (all 173 queries, nDCG@5): exact identifiers +0.165, tables
+0.168, multi-hop +0.129, graded relevance +0.123 against topical hard negatives
-0.038, superseded policy -0.034, multilingual -0.039 and short fragment notes -0.078.
The winner also sits on the edge of the grid, so a lower vector weight was never tried.
A follow-up needs a larger held-out set, not a second look at this one.

## 3. Vector compression

**Does the exact scan read the compressed vectors?** For a vault-wide query, yes, for
candidate selection. Paths below are under `crates/lattice-desktop/src/features/search/engine/vector_search/`.
Below `EXACT_SEARCH_MAX_VECTORS` (20,000; `usearch_index.rs:62`), `nearest` calls
`self.index.exact_search(query, count)` (`usearch_index/search.rs:87`) with the query
already projected into the stored space (`usearch_index/ports.rs:98`), so it scans the
vectors USearch holds: 256-d i8 under the old default. `count` is
`top_k × rescore_factor` (`candidate_window`, `usearch_index/search.rs:174`; factor 4,
`compression.rs:41`), and only that window is rescored against the full-precision
side file (`candidate_similarity`, `usearch_index/search.rs:192`, via
`RescoreVectorStore::get`, `rescore_store.rs:129`). A true neighbour the compressed
scan leaves outside the window is lost, which is where the recall goes. A *scoped*
search under the size limit is different: `exact_scoped` (`usearch_index/search.rs:105`)
reads each in-scope vector's full-precision copy (`get_into`, line 129) and scores it
exactly. Above 20,000 vectors the HNSW walk (`usearch_index/search.rs:95`) runs over
the compressed vectors, then the same rescore.

| Index | recall@5 | nDCG@5 | MRR@5 | passage recall@5 | p50 |
|---|---|---|---|---|---|
| full precision | 0.7784 | 0.6900 | 0.6971 | 0.7630 | 22.8 ms |
| 256-d i8 | 0.7611 | 0.6755 | 0.6836 | 0.7457 | 22.0 ms |
| paired delta | -0.0173 [-0.0405, 0.0000] | -0.0145 [-0.0347, +0.0008] | -0.0135 [-0.0337, +0.0010] | | |

Resident vector bytes (HNSW links, about 300 B per vector at connectivity 32, are the
same either way):

| Chunks | 1024-d f32 | 256-d i8 |
|---|---|---|
| 10,000 | 41 MB | 2.6 MB |
| 100,000 | 410 MB | 26 MB |

The compressed layout also keeps a 4 KB-per-vector full-precision file on disk for
rescoring, so it saves memory, not disk.

**Decision: full precision by default.** A 10k-chunk vault costs 41 MB and a very
large 100k-chunk one 410 MB, which a 16 GB machine can carry next to a 5-6 GB chat
model. Compression stays available in Settings for anyone who wants the memory back.
Changing the setting already renames the index generation, so the next start rebuilds
from the f32 vectors in SQLite. Existing settings files hold the old default
explicitly, so settings version 2 moves a file still on exactly 256 / i8 / truncated
to full precision and leaves any other choice alone
(`features/settings/repository/file_store.rs`).

## 4. Reranker

Qwen/Qwen3-Reranker-0.6B, revision `e61197ed`, `model.safetensors` sha256
`27cd75a4…c5e65b`, run through the existing Candle path
(`engine/qwen3_reranker.rs`: yes/no logits, F32 on Metal, one causal pass per
candidate over a shared prefix KV cache). Its `yes`/`no` token ids (9693/2152) match the
publisher's `1_LogitScore` config. For comparison, cross-encoder/ms-marco-MiniLM-L-6-v2,
revision `233902d2`, the model the app downloaded until now. All runs are on the
header-weight-2 index with today's fusion constants. Each depth is its own process.

| Run | recall@5 | nDCG@5 | MRR@5 | passage recall@5 | p50 | p95 | peak memory |
|---|---|---|---|---|---|---|---|
| no reranker | 0.7784 | 0.6900 | 0.6971 | 0.7630 | 23 ms | 25 ms | 2.1 GB |
| MiniLM, depth 48 | 0.7784 | 0.6909 | 0.6979 | 0.7630 | 1.9 s* | 4.2 s* | 4.4 GB |
| Qwen3 0.6B, depth 48 | 0.7938 | 0.7379 | 0.7347 | 0.8150 | 5.6 s | 6.4 s | 5.9 GB |
| Qwen3 0.6B, depth 64 | 0.7938 | 0.7409 | 0.7393 | 0.8150 | 7.4 s | 20.5 s* | 5.9 GB |
| Qwen3 0.6B, depth 100 | 0.7996 | 0.7466 | 0.7451 | 0.8208 | 26 s* | 43 s* | 5.9 GB |

\* Another agent's llama-server shared the GPU during these runs. An uncontended
five-query smoke test at depth 100 took about 12 s per query, roughly 120 ms per
candidate. The depth-100 row combines 109 queries from one process with the other 85
from a second, same binary and flags. Peak memory is the whole eval process (the
embedding model, index and reranker) from `/usr/bin/time -l`.

Paired against no reranker:

| | recall@5 | nDCG@5 | MRR@5 |
|---|---|---|---|
| Qwen3, depth 48 | +0.0154 [-0.0270, +0.0588] | **+0.0479 [+0.0024, +0.0909]** | +0.0376 [-0.0146, +0.0846] |
| Qwen3, depth 64 | +0.0154 [-0.0308, +0.0626] | **+0.0510 [+0.0026, +0.0968]** | +0.0422 [-0.0116, +0.0915] |
| Qwen3, depth 100 | +0.0212 [-0.0279, +0.0723] | **+0.0566 [+0.0073, +0.1055]** | +0.0480 [-0.0068, +0.1013] |
| MiniLM, depth 48 | 0.0000 [0.0000, 0.0000] | +0.0010 [-0.0096, +0.0106] | +0.0008 [-0.0137, +0.0143] |

Qwen3 against MiniLM at depth 48: nDCG@5 +0.0470 [+0.0042, +0.0898]. Depth 100 against
depth 48: nDCG@5 +0.0087 [-0.0059, +0.0293], no measurable gain for twice the work.

Qwen3 at depth 48 improved 36 queries and hurt 18 (nDCG@5). The gains are exact
identifiers (+0.140), near-duplicate distractors (+0.129), multilingual (+0.086) and
semantic paraphrase (+0.084). The losses are buried needles in long documents
(-0.114 nDCG, -0.111 recall) and long-context tail facts (-0.140, -0.167 recall): the
cross-encoder prefers a self-contained passage of a twin document over the answer
chunk deep inside a long one.

**Decisions.**

- **The app now downloads Qwen3-Reranker-0.6B** (pinned revision) into
  `models/qwen3-reranker-0.6b/` (`shared::constants::RERANKER_MODEL_DIR`). A folder
  named after the checkpoint means a MiniLM left in the old `models/reranker/` is
  never loaded under the new name. MiniLM is not kept for low-memory machines: it
  measured nothing on v3 (recall identical, nDCG +0.001), here and on 2026-09-19.
- **Reranking stays off by default.** The gate was a 95% interval that excludes zero.
  nDCG@5 clears it at every depth; recall@5 and MRR@5 do not at any depth. Even if
  recall had cleared it, 5.6 s per query on an M3 Max (more on a base M-series) and
  3.8 GB on top of the chat model are not a default a 16 GB machine can carry.
- **`rerank_max_candidates` stays 48.** That is what chat's smallest pool holds, and
  depth 100 bought nothing measurable over it.
- What would make it a default: batching candidates through one forward pass and
  loading the weights in F16 instead of F32 (both in `qwen3_reranker.rs`), then
  re-running this table. The 2026-09-19 "no gain" result came from MiniLM over a
  30-candidate pool; it said nothing about rerankers in general.

## Reproducing

```sh
# once: weights outside the app's model folder
mkdir -p ~/.cache/lattice-evals/models/qwen3-reranker-0.6b
for f in model.safetensors tokenizer.json config.json; do
  curl -fL -o ~/.cache/lattice-evals/models/qwen3-reranker-0.6b/$f \
    https://huggingface.co/Qwen/Qwen3-Reranker-0.6B/resolve/e61197ed45024b0ed8a2d74b80b4d909f1255473/$f
done

cd crates/lattice-desktop
CARGO_PROFILE_EVALOPT_INHERITS=dev CARGO_PROFILE_EVALOPT_OPT_LEVEL=3 \
CARGO_PROFILE_EVALOPT_DEBUG=false CARGO_PROFILE_EVALOPT_DEBUG_ASSERTIONS=false \
CARGO_PROFILE_EVALOPT_OVERFLOW_CHECKS=false SQLX_OFFLINE=true \
  cargo build --profile evalopt --example retrieval_eval
BIN=$CARGO_TARGET_DIR/evalopt/examples/retrieval_eval   # or target/evalopt/…
V=../../evals/retrieval/synthetic-library-v3.json
E=~/.cache/lattice/models/qwen3-embedding-0.6b
export RETRIEVAL_EVAL_EMBED_CACHE=/some/cache/dir
F="--mode production --top-k 10 --compression none --sparse off"

$BIN $F --fts-header concat $E $V > concat.jsonl                       # before
$BIN $F --fts-header-weight 2 $E $V > w2.jsonl                         # shipped
$BIN $F --fts-header-weight 2 --rrf-k 10 --weights 0.5,0.5 $E $V > f.jsonl
$BIN $F --fts-header-weight 2 --compression mrl256-i8 $E $V > c.jsonl  # the last --compression wins
$BIN $F --fts-header-weight 2 --rerank-depth 48 $E $V ~/.cache/lattice-evals/models/qwen3-reranker-0.6b > q48.jsonl

cd ..
python3 scripts/rag_eval.py $V q48.jsonl --k 5
python3 scripts/compare_rag_eval.py $V --baseline w2.jsonl --candidate q48.jsonl \
  --k 5 --samples 1000 --seed 2026
```

The keyword-only rows read `branch_ranked_ids.bm25` from each row as the ranking. Half A
and half B are `results/2026-10-12-FUSION-split.json`, with dataset and runs filtered to
those query ids. Results: `results/2026-10-12-v3-{FTS-HEADER,FUSION,COMPRESSION,RERANKER}-metrics.json`
and the run files `results/2026-10-12-v3-*-run.jsonl` (before, shipped, Qwen3 and MiniLM
at depth 48, 256-d i8).

## What this does not show

- Real vaults, answer quality, or anything outside the harness: production mode still
  skips the planner's rewrites, HyDE, the corrective retry and openings.
- The harness's chunker and prefix are not the app's (`SemanticChunker` with
  `[Document: … | Section: …]` versus `prepare_structured_with_spans` with
  `heading | title | file name`).
- Latency on a 16 GB machine, or the reranker at F16, or with batched candidates.
- The full app-side effects of the migration: it was applied by the test suite's
  `sqlx::migrate!` and replayed in Python, not run against a real library database.
