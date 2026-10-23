# Retrieval design: the four "not in your documents" misses (9B, desktop-path-v1, 2026-10-23)

Worktree `/Users/josh/Code/lattice-retrieval-pr` (eval/retrieval-rework-local = 0026ce4e). Read-only; no code, cargo or evals run.
Paths are relative to `crates/lattice-desktop/src/features/conversation/chat/retrieval/` unless stated.

## 0. What actually happened (verified)

All four needles were lost at one point, upstream of fusion, rerank, dedup and the budget. In all four turns the 9B planner
returned `"start_at_beginning": true` (calls seq 64, 72, 86, 129 in `2026-10-23-….calls.jsonl.gz`), and the pipeline treats the
flag as an **evidence filter**:

| Step | File:line | Effect when the flag is true |
|---|---|---|
| Searches confined to the chosen documents | `corpus_plan/library_search.rs:62-66` | a document the planner did not pick is unreachable (clock) |
| Scoped, title-stripped search skipped | `library_search.rs:92` | the one search that asks the question *inside* the named document never runs |
| Openings read `limit` chunks per chosen document, no cap | `library_search.rs:151-159` (`retrieval_openings(…, limit)`, `append_openings(…, usize::MAX)`) | 1–4 documents × `limit` chunks enter the pool first |
| Fused results appended behind them | `library_search.rs:181` | behind ≥ `limit` openings |
| Pool cut to `kb_search_limit` | `kb_retrieval.rs:416` | every search result discarded; the prompt is openings only |
| Rerank off, pool not widened | `kb_retrieval.rs:243-247, 276, 361` | nothing reorders the openings |
| Sufficient "by construction" | `sufficiency.rs:133-141` (`ORDERED_READING`) | no corrective pass, although `plan_correction` forces the flag false (`planner.rs:126-130`, `corpus_plan.rs:35`) and would have recovered all four |

**This is a regression of the rework, not a pre-existing weakness.** At f6ff104a (the 2026-10-22 run in `../lattice-course-eval`)
`planner.rs:233-236` zeroed `start_at_beginning` unless `whole_document_intent` matched an English phrase. Design item #7 dropped that
guard ("trust the planner flag; no replacement guard"), so the 9B's flags went live. Same questions, same model, same corpus:

| Item | 10-20 (bfcfd6a8, no guard) | 10-22 (f6ff104a, guard) | 10-23 (0026ce4e, guard dropped) |
|---|---|---|---|
| dp-buried-agreement-meeting | flag true → miss | flag true, zeroed → **pass**, section 24 was passage [1] of 24 | flag true → miss |
| dp-buried-handbook-ticket | flag true → miss | flag false → **pass**, SIG-26321 in prompt | flag true → miss |
| dp-paraphrase-clock | flag false → **pass** | flag true, zeroed → **pass**, runbook-core-04 in prompt | flag true → miss |
| dp-transcript-rollback | flag false, old openings-per-doc → miss | flag true, zeroed → needle **in prompt**, model refused | flag true → miss |

The 10-22 agreement prompt is the direct answer to "is the BM25 hit fused away?": no. With the focused path allowed to run, the
corpus-wide searches, the scoped search, RRF and the cross-encoder put section 24 first. The 10-23 planner queries both contained
"meeting"; BM25 was never consulted for the pool. (There is no 2026-10-21 desktop-path run anywhere under `/Users/josh/Code`; the
pre-rework baseline is 10-22.)

Term facts (from `synthetic-library-v3.json`): "meeting" and "proceedings" occur once in agreement-long-0 and in 3/379 documents;
"court" and "soon" nowhere; "parties", "dispute", "escalation" 56× each in the agreement (17, 17 and 84 documents). The needle is at
offset 42,861 of 87,023 (49%, section 24 of 49), not appended at the end. "ticket" and "reference" occur once each in the handbook;
"rollback" once in the transcript. Within-document rarity already singles each needle out for plain BM25; redundancy (676/951
repeated sentences) only flattens the vector branch, and paraphrase was handled by the planner's rewrite on 10-20 and 10-22.

Tool-call premise (coordinator's addition 1): confirmed — 55 answer rounds per run, tools offered in all 55, **0** tool calls on both
10-20 and 10-23. Any fallback must be app-driven.

Six passing turns also had the flag true (conflict-ring-threshold, mt-ring-thresholds t1, mt-shift-and-return ×3, mt-topic-shift ×2,
mt-combine-versions t1, mt-combine-renewals t2): short documents that reading from the start covers whole. The one genuine opening
item (dp-opening-agreement) got the flag **false** and passed through the focused path. The 9B's flag is noise.

## 1. Per failure: where the needle was lost

| Item | Planner 10-23 | Prompt held | Needle | Loss point |
|---|---|---|---|---|
| dp-buried-agreement-meeting | flag true, openings `[d133]`, 2 queries | 15 passages = agreement-long-0 sections 1–14 in order | section 24 | never in the pool: `library_search.rs:154-159` read chunks 0–14; `kb_retrieval.rs:416` cut everything behind them |
| dp-buried-handbook-ticket | flag true, 3 openings (handbook-prose-0, handbook-tree-0, handbook-prose-1) | 17 passages, round-robin, 6 chunks each from the start | 86% | same; scoped search skipped (`:92`) |
| dp-paraphrase-clock | flag true, 4 openings (3 reading notes + runbook-edge-12) | 9 passages from those four | runbook-core-04, not chosen | never searched: `:62-66` confined all three queries to the chosen four |
| dp-transcript-rollback | flag true, 3 openings | 14 passages, 09:00–09:31 of three transcripts | `[12:36:00]`, 86% | same crowding. Secondary: the question says "roll back", the needle "rollback"; `scoped_query` (`:297-317`) uses the question only, the planner's query 2 carries "rollback" (approximate BM25: 8th vs 1st in-document) |

## 2. Design, ranked by gain per cost

### A. The language-neutral replacement for the dropped guard: ordered reading is an evidence source, never a filter (fixes 4/4, no new model call)

A1. `library_search.rs:62-66` — always search `scope.document_ids`. The corpus-wide searches are the only route to a document the
planner did not choose. Cost on flag turns: none (the same searches run today and are truncated away).

A2. `library_search.rs:92` — run the scoped search whenever `reading_ids` is non-empty, flag or not. `scoped_query` takes its terms
from the question **and** `plan.queries` (union, order kept, title tokens removed, `select_informative_terms`, `MAX_QUERY_WORDS`);
fall back to `queries[0]` only when nothing remains. The planner's rewrite carries the paraphrase the user's words lack. Still one search.

A3. `library_search.rs:122-160` — one pool layout for both branches, with the even split the rework already chose:
`share = limit.div_ceil(2)`. Planner half: section lookups, then openings — flag false: 1 chunk per chosen document (unchanged);
flag true: `retrieval_openings(opening_ids, scope, share.div_ceil(n_chosen))`, appended round-robin with
`max_to_append = share - results.len()` instead of `usize::MAX` — then `scoped` (ByRelevance) fills what is left of the share.
Whole-space half: the corpus-wide fusion (AcrossDocuments) as at `:181`. Reading order keeps the front of the list (the budget fills in
list order), so a genuine reading request still reads first; it no longer owns every slot.

A4. `kb_retrieval.rs:243-247, 276, 361` — rerank whenever enabled; `candidate_limit = candidate_pool(kb_search_limit)` always.
`finalize_rerank` already pins the *leading* structural results, at most `share` after A3. Cost on flag turns: one rerank pass over
`min(3·limit, 64)` candidates — what every focused turn pays already.

A5. `sufficiency.rs:133-141` — `ORDERED_READING` becomes a reason code, not a verdict; a flag-true plan is judged by the same term
coverage over `evidence_depth`. `plan_correction` already forces the flag false, so the corrective pass is the model-driven recovery
for a misread request. Safety net; A1–A4 put the four needles in the pool on the first pass.

### B. Planner wording (the model's judgement; zero cost; supportive)

`corpus_plan.rs:90`, one sentence after the flag's definition: "A request that names a document but asks what it says about something
is a focused question: leave start_at_beginning false and put that document in opening_document_ids." Subject- and language-neutral.
A makes a wrong flag harmless; B makes it rarer and keeps full reading depth for genuine reading turns.

### E. App-driven fallback for "right document, wrong part" (coordinator's addition 1; second tier, not needed for these four)

Trigger (no model involved): the planner named documents and, after rerank, the scoped term set (A2's terms) is not covered by the
pool's passages from those documents — the same `term_matches_set` prefix rule `sufficiency.rs` uses. Mechanism: a new
`ChatRecords::retrieval_document_chunks(&reading_ids, allowed)` returns every chunk of the chosen documents (≤ 4 documents; one SQL read,
text already in `text_chunks`); score each chunk against the uncovered terms with **within-document** IDF
(`ln(1 + (N_doc − df_doc + 0.5)/(df_doc + 0.5))`, `df_doc` = chunks of that document containing the term under the prefix rule), then
select greedily: take the best chunk, drop the terms it covers, rescore, stop when no chunk covers a remaining term or the planner
half is full; insert the picks at the end of the planner half so they still pass the reranker. No embedding, no model call; cost is
proportional to the user's own chosen documents and paid only on a coverage gap.

How it relates to redundancy-aware selection: a novelty/MMR rule works on candidates retrieval already surfaced and cannot tell the
needle chunk (one new sentence plus boilerplate) from its neighbours (boilerplate plus one numbered clause) — both are ~90% repeats.
Within-document rarity weights *terms*, so boilerplate terms weigh ≈ 0 wherever they occur and the needle's rare terms dominate; the
greedy uncovered-term step is the redundancy-aware part — a second boilerplate chunk covers no remaining term and earns no slot. So
one mechanism covers both, inside the planner half. It is also the deterministic stand-in for the read tool the 9B never calls.
Honest positioning: on this corpus plain BM25 already ranks each needle first or second in its document, and 10-22 shows the focused
path surfaces them; E matters for documents whose boilerplate is rare in the library but dense in the document (global IDF cannot
tell), and for a flag-false turn where the planner's wording misses the needle's terms entirely.

### Per-document cap (coordinator's addition 2)

`PER_DOCUMENT_CAP = 4` (`search/use_cases/hybrid_search.rs:86`) played no part on 10-23: the fifteen boilerplate passages are
`document_opening` results from `retrieval_openings(…, limit)`; no fusion ran in the flag path and the scoped `ByRelevance` search
never executed (`:92`). On 10-22 the cap was in force and the agreement item passed. Neither the cap nor its bypass is implicated.
In the planner half the bypass is right: the chosen document's passages compete on relevance and the reranker orders them. What
should replace the fixed 4 in the whole-space fusion (follow-up, lattice-core, measured on v3 before merging): a further passage from a
document that already holds slots earns one only when it covers a query term that document's selected passages do not; otherwise it
is deferred, as the cap defers today. No constant; a slot is earned by new information.

### C. Redundancy, what is left once A lands (optional, speculative)

C1. Branch weight from branch spread in `fuse`: weight each branch by the normalised spread of its top-`limit` scores instead of the
fixed `DEFAULT_*_FUSION_WEIGHT`; a vector branch over 48 near-identical chunks is flat and should count less (the test `sufficiency.rs`
already applies to a flat reranker). Parameter-free. Touches lattice-core → v3 recall must be measured; separate PR.
C2. Do not change chunking: chunks are heading spans ≤ 800 tokens (`indexing/commands.rs:66`), which already isolates sections; a
chunker change needs an index-generation bump and a full re-embed of every library on next start (nothing re-indexes today;
generation is keyed on the embedding model). Not justified by the evidence.

### D. Paraphrase: nothing new

The planner's rewrites plus the vector branch found the clock needle on 10-20 and 10-22; the corrective instruction already asks for
"the specific terms the request implies but does not state". A2 is the only paraphrase-adjacent change. HyDE or extra multi-query
expansion would add a model call per turn (~40 s of planner time on this 9B with its 57k-char catalog); not recommended.

## 3. Expected effect

| | After A (+B) |
|---|---|
| agreement, handbook, clock | **pass** — the 10-22 run is the demonstration: same questions through the focused path, needle in the prompt and answered |
| transcript | retrieval fixed (needle in the pool via A2's "rollback"); the 10-22 run had the needle in the prompt and the model still refused, so the answer side may still fail — out of scope here |
| 32 passing items | the 26 flag-false first turns are untouched. The six flag-true passes keep their openings in the front half and gain the scoped search + rerank; low risk, watch conflict-ring-threshold (two 8-chunk runbooks) |
| v3 recall@5 (0.778) | unchanged: A/B/E touch only `chat/retrieval`; the retrieval_eval example does not run the planner path. Only C1 and the cap replacement move it, which is why both are separate and measured |

Cost per turn: flag-false turns unchanged (Q + 1 scoped + 1 rerank + 1 planner call). Flag-true turns: Q corpus-wide searches
(were confined), +1 scoped search, +1 rerank pass; no new model call. E: one SQL read + Rust scoring, only on a coverage gap.
A5: one planner call only when a reading plan is uncovered.

## 4. Tests (each with a non-English and a non-legal case)

1. `library_search.rs` `retrieve` with a fake `HybridSearchTrait` (model on `retrieval/tests/web_fallback.rs:93 FixedLibrary`) and a
   recording `ChatRecords` fake: flag true, one chosen 20-chunk document, limit 10, a fused hit at chunk 17 → ≤ 5 openings
   (chunks 0–4) and chunk 17 present; `search_branches` called with the whole scope. Fixtures: German ("Kapitel 1…20", "Welche Frist
   gilt für die Kündigung?") and Spanish biology ("¿Cuánto dura la fase S del ciclo celular?").
2. `scoped_query` union: "…what do we roll back to…" + planner "…rollback strategy previous build" + title → contains "rollback", no
   title token; "Im Protokoll der Quartalsbesprechung: worauf setzen wir zurück?" + "Rollback-Ziel Version" → contains "rollback";
   "In the Mozart Requiem programme notes, what key is the Lacrimosa in?" + "Lacrimosa key signature D minor" → "lacrimosa",
   "signature", "minor", not "requiem"/"notes". Keep the existing tests at `:474, :507`.
3. `append_openings` cap: 3 docs × 10 chunks, `max_to_append = 5` → exactly d1c0, d2c0, d3c0, d1c1, d2c1.
4. `finalize_rerank`: 3 leading `document_opening` + 5 search results in reversed order → openings first, then reranked order (Japanese titles, a chemistry section).
5. `kb_retrieval` gate: reranker available, flag true → rerank stage invoked (recording fake); pool = `candidate_pool(limit)`.
6. `sufficiency`: flag true, uncovered → `sufficient == false` with `LOW_TERM_COVERAGE`; flag true, covered → sufficient, `ORDERED_READING` recorded. Queries "división celular mitosis", "Lacrimosa Tonart".
7. E (if built): a 12-chunk document where the term "Fristverlängerung" occurs in chunk 9 only and "Vertrag" in all 12 → chunk 9 selected first, no second chunk selected once "Fristverlängerung" is covered; a biology document ("telomerasa" once, "célula" everywhere) likewise.
8. `PLANNER_SYSTEM`: the new sentence present; JSON schema (`planner.rs:195-196`) unchanged.

## 5. Build split (≤ 3 Opus builders, disjoint files, `cargo check` only)

- **Builder 1 — `corpus_plan/library_search.rs`**: A1, A2, A3; tests 1–3. `fuse_passes`, `expand_evidence`, `place_after_anchor` untouched.
- **Builder 2 — `kb_retrieval.rs`, `sufficiency.rs`**: A4, A5; tests 5, 6. If E is in scope: the trigger + greedy selection as a new
  `corpus_plan/document_local.rs` called from `kb_retrieval.rs` after rerank, and the `retrieval_document_chunks` port method in
  `chat/ports.rs` + `conversation/repository/chat_port.rs` (Builder 2 owns those two files too); test 7.
- **Builder 3 — `corpus_plan.rs`, `rerank.rs` (test 4 only), `docs/design/`**: B + test 8; a design note recording §0–§3 and the three-run comparison (Josh wants run history in repo docs).

Coordinator: gates once after all three, then the 9B chat eval; report against 32/38 and the 10-22 baseline. v3 recall needs no rerun.
Recommendation: ship A+B first (E optional in the same PR only if Builder 2 has room); C1 and the cap replacement as a measured follow-up.
Other follow-ups, not this PR: the pre-existing `WHOLE_DOCUMENT_PHRASES`/`ORDERED_LEARNING_PHRASES` English lists
(`corpus_plan.rs`); `expand_evidence(kb_search_limit + 8)` at `kb_retrieval.rs:421`; the planner's full 379-row catalog (57k chars,
~40 s per planner call on the 9B — the largest latency item in the turn and what makes a corrective pass expensive).
