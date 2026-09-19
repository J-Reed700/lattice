# synthetic-library-v3: a retrieval fixture with headroom

`synthetic-library-v2.json` is 75 short, clean documents. The production path
scores Recall@5 `0.9706`, nDCG@5 `0.9222` and MRR@5 `0.9461` on it, and a
cross-encoder reranker measured against it on 2026-09-18 moved nothing at all
([`2026-09-18-RERANKER-RESULTS.md`](2026-09-18-RERANKER-RESULTS.md)). A ranking
whose top result is already correct on every query cannot be improved, so v2
cannot answer whether smaller chunks, late chunking, a sparse branch, a
different embedding model or a reranker is worth its latency.

v3 is the fixture that can. It is one person's messy vault rather than a tidy
company wiki: **379 documents, 194 queries, 1.61 M characters**, 1.8 MB of JSON.
The schema is v2's, so `src-tauri/examples/retrieval_eval/`, `scripts/rag_eval.py`,
`scripts/compare_rag_eval.py`, `scripts/validate_rag_evals.py` and
`scripts/eval_integrity.py` all read it unchanged.

## The setting

Everything is invented so that no model can answer from memory. Rowan Selke is a
platform engineer at Quillon Grid; the vault mixes work and home.

| documents | collection |
| ---: | --- |
| 48 | `work/meetings` — four weekly note series, 10 to 14 instances each |
| 40 | `engineering/runbooks` — 20 symptoms × core ring and edge ring |
| 36 | `company/policy` — 12 policy families at three versions each |
| 30 | `personal/journal` |
| 24 | `personal/reading`, 24 `personal/home`, 20 `personal/recipes` |
| 22 | `archive/scans` — OCR-damaged, with repeated page furniture |
| 18 | `work/chat` — timestamped on-call logs |
| 16 | `work/long-form` — 28 k to 87 k characters each, 57 % of the corpus |
| 16 | `vendors/orders`, 8 `vendors/contracts`, 12 `work/projects` |
| 16 | `personal/notes`, 12 `personal/fragments` |
| 15 | `work/international` — German, Spanish, Russian, Japanese, Chinese |
| 18 | `engineering/records` and `finance/extracts` — tables and CSV blocks |
| 4 | `work/facilities` |

## What each dimension probes

Every query carries a `dimensions` list, so `rag_eval.py`'s `by_dimension` block
attributes a regression to a named behaviour. The twelve v2 names are kept with
the same meaning; five are new. The minimum is 12 queries per dimension and the
build fails below it.

| dimension | n | the weakness it exposes |
| --- | ---: | --- |
| `buried needle in long document` | 18 | The answer is one paragraph at 12 %, 50 % or 88 % depth of a 28 k–87 k character document, surrounded by on-topic filler. Oversized chunks dilute it; document-level topicality cannot find it. Four handbooks are a wall of prose with no headings at all, five have `##`/`###`/`####` trees, three are numbered-clause agreements, four are timestamped transcripts. |
| `long-context tail facts` | 12 | The v2 name, kept: the subset of the above whose needle sits at 88 % depth. |
| `near-duplicate distractors` | 22 | Weekly notes, 10 to 14 per series, identical in structure and mostly identical in wording, differing by one owner, one due date, one signal reference, or a decision reversed in one week. The named instance is graded 3, the weeks either side 1, the rest explicitly 0. |
| `current versus superseded policy` | 17 | The v2 name. Twelve policies exist at v1, v2 and v3 with a changed value. A current-value question grades v3 = 3 and both older versions 0; a "what did it used to say" question grades v2 = 3 and its neighbours 1. |
| `exact identifiers and numeric distinctions` | 36 | `ERR-4012`, `PO-88213`, `TCK-1044`, `QG-417-902`, `SIG-11405`, `v6.2.3`, `2029-09-11`, `£4,812.40`. Some queries are the bare identifier and nothing else, some embed it in a sentence. Dense retrieval is weak here and the lexical branch has to carry it. |
| `multilingual and cross-lingual retrieval` | 16 | Short, simple notes in five languages with same-language queries, English queries against a non-English document, and four CJK queries of three characters (`予備鍵`, `配線図`, `验收码`, `备用泵`). Same-language siblings are graded 0. |
| `noisy and OCR text` | 16 | Character confusions (`rn`/`m`, `l`/`1`, `O`/`0`, `cl`/`d`), dropped spaces, hyphenation across line breaks, doubled characters, and an identical scanner header and footer repeated every notional page. Plus chat logs with timestamps and speaker labels. |
| `tables and structured content` | 14 | Markdown tables and CSV blocks where the answer is one cell and the query names the row and the column. Every other cell in the table holds a filler value. |
| `semantic paraphrase` | 48 | The v2 name, used for vocabulary mismatch. Sixteen queries are colloquial rephrasings that share no content word with the formal provision they ask about ("I bought a monitor with my own money and nobody signed off first" against "reimbursement for personal equipment procured without a prior purchase order"). A topically adjacent provision that answers something else is planted in a sibling document and graded 0. |
| `multi-hop composition` | 12 | One document names the vendor for a project, another gives that vendor's renewal date or notice period, and the query names neither. Both documents are graded 3, so Recall@k is the metric that matters. |
| `graded and multi-document relevance` | 25 | The v2 name: queries with two or three positives at mixed grades. |
| `topical hard negatives` | 98 | The v2 name. Sibling runbooks for the other ring, superseded policy versions, the other two things sharing a codename, long notes that mention a topic without answering it, and same-language siblings. |
| `negation and exception handling` | 13 | The v2 name: rules with one named exception and an explicit "there is no other way round it", a runbook that forbids a second ticket, and a reversed decision. |
| `scope and applicability boundaries` | 13 | The v2 name: a rule scoped to the core rings with a sibling scoped to the Wendover Basin edge ring, each the wrong answer to the other's question. |
| `entity collisions` | 12 | The v2 name. Four codenames (`Garnetline`, `Ostrel`, `Sibbe`, `Dunlark`) each name three unrelated things — a product, something personal, and a building or street. |
| `short fragment notes` | 16 | One- and two-line notes with `[[wikilinks]]`, competing against twelve long notes that list every fragment topic by name and explain none of them. |
| `unanswerable questions` | 21 | The v2 marking: `relevance` is `{}`. Plausible, on-domain, with near-miss documents present — an identifier one digit outside every minted range, a named clause that does not exist, an attribute the vault never records. The build proves each of those strings occurs in no document. |

## Passage labels (`answer_spans`)

Document-level Recall@5 cannot tell whether the retrieved chunk actually
contained the fact. On a 60 k-character handbook a document can rank first on
topic alone while the chunk fed to the model holds none of the answer. So every
answerable query carries a new, optional field:

```json
"answer_spans": { "handbook-prose-0": [[46211, 46348]] }
```

The values are `[start, end]` **UTF-8 byte offsets** into that document's `text`,
start inclusive and end exclusive — the same units the Rust chunker already uses.
The field is additive: v1, v2 and `starter.json` have none, and their scores are
byte-for-byte what they were.

`rag_eval.py` scores it as `passage_recall_at_k`, overall and per dimension. A
span counts as retrieved when some chunk in the top `k` of `chunk_ranked_ids`
covers at least `PASSAGE_COVERAGE` (half) of its bytes; a chunk boundary that
splits a fact leaves neither half able to answer, so a grazing overlap is not a
hit. The metric is `null` — never a guessed zero — unless the run row carries
both `chunk_ranked_ids` and `chunk_spans`.

### What the Rust harness would need to emit

Nothing in this branch changes Rust. To turn `passage_recall_at_k` on, the
production harness needs one new field on its run rows:

```json
"chunk_spans": { "handbook-prose-0#37": [45120, 48310] }
```

The offsets already exist. `prepare_chunks` records `start_idx` and `end_idx`
against the document text as byte offsets, and `production.rs` already binds
them into `text_chunks.start_char` / `end_char`. Three edits:

1. `src-tauri/examples/retrieval_eval/production.rs:131` — add `start: usize`
   and `end: usize` to `RankedChunk`.
2. same file, around line 320 — widen `chunk_positions` from
   `HashMap<String, usize>` to carry `(chunk_index, start_idx, end_idx)`, and
   fill the two new fields in `rank()` and in the neighbour-expansion path
   (neighbours need a `SELECT start_char, end_char`, which the table already has).
3. `src-tauri/examples/retrieval_eval/main.rs:606` — beside `chunk_ranked_ids`,
   emit `"chunk_spans"` as a map from `RankedChunk::locator()` to
   `[start, end]`.

Embedding mode has no chunk ranking, so it would leave the field out and the
metric would stay null there.

## Regenerating

The fixture is generated, not hand-written, and the generator is the source of
truth. Same seed, same bytes.

```sh
python3 scripts/build_synthetic_library_v3.py
```

That rewrites `evals/retrieval/synthetic-library-v3.json` in place, then runs
`scripts/validate_rag_evals.py` over every fixture and prints a summary. Content
pools live in `scripts/synthetic_library_v3_data.py`; documents are composed from
sentence templates, per-domain vocabularies and a mint that issues every
distinctive string in the corpus.

`--check` asserts the committed file matches a fresh build without writing:

```sh
python3 scripts/build_synthetic_library_v3.py --check
```

`scripts/test_synthetic_v3_rag_eval.py` asserts the same thing in CI, plus
determinism across two builds, the per-dimension minimums, the size budget, and
that each integrity check fails on the mistake it exists to catch.

### What the build proves before it writes anything

- **Needle uniqueness.** Every needle carries distinctive tokens, and the build
  fails unless each token occurs in *exactly* the documents its query marks
  relevant. Filler cannot accidentally answer a query. The mint additionally
  refuses to issue a token that is a substring of another, so a search for
  `ERR-4012` can never match `ERR-40128`.
- **Absence.** Every string an unanswerable query asks about occurs in no
  document.
- **No self-quoting.** No query shares a 36-character run with its own answer
  passage, except in the identifier dimension where quoting the identifier is
  the point, and for the CJK queries that are three characters long.
- **Labels.** Every relevance key is a real document, every `answer_spans` key is
  a positively graded document, no query has more than three positives (so
  Recall@5 is attainable), every dimension is in the declared vocabulary, and the
  output passes `rag_eval.validate_dataset` and `eval_integrity`.

## Running and scoring it

```sh
cd src-tauri
SQLX_OFFLINE=true cargo run --release --example retrieval_eval -- --mode production \
  --top-k 50 /path/to/qwen3-embedding-0.6B \
  ../evals/retrieval/synthetic-library-v3.json > /tmp/v3-prod.jsonl
cd ..
python3 scripts/rag_eval.py evals/retrieval/synthetic-library-v3.json /tmp/v3-prod.jsonl \
  --k 5 --abstain-field sufficient
```

v3 is roughly 50× v2's text, so indexing dominates the run. Read `by_dimension`
before the overall number: an overall Recall@5 that holds while `buried needle in
long document` or `near-duplicate distractors` drops is exactly the regression
this fixture exists to catch.

Paired comparison of two configurations works unchanged:

```sh
python3 scripts/compare_rag_eval.py evals/retrieval/synthetic-library-v3.json \
  --baseline /tmp/v3-baseline.jsonl --candidate /tmp/v3-candidate.jsonl \
  --k 5 --max-regression 0.02 --min-queries 5
```

## Known limits

- **It is still synthetic.** Filler is composed from sentence templates, so it is
  more regular and more repetitive than real notes. A retriever that learns the
  template distribution would look better here than on a real vault. The needles
  are the part that is carefully constructed; the prose around them is not.
- **Needle tokens survive OCR damage.** The scans are corrupted everywhere except
  inside the identifier and the invented name, because a mangled needle cannot be
  proved unique. Real OCR damages the identifier too, so this understates the
  difficulty of the lexical branch on a genuinely bad scan.
- **Vocabulary-mismatch provisions sit in hosts that do not always match their
  topic.** A reimbursement rule may be appended as a "standing provision" to an
  encryption standard. That is deliberate — it tests that document-level
  topicality does not decide the answer — but it is not how a tidy vault reads.
- **Graded neighbours in a series are a judgement call.** Only the weeks either
  side of the named instance are graded 1. Grading all 13 instances would put
  Recall@5 out of reach of a perfect ranking.
- **No thresholds.** As with v1 and v2, this fixture supplies no pass mark. Set
  one from a measured baseline on the same corpus, model and hardware, and read
  [`EVALUATION_PROTOCOL.md`](EVALUATION_PROTOCOL.md) before treating any number
  here as a product claim.
- **`answer_spans` is unscored today.** Until the Rust harness emits
  `chunk_spans`, `passage_recall_at_k` is null on every run.
