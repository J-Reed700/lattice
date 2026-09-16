# Synthetic library v1 results

`synthetic-library-v1.json` is a deterministic stress corpus for local retrieval.
It contains 42 fictional company documents and 38 questions: 34 answerable, four
unanswerable, and 15 with more than one relevant document. Relevance grades range
from supporting context to the primary answer source.

The first completed matrix used the release build on CPU by setting
`LATTICE_FORCE_CPU=1`. Latency includes query embedding, candidate scoring, and
reranking. It excludes model loading and document indexing.

| Embedding | Reranker | Recall@5 | nDCG@5 | MRR@5 | p95 query latency |
| --- | --- | ---: | ---: | ---: | ---: |
| MiniLM | Off | 0.975 | 0.923 | 0.936 | 12.7 ms |
| MiniLM | MiniLM | 0.990 | 0.933 | 0.951 | 821 ms |
| Qwen3 0.6B | Off | 1.000 | 0.977 | 0.985 | 263 ms |
| Qwen3 0.6B | MiniLM | 1.000 | 0.988 | 1.000 | 1,038 ms |

Qwen3 embeddings produced the strongest first-stage results. They corrected
MiniLM failures involving current versus archived refund rules, historical travel
rates, an English query for Japanese text, and a service-credit question requiring
multiple sources. The MiniLM reranker improved aggregate ordering for both
embedding models. With Qwen3 embeddings it moved the only remaining first-place
miss to rank one and retained complete Recall@5.

The reranker is not uniformly better on every question. With MiniLM embeddings it
demoted supporting credential context for the settings-export question and put
global emergency intake above the primary Spanish schedule. This supports keeping
the first-stage score blend and evaluating on representative material rather than
replacing first-stage ordering outright.

A full Qwen3-embedding plus Qwen3-reranker CPU run was stopped after four of 38
queries because it projected beyond ten minutes. No partial metric is reported.
The result does not compare its quality, but it confirms that this backend is not
a practical CPU default for a 42-candidate pool. Its Metal performance and quality
should be measured on production-class hardware if a quality-first tier is needed.

These results support Qwen3 0.6B embeddings plus the compact MiniLM reranker as the
best tested quality configuration. Qwen3 embeddings without reranking are the best
tested latency-conscious configuration. The corpus remains synthetic and does not
measure answer grounding, citation precision, abstention behavior, peak memory, or
performance on a user's actual library.

## Reproduce

```sh
cd src-tauri
SQLX_OFFLINE=true cargo build --release --example retrieval_eval

LATTICE_FORCE_CPU=1 ./target/release/examples/retrieval_eval \
  /path/to/embedding-model ../../../evals/retrieval/synthetic-library-v1.json \
  /path/to/reranker-model \
  > ../../../evals/retrieval/results/my-run.jsonl

cd ../../..
python3 scripts/rag_eval.py evals/retrieval/synthetic-library-v1.json \
  evals/retrieval/results/my-run.jsonl --k 5
```
