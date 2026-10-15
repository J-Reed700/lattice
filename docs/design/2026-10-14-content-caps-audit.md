# Fixed-cap audit: lattice-outline/src-tauri (read-only, 2026-10-14)

Paths are relative to `/Users/josh/Code/lattice-outline/src-tauri/src/`. Tests are excluded. Every DISCARDS row was checked by reading its call site. A row may group several constants that share one flow.

Classes: **DISCARDS** (arbitrary cap, must go) / **BUDGET** (derived from model context, fine) / **OUTPUT-SHAPE** (bounds model output) / **DISPLAY** (never reaches a model, or only as a UI or label field) / **GUARD** (termination, work, error-on-exceed, query shaping or split).

## 1. DISCARDS (most damaging first)

| # | path:line | what is capped (value) | flows to | when exceeded | replacement |
|---|---|---|---|---|---|
| 1 | features/conversation/chat/retrieval/mod.rs:465-473; kb_retrieval.rs:387,392; defaults application/contracts/settings/retrieval.rs:324-325, settings/llm.rs:490 | KB results per chat turn: `clamp(max_results=12, 16, 48)`, so 16 by default (plus 8 neighbour expansions) | vault RAG evidence for every chat turn, then grounded_generation | Silently dropped. Results are truncated to 16 *before* `allocation.evidence` is consulted, so a 32k-128k window carries about 16 chunks. | Retrieve to an evidence-budget-sized depth (`allocation.evidence` / measured chunk cost) and let grounded_generation's best-first fill decide. Keep only an index work bound. |
| 2 | features/conversation/chat/verification/judge.rs:58,60,64 (used :220,233,248-256); application/services/evidence_retrieval.rs:6,9 | Grounding judge per claim: 3 cited sources, 3 windows/source x 1,200 chars, **4,000 chars total** | cited page/document text to the claim judge, on every cited chat answer | Silently dropped. The 4th+ cited source is ignored, and support outside the 3 lexically best windows is never seen. Result: false "not found in cited passage" / unsupported. | Per-claim room = judge context - prompt - output (`count_tokens`). Give each cited source its passage or windows until that room is full. If one claim's evidence exceeds the window, split it into sequential judge calls (any-supports). |
| 3 | features/web/services/web.rs:95; applied at web/page.rs:173 via `cap_page_text` page.rs:25 | `MAX_FETCHED_PAGE_CHARS` **50,000** chars/page | chat auto page reads (pipeline.rs:916 calls fetch_url_content), the model's `fetch_url_content` tool, and `read_web_page`. The truncated text is archived (source_snapshots) and answers later reads (pipeline.rs:887, prior_evidence.rs:60). | Truncated, with a flag. pipeline.rs:1346 tells the model to "call fetch_url_content … for the rest", but that call returns the same 50k prefix, so the tail is unreachable. | Keep the whole page (the reference path already uses `usize::MAX`, bounded by the 5 MiB body). Let page_budget / tool_result_allowance fit it. Add an offset/continuation argument to fetch_url_content so the rest becomes a split, not a drop. |
| 4 | features/learning/lessons/content_verification/mod.rs:360; evidence_selection.rs:299 | `retrieve_for_verification(claim, 8)`: 8 passages per lesson claim | factual claim checker, for every lesson claim | Silently dropped. Supporting or contradicting passages ranked 9+ are never checked: false unsupported, missed contradictions. | Retrieve every passage above a relevance floor (or up to the checker's room via `count_tokens`). evidence_checks already halves batches on overflow, so this becomes a split. |
| 5 | features/learning/assessment/assessment_generation.rs:50 | `full_text.chars().take(8000)` per source (up to 24 sources, no context check) | assessment item authoring; items must quote their source | Silently truncated. Items can only be grounded in, or quote, a source's first 8,000 chars. Later chapters are never assessed. 24 x 8k can still overflow a small window. | Retrieve per-requirement passages from full_text (ReferenceCollection), fit by `evidence_room` with no ceiling, and split items across calls when needed. |
| 6 | application/services/context_assembler/budget.rs:77,79,81,85 (applied :249-256,276); features/conversation/chat/memory_context.rs:41 (used :205,216); chat/history_tools.rs:58,60 (used :943,965) | Memory pools: summary <= **1,536** tok, recall <= **2,048** tok, pinned <= 4,096 (borrow 8,192); recall read <= **8 KiB**, 24 candidates, 6 passage groups | long-chat context: rolling summary (compaction.rs:108 `summary_pool_for`) and recalled older turns | Silently dropped. A fraction of the window is computed, then hard-capped, so a 128k model keeps the same 1.5k-token summary and 2k-token recall as an 8k model. | Keep the fractions and drop the absolute caps. Size `RECALL_BYTE_BUDGET` from `allocation.recall`. Let groups and candidates fill that budget. |
| 7 | features/conversation/memory/summarize.rs:34 (used :90) | `MAX_PASSAGE_CHARS` **4,000** chars per message fed to the summarizer | compaction summary, which replaces old history | Silently clipped. Anything past 4k chars of a long message (pasted document, long answer) never reaches the summary. | Split long messages across fold steps (`MAX_FOLD_BYTES` already plans folds) instead of clipping. |
| 8 | features/conversation/chat/prior_evidence.rs:18-19 (used :57,66) | 8 earlier-cited sources, **2,400** chars each | follow-up turns that recall earlier citations (including full archived pages) | Silently dropped, although the function already receives a `token_budget` and stops on it. | Delete both constants: rank everything and excerpt by the existing token budget. |
| 9 | features/learning/practice/practical_generation.rs:160,166; plugin/practical.rs:200; practice/practice_generation.rs:97 | 12 sources; **10,000** chars/source (practical); **12,000** chars/source (practice + tutor) | practical/practice task authoring, tutor feedback | Silently dropped | Retrieve task-relevant passages from all active sources and fit with `evidence_room` (no ceiling). |
| 10 | features/learning/recall/study_generation.rs:299,319; recall/study_service.rs:57,60,93 | `SOURCE_EVIDENCE_CEILING` **6,000** tok (`min` with the real room, model_call.rs:117); chunks per doc = max(24/n, 4); 2,400 chars/chunk; 8 focus terms; sources <= count x 2 | study-deck generation from library docs | Silently dropped | Use `evidence_room` alone (no ceiling) and choose chunks by relevance until the room is full. |
| 11 | features/learning/planning/outline_repair.rs:309-310 | `retrieve_for_verification(claim, 8)` + `retrieve(claim, 8)` | outline factual-finding repair | Dropped. A claim supported only by passage #9+ gets "No saved reference addresses this claim", then a NARROW rewrite. | Same as #4. |
| 12 | features/learning/planning/outline_linking.rs:29 (used :423) | `CANDIDATES_PER_ITEM` 4 passages per outline item | evidence-link chooser | Dropped. When the right passage ranks 5+, the item is linked to "none" or a weaker passage. | Offer candidates until the item's share of the chooser window is full (batch halving already exists). |
| 13 | features/learning/planning/outline_evidence.rs:96,99,114 | `retrieve(query, 4)` per lesson query; 600-char source prefixes as filler | outline authoring, review and repair evidence (`max_context/4` budget is fine) | Ranks 5+ are never offered even when the budget has room; the leftover room gets 600-char prefixes. | Keep pulling deeper ranks round-robin until the budget is spent. |
| 14 | features/learning/lessons/review_evidence.rs:173 | `retrieve(issue, 8)` per review finding, with no budget at all | review findings checked against saved references | Dropped (and unbounded in total) | Retrieve by `count_tokens` room per finding. |
| 15 | features/conversation/chat/tool_loop/calls.rs:314 + settings/llm.rs:488-489; retrieval/tool_format.rs:98 | Tool result clamped to `max_chars` **50,000** on top of the window-derived allowance; search results show <= 3 matches per doc x `excerpt_chars` 4,000 | results of tools the model called (get_document, KB search) | Silently truncated | Use `tool_result_allowance` alone (drop the configured ceiling) and show matches until the allowance is spent. |
| 16 | application/contracts/settings/retrieval.rs:210-219,332-336; used retrieval/pipeline.rs:816,825,862,1042,1123,1247 | Web: per-page ceiling `web_page_max_chars` 50,000 on top of `page_budget`; 3 pages read in full; 5 web results; wiki 5 results / 3 used / 1,200-char snippets | chat web evidence | Pages 4+ arrive as snippets only; each page is capped even when the budget has room. | Drop the per-page ceiling (page_budget already shares the budget fairly) and derive pages_wanted from the evidence budget. |
| 17 | features/conversation/chat/retrieval/corpus_plan.rs:698 (+ :150,155,332); repository/workspace/retrieval.rs:90 | "Start at the beginning" reads: <= 4 documents x **4** opening chunks | collection-overview and learning turns | Dropped. 16 opening chunks, whatever the evidence budget. | Read opening chunks in reading order, round-robin, until `allocation.evidence` is spent. |
| 18 | features/conversation/chat/memory_context.rs:35 (used :172-175,272,362,369) | `RECENT_CANDIDATE_MESSAGES` 64 | verbatim recent history | Lossy. Messages older than 64 force compaction into the <= 1,536-token summary even when the window could hold them verbatim. | Page candidates by the `recent_history` token pool (the 35% fraction), not by a message count. |
| 19 | features/conversation/synthesis.rs:53-63 (used :122,126,207,482,517,532,537,609,632,737) | Journal: 12 (max 24) entries, 900 chars/message, 6,000 chars/entry. Week: 12 conversations, 20 references, 8 notes, **600** chars per source. | journal and week synthesis calls | Silently truncated | The chunk split (`JOURNAL_SYNTHESIS_CHUNK_CHAR_LIMIT`) already exists: drop the per-item caps and map-reduce over chunks sized from context. |
| 20 | features/compare/use_case.rs:28,31,32 (retrieval.rs:72,119,131; use_case.rs:141) | 3 chunks per column, 10 chunks/doc, 12 documents (docs past 12 silently ignored) | per-document grounded compare call | Silently dropped | Chunks per column from the per-doc call's room; reject or split instead of silently dropping documents. |
| 21 | features/summaries/prompt.rs:16,19,42,46 (generate_summaries.rs:136,154,166; source.rs:20,24) | Document summary reads the first **3,000** tok; sections 1,500 tok; 12 section summaries per doc; 24 headings | summary index: the planner catalog and summary tier (`summary_index_enabled` defaults to false) | Long docs are described from their opening only; sections 13+ are never summarized. | Map-reduce over all sections (split); schedule all sections instead of capping them. |
| 22 | features/explorer/prompt.rs:24 (:117); index/tool.rs:17 (:128); fs.rs:24 (:193); index/run/files.rs:65; tools.rs:30 (:358); index/chunker.rs:20 (:74) | Folder chat: 5 pre-generation passages; 8 hits per `search_folder`; `read_file` refuses files > 2 MiB even though it is line-windowed; files > 1 MiB not indexed; lines clipped at 500 chars | folder-chat evidence | Dropped | Budget-derived hit counts, paged reads for big files, and indexing large files in pieces. |
| 23 | features/learning/planning/diagnostic_generation.rs:32,39; stored preview at references/source_library.rs:19,32,336 and sources.rs:17,67,75,86 | `take(6)` module outcomes; sources sent as stored `learning_sources.excerpt` (**2,400**-char previews, all sources, no budget) | starting-point diagnostic authoring | Modules 7+ are never diagnosed; sources appear only as 2,400-char previews. | Split outcomes across calls; pass retrieved passages from `verification_sources` full_text, fitted to room. |
| 24 | features/conversation/chat/retrieval/followup_context.rs:84 | Anchor test runs over the first **18,000** chars only | follow-up reuse of the previously cited document | The document is dropped when the follow-up concerns a part after 18k chars. | Test against the whole text or the document's chunk index. |
| 25 | features/conversation/chat/persistence.rs:265 (used :336) | 8 cited web sources stored per turn | durable cited sources, which feed linked sources and prior_evidence | Sources 9+ are never stored | Store all cited sources. |
| 26 | application/services/conversation_context.rs:9-13 (used :20,31,60) | Linked web sources: 10 items, 280-char excerpts, budget clamped to 120-320 tok | prompt list of pages linked to the chat (the model must know them in order to fetch them) | Sources 11+ are invisible | Use the context fraction only (drop the item count and the clamp). |
| 27 | features/conversation/chat/routing.rs:73 (:109); retrieval/conversation_helpers.rs:76,94; learning/practice/practice_generation.rs:140 | History turns: 4 for intent; 4 recent + 4 matching older for HyDE/planner context; 4 tutor turns | intent classifier, HyDE/query planning, tutor | Dropped | Fit history by `count_tokens` for each call. |
| 28 | features/conversation/memory/verify.rs:42 (:363,392); memory/mod.rs:107 + memory/selection.rs:169 | +/-1,500 chars around a memory quote; 1,000 chars per context message | memory extraction and verification calls | Clipped (flagged) | Derive from the extraction or verification call's room. |
| 29 | features/conversation/repository/knowledge.rs:338 (466); memory_semantic.rs:45,84 | 24 optional cross-chat memories; semantic scan of the newest 2,000 / 20,000 rows; 64 hits | cross-chat memory and history semantic recall | Older items are never scored | Let the assembler budget choose, and scan in pages. |
| 30 | features/conversation/chat/source_snapshots.rs:23 (:268) | `SNAPSHOT_MAX_CHARS` 64,000 | archived page; *is* a prompt ingredient (pipeline.rs:887, prior_evidence.rs:60), contrary to its doc comment | Latent behind #3; bites once #3 is lifted | Store the whole page (content-addressed). |
| 31 | features/conversation/chat/retrieval/corpus_plan/health.rs:26; shortlist.rs:28,32; prompt.rs:38,41,47; corpus_plan.rs:94 | Planner prompt <= 2,560 tok (latency-derived ceiling); <= 20 catalog rows; 300-char openings; 8 headings; history <= 512 tok; 600-char summaries | retrieval planner, which decides the opening documents | Documents beyond the shortlist cannot be opened by the planner (search still covers them). | Derive the prompt size from measured prefill rate x `PLANNER_TIMEOUT`; health already adapts it downward. |
| 32 | features/learning/references/web_research.rs:98 | `max_results` 12 per research query | outline/lesson web research breadth | Never fetched (breadth, not truncation) | Page through results while new in-scope sources keep appearing. |
| 33 | features/references/commands.rs:18 (:92) | Saved passage text 20,000 chars | stored reference, which is week-synthesis input | Silently truncated on save | Store it whole. |
| 34 | features/corpus_shape/commands.rs:149; clustering.rs:200; labeling.rs:21 (:143) | 5 docs x 500 chars per cluster | cluster labelling call | Dropped (low impact) | Budget from the label call's room. |
| 35 | features/conversation/chat/prompting.rs:357-358 (:373) | 12 prior searches x 120 chars | deep-research "already searched" list | Dropped (low impact) | Fit by `count_tokens`. |
| 36 | features/conversation/starters.rs:49 | 20 recent titles | starter-question generation (UI chips) | Dropped (low impact) | Fit by `count_tokens`. |

### Being fixed in the current change (another agent owns these files)

| path:line | cap (value) | flow | class |
|---|---|---|---|
| features/learning/lessons/generation.rs:30 (used :241; also generation/recall.rs:242 and generation/lesson.rs:168 via `source_room`) | `SOURCE_EVIDENCE_CEILING` 7,000 tok | lesson + lesson-recall-card source room | DISCARDS, being fixed in the current change (recall.rs:242 shares the constant) |
| features/learning/lessons/generation.rs:251-254 (called from lesson.rs:174, recall.rs:250) | `bounded_sources`: 12 sources, 2,400 chars/excerpt, 180-char titles | lesson and recall-card authoring sources | DISCARDS, being fixed in the current change |
| features/learning/lessons/generation/lesson.rs:68 | `take(3)` recaps of previous lessons | lesson prompt continuity | DISCARDS, being fixed in the current change |
| features/learning/lessons/reference_collection.rs:251,255,267 | `retrieve` limit clamp(1,50); library hits = limit x 3; lexical fallback `take(100)` | every learning retrieval | DISCARDS, being fixed in the current change |
| features/learning/lessons/reference_collection.rs:282-286, 366 | +/-300 chars of passage context; `SURROUNDING_CHARS` 1,700 for verification | passage text for authoring and checking | DISCARDS, being fixed in the current change |
| features/learning/lessons/reference_collection.rs:473 | `author_sources` retrieves 12 | lesson authoring sources | DISCARDS, being fixed in the current change |
| features/learning/lessons/reference_collection.rs:428-450 | catalog budget `max_context/16` (BUDGET) but 40 headings, 120-char titles | reference catalog in prompts | DISCARDS (headings/titles), being fixed in the current change |
| features/learning/lessons/reference_collection.rs:16-17 | `CHUNK_CHARS` 1,200 / `STEP_CHARS` 900 | overlapping chunking (split, nothing dropped) | GUARD, being fixed in the current change |
| features/learning/lessons/generation.rs:32-37; generation/lesson.rs:78,83 | MAX_BODY 5,000, MAX_QUESTION 1,200, MAX_OPTION 500, MAX_EXPLANATION 1,600, MAX_QUOTE_CHARS 1,200; 8-12 blocks, 6 questions | lesson output schema | OUTPUT-SHAPE, being fixed in the current change |
| features/learning/lessons/content_verification/repair.rs, research.rs | no fixed caps found (research.rs:46 `maxItems = results.len()` is dynamic) | n/a | n/a |

## 2. BUDGET (derived from the model's context; fine)

| path:line | what | flow | when exceeded | class |
|---|---|---|---|---|
| application/services/grounded_generation/mod.rs:302,437 | evidence = `allocation.evidence` | all grounded calls | best-first fill | BUDGET |
| application/services/context_assembler/budget.rs:83 | recent history = 35% of available (no absolute cap) | chat history | compaction | BUDGET |
| features/conversation/chat/retrieval/page_budget.rs:18,25 | page chars from the evidence budget, shared fairly | chat web pages | clipped, and says so | BUDGET (but see DISCARDS #3/#16 ceilings) |
| features/conversation/chat/tool_loop.rs:657-673 | `tool_result_allowance` = window share | tool results | clipped to share | BUDGET (ceiling is DISCARDS #15) |
| features/conversation/chat/tool_loop/research_notes.rs:44 | condense at 60% of the window | tool loop | condenses (split), no drop | BUDGET |
| features/conversation/chat/attachments.rs:253-261 | `truncate_to_token_budget` per attachment share | message attachments | truncated with a note to the model | BUDGET |
| features/conversation/chat/retrieval/followup_context.rs:103-106 | `truncate_to_token_budget` | follow-up document | clipped | BUDGET |
| application/services/claim_verification.rs:489; claim_verification/batch.rs:111 | batches by `max_context_tokens` | claim checks | split | BUDGET |
| features/learning/planning/outline_evidence.rs:71 | `max_context/4` evidence budget | outline evidence | pops passages | BUDGET (rank cap is DISCARDS #13) |
| features/learning/lessons/structured_call.rs:146 | `source_room(..., usize::MAX)` | two-step call choice | falls back to one-step | BUDGET |
| features/learning/lessons/content_verification/evidence_checks.rs:283-343 | halves a batch on overflow | claim checks | split | BUDGET |
| features/conversation/compaction.rs:108 | `summary_pool_for` from window | compaction | n/a | BUDGET (cap inherited from DISCARDS #6) |
| features/llm/engine/sidecar_manager/config.rs:103 (used :161-162) | `AUTO_MAX_CONTEXT_SIZE` 32,768: the window `Auto` gives llama-server | root of every `max_context_tokens` budget | n/a | BUDGET (root; note that it fixes every derived budget at 32k regardless of model or RAM) |

## 3. OUTPUT-SHAPE (bounds model output)

| path:line | what | when exceeded | class |
|---|---|---|---|
| features/conversation/chat/verification/judge.rs:55,65 | judge output 4,096 tok; quote 400 chars | truncated | OUTPUT-SHAPE |
| application/services/claim_verification.rs:45-46 (:683,689; batch.rs:90) | quote 400 / reason 600 chars | truncated | OUTPUT-SHAPE |
| features/conversation/chat/retrieval/corpus_plan.rs:203,205; corpus_plan/prompt.rs:29 | planner: 3 queries x 24 words (silently truncated), 384 output tok, <= 4 opening ids | truncated | OUTPUT-SHAPE |
| features/learning/planning/outline_review.rs:163,337 | <= 24 new findings; 6,000 + 200n output tok | schema | OUTPUT-SHAPE |
| features/learning/planning/outline_references.rs:137-139,148,162 | subtopics <= modules, 3 queries, 240 chars; output tok | schema | OUTPUT-SHAPE |
| features/learning/references/web_research.rs:30 | 3 queries, 240 chars | schema | OUTPUT-SHAPE |
| features/learning/lessons/content_verification/challenge.rs:27,84,109 | 3 queries; 1,200 output tok | schema | OUTPUT-SHAPE |
| features/learning/lessons/content_verification/coverage.rs:77-89,126; inventory/revisions.rs:63-64 | reasons 500, assertions 1,000, statements 2,000 chars | schema/reject | OUTPUT-SHAPE |
| features/learning/planning/outline_linking.rs:511,730; :373 | reason 300 chars; citation quote trimmed to whole sentences <= 1,200 | trimmed | OUTPUT-SHAPE |
| features/learning/planning/diagnostic_generation.rs:40 | task fields 1,600 / 2,000 / 1,200 | schema | OUTPUT-SHAPE |
| features/learning/lessons/generation/recall.rs:129-133,209 | recall-card fields | schema | OUTPUT-SHAPE |
| features/learning/assessment/assessment_generation.rs:57 | output tok = min(slots x 1,100 + 600, 16,000) | truncated output | OUTPUT-SHAPE |
| features/learning/lessons/teaching_review.rs:260; answer_review.rs:157; teaching.rs:372,378 | findings <= 700 chars; 6 errors x 650 chars per retry | trimmed | OUTPUT-SHAPE |
| features/summaries/prompt.rs:49-55 | summary 900 chars; 5 topics x 60 | truncated | OUTPUT-SHAPE |
| features/conversation/handoff.rs:223 | handoff summary 2,048 output tok (all that is carried into the new chat) | truncated | OUTPUT-SHAPE |
| domain/conversation/memory.rs:65-79; memory/apply.rs:169,193,202 | proposal 64 KiB, 32 ops, 4 evidence/item, 2 KiB quotes, 512 active items, 8 related | reject/trim | OUTPUT-SHAPE |
| features/search/hyde/hyde_generator/text.rs:31,169 | HyDE 90 words; web query 12 words | trimmed | OUTPUT-SHAPE |
| features/llm/di.rs:41; context_assembler/budget.rs:91 (:100,519) | utility output 8,192; chat output reservation capacity/4 capped at 32,768 | `max_tokens` | OUTPUT-SHAPE |
| features/compare/use_case.rs:29-30,33-34 | 6 columns x 80 chars; value 400; quote 320 | validate/trim | OUTPUT-SHAPE |

## 4. DISPLAY (UI, storage or labels; no evidence reaches a model through these)

| path:line | what | class |
|---|---|---|
| features/conversation/chat/verification/mod.rs:143-149 | UI summary schema (60 verdicts, 20 listed claims, 2,000/1,000 UTF-16 units, 24 citations); counts stay true | DISPLAY |
| features/conversation/chat/turn_record.rs:21-33,484 | step record (200 steps, 200-char details, 24k reasoning, 12 links) | DISPLAY |
| features/conversation/chat/tool_loop/generation.rs:161 | 24k-char reasoning capture for the UI | DISPLAY |
| features/conversation/chat/tool_loop/tool_sources.rs:16 | 480-char card excerpt (the `content` field carries the page) | DISPLAY |
| features/conversation/chat.rs:263; handoff.rs:27; branching.rs:113; repository/tangents.rs:26 | chat titles (50 / 200 / 200 / 80) | DISPLAY |
| features/learning/planning/outline_draft.rs:409-414; outline_repair.rs:80-84 | progress labels (160 / 120) | DISPLAY |
| features/learning/references/web_research.rs:132; references/sources.rs:18 | 180-char source titles (metadata) | DISPLAY |
| features/conversation/chat/retrieval/kb_retrieval.rs:444 | 400-char unavailable reason | DISPLAY |
| features/conversation/repository/workspace/retrieval.rs:328; workspace/explorer.rs:47; workspace/bookmarks.rs:202; workspace/journals.rs:395; memory_details.rs:20; commands.rs:305 | UI lists / @ picker (50-200) | DISPLAY |
| features/learning/references/source_library/reads.rs:108,214; source_library.rs:224; recall/recall_repository.rs:26-27 | source checks 50, search 50, 420-char hit context, 2,000 cards (error), 240-char query | DISPLAY |
| features/web/services/snapshot/mod.rs:29-51; archive/content.rs:140; snapshots/protocol.rs:17 | frozen-snapshot HTML for the sandboxed viewer; archive link list | DISPLAY |
| features/search/enrichment_service.rs:300; engine/snippet.rs:3-4; explorer/fs.rs:29 | UI search snippets/previews (not traced to a prompt) | DISPLAY |
| features/daily_notes/capture.rs:127,206 | note ids; 120 snapshots | DISPLAY |
| features/learning/portability/pack.rs:23-27; pack_repository/*; canvas/canvas_repository.rs:11-12; practice/practice_repository.rs:16-17; assessment/assessment_repository.rs:17-18; runtime/* | pack/canvas/learner-artifact/sandbox limits (reject) | DISPLAY |

## 5. GUARD (termination, work bounds, errors on exceed, query shaping, splits)

| path:line | what | class |
|---|---|---|
| features/search/use_cases/hybrid_search.rs:78,88 | branch depth x3; `PER_DOCUMENT_CAP` 4 reorders for diversity, and the overflow follows (nothing dropped) | GUARD |
| features/search/use_cases/hybrid_search/rerank.rs:12,15; engine/qwen3_reranker.rs:25 | rerank pool <= 64; reranker input 2,048 tok (scoring only; reranking off by default) | GUARD |
| features/search/engine/vector_search/usearch_index.rs:62,64 | exact-scan / scoped-candidate work bounds | GUARD |
| features/conversation/chat/history_tools.rs:76,81,84 | read floor 512 B; per-turn read total 128 KiB (fixed; should be window-derived); 64 messages per read, paged with a cursor | GUARD |
| features/conversation/chat/memory_context.rs:38 | 512 KiB recent-candidate read bound | GUARD |
| features/conversation/memory/segment.rs:24-39; memory/mod.rs:83; memory/summarize.rs:29; memory/verify.rs:39; evidence_checks.rs:6; outline_linking.rs:27; coverage.rs:345 | batch/segment/fold sizes (split, not drop) | GUARD |
| features/conversation/chat/tool_loop/research_notes.rs:49; retrieval/pipeline.rs:449,452; settings retrieval.rs:225 (deep_research_depth 3); llama_cpp/mod.rs:537; memory/mod.rs:74-80; outline_draft ATTEMPT_LIMIT | loop/attempt/round limits | GUARD |
| features/conversation/chat/tool_loop/document_evidence.rs:13; chat/fetch_memory.rs:22,26,162 | dedupe/fetch memory (eviction re-reads; nothing lost) | GUARD |
| features/conversation/compaction.rs:25,28 | keep >= 4 (min 2) recent messages verbatim | GUARD |
| features/conversation/chat/retrieval/sufficiency.rs:35-46 | coverage heuristic over the top 5 (code only, drops nothing) | GUARD |
| features/conversation/chat/retrieval/external_query.rs:192; search/engine/fts_query.rs:29-36; repository/memory_recall.rs:31-40; history_tools.rs:64,68; corpus_plan.rs:50; shortlist.rs:35; rerank.rs:56; repository/knowledge.rs:304,364,451; memory_semantic.rs:22; chat.rs:92; embedding/sparse_head.rs:56,62; domain/value_objects/search_query.rs:8 | query shaping (term counts, query lengths, 600-char search-hit previews that have a read tool behind them) | GUARD |
| features/web/services/web.rs:82-84,101-105; web/address_guard.rs:17 | provider pagination 10 x 5; body-size limits (error) | GUARD |
| features/function_calling/executor/web_tools.rs:103; chat/tool_loop/scoped_document_tools.rs:76; executor/document_tools.rs:287,377; executor/custom_tools.rs:125 | bounds on model-supplied tool `limit` args (re-callable / paged) | GUARD |
| features/embedding/use_cases/generate_batch.rs:47; generate_single.rs:38; embedding/input_policy.rs:14; indexing/commands.rs:66-68 | embed length (error); chunk sizing (split) | GUARD |
| features/indexing/engine/extraction/pdf.rs:29-31; xlsx.rs:12-20; archive_budget.rs:13-17; extraction/mod.rs:61 | extraction limits (error, not silent) | GUARD |
| features/learning/assessment/assessment_generation.rs:45; repository/knowledge.rs:296 | > 64 slots / > 24 sources / > 1,000 memories, all error | GUARD |

## Counts (rows above; some rows group several constants)

- DISCARDS: 36 outside the files being fixed, plus 7 in those files (being fixed in the current change).
- BUDGET: 13
- OUTPUT-SHAPE: 19, plus 1 being fixed
- DISPLAY: 14
- GUARD: 16, plus 1 being fixed
