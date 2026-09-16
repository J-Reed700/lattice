# Retrieval evaluation

Three synthetic corpora are included:

- `starter.json` is the small smoke fixture. It covers paraphrases,
  near-duplicate distractors, Spanish-to-English retrieval, an exact identifier at
  the end of a long document, and an unanswerable question.
- `synthetic-library-v1.json` is a larger fictional company library with 42
  documents and 38 questions. It adds graded and multi-document relevance,
  superseded policies, entity collisions, exceptions, four languages, exact API
  identifiers, numeric distinctions, long-context facts, and four unanswerable
  questions.
- `synthetic-library-v2.json` extends v1 to 75 documents and 78 questions.
  Recall@5 on v1 saturated at 1.0, so v2 adds 33 topical hard negatives: regional
  and superseded variants of documents that are the right answer to a different
  question, entity collisions on project codenames (an Aurora campaign and an
  Aurora mobile project beside the Aurora desktop launch), adjacent documents that
  share a topic but answer something else (a restore procedure beside a retention
  policy), near-duplicates that differ by one number or one negation, a middle
  version of the import API between the deprecated and current ones, and a second
  long drill record whose recovery phrase is one digit-transposition away from the
  first. Forty new questions target those documents, six of them unanswerable but
  phrased entirely in corpus vocabulary.

All three are regression fixtures rather than evidence of production quality.
Replace or extend them with reviewed, representative document passages before
promoting a model.

## Product decision (2026-09-16)

The v2 production-path sweep established the defaults and the boundary between
shipping behavior and experiments:

| Capability | Decision | Production behavior |
| --- | --- | --- |
| Vector + keyword fusion | Keep and default on | Weighted RRF: vector `0.7`, keyword `0.3`, `k = 10` |
| Qwen3 vector compression | Keep and default on | 256 Matryoshka dimensions with i8 storage; non-Matryoshka models fall back to full precision |
| Corrective retrieval retry | Keep | Sufficiency may trigger one retry; it must not suppress an answer by itself |
| Cross-encoder reranking | Keep as an experiment | Available, off by default because the measured gain was small relative to latency |
| Late chunking | Keep as an experiment | Available, off by default because Qwen3 was neutral and it was the slowest configuration |
| Claim verification | Keep | On by default; annotates unsupported or contradicted claims without rewriting or suppressing the answer |
| Summary index | Park | Off by default pending an in-app corpus-level evaluation |
| BGE-M3 learned sparse branch | Remove from the curated product path | Not offered in the curated catalog and not used by production fusion; retained in the harness and compatibility code for reproducibility |

On Qwen3, changing the original unweighted `k = 60` fusion to `0.7/0.3`
weighted fusion at `k = 10` raised v2 Recall@5 from `0.926` to `0.963`
and multilingual Recall@5 from `0.528` to `0.944`. Qwen3 at 256 dimensions
with i8 storage matched the full-precision run on the measured retrieval metrics
while reducing stored vector bytes by 16x. These are fixture results, not
universal quality guarantees; future default changes still require a comparable
production-path run.

### Grading convention for hard negatives

A regional or superseded variant is graded 0 for a question that does not name its
scope: an unqualified "how long are workspace snapshots kept?" is answered by the
standard policy, and surfacing the European addendum first is a scope error worth
measuring. The variant is graded 3 for the question that does name its scope, and
the general document graded 1 there. A superseded document keeps grade 1 on the
current-value question only when the question asks about the change itself.

### Dimension tags

Every query in both synthetic libraries carries a `dimensions` list drawn from the
dataset's top-level `dimensions` vocabulary. The scorer reports metrics per
dimension, so a regression lands on a named retrieval behaviour ("current versus
superseded policy", "entity collisions", "topical hard negatives") instead of on a
single average. `tags` is accepted as a synonym for runs exported from elsewhere.

Validate a dataset before running model inference. `--validate-only` also reports
how many queries carry each dimension tag:

```sh
python3 scripts/rag_eval.py \
  evals/retrieval/synthetic-library-v2.json --validate-only
```

## Retrieval modes

`cargo run --example retrieval_eval` takes `--mode embedding` (the default) or
`--mode production`, then `EMBEDDING_DIR DATASET_JSON [RERANKER_DIR]`.

`--strategy`, `--compression` and `--sparse` select the retrieval features under
measurement. Every default reproduces the behaviour that existed before the flag,
so an unflagged run is comparable with every earlier run. `--compression` and
`--sparse` describe the index and the fusion, neither of which embedding mode
builds, so passing either outside `--mode production` is an error rather than a
silent no-op.

### Embedding mode

Ranks documents by the maximum passage cosine of the raw embedding model, using
the same input preparation and query instruction the app uses. This isolates
embedding quality; it exercises no fusion, no lexical branch, and no index.

```sh
cd src/src
SQLX_OFFLINE=true cargo run --example retrieval_eval -- /path/to/model ../../../evals/retrieval/starter.json > /tmp/model-run.jsonl
cd ../../..
python3 scripts/rag_eval.py evals/retrieval/starter.json /tmp/model-run.jsonl --k 3
```

For the larger comparison, replace `starter.json` with
`synthetic-library-v2.json` in both commands and use `--k 5`. The larger fixtures
have questions with up to three relevant documents, so Recall@5 and nDCG@5 expose
candidate coverage and ordering separately.

### Production mode

Runs the app's real retrieval path over the fixture instead of raw cosine:

```sh
cd src/src
SQLX_OFFLINE=true cargo run --example retrieval_eval -- --mode production \
  /path/to/model ../../../evals/retrieval/synthetic-library-v2.json > /tmp/prod-run.jsonl
cd ../../..
python3 scripts/rag_eval.py evals/retrieval/synthetic-library-v2.json /tmp/prod-run.jsonl \
  --k 5 --abstain-threshold 0.02
```

Each fixture document is chunked by `SemanticChunker` at the production 800/120
token settings, given the indexer's `[Document: … | Section: …]` context prefix,
re-split through the embedder's own input policy with that prefix, and embedded as
the prefixed text — the sequence `IndexingActor::process_file` runs. Vectors go
into a `USearchVectorIndex` (HNSW, cosine, f32) and chunk rows into an in-memory
SQLite database whose production `chunks_fts_insert` trigger mirrors them into an
FTS5 table. Queries then run through a real `HybridSearchService` built with the
same dependencies `features::search::di::build` uses — the USearch index,
`BM25Search`, and `SearchEnrichmentService` — so both branches, reciprocal-rank
weighted fusion at k = 10, and the shared cross-encoder blend are the production code, not a
reimplementation. Finally, section-neighbour evidence expansion is applied.

`--top-k` (default 50) sets how many chunk candidates each query retrieves before
they are collapsed to a document ranking. It needs to stay well above the `--k`
used for scoring, because several chunks of one document can occupy the pool.

Two things are reimplemented rather than called, because they are not reachable
from an example: `corpus_plan::expand_evidence` and
`ConversationRepository::retrieval_neighbors` are `pub(super)` and bound to the
conversation repository, so the example repeats their rule (8 anchors, same
document and section, `chunk_index` within ±1, at most 3 rows per anchor) against
its own SQL. Note that expansion is a no-op for a single-chunk document, and for
any chunk whose section is NULL; on these fixtures only the long drill records are
long enough to produce neighbours.

Production mode still does not exercise workspace scoping, recency boosting, query
expansion, or answer generation, and it has no page ranges, so `page_number` is
always NULL.

### Embedding strategy

`--strategy chunk-first` (the default) embeds every chunk on its own with its
context prefix prepended. `--strategy late-chunking` embeds a whole span in one
forward pass and mean-pools each chunk's token states, so every chunk is
conditioned on the rest of its span. In embedding mode the span is the document
itself; in production mode it is the document with its `[Document: … | Section: …]`
prefix prepended once, mirroring
`indexing::use_cases::embedding_input::prepare_structured_with_spans`. A span that
does not fit the model window falls back to per-chunk embedding inside
`embed_span_chunks`, which is the documented behaviour and not a failure.

Late chunking changes `model_identity()` — the two vector spaces are not
interchangeable — so the row's `model_identity` is what tells two runs apart, and
a late-chunking run must never be compared against a chunk-first one on score.

### Vector compression

`--compression` (production only, default `none`) sets how the USearch index
stores vectors: `i8` quantizes at the model's full dimension, `mrl<DIMS>`
truncates to a renormalized `DIMS`-length Matryoshka prefix, and `mrl<DIMS>-i8`
does both. `none` builds exactly the index the harness built before this flag
existed. Compression is lossy, so the index becomes a candidate generator and
searches rescore over-fetched neighbours against full-precision vectors.

Truncation only works on a model trained with Matryoshka Representation Learning
(Qwen3-Embedding-0.6B is; all-MiniLM-L6-v2 is not), so measure it against an
uncompressed run on the same model before reading anything into the numbers.

### Learned sparse retrieval

`--sparse` (production only, default `auto`) adds the learned sparse branch beside
the dense and BM25 ones. `auto` turns it on exactly when the loaded checkpoint has
a sparse head; `on` fails loudly on a checkpoint that has none, rather than
reporting a run whose `sparse` field would be untrue; `off` keeps two-way fusion.
When on, chunk term weights are written to a `chunk_sparse_terms` table copied from
`migrations/20260916000000_init_schema.sql`, keyed by `model_identity`,
and `branch_ranked_ids` gains a `"sparse"` entry so the branch is separately
auditable. Chunk-first runs take dense and sparse from one forward pass; late
chunking pools dense vectors over a span and reads the sparse head from the
per-chunk texts in a second pass, exactly as `index_file` does.

```sh
cd src/src
SQLX_OFFLINE=true cargo run --example retrieval_eval -- --mode production \
  --strategy late-chunking --compression mrl256-i8 --sparse auto \
  /path/to/model ../../../evals/retrieval/synthetic-library-v2.json > /tmp/prod-run.jsonl
```

### Reranking

Pass a reranker directory as the optional third argument in either mode. In
embedding mode it reranks the top-48 passage pool; in production mode it turns on
`SearchConfig::enable_reranking`, so `HybridSearchService` widens its own candidate
pool and applies its own blend stage:

```sh
cd src/src
SQLX_OFFLINE=true cargo run --example retrieval_eval -- \
  /path/to/embedding/model ../../../evals/retrieval/starter.json \
  /path/to/reranker/model > /tmp/reranked-run.jsonl
```

The loader selects the implementation from the checkpoint's `config.json`. It
supports the BERT sequence-classification checkpoint used by
`cross-encoder/ms-marco-MiniLM-L-6-v2` and the causal
`Qwen/Qwen3-Reranker-0.6B` checkpoint. A Qwen3 embedding checkpoint is rejected.
An embedding-mode reranked run keeps the top-48 passage pool, blends first-stage
and reranker scores with the production weights, then reports document rankings;
`first_stage_ranked_ids` makes each promotion and regression auditable.

Reranking exercises the production prompt or pair encoding and the production score
blend. In embedding mode it still omits fusion and chat generation; in production
mode the blend is applied by `HybridSearchService` itself, over a fused candidate
pool. Every run records the embedding artifact/preprocessing fingerprint.
Qwen3-Reranker uses a roughly 1.1 GB checkpoint and is expected to be slower than
the 22M-parameter MiniLM cross-encoder, so compare memory and latency as well as
ranking metrics.
Set `LATTICE_FORCE_CPU=1` to run without Metal or CUDA, including in constrained
CI workers. Embedding and reranking use the same accelerator-selection policy.

Saved starter results cover all six combinations of MiniLM and Qwen3 embeddings
with reranking off, MiniLM reranking, or Qwen3 reranking under `results/`.
The first larger-corpus comparison and its interpretation are recorded in
[`SYNTHETIC_LIBRARY_V1_RESULTS.md`](SYNTHETIC_LIBRARY_V1_RESULTS.md).

## Run rows

Every row carries `query_id`, `mode`, `ranked_ids`, `scores`,
`first_stage_ranked_ids`, `top_score`, `score_spread`, `latency_ms`,
`model_identity`, `reranked`, and `embedding_strategy`. `top_score` is the best
document score and `score_spread` is that score minus the rank-five score, so a
confident hit and a corpus miss are distinguishable without reading the ranking.
`embedding_strategy` is `"chunk_first"` or `"late_chunking"`.

Production rows add these fields so a regression is attributable:

- `chunk_ranked_ids` — the chunk-level ranking as `document#chunk_index`, including
  the appended section neighbours, which is what makes a document-level miss
  traceable to a passage.
- `branch_ranked_ids` — `{"vector": [...], "bm25": [...]}`, plus `"sparse"` when the
  learned sparse branch ran, each branch run on its own through the same service and
  collapsed to documents. When fused recall drops, these say whether the vector side,
  the lexical side, the sparse side, or the fusion lost the document.
- `first_stage_ranked_ids` means "before neighbour expansion" in production mode
  and "before reranking" in embedding mode. Reranking happens inside
  `HybridSearchService`, so the pre-rerank order is not separately observable there.
- `compression` — the stored vector layout token (`"mrl256i8"`, `"mrl512f32"`, …) or
  `"none"`. Two runs with different tokens indexed different vector spaces.
- `sparse` — whether the learned sparse branch actually ran.
- `sufficient`, `sufficiency_reasons`, `term_coverage` — the chat pipeline's own
  post-rerank verdict on the fused ranking, from
  `chat::retrieval::assess_retrieval_sufficiency`, computed before neighbour
  expansion over a plan holding just this query. `sufficiency_reasons` are the
  pipeline's reason codes (`"low_term_coverage"`, `"no_reranker"`, …), not prose.
  The verdict only runs in production mode; embedding mode has no fusion to judge.

Latency covers query embedding plus retrieval (plus neighbour expansion in
production mode). It excludes model loading, indexing, and the branch diagnostics,
and has no warm-up discard.

The scorer also accepts rankings exported from the production retrieval pipeline.
Every labeled query must appear exactly once; missing, duplicated, or unknown
results fail the run. Answerability comes from whether positive relevance labels
exist. Empty labels are excluded from Recall/nDCG/MRR, not counted as perfect
retrieval. Unknown fields on a row are ignored, so rows recorded before these
fields existed still score.

## Retrieval-side abstention

`--abstain-threshold T` predicts "unanswerable" for any row whose `top_score` falls
below `T` and scores that against the queries with no positive labels, reporting
`retrieval_abstention_accuracy`. Rows written before `top_score` existed fall back
to the first entry of `scores`. Without the flag the metric is null rather than
guessed.

`--abstain-field NAME` predicts "unanswerable" when `row[NAME]` is `false`, which is
what reads a production run's own `sufficient` verdict as an abstention signal
instead of a hand-picked score cutoff. Unlike `top_score` it has no fallback: a row
missing the field, or carrying something other than `true`/`false`, fails the run.
The two predictors are mutually exclusive, and `abstention_predictor` in the output
records which one ran (`"threshold"`, `"field:<name>"`, or null), so two runs are
never compared across predictors by accident.

```sh
python3 scripts/rag_eval.py evals/retrieval/starter.json /tmp/prod-run.jsonl \
  --k 3 --abstain-field sufficient
```

The two modes are on different scales and need different thresholds. Embedding mode
reports cosine similarity, roughly 0.3–0.9, so a threshold near 0.6 is a starting
point. Production mode reports reciprocal-rank-fusion scores, which are ranks and
not probabilities: a document found at rank one by both branches scores about
2/61 ≈ 0.033, and the whole usable range sits under about 0.04, so start near 0.02.
Pick the threshold from a measured sweep on a baseline run, never from these
figures alone, and never compare a cosine threshold with an RRF one.

To evaluate generated answers, append independently reviewed `supported_claims`,
`total_claims`, `correct_citations`, `total_citations`, and `abstained` fields to
run rows. Missing answer judgments produce null metrics, never invented scores.
Review answer quality blind to model name. Store answer text separately for audit.

Before promoting a model, compare Recall@k, nDCG@k, MRR, citation precision,
supported-claim fraction, abstention accuracy, retrieval abstention accuracy, p95
latency, peak memory, and API cost on the same corpus and hardware. Read the
`by_dimension` block as well as the overall numbers: an overall Recall@5 that holds
while "topical hard negatives" or "current versus superseded policy" drops is the
regression this fixture exists to catch. Establish thresholds from a measured
baseline. This fixture does not supply meaningful universal pass thresholds.

Scorer validation:

```sh
python3 -m unittest discover -s scripts -p test_rag_eval.py
```

## Live manual retrieval smoke check

The native opt-in `live_corpus_retrieval_and_answer` test exercises the shared
corpus planner, scoped retrieval, prompts, and llama.cpp streaming adapter against
a read-only database connection. Use a consistent SQLite backup of the library;
do not copy a live WAL database with a plain file copy. It does not run the full
Tauri chat/tool loop or grade factual correctness.

Supply `LATTICE_RETRIEVAL_DB` (backup path), `LATTICE_LLAMACPP_SETTINGS` (the app's
settings file), and `LATTICE_RETRIEVAL_REPORT` (output JSON path). Set
`LATTICE_RETRIEVAL_CONVERSATION_ID` to select an existing conversation's document
scope. `LATTICE_RETRIEVAL_QUESTION_FILE` supplies a UTF-8 acceptance question when
the original conversation message has been removed; otherwise the test uses the
first user message in the selected conversation. Defaults use the most recently
updated conversation. The acceptance assertions currently target the MPEP
beginner case, including the first chapter and two specific chapter searches.

```sh
cd src/src
cargo test --lib live_corpus_retrieval_and_answer -- --ignored --nocapture
```

This makes real inference requests to the configured llama.cpp endpoint and uses
the installed embedding model. It leaves conversation history and the source
index untouched. Generation is bounded by the production 300-second deadline;
the report retains failures, first visible answer time, generation duration,
finish reason, and streaming consistency. A completed smoke check is not a
citation-support or legal-accuracy assessment.
