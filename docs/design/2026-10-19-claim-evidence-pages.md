# Claim evidence pages sized by relevance, not by room

Date: 2026-10-19. Status: design, decisions taken (§10); nothing built. Code read on `fix/claim-check-shrinks` in `/Users/josh/Code/lattice-fit` at `cb366a3a` (part 1, PR #114, landed). `path:line` is under `crates/lattice-desktop/src/features/learning/` unless prefixed `crates/`. Part 1 gave the checker a typed window refusal, `AppError::PromptExceedsWindow` (`crates/lattice-core/src/shared/error/types.rs:114-119`), a batch that is checked in halves when refused (`content_verification/evidence_checks.rs:355-380`), a single claim that drops its least-needed passages and asks again (`content_verification/judge.rs:50-129`, with `ranking()` at `:14-31`), and receipts that name only the passages the judge was shown (`ClaimReceipt.shown`, `content_verification/mod.rs:269-280, :298-303`). This design sits on top of that and never replaces it.

## 1. Problem

Live run, 2026-10-19: a 27B model behind a llama-server with a 131,072-token configured window checked 89 lesson claims. 52 reused saved checks, 32 went to the model in groups of 8, then deepening re-checked the claims still unsupported against further pages. Every model request was packed to the window:

| request | claims | prompt tokens (exact) | evidence per claim |
|---|---|---|---|
| first checks | 8 | 102,915–112,682 | ~13k |
| page 2 | 8, 8, 5 | 116,432 / 112,201 / 77,252 | ~14k |
| page 3 | 7 | 119,027 | ~17k |
| page 4 | 4 | 126,617 | ~31k |
| page 5 | 3 | 131,903 → refused, job failed | ~43k |

Each request took 2–4 minutes (prompt eval 100–160 s, prefix-cache reuse 1%); the phase took 34 minutes and then failed. Deepening log: round 1 settled=20 looking=7; round 2 settled=3 looking=4; round 3 settled=1 looking=3 (round 1 covers 27 walkers: 21 sent to the model in three calls, the rest restored from saved page receipts, `evidence_checks.rs:635-647`).

The size is the design, not a bug. A claim's page is an equal share of its batch call's room, and the share is filled with ranked passages until it is full. Fewer claims left means more evidence each; a bigger window means bigger prompts. Part 1 stops the failure; it does not change what a request reads.

## 2. Current behaviour

- Page size. `check_with_citations` sets `sharing = BATCH_SIZE.min(total)`, `BATCH_SIZE = 8` (`evidence_checks.rs:20, :463`). `claim_share` (`mod.rs:422-438`) is `ClaimChecker::batch_evidence_room(8 copies of the claim) / 8` (`crates/lattice-core/src/application/services/claim_verification/batch.rs:118-129`): the window less the batch system prompt, the claims and `ANSWER_ROOM_TOKENS` (1,200) per claim, divided equally. At 131k that is ~14k tokens per claim.
- Filling. `first_page` (`mod.rs:492-526`) takes passed example runs, the section's own cited passages while they fit (`citations.rs:209-229`), then `lesson_sources_first` (`citations.rs:122-153`): passages of sources the lesson cites that mention the claim's subject, then passages from anywhere, each list taken best-first "while `fits` accepts them" (`within_share`, `mod.rs:451-462`). `verification_page_within` (`lessons/reference_collection.rs:428-459`) fetches 16 hits, doubles while everything still fits, and keeps the longest fitting prefix. The page ends exactly when the room runs out.
- Deepening. `deepen` (`evidence_checks.rs:554-720`) recomputes `sharing = BATCH_SIZE.min(walks.len())` each round (`:589`), so the share grows as claims settle: 7 walkers → ~17k, 4 → ~31k, 3 → ~43k. A walk ends when a page has no cited passage and no passage mentions the claim's subject (`:624-631`); deepening stops after a round that settles nothing (`:714`). Since part 1 a walk's `shown` grows only by what the judge actually saw (`:694`).
- Passage size. A hit is a 1,200-char chunk widened 300 chars each side to paragraph edges (`reference_collection.rs:16-19, :342-399`), then `verification_context` adds 1,700 chars each side and merges overlapping windows (`:642-704`): ~5,200 chars, ~1.3k tokens. ~13k per claim is ~10 passages.
- Where the retrieval scores are dropped. The library's scoped hybrid search returns each chunk with its weighted reciprocal-rank fusion score (`search/use_cases/library_passages.rs:74-100`, `result.score` from `HybridSearchUseCase::execute_scoped` → `fuse`; no reranker runs on the scoped path). `retrieve` maps each hit to a `Span { source, start, end }` through `library_spans.get(&hit.chunk_id)` (`reference_collection.rs:329`), and the score is gone. `keyword_ranking` (`:533-566`) computes BM25 per chunk, sorts, and returns `Vec<Span>` (`:558`), and that score is gone. `merge` (`:342-399`) assigns `ReferencePassage.score = 1/(60 + rank + 1)` per list (`:349-353`): the only score the page builder, `judge::ranking()` and the report ever see is a rank position. The two lists come from disjoint sources (snapshots the library indexed vs the rest) and are interleaved, deliberately not fused (`crates/lattice-core/src/application/contracts/search/fusion.rs:11-13`).
- Content signal. `mentions_subject_of` (`reference_collection.rs:486-508`): whether a text contains one of the claim's distinctive content terms, the rarer half of its terms by collection frequency, ignoring terms in no passage or in every passage.
- Order and keys. `canonicalize_evidence` (`mod.rs:528-550`) sorts a page by kind (`RESEARCHED`, `CITED`, retrieved), source and offset; the judge reads that order (`target`, `evidence_checks.rs:230`; `render_batch_evidence`, `batch.rs:26-56`, interns identical texts and keeps each claim's `evidenceIds` in the given order). `ClaimChecks::key` (`mod.rs:211-222`) digests unit, statement and the canonical passages (source, text, offsets), never `score`. The receipt key (`:283-295`) adds policy, model, window and trust version. The saved selection (`evidence_selection.rs:209-213`, key includes `RANKING = "lesson-sources-first"`, `:14`) restores a claim's page-1 passages across runs.

## 3. Evidence

- Fact-checkers that work use a few short passages per claim: FActScore retrieves 5 × 256-token passages (https://arxiv.org/abs/2305.14251); SAFE found 3 results per query best and more queries no help (https://arxiv.org/html/2403.18802v4); an atomic-fact sweep on HOVER peaks at 1–2 passages (https://arxiv.org/pdf/2506.07446).
- Accuracy follows an inverted U as passages are added, and the fall is driven by on-topic hard negatives, not off-topic noise (https://arxiv.org/abs/2410.05983). Length alone costs 13.9–85% across tasks (https://arxiv.org/html/2510.05381v1).
- Effective context is well below the advertised window: RULER (https://arxiv.org/abs/2404.06654); NoLiMa, where GPT-4o falls from 99.3% to 69.7% at 32K when the match is not lexical (https://arxiv.org/abs/2502.05167). Sufficient Context (Google, ICLR 2025): insufficient context raises confident wrong answers (Gemma 10.2% → 66.1%).
- Batch prompting holds at small sizes; degradation and position effects grow with the number of items in a call more than with its length, and weak models degrade first (https://arxiv.org/abs/2301.08721; BatchPrompt, ICLR 2024).
- Iterative, confidence-stopped checking (FIRE) reaches the same quality at about 7.6× lower cost than front-loading evidence (https://aclanthology.org/2025.findings-naacl.158).
- Support-, refute- and neutral-framed queries retrieve about 50% different evidence for the same claim (https://arxiv.org/html/2506.21745v1).
- Cut-offs from the score distribution itself: Adaptive-k takes the largest gap in the sorted similarity scores (EMNLP 2025, https://arxiv.org/abs/2506.08479); a tail-aware knee variant (https://arxiv.org/pdf/2606.11907).

No effective-context cap is set anywhere in this design: it would need per-model measurement, which the no-probing rule excludes; the configured window is the contract and the only hard bound.

## 4. The page rule

A claim's evidence is walked outward in rings; a page ends where the claim's own score distribution says relevance drops; the window is only a hard bound.

### 4.1 Rings

- Ring 0: the passages the claim's own section cites (`Citations::of`), in citation order. Always in page 1; what the window cannot hold leads the next page, as today.
- Ring 1: the sources the lesson cites anywhere (`Citations::cites`): each source's best-ranked passage that mentions the claim's subject and has not been shown.
- Ring 2: the course's other sources, likewise. Rings 1 and 2 always run to completion: every source with an on-subject passage shows its best one before web research (decision D2).
- Ring 3+: every source's next-best such passage, lesson-cited sources ahead at equal rank, until no source has one left. Runs under the group stop (§7).

Page 1 holds ring 0 + ring 1 (decision D1). A ring with no candidates is skipped, so a page is never empty while any ring has candidates. Passages that do not mention the subject never make a page; today they are the filler (`verification_page` with `|_| true`, `citations.rs:147`) and already the passages the walk-end rule treats as uninformative.

### 4.2 The cut-off

Carrying the scores. `Span` gains `branch: Branch { Library, Keyword }` and `score: f64`; `retrieve` keeps `hit.score` at `reference_collection.rs:329` and `keyword_ranking` keeps the BM25 score at `:558`; `merge` keeps the position score in `ReferencePassage.score` (what `judge::ranking()` and the report use) and adds `branch` and `branch_score`. `verification_context` carries the better hit's branch and score when it merges windows. `ReferencePassage` is `EvidencePassage`, so `Finding` gains two `#[serde(default)]` fields; `ClaimChecks::key`, the selection digest and the receipt exclude them as they exclude `score`. The library's score is a fused rank score: its gaps mark branch agreement (a chunk found by the dense and lexical branches scores about twice one found by one), which is a real signal but a coarse one; if the page-recall eval (§9) shows its gaps too coarse, the dense branch's similarity can be carried through `RankedBranch` in a follow-up. BM25 scores are real-valued and gap freely.

Computing the gap. Per branch, because the two lists' scales are incomparable and their sources disjoint; never on the position score. For a claim and page: candidates = its unshown, on-subject, widened passages of one branch, sorted by `branch_score` descending, s₁ ≥ … ≥ sₙ. Gaps gᵢ = sᵢ − sᵢ₊₁. The cut falls after the largest gap, and the gap is clear when it is larger than all the other gaps together: g_max > (s₁ − sₙ) − g_max, that is, one drop is more than half the list's whole descent. This is the Adaptive-k rule with a constant-free test for a knee, and it is tail-aware without a parameter: a long flat tail adds little to the descent, so a head drop still dominates it, and a drop deep in the tail dominates only when the head is flat, which is when it is the real knee. A list of one candidate is above the cut; a list of two has one gap, which is the whole descent, so only the first is above.

No clear gap. The scores descend smoothly: the retrieval cannot tell these candidates apart, so nothing about them ends the page. The ring rule alone applies (one passage per source, rank order) and the window is the bound. The per-round log records how often this happens; the eval reads it.

Where the cut applies. Ring 0 goes in regardless. In rings 1–3+, a source's best unshown passage joins the page only if it is above its branch's cut, computed fresh each page over what is still unshown. A source whose best is below the cut is not shown and waits; on the next page the tail's own head emerges from the recomputed gap, so every page makes progress and the walk ends at exhaustion. A ring may therefore take several pages; the ring is finished when its per-source candidates are exhausted, not when a page ends.

```
fn page(claim, unit, stage, shown) -> Vec<Passage>:
    page = ring0_not_yet_shown(unit)                         // always; the window alone bounds it
    cands = ranked hits for claim, widened, minus overlaps with shown ∪ page,
            keeping only references.mentions_subject_of(claim, text)
    above = ∪ over branch in {Library, Keyword}:
              list = cands of branch sorted by branch_score desc
              cut  = largest_gap(list); if clear(cut, list) { list[..cut] } else { list }
    best_per_source = first of `above` per source_id, in rank order
    ring1 = best_per_source where citations.cites(source)
    ring2 = best_per_source where !citations.cites(source)
    take = match stage { Ring1 => ring1, Ring2 => ring2, Deeper => ring1 ++ ring2 }
    if take.is_empty(): advance stage (Ring1 → Ring2 → Deeper) and retry; Deeper exhausted ⇒ walk ends
    if stage >= Ring2: take ++= counter_page(claim, shown ∪ page ∪ take)      // §6
    page ++= take
    canonicalize_evidence(page)                                // the keyed order
    return page
```

### 4.3 Reading order: cited passages at the edges

The page has two orders. The canonical order (kind, source, offset) is what `ClaimChecks::key` digests and what `ClaimReceipt.shown` positions index; it does not change. The reading order is a permutation the judge is given: `reading_order(page)` places the passages by need, using `judge::ranking()` (cited, researched, user-selected, execution, then retrieved by score), most needed at the start, next at the end, the rest inward alternately. One cited passage opens the page; two bracket it; retrieved passages fill the middle with the least needed at the centre, where lost-in-the-middle costs least. `target()` renders `passage-N` ids in reading order and `located_quote` resolves by text, so the judge's `Source passage` is order-independent; positions the shrink loop returns are mapped back through the permutation before `shown_of` and the receipt. In a packed request (§5), each claim's passages are a contiguous block in reading order; `render_batch_evidence` interns a passage shared by two claims of a section at its first appearance, which is the first block's edge. The key stays order-independent by construction: it is computed on the canonical page whichever order the judge read.

### 4.4 The window as the hard bound

There is no per-claim share of the window and nothing grows as claims settle. `claim_share` and `within_share` are removed. A page is what §4.1–4.2 produce; whether it fits a request is `batch_fits`/`located_fits` by the calibrated estimate, and a request the backend counts past the window comes back as `PromptExceedsWindow`, which part 1 answers by halving the request or dropping the claim's least-needed passages (`judge.rs:72-129`), binding the finding to the passages shown. A passage cut for size is unshown and leads the claim's next page (`evidence_checks.rs:694`). That is the whole fallback.

## 5. Packing claims into requests

`BATCH_SIZE` goes. Claims are packed into a request by their actual page sizes, and a request holds claims of one section only: a section's claims share ring 0, so the bank interns each cited passage once and the request is smaller than the sum of its pages. Pending claims are sorted by (unit, ordinal); within a section, claims are added while `ClaimChecker::batch_fits(&targets)` holds (the calibrated estimate with `ANSWER_ROOM_TOKENS` per claim); the first claim that does not fit starts the next request; a claim whose page does not fit a request on its own goes to `individual()` and part 1's shrink loop. Deepening rounds pack their walkers the same way. A claim with a large page usually ends up alone; a section with many small pages goes in one request. No request is ever built that the estimate says the window does not hold, and one the backend counts past it is halved on the same evidence (`evidence_checks.rs:355-380`).

Item count. The evidence (§3) says the number of items predicts failure more than length does, and no item cap is set here. The section is the bound: a section has single digits to low tens of claims, so a request has at most that many items, and a request's items shrink further as pages do. A batch answer the parser cannot use still sends its claims to `individual()` one by one, as today (`:398-402`). On a single-slot server a small request costs almost nothing extra: each claim's answer is decoded whether batched or alone, and the identical system prompt is served from the prefix cache, so section-sized requests are not a cost. If a live run shows a weak model degrading inside section-sized requests, that is a new decision for the owner with that run's numbers; nothing in this design needs it.

Progress and the split key. The stage text becomes "Comparing {n} claims of section {unit + 1} with their evidence" (`:349`); `begin_checks`, `plan_model_checks`, `checked` and the deeper-page counters are unchanged. The split checkpoint key stays `claim-batch-split-v1:{digest(receipt keys of the request)}` (`:332-340`): packing is deterministic (same pages, same sort, same fits), so a restart rebuilds the same request and finds the key; the halves carry their own keys.

## 6. Escalate, do not front-load

Deeper pages and web research run only on a grounded unsupported verdict, as today (`deepen` walks `Unsupported` results, `:566-570`; research follows the pass, `repair_loop.rs:124-170`). Page 1 asks whether the writer's own sources say it. A claim that settles there has a supported verdict on a passage the provenance gate then weighs; nothing more is read for it.

From ring 2 on, each page also carries a counter-evidence page, because a support-framed query and a refute-framed one retrieve about half different evidence (§3). It is not a model call. The claim's terms are split into subject terms (its distinctive content terms, as `mentions_subject_of` computes them) and value terms: tokens with digits, month names, capitalised words after the first, and negation markers, the classes `conversation/chat/verification/lexical.rs` already names (`NEGATION_MARKERS :182-199`, `MONTH_MARKERS`, `capitalised_words :395-405`, digits `:257`), moved to a shared helper. The counter query is the subject terms alone; with a negation marker, the claim's terms without it (passages asserting the positive). Its candidates are ranked, widened, on-subject and unshown as in §4.2, and ordered to lead with passages that carry a different value of the same class as the claim's (another number, date or name) or none of the claim's values, before passages that repeat them, which the support query already finds. The per-source rule and the branch cut-off apply to this list on its own scores. Passages carry `retrieval_kind = "counter"`, which `ranking()` and `canonicalize_evidence` treat as retrieved. A claim with no value terms has a counter query equal to its support query, so none is run. The strict prompt already demands that every supplied passage be inspected for exceptions and conflicts; it does not change.

## 7. How it interacts with the rest of the checker

- Deepening. `Walk` gains its stage (`Ring1 | Ring2 | Deeper`); a round builds each walker's page with §4 and §6. Rings 1 and 2 run to exhaustion for every walker. The group stop, "another round runs only while the last one settled a claim" (`:714`), applies to a claim only once it is in `Deeper`: a long document's fifth passage is where the cost is, and that is where the heuristic stays. A walk ends when ring 0 has nothing left and `Deeper` is exhausted. One `tracing::info!` per round records, per page, the stage, passages, exact tokens, whether each branch had a clear gap, and whether a counter page ran.
- Receipts and reuse. `ClaimChecks::key`, the receipt key and `ClaimReceipt.shown` are unchanged; a page under the new rule is a new comparison, so each claim is checked once more on first run. Deeper pages keep their own receipts (`:635-647`). `claim_reuse::legacy_receipt`, `identities` and `settled_finding` are untouched. `RANKING` is bumped once (decision D4), to `rings-by-gap-v1`, or `select` restores the old 13k pages for every unchanged source forever, which is its purpose (`evidence_selection.rs:1-5`). `POLICY` (`mod.rs:99`) stays: the judge's prompts do not change, so a receipt for an identical comparison stays valid.
- Research-led pages (`evidence_selection.rs:300-333`). The added sources' best on-subject passage each, above the cut, lead the recheck page; the earlier page's passages follow. `select` loses its `sharing` argument (`:206, :310`).
- Citations. Ring 0 keeps the section's own passages first on page 1 and leads later pages with overflow (`take_cited`, `deepen :595-610`). Lesson-cited sources are ring 1 and the tie-break in `Deeper`. `as_checked`/`Citations::label` unchanged.
- The judge and the gate. `judge_for_publication` and its shrink loop, `judge_batch_for_publication`, the strict prompts, `provenance_gate::weigh`, `support_gate` and the ledger are untouched. An unsupported claim still goes to research, repair and the user's "needs your review" queue (`repair_loop.rs:285`); nothing here publishes a claim the judge did not support.
- Part 1. `ranking()` already orders by kind then `score`; `reading_order` is built on it, and `kept_after_refusal` (`judge.rs:139-168`) keeps working on the reading order's positions, mapped back to canonical for `shown`. A counter passage carries a position score like any retrieved passage.
- Request cost. The system prompt is identical across requests and prefix-cached; the bank differs every request, so the 1% reuse is structural and smaller pages shrink the uncached part. Requests run one at a time (`:521, :681`). Decode per claim per page is unchanged and is the floor of a round's latency; the cut-off, not a passage count, decides how many passages a round reads.

## 8. Cost estimate for the logged run

Estimates from the log's numbers: ~1.3k tokens per passage, prefill ~850 tok/s (103–113k in 100–160 s), ~5 s decode per claim, a lesson citing ~4 sources, a section citing 1–2 passages, ~8 sections among the 32 model-checked claims. The cut-off's selectivity on a real course is the unknown; the eval measures it.

| phase | today (log) | new rule (estimate) |
|---|---|---|
| first checks, 32 claims | 4 requests × ~108k, ~12 min | ring 0 + ring 1 above cut ≈ 3–4 passages ≈ 4–5k per claim; ~8 section requests ≈ 15–25k each (shared cited passages interned); ~3 min prefill + ~3 min decode ≈ 6 min |
| page 2, 21 walkers | 3 requests, 116k / 112k / 77k, ~8 min | ring 2 above cut + counter page ≈ 3–5 passages ≈ 4–6.5k per claim; ~10 section requests ≈ 10–15k each; ~2.5 min + ~2 min ≈ 4.5 min |
| pages 3–5, 7 / 4 / 3 walkers | 119k / 127k / 132k refused | ≈ 3 passages ≈ 4k per claim; ~28k / 16k / 12k in total per round; ≈ 1 min each |
| total | ~1.12M prompt tokens, 34 min, failed | ~330k prompt tokens (~30%), ~13–14 min, largest request ~25k, no refusal |

Whether the same share of claims settles on page 1 is open: fewer passages, but all on-subject, from the writer's sources, with no on-topic hard negatives from elsewhere in the course (§3). If the cut-off finds no clear gap often, pages approach one passage per source and the estimate moves toward today's page sizes with the window as the bound; the per-round log makes that visible on the first run.

## 9. Test plan

Unit tests on the existing `PagingModel`, `related(i)` sources and `ReferenceCollection::lexical` (`evidence_paging_tests.rs`), plus pure-function tests where noted:

1. A page's size does not change with the window: the same claim and references at 32k and 128k windows produce the same page 1 (replaces `each_claim_in_a_batch_call_stays_within_its_share`, `:219-249`, whose "a shared call gives each claim less" is the behaviour being removed).
2. Scores are carried: a retrieved passage reports its branch and branch score; `ClaimChecks::key`, the selection digest and the receipt are identical for two passages differing only in those fields (`reference_collection/tests.rs`, `mod.rs` tests).
3. The cut-off (pure): a list with one dominant drop cuts after it; a list whose largest drop is less than the rest together has no clear gap and passes whole; one candidate passes; two pass the first only; a drop deep in a flat-headed list is the knee; the two branches are cut independently.
4. Page 1 is ring 0 + ring 1 above the cut: eight related sources, a lesson citing two, a section citing one passage; the first check shows that passage and the cited sources' best on-subject passages above the cut, and nothing from the other six.
5. No page holds an off-subject passage even with room to spare.
6. Rings run to exhaustion: a source below the cut on one page is shown on a later page; ring 2 is complete before `Deeper`; the walk ends when nothing on-subject is left (adapts `a_claim_backed_only_past_its_first_page_is_found_by_deepening`).
7. Reading order (pure and end to end): `reading_order` places the first cited passage first and the second last with retrieved passages inward by score; the judge receives that order; the key and `shown` positions are the same whatever the reading order; a page cut by the shrink loop binds the canonical positions of what the judge read (extends `a_page_loses_its_least_needed_passages_not_its_last`, `a_restored_receipt_names_only_the_passages_its_judge_was_shown`).
8. Packing: claims of two sections never share a request; a request's claims fit by `batch_fits`; a claim too large alone is judged by the shrink loop; packing is identical across two runs so the split key matches; the stage text names the section and count; a request refused by exact count is halved (extends `batch_scheduler_tests.rs`).
9. Counter-evidence (pure and end to end): the query drops the claim's numbers, months, later capitalised words and negations and keeps its subject terms; a claim without value terms runs none; a fixture whose contradicting passage carries a different number for the same subject is retrieved on page 2 and the claim ends contradicted; counter passages are never on page 1.
10. The group stop applies only in `Deeper`: a claim whose support is in an uncited source's best passage settles on ring 2 after a round that settled nothing for anyone (replaces `deepening_stops_after_a_round_that_settles_nothing`).
11. Receipts: a second pass with unchanged sources makes no model call; a changed `RANKING` rebuilds the selection once (extends `incremental_tests.rs`). Research: added sources' passages above the cut lead the recheck page (adapts `research_found_for_a_claim_leads_its_full_first_page_and_is_rechecked`).

Eval. The judge does not change, so `live_claim_pairs_eval.rs` on `evals/grounding/lesson-claims-w1/gold-w1.jsonl` and `evals/grounding/claim-pairs-v1.jsonl` should not move, and runs once to prove it. What can move is whether the supporting passage reaches a page, and when. A page-recall eval, `live_page_recall_eval.rs` beside `live_claim_pairs_eval.rs`, reads a stored run (`learning_lesson_verifications.report_json` and `learning_sources`, the tables `scripts/lesson_claim_pairs.py extract` reads), rebuilds `ReferenceCollection::lexical` from the lesson's sources, and reports for every `approved` finding the page on which the judge-cited passage appears under the old rule and the new, the tokens read up to it, the branch gap at each page (clear or not), and the count of approved findings whose cited passage fails `mentions_subject_of` (decision D5 waits on this number). Results go in `evals/grounding/results/` with a README section. Then one live run of the owner's course: compare the report's verdict tally, the "needs your review" count and the per-round log with the 2026-10-19 run; every newly supported claim cites a passage on its page (the receipt invariant); every newly unsupported claim is checked against the page-recall report. `live_small_lesson.rs` with `live_fixtures/verification_controls.json` stays the end-to-end control.

## 10. Decisions

Taken by the owner, 2026-10-19:

- D1 = page 1 holds ring 0 + ring 1.
- D2 = rings 1 and 2 always run to completion; ring 3+ under the group stop.
- D3 = the relevance cut-off, edge ordering, window as hard bound, packing by size per section with no item cap, escalation with counter-evidence retrieval, no effective-context cap (§4–6). The earlier per-claim ceiling and `BATCH_SIZE` discussion are withdrawn.
- D4 = bump `RANKING` once.
- D5 = whether a claim whose support shares none of its distinctive terms gets a final page of top library (semantic) hits: decided after the page-recall eval reports how often a judge-cited passage fails `mentions_subject_of`.

No new decision is needed. The one place a decision could arise is a weak model degrading inside a section-sized request (§5); that waits on a live run's numbers.

## 11. Implementation outline

- `lessons/reference_collection.rs`: `Span` and `ReferencePassage` carry `branch` and `branch_score` (`:52, :31-39`); `retrieve` keeps the hit score (`:329`), `keyword_ranking` the BM25 score (`:558`), `merge` and `verification_context` carry both; `next_by_source(query, shown, within)` returns ranked, widened, unshown, on-subject candidates with their branch scores (depth doubled until the ranking is exhausted or every source has a candidate); `gap_cut(list) -> Option<usize>` (pure); `counter_query(claim)` and the value-term helpers shared with `conversation/chat/verification/lexical.rs`. `verification_page*` stay for `retrieve_from`.
- `content_verification/citations.rs`: `lesson_sources_first` replaced by `ring_page(llm, references, claim, stage, shown, citations)` per §4.2 and `counter_page` per §6; `take_cited`, `overlaps` unchanged.
- `content_verification/mod.rs`: `first_page` builds ring 0 + ring 1; `claim_share` and `within_share` removed (`:422-462`); `reading_order(page) -> Vec<usize>` beside `canonicalize_evidence`; `Finding`'s passages gain the two serde-defaulted fields.
- `content_verification/judge.rs`: `judge_for_publication` takes the reading order and maps returned positions back to canonical; `ranking()` unchanged.
- `content_verification/evidence_checks.rs`: `BATCH_SIZE` and `sharing` removed (`:20, :463-465, :483, :519, :589, :679`); `pack(pending) -> Vec<Vec<PendingClaim>>` by section and `batch_fits`; `target()` renders in reading order; `Walk { stage }`; `deepen` builds pages through `ring_page` + `counter_page`, advances stages, applies the group stop in `Deeper` only, logs per round; stage texts per §5.
- `content_verification/evidence_selection.rs`: `RANKING = "rings-by-gap-v1"`; `select` without `sharing`; researched page via `next_by_source` restricted to added sources.
- `crates/lattice-core/.../claim_verification/batch.rs`: `render_batch_evidence` unchanged (it preserves the given order and interns shared texts); `batch_fits` is the packer's test.
- Tests per §9 in `evidence_paging_tests.rs`, `batch_scheduler_tests.rs`, `incremental_tests.rs`, `reference_collection/tests.rs`; `lessons/live_page_recall_eval.rs` (new, `#[ignore]`), `evals/grounding/README.md` and `evals/grounding/results/`.
- Not changed: the judge prompts and `POLICY`, `ClaimChecks::key`, the receipt format, `provenance_gate`, `support_gate`, the ledger, research and repair, `lesson_progress` counters, part 1's refusal handling.

## 12. Other places that size a prompt to the window

| where | what it does | verdict |
|---|---|---|
| `lessons/generation/lesson.rs:180` `evidence_room`; `generation/evidence.rs:164-172` `depth = room / (searches × passage tokens)`; `:622-626` `pack` fills the room round-robin | the lesson writer's evidence is sized to fill what the window leaves | same problem; worth a follow-up (the writer needs breadth, so its rule will differ) |
| `lessons/generation/recall.rs:273` | prior-lesson recall takes passages "as many as the window holds" | same problem; worth a follow-up |
| `lessons/review_evidence.rs:180, :194` | teaching review: each issue gets an equal share of `source_room`, filled by `retrieve_within` | same problem; worth a follow-up |
| `content_verification/repair.rs:180-250` | section repair groups failed claims to fit the room and sends the evidence their check used | fine as is (packs what exists; shrinks with this design) |
| `planning/outline_evidence.rs:75` | outline evidence budget is `window / 4` | same problem (a fraction of the window); worth a follow-up |
| `lessons/reference_collection.rs:711` `catalog` | structural listing trimmed to `window / 16` | fine as is (a listing, not evidence) |
| `conversation/chat/verification/judge.rs:117, :285-323` | grounding judge reads the cited source text, split into parts that fit the room | fine as is (reads what the answer cited; the window only pages it) |
| `conversation/chat/turn/budget.rs:93-135`, `turn/retrieve.rs:30` | chat evidence budget is the room after fixed parts; retrieval fills it | same shape; worth a follow-up (check whether the retriever's cutoff ends before the budget) |
| `conversation/chat/memory_context.rs:183-192` | recall pool from `BudgetAllocation`; recall reads up to the pool | same shape, not verified here; worth a look |
| `conversation/memory/mod.rs:125`, `memory/summarize.rs:241` | utility calls size material to the window; the material is the conversation itself | fine as is |
| `conversation/synthesis.rs:77` | research synthesis parts are half the input budget | fine as is (parts the material; does not retrieve to fill) |
| `conversation/chat/retrieval/corpus_plan/prompt.rs:52-62` | planner catalog fills 80% of the window room | same problem; worth a follow-up |
| `corpus_shape/labeling.rs:27` | document previews share the room equally, "a larger window sees more of each document" | same problem, low stakes; worth a follow-up |
| `compare/use_case.rs:40` | table cells from `window / 4` | fine as is |
| `crates/lattice-core/.../grounded_generation/mod.rs:299-325` | the room planner itself | fine as is (a mechanism; callers decide how much to put in it) |
