# Rework design for 8f01c13e "Improve retrieval bridging and memory robustness"

Worktree `/Users/josh/Code/lattice-retrieval-pr`. Read-only review; no code.

Principle applied throughout: the planner (a model) does every "hop" and every intent call; the app keeps only
language-neutral structure (chunk 0, breadcrumbs, line starts, token identity with a document's own title,
IDF/entropy-ranked terms, positions, scores) and derives every size from the window or the candidate pool.

## 1. Verdict per change

| # | File (HEAD line) | Change | Verdict | Why / how |
|---|---|---|---|---|
| 1 | `indexing/use_cases/embedding_input.rs:160-203` | `has_body`: a heading line with no body does not close a span | **KEEP** | Pure structure, any language. Note: no forced re-index, so existing libraries keep heading-only chunk 0 until a document is re-indexed; the gain is real only for newly indexed data. Say so in the PR. |
| 2 | `repository/workspace/retrieval.rs:203-208` | breadcrumb match also accepts `"7.4."` (trailing dot) | **KEEP** | Numbered headings end in a dot in much European/German numbering. Structural. |
| 3 | `repository/workspace/retrieval.rs:209-215` (review Med) | any dotted identifier also scans `c.content` for `"\n7.4 "` **or `" 7.4 "`** | **REWORK** | Keep only the line-start form (`'\n' || c.content` with `"\n{id} "` and `"\n{id}. "`). A label at line start is a numbered paragraph; mid-line `pH 7.4`, `Python 3.12`, `v5.2` are prose. Order inside the identifier's share: breadcrumb hits first, body hits after (`ORDER BY CASE WHEN <breadcrumb match> THEN 0 ELSE 1 END, chunk_index`). Cost class unchanged: the pre-existing breadcrumb `INSTR` is already an unindexed scan over the space. |
| 4 | `repository/workspace/retrieval.rs:262-281` | neighbours include the anchor's child sections; return the neighbour's real breadcrumb | **KEEP** | Structural; fixes wrong `section` labels on neighbours. |
| 5 | `corpus_plan.rs:86` | PLANNER_SYSTEM: "when a catalog row names the party/system/document the request refers to indirectly, put that name in queries" | **KEEP (reword)** | This is the language-agnostic bridge: the model reads the catalog and does the hop. Reword to subject-neutral: "…names the thing the request refers to indirectly (a person, organisation, system, place, work, document…), include that name in a query." |
| 6 | `corpus_plan.rs:300-320` (review High #2) | `wants_document_openings` English phrase list | **DROP** | Openings are read whenever the planner returned `opening_document_ids` (its prompt already says "leave empty unless opening text is relevant"). No wording test. Pre-existing `WHOLE_DOCUMENT_PHRASES` (used at `corpus_plan.rs:338`, `kb_retrieval.rs:745`, `shortlist.rs:446`) is the same debt from before this commit; record as follow-up, out of scope here. |
| 7 | `corpus_plan/planner.rs:229-236` (review High #1) | `start_at_beginning` zeroed unless `whole_document_intent` matches | **DROP** | Trust the planner flag; no replacement guard. |
| 8 | `library_search.rs:52` | catalog fetched at the top of `retrieve` | **KEEP** | Same single query as before, just earlier; needed for titles. |
| 9 | `library_search.rs:85-156` | title-stripped scoped search: overlap≥2 gate, 1+N `search_branches`, `fuse(…, 2, AcrossDocuments)` | **REWORK** | Trigger = planner chose `opening_document_ids` and `!start_at_beginning` (the planner's choice is the signal; no overlap threshold). Query = ONE string: informative terms of `question` (via `select_informative_terms`, entropy-ranked, language-neutral) minus tokens that occur in the chosen documents' own titles (token identity with the document's name is not an English rule); if nothing remains, `plan.queries[0]` unchanged. ONE `search_branches` over the chosen docs with `limit`, fused `Spread::ByRelevance` (the per-document cap is the dilution being fixed), merged after structural results up to `limit.div_ceil(2)` — the named documents and the corpus-wide search split the pool evenly; neither has a prior claim. Dedupe by id. |
| 10 | `library_search.rs:158-205` (review High #2) | openings only when `use_openings`; `per_doc = 2` | **REWORK** | Outside `start_at_beginning`: read **1 chunk per planner-chosen document** (chunk 0 = the document's opening by definition; depth beyond that is reading, which is the `start_at_beginning` branch and already uses `limit`). Pinned as `document_opening`. The `2` goes. |
| 11 | `library_search.rs:222-298` (review High #3/#4/#5, Med tokenizer) | entity bridge in `retrieve`: open target's 2 openings, tag, extra `search_branches(…, 8)` fused to 6 | **DROP** | Replaced by #5 (first pass) and #19 (retry). Removes `tag_entity_bridge`, all `entity_bridge` metadata, the 8/6 constants. |
| 12 | `library_search.rs:347-369` | `append_openings` helper | **KEEP** | Refactor of the pre-existing round-robin. |
| 13 | `library_search.rs:419-446` (review Low) | neighbour expansion: 4 neighbours of the first 4 anchors spliced at index 6, second loop restarts at anchor 4 | **REWORK** | Walk anchors in rank order; insert each anchor's unseen neighbours **directly after that anchor's current position** (reading-order adjacency: the neighbour belongs with its anchor, and the in-order budget fill at `budget.rs:407` then keeps anchor+neighbour together). Stop when `results.len() >= limit`. No 4, no 6, no gap. `anchors.take(8)` is pre-existing; leave. |
| 14 | `shortlist.rs:43-85` | `candidate_documents` takes `catalog`, runs a bridge search | **DROP** | Back to the pre-commit signature and 1–2 searches. |
| 15 | `shortlist.rs:88-346` + tests `480-730` | `EntityBridge`, `first_body_paragraph`, `referenced_entity_phrases`, `find_entity_bridge` | **DROP** | Capitalisation-based names (fails German nouns, caseless scripts), English sentence-leader list, two tokenizers ("World War II"/"T-2194" never match), overlap guard counting 3-letter function words. Not fixable into a neutral rule; the planner owns the hop. |
| 16 | `kb_retrieval.rs:276, 358` | rerank gate `opening_document_ids.is_empty()` → `!plan.start_at_beginning` | **KEEP** | Focused questions with planner-chosen openings must still be reranked; only ordered reading is exempt. Consistent with #10. |
| 17 | `kb_retrieval.rs:291-293, 375-377, 771-834` (review High #5) | `promote_entity_bridge` + its test | **DROP** | Demotes reranked target passages past the evidence budget; test asserts that ordering. Goes with #11/#15. |
| 18 | `kb_retrieval.rs:558, 584, 596, 634` | `catalog` threaded into `shortlist_candidates`/`corrective_pass` | **DROP** | Only served the bridge. |
| 19 | `kb_retrieval.rs:606-662`, `planner.rs:82-130` | (new, replaces the bridge on retry) | **ADD (small)** | `plan_correction` already receives `queries_already_tried` and `why_insufficient`. Add the first pass's top passages as bounded context: for each of the first `evidence_depth` results, `document name` + the passage's first paragraph, clipped to the planner's existing prompt budget. One prompt sentence: "If a passage names the thing the request refers to indirectly, search that name." Model-driven second hop, no new search unless the turn was already insufficient. Optional if the budget plumbing proves awkward; the design stands without it. |
| 20 | `rerank.rs:29-85` + tests | `finalize_rerank`: unapplied → original response; applied → leading `section_lookup`/`document_opening` restored to the front, scores kept | **KEEP** | Not flagged; principled (structure pinned, scores honest). With #11 gone, nothing else is pinned. |
| 21 | `sufficiency.rs:37-40` | `TERM_COVERAGE_TOP_N = 10` | **REWORK** | Replace with `evidence_depth: usize` passed from `kb_retrieval.rs:149` (`kb_search_limit`, derived from the window). Coverage over `results.iter().take(evidence_depth)` = exactly "the passages that will reach the answer". |
| 22 | `sufficiency.rs:118-134, 179-192` | `score_summary` sorts scores; ordered-reading bypass only for `start_at_beginning` | **KEEP** | Pinned order must not hide the best score; bypass matches #16. |
| 23 | `sufficiency.rs:203-234` (overfitting bullet: ASCII plural/verb stems) | `terms_are_simple_inflections` (s/es/ed/ing/y→ies) | **REWORK** | Rule: two terms match when equal, or when one is a prefix of the other and the shorter is ≥ 4 chars (the agent's bound; keep rather than add a number). Works for any script's suffixing (`monitor/monitoring`, `vertrag/vertrages`, `célula/células`), rejects `contract/contrary` (neither is a prefix). Known non-match: `policy/policies`; accepted, it can only cost an extra corrective pass, never a wrong answer. |
| 24 | `sufficiency.rs:236-262` | per-query best coverage; section metadata terms counted | **KEEP** | Language-neutral; three queries should not all need the same passages. |
| 25 | `prompting.rs:236, 362-373` (review High #4; "same general type or template") | `EVIDENCE_SYNTHESIS_NOTE` | **REWORK** | Keep: answer intent not literal matches; join facts across passages and cite every link; broader user wording → closest supported statement with its narrower scope stated; different terminology is not missing evidence. **Delete** the sentence "When a passage explicitly links another document, do not treat unlinked documents of the same general type as alternative answers." |
| 26 | `prompting.rs:514-573` (review High #4) | `kb_passage_with_relationship`: "Passage [n] establishes that this is the linked document… not alternatives" | **DROP** | States a guess as fact. Revert `kb_passages`/`kb_passage` to the pre-commit form. Nothing is left to annotate once #11 is gone. |
| 27 | `prompt_settings.rs:46-52` ("provision") | system-prompt paragraph | **REWORK (wording)** | "closest directly applicable provision" → "closest directly applicable statement in the sources"; keep "state its exact mechanism, conditions and limits", "do not call a documented mechanism unavailable because its label differs", and the chain-citation sentence. Update the two assertions in `chat/tests.rs:716-717`. |
| 28 | `citation_numbering_tests.rs:149-175` | bridge relationship test | **DROP** | Goes with #26. |
| 29 | `corpus_plan/tests.rs:36-38, 55-86` | fixture rows + dotted-body/child-neighbour test | **REWORK** | Keep the child-neighbour half; rewrite the dotted half for the line-start rule (see §4). |
| 30 | `memory/prompts.rs:93` | SUMMARIZER rule: assistant-invented limits are proposals, user adoption preserved | **KEEP** | Sound; subject-neutral. Keep the `SUMMARIZER_PROMPT_VERSION` bump and the docs line. |
| 31 | `memory/prompts.rs:467` (review Low) | `to_string_pretty` payloads | **DROP** | Revert to compact. Then revert the unexplained `EXTRACTOR`/`VERIFIER`/`VALIDATOR` version bumps (their text is unchanged; pre-release, no shims). |
| 32 | `memory/tests.rs:831-845, 1014-1015` | FakeLlm parses JSON for the task; new rule assertions | **KEEP** | Works with compact or pretty JSON. |
| 33 | `scripts/shipped_path_eval.py`, `test_shipped_path_rag_eval.py` | abstention-disposition scorer | **KEEP** | Verified by the caller: ≤ +1 on old answers, 0 on new. |
| 34 | `docs/design/2026-09-19-conversation-memory.md` | one-line rule | **KEEP** | Matches #30. |

Magic numbers accounted for: 2 openings/doc (#10 → 1 = chunk 0), insert-at-6 and 4 neighbours (#13 → gone), overlap ≥ 2 (#9 → gone), top 10 (#21 → `evidence_depth`), bridge 8/6 (#11 → gone). Capitalisation and sentence-leader list (#15 → gone). English phrase lists (#6, #7 → gone). ASCII stems (#23 → prefix rule). Contract wording (#25, #27 → reworded).

## 2. Eval failures → surviving mechanism, and expected regressions

| Item | What still addresses it after the rework | Expectation |
|---|---|---|
| dp-multihop-dunlark-notice | #1 (chunk 0 of the pilot doc now carries "on-site work by Quorvin Metrology"); #5 planner hop from the catalog row; #19 retry hop | **Likely regresses on the 9B.** A 600-char catalog row may not surface the name, the first pass will usually be judged sufficient (its terms are covered by generic contracts), so #19 rarely fires. Honest loss. |
| dp-ocr-stock-reference | #1 (heading-only chunk 0 no longer wins BM25); #9 scoped search inside "scanned minutes 05"; #10 opening | Holds (new index only; see #1 note). |
| dp-mt-combine-renewals | #1; planner follow-up resolution (pre-existing); #9 | Holds; turn 3 depends on history, unchanged. |
| dp-transcript-rollback | #9 scoped search, title tokens stripped; #10 | Holds. |
| dp-buried-agreement-meeting | #9; #13 neighbours after their anchor | Holds. |
| dp-section-after-clause | #3 line-start `24.111` body match (a numbered clause starts its line); #13 neighbour right after the anchor; #9 as backstop | **At risk** if the clause number sits mid-line in the chunk; then it rides on #9 and BM25's tokenisation of `24.111`. |
| dp-buried-handbook-ticket | #9 (title tokens of the handbook stripped, "reference ticket raised" remains); #10 | Holds. |
| dp-mt-encryption-versions | #9 + #10 for the two versions; #16 rerank on; #25/#27 wording for "any way round it" | Holds. |
| dp-paraphrase-monitor | #23 (`monitor` is a prefix of `monitoring`); #24; #25/#27 | Holds. |
| dp-paraphrase-clock | #24, #25/#27 | Holds. |

Net expectation: 29–30/38 instead of 31 (loss of dunlark-notice, possibly section-after-clause), with no English, legal or capitalisation rule left in the retrieval path. Recall@5 unaffected (the v3 eval does not run the planner path).

## 3. Cost per focused turn (searches = `search_branches`/`fused_search`, each one embedding + vector + BM25)

| Stage | Before the commit | At HEAD | After rework |
|---|---|---|---|
| Shortlist | 1–2 | 2–3 (+bridge) | 1–2 |
| Planner | 1 LLM call | 1 | 1 |
| Retrieve | Q = 1–3 | Q + (1+Q scoped) + 1 bridge = 4–8 | Q + 1 scoped (only when the planner chose documents) = 1–4 |
| Rerank | 1 pass | 1 | 1 |
| **First pass total** | **2–5** | **6–11** | **2–6** |
| With corrective pass | ×2 | ×2 | ×2 (+ a few hundred planner prompt tokens for #19) |
SQL: openings 1 row per chosen doc (was up to `limit` per doc before the commit, 2 at HEAD); dotted-identifier scan ≤ 4 identifiers, same as before, line-start only.

## 4. Tests to add or fix (each with a non-English and a non-legal case)

1. **embedding_input.rs** — keep the two new tests; add `# Überblick\n## Vertragslaufzeit\nDie Laufzeit…` → one span, breadcrumb `Überblick > Vertragslaufzeit`; and `# Photosynthesis\n\n## Light reactions\nChlorophyll absorbs…\n## Calvin cycle\nCarbon fixation…` → two spans with bodies.
2. **corpus_plan/tests.rs (repository)** — replace `dotted_body_labels_are_found_…`: rows `"7.4 Die Kündigungsfrist beträgt…"` (line start, found), `"Der pH-Wert im Blut liegt bei 7.4 und…"` (mid-line, **not** found), `"3.12 Dominant seventh chords resolve…"` (music, line start, found), `"Python 3.12 added…"` (not found); a breadcrumb `"Kapitel 7 > 7.4. Fristen"` ranks before a body hit for `7.4`. Keep the child-neighbour assertions.
3. **library_search.rs** — extract `scoped_query(question, queries, chosen_titles) -> Option<String>` and test: title "Tessivar Cloud framework agreement" stripped from the question; "Im Handbuch Plattformbetrieb: welche Referenz gehört auf ein Ticket?" with title "Handbuch Plattformbetrieb" → no title tokens remain in the output; "In the Mozart Requiem programme notes, what key is the Lacrimosa in?" with title "Mozart Requiem programme notes" → `lacrimosa key`-class terms only; question entirely made of title tokens → falls back to `queries[0]`.
4. **library_search.rs `expand_evidence`** — fake `ChatRecords` returning fixed neighbours: anchors `a1,a2,a3` → order `a1,n1a,n1b,a2,n2a,a3,n3a`; stops exactly at `limit`; no anchor skipped. Fixture breadcrumbs in German (`Leitfaden > Zellteilung`) and biology.
5. **sufficiency.rs** — replace the inflection test: `monitor↔monitoring` ✓, `vertrag↔vertrages` ✓, `contract↔contrary` ✗, `policy↔policies` ✗ (documented); coverage counted over `evidence_depth`: a matching 6th passage counts at depth 6, not at depth 3. Plan queries in Spanish (`"división celular mitosis"`) to show no English dependence.
6. **rerank.rs** — keep the three tests as they are.
7. **prompting.rs** — delete the bridge relationship test; add: `kb_passage` output never contains "Evidence relationship"; the synthesis note appears once when `kb_context` is set and contains neither "alternatives" nor "template".
8. **kb_retrieval.rs** — delete `an_entity_bridge_groups_its_source_and_target_without_deleting_other_results` (the test that asserts the bad ordering: `target-5` pushed to the end) together with `promote_entity_bridge`.
9. **planner.rs** — a planner reply with `start_at_beginning: true` for "Wo fange ich am besten an?" (no English phrase) keeps the flag (use the existing planner fake if there is one; otherwise unit-test the post-parse normalisation as a pure function).
10. **memory** — keep the FakeLlm change and the two new rule assertions; the payload test (if any) expects compact JSON.
11. **chat/tests.rs:716-717** — assert the reworded strings.

## 5. Build split (3 builders, disjoint files, `cargo check` only)

**Builder 1 — retrieval pipeline** (`chat/retrieval/corpus_plan.rs`, `corpus_plan/planner.rs`, `corpus_plan/library_search.rs`, `corpus_plan/shortlist.rs`, `chat/retrieval/kb_retrieval.rs`, `chat/retrieval/sufficiency.rs`; new unit tests live inside these files, not in `corpus_plan/tests.rs`)
- Delete: `wants_document_openings` (#6), planner override (#7), everything bridge (#11, #14, #15, #17, #18, `tag_entity_bridge`), `TERM_COVERAGE_TOP_N` (#21), `terms_are_simple_inflections` (#23).
- Rework: scoped search to one search + `div_ceil(2)` share + `ByRelevance` (#9); openings = 1 chunk per planner-chosen doc, read whenever chosen (#10); `expand_evidence` neighbours directly after their anchor (#13); `assess_sufficiency(results, plan, tuning, reranked, evidence_depth)` with the prefix rule (#21, #23); PLANNER_SYSTEM reword (#5).
- Add (#19): first-pass top passages' first paragraphs into `plan_correction`, clipped to the planner budget.
- Keep: #8, #12, #16, #20 (rerank.rs untouched), #22, #24. Tests 3, 4, 5, 8, 9.

**Builder 2 — prompts** (`chat/prompting.rs`, `chat/prompting/citation_numbering_tests.rs`, `chat/prompt_settings.rs`, `chat/tests.rs`)
- #25 reword, #26 revert to pre-commit `kb_passage`/`kb_passages` (no relationship line), #27 reword, #28 delete, test 7, test 11. Must not reference any `bridge*` metadata key.

**Builder 3 — storage, chunker, memory** (`repository/workspace/retrieval.rs`, `corpus_plan/tests.rs`, `indexing/use_cases/embedding_input.rs`, `memory/prompts.rs`, `memory/tests.rs`, `docs/design/2026-09-19-conversation-memory.md`)
- #3 line-start-only body match with breadcrumb-first ordering; keep #2, #4; tests 1, 2; #31 revert pretty JSON and the three unneeded version bumps (keep the summariser bump, #30); test 10.

Ordering: the three touch disjoint files and compile independently (Builder 1 removes every caller of what it deletes; Builder 2 only removes a metadata lookup). Coordinator runs the gates once after all three, then the eval on the 9B, and reports the honest score against 26 and 31.

Follow-up (not this PR): retire the pre-existing `WHOLE_DOCUMENT_PHRASES` list behind the summary tier and `apply_summary_openings` in favour of the planner's `start_at_beginning`.
