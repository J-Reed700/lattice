# From many misses to two: retrieval and answer evals, 2026-09-16 to 2026-10-24

This file follows the retrieval and answer evals from the first diagnosis to the
last two known misses. For each step it gives the failure, the cause, the fix
and the measured result. It also lists what was measured and not shipped. Every
number comes from a file in this repo: the dated docs in this folder, the design
docs in `docs/design/`, the result files under `results/`, or the message of the
commit named. The source is named next to each number or at the top of its section.

Every shipped-path number is Qwen3.5-9B Q4_K_M on a local llama-server, a
16,384-token window and one slot, unless a row says otherwise.

## Where the answers are now

| Measure | Result | Source |
|---|---|---|
| Long-document answers, 8 items run three times | **23/24** | `2026-10-24-CLOSE-READ.md` §1 |
| Short single-fact answers, 6 items run once | **5/6** | same |
| Close read as shipped, offline on 51 saved readings: facts noted | 281/281 | same, §2 stage 1 (arm C1R0) |
| Same: lines accepted that hold no fact | 31 (66 with the first wording) | same |
| Answers written from those notes: passed | 101/102 (2 repeat a sentence) | same, §2 stage 2 (arm W0K0) |
| v3 passage recall@10, no reranker / Qwen3-Reranker-0.6B | 0.822 / 0.899 | `README.md`, "Section context after every hit" |
| section-context-v1: answers in the passage after the hit that reach the prompt | 11/12 | same |
| Last full `desktop-path-v1` run with result files in this folder | 26/38 (2026-10-20) | `results/chat/qwen3.5-9b-local/2026-10-20-shipped-path-desktop-path-v1.scored.jsonl.summary.json` |

The 23/24 and 5/6 runs used the close read's first wording (commit 7503dcc8,
merged in #181). The wording that shipped was measured offline only. The
long8 and short6 runs are scored by the mechanical checks; their summary files
have no judge verdict.

### Timeline

| Date | PR / commit | Change | Eval | Before → after |
|---|---|---|---|---|
| 2026-09-16 | doc in #14 (79edfb4c) | Grounding verifier fixes | 38 v1 answers (27B); 4 adversarial grounding cases | Legacy runner 26/38 was a regex rate, not accuracy; 4 cases fail → pass |
| 2026-09-18 | doc in b00c0831 | MiniLM reranker made reachable, then measured | v2, production mode | nDCG@5 0.9222 → 0.9222 at 2.01× latency; kept off |
| 2026-09-19 | #21 (ee6587b2) | Chunking, fusion, exact vector scan, new v3 fixture | v3, production mode | recall@5 0.6946 → 0.7755; nDCG@5 0.6139 → 0.6925 |
| 2026-10-09 | #39 (3739f609) | Eval's production mode runs chat's own search | v3 | recall@5 0.7755 → 0.7755; nDCG@5 0.6925 → 0.6904 |
| 2026-10-12 | #43 (57b886e7) | Four retrieval defaults tested | v3, paired bootstrap | Header column kept (keyword nDCG@5 +0.011); Qwen3 reranker off; RRF retune not confirmed; full-precision vectors |
| 2026-10-13 | run at 70cc8c92, doc in #43 | First end-to-end run of the app's chat turn | `desktop-path-v1`, 34 items | 20/34 judge pass^3; 31/34 items fail before generation at 8K |
| 2026-10-20 | run at bfcfd6a8 (after #47, #60, #124) | Planner shortlist, budget cuts evidence, fixed caps removed | `desktop-path-v1`, 38 items | 20/34 → 26/38 (27/38 with a 27B judge) |
| 2026-10-23 | #159 (1018799e) | Retrieval rework: English-phrase gates and title-word bridge dropped | `desktop-path-v1`, 38 items | 32/38 on the branch; 4 items newly "not in your documents" |
| 2026-10-23 | #165 (c05a48f2) | Ordered reading is one evidence source, not a filter | Saved requests of #167's run, 2 seeds | Those 4 items: 2/2 each |
| 2026-10-23 | #167 (775f03ee) | Per-document cap removed | v3 passage recall@10 | 0.782 → 0.822; 14 queries better, 0 worse |
| 2026-10-23 | first commit on the #173 branch | Neighbours placed behind their hit, inside the budget | v3 passage recall@10 | 0.822 → 0.752; redesigned |
| 2026-10-23 | #173 (40fd68f9) | Hits first, neighbours only in the room left | v3; section-context-v1 | 0.822 held; reranked 0.831 → 0.899; 11/12 carried |
| 2026-10-23 | #174 (30c32ed2), evals in #175 | Reading round, then the answer, then a coverage continuation | long8 ×3, short6 ×1 | Long 1/16 → 14/24; short 11/12 → 2/6 |
| 2026-10-24 | #181 (eff77b8d), at 7503dcc8 | Empty readings withdrawn, close read, stop sequences | long8 ×3, short6 ×1 | Long 14/24 → 23/24; short 2/6 → 5/6 |
| 2026-10-24 | #181, last commits | Close read states its criterion after the passages, shows each once | Offline A/B, 51 readings | Facts 281/281 held; non-fact lines 66 → 31; whole passages 13 → 0 |

Denominators change between rows. "1/16" is 8 items replayed on 2 seeds; "14/24"
is the same 8 items through the app three times. The 34-item set grew to 38 on
2026-10-19 and to 50 on 2026-10-23.

## The retrieval-metric era (September to 2026-10-12)

These evals measured components: rankings, and later one search call. The
2026-09-16 run wrote answers, but from saved rankings and whole documents, not
through the app's chat turn.

**2026-09-16, diagnosis** (`2026-09-16-DIAGNOSIS.md`). The legacy runner passed
26 of 38 answers from a 27B endpoint. Reading the failures showed a regex rate,
not accuracy: `do-not-retry-auth` refused correctly but lacked the literal `401`
the question never asked for. The real defects were in the grounding verifier.
It read citation numbers as array positions, dropped invalid citations so other
evidence could rescue a claim, and accepted a judge verdict whose quote was not
in the evidence. Six fixes shipped. The four adversarial cases failed before and
passed after.

**2026-09-18, reranker on v2** (`2026-09-18-RERANKER-RESULTS.md`). The reranker
switch had never worked: nothing downloaded the model. Once it ran, MiniLM changed
nothing (recall@5 0.9706 both, nDCG@5 0.9222 both) and doubled latency. v2 left
no headroom: the first stage already had 0.97 recall. The reranker stayed off,
and the result led to v3.

**2026-09-19, v3 and the modernization, #21** (`2026-09-19-V3-MODERNIZATION-RESULTS.md`,
`SYNTHETIC_LIBRARY_V3.md`). v3 is a messy personal vault: 379 documents, 194
queries, 173 scored.

| Run | recall@5 | nDCG@5 | passage recall@5 | median latency |
|---|---|---|---|---|
| Before | 0.6946 | 0.6139 | 0.6590 | 32.8 ms |
| Before, ms-marco reranker | 0.7100 | 0.6199 | 0.6763 | 69.5 ms |
| **After** | **0.7755** | **0.6925** | **0.7688** | **24.9 ms** |
| After, late chunking | 0.7110 | 0.6353 | 0.6965 | n/a |
| After, ms-marco reranker | 0.7755 | 0.6910 | 0.7688 | 533 ms |

The first merged run was flat (recall@5 0.7004) and entity collisions fell from
1.000 to 0.583. The chunker looked guilty but was not. The answer chunk was
byte-identical in both runs and exact cosine ranked it first; the HNSW index
(16/128/64) did not return it. Against exact cosine it found the true top hit for
181 of 194 queries. Indexes up to 20,000 vectors now use an exact scan (194/194).
Late chunking and the reranker stayed off.

**2026-10-09, the eval runs chat's search, #39** (`README.md`, "Production mode
now runs chat's search"). Production mode had built `HybridSearchService`, which
chat never called. Every production-mode number from 2026-09-16 to 2026-09-19
described a path chat did not run. On chat's orchestrator, recall@5 stayed 0.7755
and nDCG@5 was 0.6904.

**2026-10-12, four defaults, #43** (`2026-10-12-RERANKER-FTS-HEADER-FUSION.md`),
each decided by a paired bootstrap on v3:

| Option | Decision | Evidence |
|---|---|---|
| Qwen3-Reranker-0.6B on by default | Swap the download, keep it off | nDCG@5 +0.048 [+0.002, +0.091], recall@5 +0.015 [-0.027, +0.059]; 5.6 s/query, +3.8 GB |
| Context prefix in its own FTS5 column | Yes, weight 2.0 | Keyword nDCG@5 +0.011 [+0.001, +0.022]; hybrid unchanged |
| Retune RRF `k` and branch weights | No | Best of 12 on half A did not confirm on half B: +0.030 [-0.010, +0.070] |
| 256-d i8 vectors by default | No, full precision | recall@5 -0.017 [-0.041, 0.000] |

## The shipped-path era (2026-10-13 onward)

From here the eval drives the app's own chat turn
(`crates/lattice-desktop/examples/shipped_path_eval`):
intent, the corpus planner, hybrid search, the sufficiency check and retry,
openings and sections, the tool loop, the answer and the grounding check. A
recording proxy keeps every model request verbatim (`*.calls.jsonl`). Those logs
made the later offline A/Bs possible.

### Baseline, 2026-10-13 (`2026-10-12-SHIPPED-PATH-BASELINE.md`)

**Result.** 20 of 34 items passed the rubric judge at pass^3 (Wilson 95% CI
0.42–0.74); 22 of 34 passed the mechanical checks.

**Failures and causes.**
- The planner never planned. Its prompt carried the whole catalog (53–62k
  characters), sized at chars/4. It overflowed the window or missed its 20 s
  timeout, which stood planning down for 10 minutes. The corrective retry needs
  the planner, so it never ran either.
- The budget refused turns. At 16K, 3 of 44 turns failed before generation; at
  8K, 31 of 34 items did. Evidence was sized without reserving room for the
  tool definitions (about 2,100 tokens with the policy), so nothing could be cut.
- Intent sent 4 to 6 of 41 vault questions per arm to the public web.
- The model made no tool calls in 82 answered turns.

**Fixes.** #47 gave the planner a shortlist from a hybrid search, aimed its
prompt at 2,560 tokens counted with the server's tokenizer, and let the budget
cut the lowest-ranked evidence first. #60 replaced fixed content caps (16 chunks
per turn whatever the window, 50,000 characters per web page) with sizes from
the model's window. #124 added a paired exact test and a judge on its own
endpoint. The 10-13 pass^3 had been three greedy grades of one answer, not three
runs.

**Result.** The 2026-10-20 run, on an eval branch (bfcfd6a8) that holds #47, #60
and #124, passed 26 of 38 with the 9B judging itself and 27 of 38 with a 27B
judge. The set had grown by four multi-turn conversations, and many other PRs
landed in that week, so this doc does not attribute the change to one PR.

### Retrieval rework, #159 (`docs/design/2026-10-23-retrieval-rework.md`)

**Failure.** Commit 8f01c13e added retrieval fixes built on rules for English
and for legal text: an entity bridge built on capitalised names, English phrase
lists that gated the planner's `start_at_beginning` flag and its openings, and a
prompt line that told the model a guessed link was fact.

**Fix.** The review kept the structural parts (heading spans, line-start section
numbers, neighbours, coverage sized by evidence depth) and dropped every
English, legal or capitalisation rule. The design expected to lose one or two
items (29–30/38).

**Result.** The branch run (0026ce4e) passed 32 of 38
(`docs/design/2026-10-23-ordered-reading-as-source.md` §3). But four items that
had passed on 2026-10-22 now answered that the documents held nothing.

### Ordered reading as a source, #165 (`docs/design/2026-10-23-ordered-reading-as-source.md`)

**Failure.** `dp-buried-agreement-meeting`, `dp-buried-handbook-ticket`,
`dp-paraphrase-clock` and `dp-transcript-rollback` missed on 2026-10-23.

**Cause.** In all four turns the 9B planner returned `start_at_beginning: true`,
and the pipeline treated the flag as a filter. Searches were confined to the
chosen documents, the scoped search was skipped, opening chunks filled the pool,
the cut to `kb_search_limit` discarded every search result, rerank was off, and
the plan was "sufficient by construction", so no corrective pass ran. #159 had
dropped the English-phrase guard that used to zero the flag. The flag itself was
noise: six passing turns also had it true, and the one real opening question got
it false. The 9B made 0 tool calls in 55 answer rounds on both 10-20 and 10-23,
so any fallback had to be driven by the app.

**Fix.** Every query searches the whole space. The scoped search runs whenever
documents are chosen. Openings take at most half the pool. Every plan is
reranked, and term coverage decides sufficiency.

**Result.** No full-suite run is recorded for #165 alone. On the saved requests
of #167's run, which includes #165, all four items passed on both replay seeds
(`2026-10-23-ANSWER-COVERAGE.md`, "Before" column).

### Per-document cap removed, #167 (`docs/design/2026-10-23-per-document-cap.md`)

**Failure.** `PER_DOCUMENT_CAP = 4` deferred each document's fifth and later
passages behind every other document and then truncated. A long document's
later passages never reached the reranker or the prompt. The cap was a literal
from the first commit; no run had measured it. #129 measured it dropping 12
claims (#167 commit message).

**Why no eval saw it.** No v3 query needed more than two passages of one
document. In `desktop-path-v1` the most facts one document had to supply was 3,
all in one chunk. Document-level recall cannot fall under the cap.

**Fix.** Rank on relevance alone; drop only a passage whose text is identical
to one already placed. New cases: v3 gained 26 documents and 25 queries (many
passages from one document, repetitive documents), and `desktop-path-v1` gained
12 long-document items with six facts each, each in a different chunk.

**Result.** v3, production mode, `--top-k 10` (`README.md`, "Per-document cap removed"):

| passage recall@10 | with cap | no cap |
|---|---|---|
| overall | 0.782 | 0.822 |
| many passages from one document | 0.778 | 0.944 |
| repetitive document | 0.615 | 1.000 |
| buried needle in long document | 0.667 | 0.722 |
| long-context tail facts | 0.583 | 0.667 |

14 queries better, none worse (sign test p = 0.0001). Document recall@5, nDCG@5
and MRR@5 were identical, so multi-document questions held.

On #167's shipped-path run, 8 of the 12 new long-document items still failed
(`2026-10-23-ANSWER-COVERAGE.md`). Every missing fact was in the prompt.

### Neighbour context, #173 (`docs/design/2026-10-23-neighbour-context.md`)

**First design, regressed.** The cap design's §3.3 put each hit's section
neighbours directly behind it, inside the budget. On v3, passage recall@10 fell
from 0.822 (cap removed) to 0.752, and "many passages from one document" to
0.569 (main 0.778). In all 14 lost queries the hits at ranks 4–10 were pushed out.

**Cause.** The prompt fills by tokens in list order, so a neighbour placed early
takes the room of a later hit. Main's app already did this inside
`kb_search_limit + 8`. Main's eval appended neighbours after all hits instead,
so it had never measured what the app sent.

**Fix.** Hits go first, in rank order. Each hit's unseen neighbours follow all
the hits and are carried only in the room the hits leave. The eval mirrors it
with a budget of `top_k` passages of the corpus's mean length. A new fixture,
`SECTION_CONTEXT_V1.md` (German biology, Spanish chronicle), puts the answer in
the passage after a pointing clause, which v3 cannot test.

**Result** (`README.md`, "Section context after every hit"):

| passage recall@10 | main | cap removed | cap removed + neighbours behind their hit | #173 |
|---|---|---|---|---|
| v3 | 0.782 | 0.822 | 0.752 | 0.822 |
| v3, Qwen3-Reranker-0.6B judging | 0.831 | | | 0.899 |
| section-context-v1 | 0.958 | | | 0.958 |

On section-context-v1, 11 of the 12 following-passage answers are carried; all
12 same-passage controls are found.

### Answer coverage: the reading round, #174 and #175 (`docs/design/2026-10-23-answer-coverage.md` §1–8)

**Failure.** On #167's run the 8 long-document items failed with every fact in
the prompt. Replayed on two seeds, they passed 1 of 16.

**Cause.** The answer was a one-pass skim that stopped early. The rubric facts
sat in passages [2]–[8]. The model wrote one bullet per passage it noticed and
stopped with `finish_reason=stop`, well under its token limit. Which passage it
skipped changed with the seed; that it skipped one or two did not.

**Measured and rejected before building.**
- A shorter prompt: hits-only prompts (10 passages, about 4k tokens, no tools)
  still missed 3 of 6 facts on chronicle and on choir.
- Dropping the tools: one win, one loss, three ties.
- Thinking: 133–337 s per answer; on chronicle it ran to `finish_reason=length`
  at 7,690 completion tokens with no answer.

**Rubric problems found on the way.** On `dp-long-lab-notebook` the model was
right to leave out a site visit that is neither a reading nor an observation.
Chronicle's rubric wanted ISO dates and failed "December 1887". #174 changed
four asks to match what their rubrics score and let chronicle accept written-out
dates.

**Fix.** The answer round became a reading round and a writing round on the
same cached prompt. The model first writes one line per passage: the sentences
that bear on the question, copied exactly, or a hyphen. It then answers from its
notes. Passages its notes copy but its answer does not cite are asked for in a
continuation. #175 kept all of it inside the answer's output reservation.

**Result** (`2026-10-23-ANSWER-COVERAGE.md`):

| Set | Before | After |
|---|---|---|
| 8 long-document items | 1/16 (replay, 2 seeds) | 14/24 (three app runs: 4/8, 5/8, 5/8) |
| 6 short single-fact items | 11/12 (replay, 2 seeds) | 2/6 |

Median time to an answer: 92 s. The long items improved, and the short items
regressed into abstentions.

### Reading misses and the close read, #181 (`docs/design/2026-10-23-answer-coverage.md` §10)

**Failure, long items.** Across the 24 readings, 144 passages held a rubric fact:

| What the reading did with a fact-bearing passage | Count |
|---|---|
| Quoted it under its own label | 121 |
| Quoted it under [1], then wrote a hyphen for its real label | 7 |
| Hyphen in a reading that wrote only hyphens (hall r3) | 6 |
| Hyphen, and the sentence quoted nowhere | 10 |

**Cause, long items.** The misses follow rank, not position in the passage.
Among an item's fact-bearing passages, ranks 1–2 were missed 0/46 times, ranks
3–5 5/69, and rank 6 5/23. The willingness to quote drops as the list grows.

**Failure and cause, short items.** Five of six short readings used a numbered
form (`1. [1] -`). Four of those wrote a hyphen for every passage, and one ran
past the last label to `373. [373] -` and the 4,096-token limit (about 165 s).
The all-hyphen notes, replayed as the model's own message, led the answer into
abstaining. On `dp-paraphrase-clock` the writing instruction drew a hedge
("The provided context contains no information…") before the right fact.

**First offline A/B** (`scripts/replay_reading_ab.py`, 30 saved prompts; the
replay reproduced the logged notes on 29 of 30):

| Arm | Facts copied, long | Facts copied, short | Long answers | Short answers |
|---|---|---|---|---|
| A1: fixes to how notes are read, withdrawal, stop sequences | 128/144 | 2/6 | 27/48 | 10/12 |
| A2: second look (ask again for hyphen labels) | none recovered | none recovered | 26/46 | 4/4 |
| W: A1's notes, new write wording | 128/144 | 2/6 | 29/48 | 10/12 |
| F: new reading instruction | 135/144 | 3/6 | 27/48 | 10/12 |

The close read (arm A3) shows the passages a reading marked with a hyphen
again. It ran on the 15 long readings that had missed facts; one (hall r3) was
withdrawn and not read again. On the other 14 it copied 84/84 facts, and answers
passed 27/28, against 74/90 and 11/30 for A1 on the same 15 readings.

**Fix (built).** A reading with no verified copy is withdrawn, and the answer is
asked for as before #174. A note copied from another passage moves to that
passage's label. `1. [1] -` reads as label 1. Text after the note lines is
dropped. Readings stop at the first label past the last one shown. The close
read runs after the reading. The write instruction asks for the passages' own
words and no talk about the notes.

**Not built.** The second look recovered no fact. The new reading instruction
copied more facts and changed no answer.

**Result, end to end at 7503dcc8** (`2026-10-24-CLOSE-READ.md` §1):

| Set | After #174 | 7503dcc8 |
|---|---|---|
| 8 long-document items, three runs | 14/24 | **23/24** |
| 6 short single-fact items, one run | 2/6 | **5/6** |

Median time to an answer: 153 s short, 203 to 231 s long. A C++ build shared the
machine for part of the runs, so these are upper bounds.

### The close-read wording A/B, #181 (`docs/design/2026-10-24-close-read-precision.md`)

**Failure.** On short questions the close read copied whole passages that said
nothing on the question (`dp-paraphrase-clock`: 1,385 and 1,214 output tokens).
The answer then repeated one sentence, ended with "(The answer was cut
short…)", and some answers carried 15 to 17 citations.

**Cause.** The instruction gave no criterion. It said "write its line in the same
form", and the 9B took the form from the first passage in the request. The false
notes then broke the coverage continuation: it quoted 13 whole-passage notes
(about 2,750 tokens) into a window with 2,795 free, so the request went out with
39 tokens of room and was cut off. Repeated passes over the same passages
recovered no fact.

**Fix.** The instruction restates the reading's criterion after the passages,
and each marked passage is shown once (arm C1R0).

**Result** (`2026-10-24-CLOSE-READ.md` §2, offline on 51 saved readings, 15 of
them missing a fact before the close read):

| Arm | Facts noted | Non-fact lines accepted | Whole passages | Requests | Prompt tokens | Output tokens |
|---|---|---|---|---|---|---|
| C0R0, as first built | 281/281 | 66 | 13 | 435 | 490,300 | 15,315 |
| C1R1, new wording, room from the copy bound | 280/281 | 38 | 0 | 125 | 235,437 | 8,225 |
| C2R1, same sentences before the passages | 280/281 | 50 | 0 | 125 | 234,740 | 8,432 |
| **C1R0, new wording, old room (shipped)** | **281/281** | **31** | **0** | 494 | 264,502 | 8,859 |

Stage 2 wrote 102 answers per arm from C1R0's notes:

| Arm | Passed | Repeated a sentence | Continuations | Citations |
|---|---|---|---|---|
| W0K0, as built (kept) | 101 | 2 | 3 | 728 |
| W1K1, one write clause + coverage by statement | 101 | 2 | 7 | 726 |

**Not built.** Sizing the room from what a reply can copy (R1) lost a fact.
W1K1 tied. The design's rule ships it on a tie, but it bought nothing and cost
four more model calls, so the built writing stays.

## Measured and not shipped

| Change | Result | Source |
|---|---|---|
| ms-marco MiniLM reranker | No gain on v2 or on the modernized v3 path; 2× latency on v2, 533 ms/query on v3 | 2026-09-18, 2026-09-19 docs |
| Late chunking | Worse on every v3 metric (recall@5 0.7110 vs 0.7755) | `2026-09-19-V3-MODERNIZATION-RESULTS.md` |
| Qwen3-Reranker-0.6B on by default | nDCG@5 +0.048, recall@5 within noise; 5.6 s/query | `2026-10-12-RERANKER-FTS-HEADER-FUSION.md` |
| RRF retune | Did not confirm on the held-out half | same |
| 256-d i8 vectors by default | recall@5 -0.017 | same |
| Title-word entity bridge, English phrase gates | Dropped on review as language- and subject-specific | `docs/design/2026-10-23-retrieval-rework.md` |
| Neighbours behind their hit, inside the budget | v3 passage recall@10 0.822 → 0.752 | `docs/design/2026-10-23-neighbour-context.md` |
| Fewer passages in the answer prompt | Still missed 3 of 6 facts on two items | `docs/design/2026-10-23-answer-coverage.md` §2 |
| No tools on the answer round | 1 win, 1 loss, 3 ties | same |
| Thinking on | 133–337 s; one answer ran out of window | same |
| Second look by label | 0 facts recovered | same, §10.7 |
| New reading instruction (F) | 135/144 facts vs 128/144; answers unchanged | same |
| Close-read room from the copy bound (R1) | Lost 1 fact | `2026-10-24-CLOSE-READ.md` |
| Write clause + coverage by statement (W1K1) | Tied: 101/102, 2 repeats, 4 more continuations | same |

## Lessons that held

- **Measure the path that ships.** The September production-mode numbers
  measured a search chat never called. The first end-to-end run found a planner
  that never planned, which no component eval could show. Until #173 the
  retrieval eval appended neighbours while the app inserted them.
- **Read the failing answers before trusting the score.** The 2026-09-16 regex
  failures were correct answers. In #174 the model was right on
  `dp-long-lab-notebook`, and chronicle failed on a date format.
- **A fixture can only see what it contains.** v2 was saturated (recall 0.97), so
  v3 was built. v3 had no question that needed more than two passages of one
  document, so the cap cases were added. v3's answers sat whole in one passage,
  so section-context-v1 was built.
- **Fixed caps caused failures.** The per-document cap of 4, the 16 chunks per
  turn and the `+ 8` neighbour slots each hid or caused misses. None had been
  measured. A cap tried as a fix, trimming the answer prompt's passages, did not
  stop the skim.
- **The misses moved from retrieval to the answer.** After #167 and #173 every
  missing fact was in the prompt. The rest of the work was on how the 9B reads
  and writes: first the skim, then the reading's hyphens, then the write step.
- **On a 9B, model flags and tool calls are not a control path.** The planner's
  `start_at_beginning` flag was noise, and the model made no tool calls in 82
  turns on 10-13 or in 55 answer rounds per run on 10-20 and 10-23. Fallbacks
  are driven by the app.
- **Replay saved requests for A/Bs.** Every run logs its requests verbatim. Three
  replay scripts (`scripts/replay_answer_round.py`, `scripts/replay_reading_ab.py`,
  `scripts/replay_close_read_ab.py`) change one thing at a time on the same
  prompts, with no app build.
- **Ship what changes the answer.** The new reading instruction copied more
  facts and changed no answer. The Qwen3 reranker raised nDCG but not recall,
  at 5.6 s a query. W1K1 tied. None shipped.
- **Count tokens, not seconds, on a shared machine.** C1R0 took 4,457 s against
  C0R0's 2,699 s while processing about half the prompt tokens. It ran two hours
  later, with another build on the machine.
- **Small sets need paired tests and repeated runs.** On 34 items a difference
  under about 25 points is noise. The 10-13 pass^3 was three grades of one
  answer. #124 compares runs per question with an exact test, and the long items
  were run three times.

## What is still open

- Two misses in the last end-to-end run (7503dcc8):
  - long8-r2 `dp-long-chronicle`: the answer dropped the dates its notes held.
    This is the write step, not the reading.
  - short6 `dp-section-after-clause`: the reading marked all 25 passages with a
    hyphen and was withdrawn, and the answer took clause 24.111 from the wrong
    agreement. The item has never passed.
  - Both are worked through in "The last two misses" below.
- 2 of 102 answers repeat a sentence, with either write arm.
- No full `desktop-path-v1` run since #181. The last full run with result files
  here is 2026-10-20 (38 items). The 50-item set, with its 12 long-document
  items, has no full-run files in this folder.
- The shipped close-read wording (C1R0) has been measured offline only; the
  23/24 and 5/6 runs used the first wording.
- Only the 8 long items that failed on #167's run and 6 short items were re-run
  after #174.

## The last two misses

Worked out on 2026-10-24 from the saved request logs (`*.calls.jsonl.gz`) and
score files in `results/chat/qwen3.5-9b-local/`. No model was run for this.

### `dp-long-chronicle`: the writer drops dates that stand alone

The question asks for everything the Mill Lane chronicle records about
Greassby Mill, in order. The rubric has six facts, three of them dates:
1850-11-18, 1860-02-24 and 1887-12-21.

In the failing run (`2026-10-24-reading-misses-long8-r2`) the reading copied
all six facts with their dates, and the close read added nothing. The answer
kept "In 1860-02-24", wrote "Later" in place of 1850-11-18 and "Eventually" in
place of 1887-12-21, and put the 1850 event after the 1860 one.

The chronicle writes each date as its own sentence before the event:
`1850-11-18. The wheel at Greassby Mill turned again…`. Every date the writer
has dropped is one of these:

| Run | Date dropped |
|---|---|
| `2026-10-23-answer-coverage-long8-r1` | 1850-11-18 |
| `2026-10-24-reading-misses-long8-r2` | 1850-11-18, 1887-12-21 |
| close-read A/B stage 2, arm W0K0, seed 1, on the 10-23 long8-r3 | 1860-02-24 |
| close-read A/B stage 2, arm W1K1, seed 1, on the 10-24 long8-r3 | 1887-12-21 |

The figures written inside a sentence (17.5 per cent, 44 weeks, £4,525.32)
were never dropped. The writer quotes the event sentence and treats the date
before it as a heading.

How often: the item failed 2 of its 6 end-to-end runs (10-23 r1–r3, 10-24
r1–r3). In the A/B's 204 writes, the only 2 failures were this item; its writes
passed 22 of 24.

Why nothing caught it:

- The coverage check counts a passage as covered when the answer cites it. The
  answer cited all six.
- The grounding verifier rated all 7 claims supported. "Later" is not false,
  only less than the passage says.
- The passages carry no position in the document, only a rank, so "in order"
  can only be honoured through the dates. The rubric does not score order.

`docs/design/2026-10-23-answer-coverage.md` §10.3 already turned down a check
that matches dates or digits word for word in the coverage step (it breaks
across languages and formats). It names a verifier verdict, "states less than
the passage", as the place for this. That verdict is not built.

### `dp-section-after-clause`: never passed

The question: "In the Tessivar Cloud framework agreement, what does the
paragraph right after clause 24.111 say, before Section 25 begins?" The answer
is an unnumbered paragraph: "The Reackgate step obliges the parties to hold an
escalation meeting within 31 weeks of a notified dispute before either may
commence proceedings."

It has failed in every saved run: 2026-10-13 (twice), 2026-10-20 (and its
27B rescore), 2026-10-23 short6 and 2026-10-24 short6.

1. **Retrieval.** In the 10-13 and 10-20 runs the answer passage never reached
   the prompt. Since #167 removed the per-document cap it arrives as passage
   [2]. Passage [1] in both runs is clause 24.111 of the Brackleton Facilities
   agreement, a different document. The planner named the Tessivar agreement as
   the document to open both times, but since #165 a named document is one
   more source, not a filter, so a matching clause number elsewhere still ranks
   first.
2. **Reading.** The reading marked all 25 passages with a hyphen and added a
   paragraph saying the answer is not available. The question asks by position
   ("right after clause 24.111"); the reading copies sentences that bear on the
   question, and the answer sentence shares no words with it.
3. **Withdrawal.** A reading with no verified copy is withdrawn (#181), so the
   close read never ran.
4. **Writing.** Without notes the writer answered from [1], the wrong
   agreement, and said Tessivar's Section 24 ends at 24.111. The verifier rated
   the claims 2 supported, 1 contradicted, 3 unsupported in the 10-24 run; the
   answer was still shown.

`2026-10-24-CLOSE-READ.md` §1 first said the reading "quoted the wrong
agreement". The logs show it quoted nothing; the wrong agreement came in at the
write step.

### Next steps (not run)

Each needs the 9B on an otherwise idle machine; together about four calls on
saved prompts.

- Close read passage [2] on its own with the shipped wording. If the 9B copies
  the Reackgate sentence, measure a close read of withdrawn readings limited to
  the document the planner named.
- Replay the saved no-notes write with [1] removed. If the answer then comes
  from [2], measure ranking the named document's passages ahead of other
  documents' hits. That reorders; it drops nothing.
- For the chronicle, run the four failing answers through a prototype of the
  "states less than the passage" verdict, and check it stays quiet on the
  passing writes of the A/B before building it.

## How to rerun

Do not copy commands from here; use the sections below, which carry the exact
recipes and flags.

- **v3 retrieval metrics**: `SYNTHETIC_LIBRARY_V3.md`, "Running and scoring it";
  the evalopt build and reranker runs in `2026-10-12-RERANKER-FTS-HEADER-FUSION.md`,
  "Reproducing"; paired comparison in `EVALUATION_PROTOCOL.md`, "Paired retrieval
  comparison".
- **section-context-v1**: `SECTION_CONTEXT_V1.md`, "Running and scoring it".
- **Shipped path** (`desktop-path-v1`, long8, short6): `2026-10-12-SHIPPED-PATH-BASELINE.md`,
  "Reproduce". Use `--only ID,ID` for a subset. Recipes are the
  `shipped-path-recipe-9b-local*.json` files in this folder; the long8 and short6
  question and recipe files are in `results/chat/qwen3.5-9b-local/2026-10-23-answer-coverage-*`.
- **Comparing two shipped-path runs and checking the judge**: `EVALUATION_PROTOCOL.md`,
  "Shipped-path statistics: comparing runs and calibrating the judge".
- **Answer-round replay**: `scripts/replay_answer_round.py` (`docs/design/2026-10-23-answer-coverage.md` §8).
- **Reading A/B**: `scripts/replay_reading_ab.py` (`docs/design/2026-10-23-answer-coverage.md` §10.4).
- **Close-read A/B**: `2026-10-24-CLOSE-READ.md`, "3. Running it".

Run one llama-server with the 9B and nothing else compiling or serving.
