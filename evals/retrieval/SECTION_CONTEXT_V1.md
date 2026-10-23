# section-context-v1: answers in the passage after the hit

Chat keeps the ranked hits as evidence and appends each hit's unseen section
neighbours after all of them; the prompt carries that list by tokens, so a
neighbour reaches the model only through the room the hits leave
([`docs/design/2026-10-23-neighbour-context.md`](../../docs/design/2026-10-23-neighbour-context.md)).
v3 cannot say whether that context is worth anything: its needles sit whole inside
one passage, so a carried neighbour can only repeat what a hit already holds.

This fixture's needles are split on purpose. A pointing clause names the topic in
one passage and defers the values to the next ("siehe nächster Abschnitt", "como
se detalla a continuación"); the passage holding the values does not repeat the
topic's name. A query asks for the values in the topic's words, so the passage
that matches it best is the one without the answer. Plain facts from the same
documents, stated whole in one sentence, are the controls.

## Contents

| document | language | characters | passages | pointing | plain fact |
| --- | --- | ---: | ---: | ---: | ---: |
| `kurshandbuch-zellbiologie` — a cell-biology course handbook (mitosis, cell cycle) | de | 45,963 | 24 | 6 | 6 |
| `cronica-san-bartolome` — a parish chronicle, 1911 to 1913 | es | 39,831 | 19 | 6 | 6 |

Passages are counted under the Qwen3-Embedding-0.6B tokenizer. Between the needles
the documents are dated entries and specimen records of the same kind as each
other (baptisms, marriages and burials; slides, fixatives, stains and counts), so
most passages look alike to both branches.

Every query carries one of two placement dimensions and one subject dimension, so
`rag_eval.py`'s `by_dimension` block splits them:

| dimension | queries |
| --- | ---: |
| `answer in the following passage` | 12 |
| `answer in the same passage` | 12 |
| `German cell-biology handbook` | 12 |
| `Spanish parish chronicle` | 12 |

Every query is answerable, graded 3 on its one document, and carries
`answer_spans`, the UTF-8 byte range of the answering paragraph (pointing) or
sentence (control).

## How it is built

`scripts/build_section_context_v1.py` generates the file from a fixed seed. It
re-implements the indexer's `SemanticChunker` (800 tokens, 120 overlap, sentence
boundaries; byte offsets as the Rust tokenizer reports them) and places each
pointing needle by growing the filler before and after the clause until the clause
lies in exactly one passage *i* and its answer in passage *i + 1* and nowhere else
(neither *i* nor the overlap into *i + 2*). It then asserts, over the finished
documents, that every placement still holds, that every control lies whole inside
a passage, and that every needle's text occurs once. Regenerate it rather than
editing it:

```sh
python3 -m venv /tmp/tok && /tmp/tok/bin/pip install tokenizers
/tmp/tok/bin/python scripts/build_section_context_v1.py \
  ~/.cache/lattice/models/qwen3-embedding-0.6b/tokenizer.json \
  evals/retrieval/section-context-v1.json
python3 scripts/validate_rag_evals.py
```

The placement is a property of the tokenizer. Under another embedding model's
tokenizer the passage boundaries move, and the set must be rebuilt with that
model's `tokenizer.json` before its pointing scores mean anything.

## Running and scoring it

Production mode only: embedding mode has no passages to carry. Each row lists the
`--top-k` hits, then the carried neighbours, so score the same run twice — at
`--top-k` for the hits, at twice `--top-k` for what the prompt would carry:

```sh
cd crates/lattice-desktop
LATTICE_EMBED_GGUF=~/.cache/lattice/models/qwen3-embedding-0.6b/Qwen3-Embedding-0.6B-f16.gguf \
SQLX_OFFLINE=true cargo run --release --example retrieval_eval -- --mode production \
  --top-k 10 ../../evals/retrieval/section-context-v1.json > /tmp/sc-prod.jsonl
cd ../..
python3 scripts/rag_eval.py evals/retrieval/section-context-v1.json /tmp/sc-prod.jsonl --k 10
python3 scripts/rag_eval.py evals/retrieval/section-context-v1.json /tmp/sc-prod.jsonl --k 20
```

Read `passage_recall_at_k` per dimension. On `answer in the following passage`, the
gap between @20 and @10 is what carried context recovered; on `answer in the same
passage` it should be near zero. Each row's `carried_neighbors` and `budget_chars`
say whether a miss was a missing neighbour or a budget the hits had already filled:
with 43 passages of near-equal length, ten full hits leave little room, and a gap of
zero with `carried_neighbors` at zero is that signal, not a retrieval failure. Add a
reranker directory as the last argument to measure the reranked path too.
