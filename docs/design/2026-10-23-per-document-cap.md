# The per-document cap: verdict, eval blind spot, replacement

Read-only review, 2026-10-23. No cargo, tests or evals were run; every number below comes
from the code, git history, the fixtures, and two result files already on disk.
Line numbers are main (`/Users/josh/Code/lattice-temp`, HEAD c694fc79) unless marked
"branch" (`/Users/josh/Code/lattice-retrieval-pr`, eval/retrieval-rework-local at 0026ce4e).

> **Status, 2026-10-23 (build).** The retrieval rework merged as #159 (1018799e), so the
> "branch" line numbers below are now main's, and section 4's sequencing is settled: the cap
> work branches from main. It is built as two PRs: `fix/per-document-cap` (3.1, 3.2, 3.4 and
> the eval cases) and `fix/neighbour-budget` (3.3). One change from section 2: eval cases A
> and B go in **new** documents, so every existing v3 document and query stays byte-identical
> and per-query comparisons with earlier runs still hold.
>
> **Measured, 2026-10-23.** v3 at `--top-k 10` (the sweep's recipe, not the `--top-k 50`
> section 5 names): passage recall@10 0.782 → 0.822, many passages from one document
> 0.778 → 0.944, repetitive document 0.615 → 1.000, buried needle 0.667 → 0.722; 14
> queries better and none worse (p = 0.0001); document recall@5, nDCG@5 and MRR@5
> identical, so risk 1 did not show. Section 3.3 as written here (neighbours placed after
> their anchor inside the budget) lost on v3 and was redesigned in
> `2026-10-23-neighbour-context.md`. Numbers and runs: `evals/retrieval/README.md`,
> "Per-document cap removed".


## 1. Verdict: harmful, and never justified by a measurement

**What it is.** `PER_DOCUMENT_CAP = 4`,
`crates/lattice-desktop/src/features/search/use_cases/hybrid_search.rs:86`. In `fuse()`
(:395-452) the RRF-ordered list is walked once; a document's 5th and later passages go to
`deferred`, which is appended after every `selected` passage, then `truncate(limit)`
(:433-451). So the cap does not reorder within the list so much as **cut**: in any library
where more than `limit / 4` documents return hits, `selected` already fills `limit` and
the deferred tail is dropped. It applies before the cross-encoder: chat fuses at the rerank
pool size (`kb_retrieval.rs:243-254` -> `retrieve(.., candidate_limit)` ->
`library_search.rs:215 fuse(branches, limit, AcrossDocuments)`), so whatever the cap
defers is gone before the reranker scores anything.

**History.** It is a bare literal `4` from the repo's first commit (eb765be9, 2026-09-16,
`fuse_branches` in corpus_plan.rs: `if *count < 4`), with no comment and no measurement.
5d8b7410 (2026-10-09, "run chat, tools, search page and eval through one orchestrator")
moved it into the named constant and wrote today's rationale ("a long document's run of
near-identical chunks cannot fill the list alone") *after the fact*; that commit's own
message reports v3 recall@5 0.7755 "unchanged", i.e. the cap was carried, not tested.
The only measurement in the history is against it: ebfe56ff (#129, 2026-10-18) found that
for claim checking "the one long source holding most of a lesson's evidence" was buried
and "12 claims dropped out of their lists", and added `Spread::ByRelevance` as an escape
hatch for course ranking (`learning/lessons/reference_collection/ranking.rs:146`). The
retrieval rework branch added a second escape hatch for the planner-chosen-document search
(branch `library_search.rs:110-112`, comment: "a per-document cap would only dilute the one
that answers"). Two of the five fusion call sites have already opted out because it hurt.

**What it protects against today, and whether anything else handles it.**
- The stated worry (near-identical chunks from one long document flooding the list) is
  not handled by the cap either: four copies still take four slots. Nothing else dedups
  text: `deduplicate_sources` (`retrieval/source_citations.rs:190`) keys on
  `(document_id, chunk_id)` (URL for web); the reranker reorders but does not dedup;
  neighbour expansion adds adjacent chunks; `EvidenceBudget::select`
  (`lattice-core/.../context_assembler/budget.rs:407`) charges tokens and does not
  recognise repeats. So there is no duplicate handling anywhere, and the cap is not one.
- Its real effect is a document-diversity prior on every ranked list: chat first pass,
  the corrective pass union (`fuse_passes`), the model's `semantic_search` tool
  (`function_calling/executor/search_tools.rs:196`), the search page, Explorer's folder
  search (`explorer/index/search.rs:113`, "per file"), scoped document tools, and the
  planner's shortlist search (`shortlist.rs:27`, which *relies* on the cap for document
  reach: "Fusion keeps at most four passages a document, so this reaches at least ten
  documents"). That last dependency is the one thing that needs a replacement (section 3).

**Evidence it bites on the shipped path (from existing result files, no run).**
`evals/retrieval/results/2026-10-20-v3-LLAMA-EMBED-llama-run.jsonl` (194 queries,
production mode, top-k 50; the llama.cpp embedding path that shipped with #146):
- 39/194 queries have one document holding >= 4 of the top 10 (cap saturated; its 5th+
  passages were deferred). 14/194 have one document holding 4 of the top 5.
- 11 of those 14 are `buried needle in long document` queries where the saturating
  document *is* the gold document. In every one of the 11 the gold document holds
  **exactly 4** chunks in the 50-long list, never 5: the cap's signature.
- In 3 of them (`v3-buried-handbook-prose-1-2`, `v3-buried-agreement-long-1-2`,
  `v3-buried-transcript-long-2-2`) the answering chunk is **nowhere in the 58-entry list**
  (50 + neighbours): passage recall 0 at every k, while document recall@5 is 1. The
  2026-10-19 candle run of the same fixture (fewer distinct documents in its pools, so the
  deferred tail survived the cut) shows the same gold documents with 8-18 chunks in the
  list and `agreement-long-1-2`'s needle at position 6. The recorded data cannot prove the
  needle was in the branch pools in the llama run (branch lists are recorded at document
  level), so the before/after run in section 5 is what settles it; the pattern is exactly
  what the cap does.

Verdict: delete it. It is the arbitrary number Josh describes, it has cost passages twice
where someone looked, two call sites already bypass it, and the fixture that "passes"
cannot see it (next section).

## 2. Eval blind spot: the current evals cannot see the cap

**Fixtures.** No query in any set needs more than two passages from one document:
- `synthetic-library-v3.json`: 194 queries; positives per query 1 (139), 2 (16), 3 (18),
  0 (21). Max `answer_spans` in one document per query: 1 (170 queries), 2 (3 queries:
  `v3-id-runbook-sentence-05/06/07`, spans 1.8-2.2 k bytes apart in 2.7-4 k-byte
  documents, i.e. one or two chunks). Nothing needs a 3rd passage, let alone a 5th.
- `desktop-path-v1.json`: 38 items, 55 turns; `expected_citations` per turn 1 (42),
  2 (7), 0 (6). The most facts one cited document must supply is 3
  (`dp-multihop-garnetline-renewal`, `contract-00`), and they sit 132 bytes apart: one
  chunk. The widest spread of two facts in one document is 2,683 bytes
  (`dp-multihop-part-undertaking`): two chunks.
- v1/v2/starter: document-level relevance only, short documents.

**Metrics.** Document-level recall@k (`ranked_ids` are documents, first appearance) is
*monotone non-decreasing under the cap*: swapping a document's 5th chunk for another
document's chunk can only add a document to the top k. nDCG/MRR are document-level too.
The only metric that can register the harm is `passage_recall_at_k`
(`scripts/rag_eval.py:115`, coverage >= 0.5 of a span), and with at most 2 spans per
document it can only see the cap through the displacement of a single needle (section 1's
3 queries), never through a question that needs five passages. The shipped-path judge
scores facts, but no question asks for more facts than one chunk holds.

**The other half of the blind spot.** The fixture also cannot show the cap *helping*: of
the 123 v3 documents long enough to window, 0 have any pair of 1,200-char windows with
token-5-gram Jaccard > 0.5. There is no near-repetitive document in the eval at all. So
"evals run fine" says nothing about the cap in either direction.

**Eval cases to add** (generated, subject-agnostic, through
`scripts/build_synthetic_library_v3.py`; `DIMENSIONS` at :52, `MIN_PER_DIMENSION = 12` at
:92, long-form builder `build_long` at :874; every query carries `answer_spans`; update
the dimension table in `SYNTHETIC_LIBRARY_V3.md`):

A. **`many passages from one document`** (>= 12 queries). In 12 of the existing 28-87 k
   character long-form documents, plant K = 6 facts about one named thing at six depths
   (one per chunk, > 1,500 bytes apart). Query: "everything the document says about
   <thing>" phrased naturally, graded 3 on that document only, six spans. Non-software by
   construction: a household maintenance log with six repairs of the same boiler, a
   transcript where one speaker makes six commitments, a chronicle with six dated entries
   on one place, a lab notebook with six readings of one specimen, a numbered agreement
   with six clauses binding one party, a cookbook with six variations of one base.
   Score: passage recall@10. With the cap, at most 4 of the 6 spans can ever be in the
   list once `selected` fills (always, on 379 documents), so the ceiling is 0.667 by
   construction; without it the metric is free to reach 1.0.

B. **`repetitive document`** (>= 12 queries, two kinds). Build 6 documents that repeat
   one template many times with one field changing per repeat: scanned minutes whose 10
   pages share header, footer and boilerplate; a saved message thread where every reply
   re-quotes everything before it; a maintenance checklist filled in weekly.
   B1 (flooding guard, 6 queries): the answer lives in a *different, short* document that
   shares the template's topic; grade the short document 3, the repetitive one 0. Score:
   document recall@5 and passage recall@5. This is the number that would show the cap
   doing real work; the prediction is that it does not move.
   B2 (variant check, 6 queries): the answer is the one changed field in repeat 7 of 10;
   grade that document 3 with one span. Score: passage recall@5. This case exists to
   stop anyone replacing the count with a text- or embedding-similarity threshold: the
   nine siblings are > 95 % identical to the answer and a dedup by similarity drops it.

C. **Shipped-path item(s)** in `evals/retrieval/desktop-path-v1.json` (schema: `id`,
   `tags`, `turns[{ask, rubric{must_mention, must_not_claim, expected_citations,
   abstain, notes}}]`; `shipped_path_stats.py compare` leaves unmatched questions out, so
   adding items keeps the 2026-10-13 baselines comparable): one item per planted long
   document from A, asking for all six facts, `must_mention` = the six facts,
   `expected_citations` = that document. With the cap, the fused pool
   (`candidate_pool` = min(3 x depth, 64)) can hold at most 4 passages of that document,
   so the answer can state at most 4 facts: a mechanical fail before any model quality
   enters. Add one `multi-hop` control that must keep passing (the existing
   `dp-multihop-*` items serve).

## 3. Replacement design

Principle: fusion ranks by relevance; counts come from the window, the evidence budget
or the candidate pool; nothing is thresholded on a guessed similarity.

### 3.1 Delete

- `PER_DOCUMENT_CAP` and the `per_document`/`deferred` walk in `fuse()`
  (`hybrid_search.rs:83-86, 433-451`); `fuse` becomes: RRF order, apply branch signals,
  `truncate(limit)`.
- `Spread` (`lattice-core/src/application/contracts/search/ranking.rs:29-36`), its
  re-export (`contracts/search.rs:19`), the `spread` parameter on the port
  (`lattice-core/src/application/ports/hybrid_search.rs:50-55`), on the impl
  (`search/trait_def.rs` fuse), and on `HybridSearchUseCase::fuse`.
- Call sites (branch line numbers): `hybrid_search.rs:318`; `library_search.rs:112, 181,
  386`; `learning/.../ranking.rs:146`. Each loses the third argument. No flag, no shim.
- Comments that describe the cap: `library_search.rs:15-17`, `shortlist.rs:24-26`,
  `search/trait_def.rs:69`, `explorer/index/search.rs:10`,
  `infrastructure/passage_index/text_search.rs:111`,
  `examples/retrieval_eval/production.rs:17`, `hybrid_search.rs:395-397`.
- Tests: `hybrid_search/tests.rs:256-283` keeps its score-scale half and loses the
  `diverse[4] == "b0"` half; `:285-298` (`fusion_by_relevance_keeps_one_documents_run_in_
  fused_order`) becomes the only behaviour, renamed `fusion_keeps_fused_order`. Add: a
  document with 12 top-ranked passages and `limit` 8 returns its first 8.

### 3.2 Near-duplicates: identity, not similarity

Recommendation: **no similarity threshold**, in fusion or anywhere else, and the reasons
are on the record so it is not re-added later:
- The signal cannot be made safe. Text shingles and embedding cosine both say that the
  current and superseded policy (one changed value), week 7 and week 8 of a meeting series
  (one date), page 7 of a repetitive scan (one field) are duplicates; the v3 fixture is
  built from exactly those, and in each the answer is the "duplicate". Any threshold is
  the arbitrary number again with a worse failure: dropping the answer because it looks
  like a non-answer. Eval case B2 encodes this.
- Which near-duplicate is the right one is a *query-conditioned* question, and the
  pipeline already has two query-conditioned rankers: fusion and the cross-encoder. They
  are the arbiters; a similarity step would second-guess them with a query-blind number.
- The cost of a true repeat is tokens, and tokens are counted exactly once, in
  `EvidenceBudget::select`. There is no measured flooding to fix (section 2); if case B1
  ever shows it, the fix is query-conditioned (rerank on, or document-level fusion where
  documents are what is wanted), not a count.

What does go in: **exact-text identity**, which has no threshold. In `fuse()`, after RRF
order, drop a passage whose normalised content (Unicode-NFKC, lower-cased, whitespace
collapsed) hashes equal to one already emitted. It costs one hash per candidate, catches
the same file indexed twice and template blocks that align on chunk boundaries, and can
never drop a passage that carries a different fact. Keep the first (better-ranked)
occurrence; its id is the citation. Test: two chunks with identical text and different ids
in two documents fuse to one result, the higher-ranked.

If a future measurement (B1) does force a near-duplicate rule, this is the only shape
that satisfies the standing rules, recorded here so it is not reinvented: signal = novelty,
the share of a passage's token shingles absent from every passage already selected
(text is in `SearchResultDto.content`; no vector fetch); action = *defer* behind the
non-redundant remainder, never drop; place = after the reranker and before
`EvidenceBudget::select`, in `kb_retrieval.rs`, so the order it works on is already
query-conditioned and the thing it saves is budget; threshold = the chunker's own overlap
ratio (`ChunkerConfig.overlap_tokens / max_tokens`, 120/800 by default,
`indexing/engine/chunker.rs:58-63`): a passage that adds less new text than one overlap
region adds nothing the index does not already repeat by design. Derived, not picked, and
inert for anything that differs by more than an overlap's worth of text.

### 3.3 Neighbour expansion from the budget

On the branch, `expand_evidence` already walks anchors in rank order and inserts each
anchor's unseen neighbours directly after it (`place_after_anchor`); the neighbour query
is structural (previous/self/next chunk inside the same section, SQL `LIMIT 3`,
`repository/workspace/retrieval.rs`). Two counts remain: `anchors.take(8)` and the call
`expand_evidence(.., kb_search_limit + 8)` after `truncate(kb_search_limit)`
(branch `kb_retrieval.rs:416-421`). The `+ 8` is the only room neighbours ever get, and it
is the same 8 in the eval (`NEIGHBOR_HEADROOM`, `examples/retrieval_eval/main.rs:176`).

Derivation: `kb_search_limit = retrieval_depth(evidence_tokens, passage_chars, ...)`
(`kb_retrieval.rs:149`; `lattice-core/.../grounded_generation/mod.rs:246`) is already the
evidence budget expressed in passages. Neighbours are evidence and cost budget like any
passage, and the rework's own placement says anchor and context are "kept or cut
together". So:
- Do not truncate before expanding. Walk the reranked list in rank order; for each anchor
  whose current position is `< kb_search_limit`, insert its unseen neighbours after it;
  stop at the first anchor whose position is `>= kb_search_limit`; then
  `truncate(kb_search_limit)`. No `take(8)`, no `+ 8`: how many anchors get context is
  decided by the budget, and neighbours of the best hits displace the weakest hits rather
  than living in a side allowance. `EvidenceBudget::select` then charges by tokens as now.
- The eval example mirrors it: `evidence_limit = top_k`, expansion in the same walk.
This is a behaviour change in the rework's file and separable from the cap deletion; see
section 4 for where it lands. The trade it makes (context for the top hits over the tail
of the list) is the one the rework already chose for placement; the multi-hop dimension
and `dp-multihop-*` items are its canaries.

### 3.4 The planner shortlist: rank documents, not passages

`candidate_documents` (branch `shortlist.rs:43-68`) asks `fused_search` for 40 passages
and keeps first-appearance document order; without the cap a long document can return
all 40 and the planner sees three documents. The shortlist wants a *document* ranking, so
fuse at document granularity: collapse each branch's chunk list to documents by first
appearance (`documents_in_rank_order` per branch), then RRF the document rankings with
the same weights and `k`. A document that dominates both branches heads the list once,
as it should; the next documents follow by their own best evidence. Size: the planner
takes rows "from the front until its token budget runs out" (`shortlist.rs:131-133`,
`prompt::build` in `planner.rs:163`), so the number of documents wanted is the rows that
budget holds. Expose that estimate from `prompt` (target tokens over the catalog's
measured row cost, which `build` already computes as `shown`/`exact`) and pass it as the
search limit; `search_branches` already asks each branch for `BRANCH_DEPTH x limit`
candidates. `CANDIDATE_SEARCH_PASSAGES` goes. `CONVERSATION_TERMS = 12` is the same kind
of number and out of scope here; note it for the follow-up list.

### 3.5 Callers that change

| Caller | Today | After |
|---|---|---|
| `hybrid_search.rs:318` execute_scoped (search page, Explorer, `semantic_search` tool, scoped document tools, `library_passages`) | `fuse(.., limit, AcrossDocuments)` | `fuse(.., limit)`; identity dedup inside |
| branch `library_search.rs:181` chat first pass | capped at the rerank pool size | fused order; reranker sees every candidate fusion ranked |
| branch `library_search.rs:112` planner-chosen documents | `ByRelevance` | unchanged behaviour, argument gone |
| branch `library_search.rs:386` `fuse_passes` (corrective union) | capped | fused order |
| `learning/.../ranking.rs:146` course index | `ByRelevance` | unchanged behaviour, argument gone |
| branch `shortlist.rs:54` planner shortlist | 40 passages, cap gives reach | document-level RRF, sized by the planner's rows |
| branch `kb_retrieval.rs:416-421` + `expand_evidence` | truncate, then `+ 8` | budget walk (3.3) |
| `examples/retrieval_eval` | mirrors chat incl. `NEIGHBOR_HEADROOM` | mirrors 3.3 |

Related fixed numbers seen on the way, not in scope, listed so they are not mistaken for
derived ones: `BRANCH_DEPTH = 3` (`hybrid_search.rs:76`), `POOL_MULTIPLIER = 3` and
`POOL_MAX = 64` (`lattice-core/.../ranking.rs:47-49`, the latter with a cost rationale),
`HYBRID_VECTOR_FLOOR = 0.15` (:82), `CONVERSATION_TERMS = 12`.

## 4. Sequencing: follow the rework branch, do not fold into it

- The branch is already built past the design doc (365c4d14 "Make the retrieval fixes
  language- and subject-agnostic"): scoped search is `ByRelevance`, neighbours sit after
  their anchor, `evidence_depth` sizes term coverage. Its shipped-path eval is running
  now; its expected score (29-30/38 against 26 and 31) is the baseline the cap change
  must be measured against. Folding the cap in would mix two measurements and reopen a
  branch under test.
- `lattice-temp` main's uncommitted `corpus_plan/*` edits are the **pre-rework** state
  (the entity bridge, `wants_document_openings`, `per_doc = 2`, the `4/6/8` constants the
  rework deletes). They must not be committed over the branch; the cap work starts from
  the rework branch (or from main after it merges), in its own worktree.
- Order inside the cap PR: 3.1 + 3.2 identity dedup + 3.4 shortlist + eval cases A/B/C in
  one PR (3.4 is required by 3.1: without it the shortlist loses reach). 3.3 neighbour
  budget as a second small PR on top, so its effect on multi-hop is attributable. Both
  are deletions plus one mechanism each; builders run `cargo check` only; the coordinator
  runs gates and the two evals once at the end.
- The rework's own follow-up list (`WHOLE_DOCUMENT_PHRASES`) stays separate.

## 5. Risks, and the numbers that prove it

Risks:
1. Multi-document questions with no reranker (search page, Explorer, tool search, chat
   with rerank off): a long on-topic document can fill the list ahead of the second
   document's one needed chunk. The planner's several queries, the scoped search and the
   corrective pass are the mechanisms that cover chat; the search page shows what ranks.
   If it regresses, the answer is relevance (rerank on by default for tool search, or
   document-level fusion where the consumer wants documents), not a count.
2. Shortlist reach if 3.4 is done after 3.1 instead of with it: do them together.
3. Identity dedup hides a second copy's citation when the same text exists twice; the
   kept one is the better ranked and the text is identical, so the citation is correct.
4. The 3 "needle absent" queries may be below branch depth rather than cut by the cap;
   the before/after run tells.

Before/after, same commit pair (branch head, then branch head + cap PR), same recipe:
- v3, llama embeddings, production mode, `--top-k 50`, scored at k = 5 and k = 10:
  recall@5, nDCG@5, MRR@5, passage recall@5/@10 overall and by dimension. Expected:
  `buried needle in long document` passage recall up (the three cut needles:
  `handbook-prose-1-2`, `agreement-long-1-2`, `transcript-long-2-2`), `long-context tail
  facts` up or flat, `near-duplicate distractors` flat (cross-document; the cap never
  touched it), `multi-hop composition` and `graded and multi-document relevance` the
  regression watch (document recall@5 must hold within the paired bootstrap's interval,
  `compare_rag_eval.py --k 5`).
- Mechanical signature from the run file: queries where the gold document holds exactly
  4 chunks in the 50-list, 11 before, ~0 after; queries where the answering chunk is
  absent from the list, 3 before, 0 after if risk 4 is false.
- New dimension A: passage recall@10 ceiling 0.667 before, ~1.0 after. B1: document
  recall@5 of the short document unchanged. B2: passage recall@5 unchanged (and it stays
  the guard against any later similarity dedup).
- Shipped path on the 9B (`shipped_path_stats.py compare`, paired McNemar): the new
  long-document items flip fail -> pass by construction (4 facts reachable before, 6
  after); `dp-multihop-*` hold; overall "can't tell from noise" or "better", never
  "worse". Also report `kb_search_limit`, pool size and the count of passages per
  document in the evidence for one long-document turn, so the change is visible in the
  trace and not only in the score.
