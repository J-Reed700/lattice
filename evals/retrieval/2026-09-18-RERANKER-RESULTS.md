# Does the cross-encoder reranker earn its place? (2026-09-18)

**Answer: not on this fixture. The default stays off.**

## Why this was run

`search.enableReranking` defaults to `false`, and the toggle in Settings had
never been able to take effect: `ensure_reranker_available` existed on the model
manager and was covered by tests, but nothing in the running app ever called it,
and no model role offered to download a reranker. So the artifacts were never on
disk, `LazyReranker::is_available` returned false, and `HybridSearchService`
quietly returned its unreranked shortlist while Settings showed a switch
implying otherwise.

Making the switch reachable raised the obvious question: should it default on?

## Method

```
retrieval_eval --mode production --top-k 10 <qwen3-embedding-0.6B> \
  evals/retrieval/synthetic-library-v2.json [<reranker-dir>]
```

Production mode runs the app's real retrieval path — contextual chunking,
weighted RRF at k = 10 with the 0.7 / 0.3 branch weights, and, when a reranker
directory is supplied, `HybridSearchService`'s own blend stage. The two runs
differ only in that third argument.

- Fixture: `synthetic-library-v2.json`, 75 documents, 78 queries, 12 dimensions.
- Embedding: Qwen3-Embedding-0.6B, real weights.
- Reranker: `cross-encoder/ms-marco-MiniLM-L-6-v2`, real weights.
- Scored with `scripts/rag_eval.py --k 5`.

## Result

| metric | no rerank | reranked | delta |
| --- | ---: | ---: | ---: |
| recall@5 | 0.9706 | 0.9706 | +0.0000 |
| nDCG@5 | 0.9222 | 0.9222 | +0.0001 |
| MRR@5 | 0.9461 | 0.9461 | +0.0000 |
| median latency | 29.0 ms | 58.4 ms | **2.01×** |
| p95 latency | 30.5 ms | 71.2 ms | **2.33×** |

The reranker did run: latency doubled and the rows carry `reranked: true`. It
reordered the top five on 24 of 78 queries. It never changed the top result on
any of them, and no dimension moved by more than ±0.003 nDCG — including the
ones built to be hard:

| dimension | no rerank | reranked | delta | n |
| --- | ---: | ---: | ---: | ---: |
| near-duplicate distractors | 0.8666 | 0.8666 | +0.0000 | 11 |
| topical hard negatives | 0.9479 | 0.9479 | −0.0000 | 34 |
| current versus superseded policy | 0.8805 | 0.8817 | +0.0012 | 18 |
| multilingual and cross-lingual | 0.8650 | 0.8660 | +0.0010 | 6 |
| negation and exception handling | 0.9864 | 0.9855 | −0.0009 | 20 |
| scope and applicability boundaries | 0.9891 | 0.9861 | −0.0029 | 8 |

## Reading it

The first stage already answers this fixture at 0.97 recall and 0.95 MRR. There
is almost no headroom for a second stage to recover, and a reranker cannot
improve a ranking whose top result is already correct on every query. What the
numbers establish is narrow but real: **on a corpus this size and this clean,
reranking costs 2× latency and buys nothing measurable.**

What they do not establish is that reranking is useless. A 75-document
synthetic library is not a real vault: no OCR noise, no near-identical meeting
notes, no documents whose relevance depends on a passage buried at page 40.
Those are the conditions a cross-encoder exists for, and this fixture has none
of them. Re-run this before concluding anything about a corpus with real text
in it.

## What changed as a result

- Nothing about the default. `enable_reranking` stays `false`.
- The toggle now says what it is doing, and offers the download when the model
  is missing, instead of silently doing nothing.
- Because reranking is off, the sufficiency gate runs permanently in its
  `no_reranker` branch, judging on result count rather than on a calibrated
  score. That is the branch it was designed for, not a fault — but it means
  `sufficiency_min_top_score` is inert in the shipped configuration.
