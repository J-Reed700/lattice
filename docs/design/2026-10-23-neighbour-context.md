# Section context around a hit: carried with the room the hits leave, never instead of a hit

Status: design, 2026-10-23. Follows `2026-10-23-per-document-cap.md` (§3.3 of that design is
superseded by this one). Measured on synthetic-library-v3 (405 documents, 219 queries, 273
answer spans, production mode, top-k 10, no rerank, qwen3-embedding-0.6b).

Owner's rules applied: no count, ratio or threshold that is not derived from the window, the
token budget or the data; no shims or flags; server-, subject- and language-agnostic; no
per-document cap, spread mode or "+N headroom".

## 1. Verdict

**The losses are B's walk. None is attributable to A.**

In all 14 lost queries the A+B list opens with the same hits as main, in the same order; every
entry that is new is a chunk adjacent to an earlier chunk of the same document, inserted by
`place_after_anchor` (`library_search.rs:341-360` on B) directly behind its anchor, and the
hits main had at ranks 4–10 are what fell off the end of the `kb_search_limit` cut. The two
gains are the same mechanism landing on a near-duplicate chunk by luck (both "repetitive
document" variants). A's exact-text dedup removed no leading hit in any lost row, and the
planner shortlist does not run in production eval mode. The A-alone run
(`scratchpad/evals/a-v3-*`) had not been written when this was finished; see §6 for what it
can still show and what would follow.

**The mechanism is older than B.** On main the app does the same thing: `kb_retrieval.rs:401-407`
truncates to `kb_search_limit`, then `expand_evidence(.., kb_search_limit + 8)` with
`anchors.take(8)` (`library_search.rs:315`) inserts each anchor's neighbours *behind the
anchor*, inside a list of at most `kb_search_limit + 8`. The prompt then fills by tokens in list
order (§2), and the budget was sized for `kb_search_limit` passages, so roughly the first
`kb_search_limit` entries of that interleaved list are carried: the first anchors' neighbours
in, hits 4–10 out. Main's eval mirror did not mirror that: `production.rs:533-555` *appends*
neighbours after all hits (`NEIGHBOR_HEADROOM = 8`, `main.rs:176, 562`), so the "main @10"
column measured hits alone and "main @20" measured neighbours that cost nothing. B's mirror
is the first honest measurement of the shipped mechanism, and it says: context inserted ahead
of ranked hits costs far more than it earns.

## 2. How the evidence budget is really consumed

| Fact | Where |
|---|---|
| `kb_search_limit = retrieval_depth(evidence_tokens, passage_chars, chars_per_token)` = `ceil(evidence_tokens / mean passage tokens)`, the evidence budget expressed in passages, sized so the hits alone can fill it | `kb_retrieval.rs:138-149`; `lattice-core/.../grounded_generation/mod.rs:245-262` |
| `evidence_tokens` is `evidence.remaining()` when KB retrieval starts; web context is charged later, so the real passage room is ≤ that | `pipeline.rs:164-168`; `turn/assemble.rs:128-135` |
| The prompt carries passages by **tokens, in list order, skip-and-continue**: each result is kept when its rendered cost fits what is left and skipped otherwise | `turn/assemble.rs:136-140`; `context_assembler/budget.rs:404-419` |
| A passage is priced as rendered (document name, id, heading, OCR note, content) | `prompting.rs:519-547` (`kb_passage`) |
| Sources not carried are dropped after the cut; citations are numbered after that, in sources order | `turn/assemble.rs:150-154, 223` |
| The reranker (on for every plan since #165) judges `title + content` of the first `max_candidates` of a pool `candidate_pool(limit) = min(3 × limit, 64)`; the rest keep fused scores | `kb_retrieval.rs:241-279`; `hybrid_search/rerank.rs:42-48`; `contracts/search/ranking.rs:48-76` |
| A neighbour is the previous/next `chunk_index` in the same section (or a subsection of it), at most 3 rows; a chunk with no section has no neighbourhood | `repository/workspace/retrieval.rs:253-285` |

So: **the budget is tokens; the list order is the priority order; `kb_search_limit` is only
the depth the search is asked for so that the hits can fill the room.** Anything placed ahead
of a hit in the list takes that hit's budget one for one (less length variance). On main the
`+8` neighbours reach the prompt exactly when they are placed before a hit that then does not
fit — which is what the interleaving guarantees for the first anchors.

## 3. What the rows say

Per answer span, where main found it (`scratchpad/rank_buckets.py` over `base-v3-run.jsonl`):

| bucket | spans | share |
|---|---|---|
| in hits 1–3 | 162 | 59% |
| only in hits 4–10 | 46 (34 queries) | 17% |
| only in an appended neighbour (ranks 11–18) | 3 (3 queries, all "repetitive document") | 1.1% |
| not retrieved | 62 | 23% |

A+B: adjacency-inserted entries occupy 3.9 of the 10 slots on average, ≥ 6 of 10 in 69 of 219
queries; spans found via such an entry: 4 of 273. Answer spans of the same document sit a
median 7,405 bytes apart (min 1,491); chunks are a median 1,599 bytes (p10 644, p90 2,135).
Adjacency is not where the next answer is; it is 1% of the recall and it was being paid for
with the 17% that lives in hits 4–10.

## 4. Design: context is carried with the room the hits leave

Three rules, no constants.

1. **The hits are the evidence.** After the rerank/corrective passes, the list is cut to
   `kb_search_limit` as today. Those passages, in that order, are the first claim on the
   budget. Nothing is inserted among them.
2. **Context candidates follow the hits.** For every hit, in rank order, its unseen section
   neighbours (the repository's definition: same section or subsection, `chunk_index` ± 1)
   are **appended** after all hits, tagged `retrievalMethod = section_neighbor`, in the
   order the section reads (previous, then next). A neighbour that is itself a hit keeps
   its rank and is not repeated; a neighbour of two anchors appears once, after the first.
   No `take(n)`, no `+ n`, no truncation of the context tail: how many are fetched is "one
   neighbourhood per hit", and how many are carried is decided by rule 3.
3. **The budget decides.** `turn/assemble.rs` is unchanged: `evidence.select` walks hits
   then context, skip-and-continue by rendered tokens. Context is carried exactly when the
   hits leave room — a thin result set, short passages, a window larger than the library's
   passages fill. A hit is never displaced by a neighbour: a neighbour can only use room a
   hit did not fit into, which is the same rule two hits already live by.

Derivations: the room for context is `evidence_tokens − Σ tokens(hits carried)`, measured
by the prompt builder at the moment it is known. The number of anchors is the number of
hits (`kb_search_limit`, itself derived from the window and the measured passage length).
The neighbourhood is the section structure of the document. Nothing else enters.

What this does to the three layers:

- `corpus_plan::expand_evidence` (B: `library_search.rs:313-334`) becomes
  `append_context(repository, results, allowed, limit)`: `results.truncate(limit)`; snapshot
  the hit ids; for each, `retrieval_neighbors`, push each neighbour not already present onto
  the end. `place_after_anchor` is deleted. The call in `kb_retrieval.rs:403-410` keeps its
  arguments; its comment changes to say what §4 says. `search_response.total` counts hits
  plus context candidates; the step line written earlier (`passage_count_line`) still counts
  hits, which is right.
- Reranker on: the pool is `3 × kb_search_limit` (≤ 64) and already contains every neighbour
  with any lexical or dense signal, judged like any other passage; a neighbour that the
  reranker ranked below the cut is still a context candidate, behind the hits. Reranker off
  or unavailable: identical rules over the fused order. The mechanism does not change with
  configuration, which is the point.
- Presentation: the prompt shows hits in rank order, then carried context, each passage
  already headed by its document and `Source heading` (`kb_passage`), so a neighbour names
  the section it continues. Grouping a carried neighbour directly after its anchor in the
  prompt is a separate change (it must reorder `retrieval.sources` the same way before
  `assign_citation_ids`, `assemble.rs:223`, so numbers run in prompt order) and is listed in
  §10, not built here.

### 4.1 The eval mirrors the shipped rule

`examples/retrieval_eval` has a count, not a budget, so the mirror derives the budget the
same way the app derives the count, inverted:

- `budget_chars = top_k × mean_chunk_chars`, with the mean measured over the fixture's chunk
  store (`AVG(LENGTH(content))` of `text_chunks`), the measurement the app makes with
  `retrieval_mean_chunk_chars` over the scope. (App: `limit = ceil(budget / mean)`.)
- `rank()` as today (fused, reranked when a reranker is given, cut to `top_k`). Then
  `carry_context(&mut chunks, budget_chars)`: `room = budget_chars − Σ content chars of the
  hits`; for each hit in rank order, each unseen neighbour is appended when its content
  fits `room` (skip-and-continue, the app's `select`), charging as it goes. Hits are not cut
  by the eval's budget: the row keeps its established meaning, so earlier runs stay
  comparable.
- Row fields: `chunk_ranked_ids` = hits (top_k) then **carried** context, in that order (so
  passage@top_k measures the hits, passage@2·top_k measures what the prompt would see);
  plus `budget_chars`, `carried_neighbors`. `NEIGHBOR_HEADROOM` and `EVIDENCE_ANCHORS` go.
- The eval's neighbour query (`production.rs:577`) must gain the app's subsection clause
  (`OR INSTR(c.section, ? || ' > ') = 1`, `retrieval.rs:270`); today the two differ.
- To measure the shipped configuration, v3 is run twice: without a reranker (the path when
  none is loaded) and with the reranker the app ships (`RERANKER_DIR`, `--rerank-depth`
  left at the tuning default so the pool is `candidate_pool(top_k)`, `production.rs:399`).
  Both are shipped behaviours; the table in §7 is the first.

## 5. Alternatives, and why not

- *Neighbours earn a place by relevance in the rerank pool*: a cross-encoder pass per
  neighbour (≤ 2 per pool member, pool ≤ 64) on every turn, for a gain the rows put at ~1%
  and the v2 history put at zero; and with no reranker loaded there is no judge, so the
  mechanism would exist in one configuration only. Revisit only if the reranked v3 column
  shows spans in context that the leftover fill does not carry (§10).
- *Score neighbours with the same branches*: the branches already rank every chunk with any
  signal, `BRANCH_DEPTH × limit` deep; a neighbour outside the pool has less signal than
  every pool member by the branches' own measure; re-scoring it changes nothing.
- *Merge adjacent hits into one segment*: presentation only; changes citation granularity
  (one number per chunk, and `chunk_spans` scores per chunk) to save a header per merge; not
  a recall mechanism. Belongs with prompt grouping, §10.
- *Drop expansion entirely*: it is measured to carry 3 of 273 spans and, behind the hits,
  costs nothing; the #165 section-after-clause items need exactly this context. Deleting it
  would lose those for no gain.
- *Reserve a share of the budget for context*: a ratio nobody can derive. The honest lever
  for more context is relevance (the first bullet), not a reservation.

## 6. A alone

Not measured when this was written (`a-v3-run.jsonl` absent). From the rows: every @10 loss
is adjacency; A can only reorder within the fused list (the cap only ever deferred a
document's 5th+ passages behind other documents), so A alone should recover the "many
passages" spans main's cap pushed behind other documents (e.g. `v3-many-allotment-agreement-long`,
ranks 5–8 were `repeated-allotment-checklist`). One mechanism *could* show as an A loss: a
multi-document query where a dominant document's lower hits now outrank another document's
only hit (`v3-multihop-notice-07`, `project-brief-07#0` at rank 8 behind `allotment-agreement-long`'s
deferred chunks). If `a-v3` shows that on "multi-hop composition" or "graded and
multi-document relevance", the ranking said what it said; the fix is retrieval signal for the
second hop (the planner's second query reaching fusion with its own results), not a diversity
constraint. Nothing in §4 depends on it; the comparison to run is `a-v3-run.jsonl` vs
`base-v3-run.jsonl` on those two dimensions with the same loss script.

## 7. Expected effect on v3 (no rerank)

| dimension | main @10 | A+B @10 | expected @10 | expected @20 (hits + carried context) |
|---|---|---|---|---|
| overall | 0.782 | 0.752 | 0.78–0.80 | +0.00–0.015 over @10 |
| many passages from one document | 0.778 | 0.569 | 0.78–0.86 (A's cap removal; §6) | same (long passages leave no room) |
| buried needle | 0.667 | 0.611 | 0.667 | ≥ 0.667 |
| noisy/OCR | 0.688 | 0.500 | 0.688 | ≥ 0.688 |
| long-context tail | 0.583 | 0.500 | 0.583 | ≥ 0.583 |
| semantic paraphrase | 0.500 | 0.417 | 0.500 | ≥ 0.500 |
| multi-hop | 0.542 | 0.500 | 0.50–0.54 (§6 risk) | same |
| repetitive document | 0.615 | 0.769 | ~0.615 (the 2 @10 gains were adjacency luck) | 0.62–0.85, as room allows |

Document recall@5 ≈ A alone (0.78–0.79). Uncertainty: the @10 column is A's hit order, which
no run has yet measured in isolation; the @20 gain depends on how much room short-passage
queries leave, which the budget model in §4.1 computes rather than assumes. The shipped-path
(9B) set should move the same way: `dp-long-*` were paying the same displacement in the app;
`section-after-clause` now depends on room, and if it fails for lack of room that is the
signal to open §5's first bullet, measured.

## 8. Tests

All fixtures non-English and non-software; the existing `mitose` fixture (German, cell
biology: `library_search.rs:466-492`, eight chunks of `Zellteilung > Mitose` plus three
sectionless glossary chunks) stays; add `crónica` (Spanish, a parish chronicle:
`Crónica parroquial > 1911` chunks 1–6, a subsection `Crónica parroquial > 1911 > Cosecha`
chunks 7–8, and a `Crónica parroquial > 1912` chunk 9).

Unit, `library_search.rs` (replace B's six `expand_evidence` tests):
1. `context_follows_every_hit_in_rank_order`: hits `[m2, m6, t1]`, limit 10 →
   `[m2, m6, t1, m1, m3, m5, m7]`; nothing between the hits.
2. `a_hit_that_is_also_a_neighbour_keeps_its_rank_once`: hits `[m3, m2]` → `[m3, m2, m4, m1]`.
3. `hits_past_the_limit_are_cut_before_context_is_read`: hits `[m1, m5, m8]`, limit 2 →
   `[m1, m5, m2, m4, m6]` (m8 cut, no neighbour of m8).
4. `a_subsection_is_its_sections_context` (crónica): hit chunk 6 → chunk 7 (`… > Cosecha`)
   is appended, chunk 5 before it; hit chunk 8 → chunk 9 (`1912`) is not.
5. `context_never_takes_a_hits_room`: results from test 1 priced by content chars through
   `EvidenceBudget::select`; a budget equal to the hits' total carries all hits and no
   context; one passage of room more carries exactly `m1`; a budget short of the hits
   carries the hits that fit and still no context ahead of a hit.
6. `a_chunk_without_a_section_has_no_context`: hit `[t2]` → `[t2]`.

Eval, `examples/retrieval_eval/production.rs` tests (sqlite fixture = crónica):
7. `carry_context_appends_within_the_room_the_hits_leave`: top_k 2, mean 100 chars, hits
   of 80 and 90 chars → room 30; neighbours of 25, 40, 20 chars in anchor order → the 25
   and the 20 are carried, the 40 is skipped; `carried_neighbors == 2`.
8. `the_mirror_reads_subsections_like_the_app`.
9. `schema_matches_what_the_search_services_query` stays.

Eval cases: v3 entries stay byte-identical. Add `evals/retrieval/section-context-v1.json`
(new file, not an edit): a German cell-biology handbook and a Spanish parish chronicle whose
answer spans sit in the chunk *after* a clause that points at it ("siehe nächster Abschnitt",
"como se detalla a continuación"), plus the same documents' plain-fact queries as controls;
documented in `docs/evals/` beside the v3 notes with the run recipe. It measures the only
thing v3 cannot: whether carried context holds answers that hits alone miss.

## 9. Build split (two Opus builders, disjoint files, `cargo check` only)

**Builder 1 — app** (`crates/lattice-desktop/src/features/conversation/chat/retrieval/`):
- `corpus_plan/library_search.rs`: `expand_evidence` → `append_context` as in §4; delete
  `place_after_anchor`; tests 1–6; `mitose` kept, `crónica` added.
- `corpus_plan/mod.rs` (re-export rename) and `kb_retrieval.rs:400-412` (call + comment).
  No signature change on `ChatRecords::retrieval_neighbors`; no change in `turn/assemble.rs`.

**Builder 2 — eval** (`crates/lattice-desktop/examples/retrieval_eval/`, `evals/`, `docs/evals/`):
- `production.rs`: `mean_chunk_chars()` on the index; `expand` → `carry_context(ranked,
  budget_chars) -> usize` (carried count); subsection clause in `neighbors`; tests 7–9.
- `main.rs`: delete `NEIGHBOR_HEADROOM`; `budget_chars = top_k × mean`; row fields per §4.1;
  doc comment lines 20–33.
- `evals/retrieval/section-context-v1.json` and its `docs/evals/` note.

Coordinator after both: full gates once; v3 twice (§4.1); `section-context-v1`; the
shipped-path set; `scripts/rag_eval.py` needs no change.

## 10. LATER (recorded, not built)

- Group each carried neighbour behind its anchor in the prompt, renumbering sources in the
  same order before `assign_citation_ids` (`assemble.rs:138-223`); then merging adjacent
  carried chunks of one section into one segment, which needs per-chunk citation spans kept.
- Neighbours of the would-be-carried hits into the rerank pool, if the reranked v3 column or
  `section-context-v1` shows answers in context that room did not carry; measured cost per
  turn goes in the same table.
- Batched neighbour query (one `json_each` join for all anchors) if the per-anchor loop shows
  in `kb_total_ms` on large windows; `build_source_citations` over context candidates is the
  other per-row cost to watch (`kb_build_sources_ms`).
- `CONVERSATION_TERMS = 12` and `BRANCH_DEPTH` remain the two counts noted in the cap design.
