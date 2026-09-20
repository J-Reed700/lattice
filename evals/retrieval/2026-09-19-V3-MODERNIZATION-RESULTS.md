# Retrieval modernization on synthetic-library-v3 (2026-09-19)

Fixture: `synthetic-library-v3.json`, 379 documents, 194 queries (173 scored for
retrieval). Embedding model: Qwen3-Embedding-0.6B on Metal. Harness:
`retrieval_eval --mode production --top-k 10`, scored with `rag_eval.py --k 5`.
"Before" is commit 9974c7f2 plus only the harness change that reports chunk
spans. "After" is the merged modernization branch. Metric files are in
`results/2026-09-19-v3-qwen3-*`.

| Run | recall@5 | nDCG@5 | MRR@5 | passage recall@5 | median latency |
|---|---|---|---|---|---|
| Before | 0.6946 | 0.6139 | 0.6239 | 0.6590 | 32.8 ms |
| Before, ms-marco reranker | 0.7100 | 0.6199 | 0.6266 | 0.6763 | 69.5 ms |
| **After** | **0.7755** | **0.6925** | **0.7007** | **0.7688** | **24.9 ms** |
| After, mrl256-i8 index | 0.7640 | 0.6811 | 0.6894 | 0.7572 | 24.1 ms |
| After, late chunking | 0.7110 | 0.6353 | 0.6289 | 0.6965 | n/a |
| After, ms-marco reranker | 0.7755 | 0.6910 | 0.6986 | 0.7688 | 533 ms |

On synthetic-library-v2, recall@5 stayed at 0.9706, while nDCG@5 moved from
0.9222 to 0.9170 and MRR@5 from 0.9461 to 0.9387. The earlier description of
all three metrics as unchanged was incorrect. Its high recall leaves little
headroom, but its ranking regressions still need to be reported.

These are synthetic-fixture retrieval measurements, not verified answer
accuracy or evidence of robustness on real libraries. The graph parameter
sweep below used 1,145 vectors; it does not establish recall or latency above
the 20,000-vector production threshold. Runtime figures apply to the tested
Mac and configurations only. No desktop fresh-install or cross-platform
acceptance run was part of this evaluation.

## nDCG@5 by dimension

| Dimension | Before | After |
|---|---|---|
| buried needle in long document | 0.924 | 0.927 |
| current versus superseded policy | 0.674 | 0.692 |
| entity collisions | 1.000 | 1.000 |
| exact identifiers and numeric distinctions | 0.463 | 0.611 |
| graded and multi-document relevance | 0.449 | 0.611 |
| long-context tail facts | 0.969 | 0.891 |
| multi-hop composition | 0.321 | 0.512 |
| multilingual and cross-lingual retrieval | 0.852 | 0.914 |
| near-duplicate distractors | 0.569 | 0.647 |
| negation and exception handling | 0.911 | 0.940 |
| noisy and OCR text | 0.247 | 0.344 |
| scope and applicability boundaries | 0.943 | 0.972 |
| semantic paraphrase | 0.426 | 0.492 |
| short fragment notes | 0.754 | 0.839 |
| tables and structured content | 0.563 | 0.739 |
| topical hard negatives | 0.694 | 0.744 |

The one dimension below the old code is long-context tail facts: two queries
where a near-twin document (`handbook-tree-*` against `handbook-prose-*`) now
outranks the answer document in the vector branch.

## What the first merged run got wrong, and why

The first merged run was flat (recall@5 0.7004) and entity collisions fell from
1.000 to 0.583. Running each branch alone put the loss on the chunking change,
but the chunker was not the cause. For the failing queries the answer chunk was
byte-identical in both runs, exact cosine ranked it first, and the HNSW index
did not return it.

Measured against exact cosine over the 1,145 chunk vectors and all 194 queries:

| Index | true top-1 found | recall@10 | mean search |
|---|---|---|---|
| HNSW 16 / 128 / 64 (old) | 181 / 194 | 0.953 | 0.26 ms |
| HNSW 16 / 128 / 256 | 191 / 194 | 0.994 | 0.55 ms |
| HNSW 32 / 256 / 256 (new, large indexes) | 194 / 194 | 0.999 | 0.60 ms |
| Exact scan (new, up to 20,000 vectors) | 194 / 194 | 1.000 | 0.68 ms |

Smaller chunks tripled the vector count and a templated corpus packs into tight
clusters, so a narrow graph walk never reached a chunk whose neighbours score
badly against the query. The old code had the same defect at a lower rate.

## Decisions these numbers support

- Late chunking stays off by default: worse than chunk-first on every metric.
- The ms-marco MiniLM reranker stays off: no gain, about 500 ms per query.
- The mrl256-i8 index costs about one point of recall and nDCG.
- Branch contributions, each measured alone against "Before": fusion and
  lexical changes +0.046 nDCG; embedding runtime and index persistence neutral
  on quality, as intended.

## Re-running

Embedding the corpus is about seven minutes of a run. Set
`RETRIEVAL_EVAL_EMBED_CACHE=<dir>` and a repeat chunk-first run takes about
fifteen seconds. Clear the directory after changing how text is embedded.
