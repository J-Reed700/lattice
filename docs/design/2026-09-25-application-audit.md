# Application audit, 2026-09-25

Six read-only reviewers, one per area, each handed the 2026-09-19, 09-22 and 09-23 findings so they would report only what is new. Every finding below carries the `path:line` the reviewer read. The items marked **(v)** were re-read by the main session before this document was written; the rest are reviewer claims, verified to the extent that the quoted evidence line exists.

Branch: `conversation-memory-release-readiness`, tree as of the Track A (local tool calling) and Track B (verifier phases 2–3) edits.

## 1. Verdict

The skeleton is sound in every area: blob leases and GC, atomic turn commit, sidecar lifetime, event subscriptions, the CSP and IPC boundary, and space scoping on every retrieval path. The defects cluster at the edges: file-format extractors that lose content, error paths that leave a row `pending` or replace a real error with a fake one, timeouts that do not match the budgets above them, and three file-level database operations that ignore the write-ahead log. Two findings are security issues worth fixing before any external release: a web page can poison the page cache for another URL, and redirects reach private addresses before validation.

## 2. Fix first

Ranked across all areas by how many users hit it and how bad the result is.

**Status after the 2026-09-25 fix wave** (six agents, one per area, cut off by the spend limit; tree
reconciled by hand afterwards: 3599 Rust tests, 1132 vitest, clippy/fmt/tsc/eslint clean). Items 1–15
and 17–20 are fixed in the tree, each with a test named for the behaviour. Item 16 (eval re-pointed at
chat retrieval, run in CI) is deferred. Also deferred from §3: remote LLM auth headers still live in
`settings.json` (they are now left out of exports and restored on import); download hash / `If-Range`
resume; tool-calling support persisted on the downloaded-model row; per-token re-render and message
virtualisation; the Intel binary lock pin; dropping `space_id` from the memory attributes table (an
index on it was added instead). Behaviour changes worth knowing: attaching an already-attached file to
a second chat moves it to that chat; the asset protocol scope is now app data + library + the folders
the user has opened; a second launch focuses the running instance; log files keep a week; the
standalone `ask_question` commands and the frontend `progressStore` are gone.

| # | Finding | Area | Severity |
|---|---|---|---|
| 1 | **(v)** DOCX extraction returns XML junk plus the first text run. Word writes `document.xml` on one line; the parser reads lines. Every Word file in every library is indexed wrong. `extraction/docx.rs:90-121` | Ingestion | wrong-result |
| 2 | **(v)** A failure after generation, or a crash mid-turn, leaves the user message `pending` forever; nothing at startup resets it. `chat.rs:1292` (`.await?` skips `mark_user_message_failed`) | Conversation | data-loss |
| 3 | **(v)** A crashed sidecar is never restarted; the cached port is returned without a liveness check until a settings change. `sidecar_manager.rs:2636-2650`, `infrastructure/model_cache.rs:40-50` | LLM | wrong-result |
| 4 | **(v)** Non-tool local models (any GGUF outside the curated tool list, including Ornith) stream through the legacy decoder: truncated answers saved as complete, `reasoning_effort` and `max_output_tokens` dropped, history flattened into one system message. Gate is `provider_name() == "llamacpp"` and the sidecar says `"local-sidecar"`. `tool_loop.rs:272-274`, `factory.rs:488` | LLM | wrong-result |
| 5 | **(v)** Non-streaming sidecar calls are capped at 120 s whatever the caller's budget; memory extraction (8192 tokens) and HyDE hit it. `sidecar_client.rs:71, 465-474` | LLM | wrong-result |
| 6 | **(v)** One malformed or truncated tool call fails the whole turn and discards streamed text. With tools now on for the sidecar this is reachable by every small model. `llama_cpp/mod.rs:513-535`, `tool_loop.rs:303-313` | Conversation / LLM | wrong-result |
| 7 | **(v)** Web URL imports never reach the live vector index; the article is missing from dense search until the next launch, which then rebuilds the whole index. `web/services/ingestion/service.rs:181` | Ingestion | wrong-result |
| 8 | **(v)** A page read through the hidden browser can poison the cache entry for another URL: `final_url` is taken from the JSON the page posts. `browser_reader.rs:130-137, 210`, `page_cache.rs:175` | Platform | security |
| 9 | **(v)** Redirect hops are followed before validation, so a public page can make the app GET a LAN address. `is_private_ip` misses `::ffff:` mapped, `100.64/10`, `0.0.0.0/8`. `web.rs:354-358, 455-461, 1426-1444` | Conversation | security |
| 10 | **(v)** Pre-restore safety copy is taken with `fs::copy` while the pool is open in WAL mode; recent commits are not in it. `backup/adapter.rs:483-492` | Platform | data-loss |
| 11 | **(v)** Corruption recovery renames `lattice.db` but leaves `-wal`/`-shm`, which SQLite can replay into the fresh file. `connection.rs:92-104` | Platform | data-loss |
| 12 | **(v)** A second launch SIGKILLs the first instance's llama-server (no single-instance guard; reaper kills every installed sidecar). `main.rs:35`, `sidecar_manager.rs:1109-1134` | Platform | crash |
| 13 | **(v)** The API router retries any error containing "not found" as a different route, so "Space not found" reaches the user as "command not found". `src/lib/api.ts:437-475` | Frontend | wrong-result |
| 14 | **(v)** One failed batch-status poll sends the message with `documentIds: []`, so the answer never reads the attachments. `ChatPanel.tsx:707-708, 744-749` | Frontend | wrong-result |
| 15 | **(v)** A search failure longer than 400 chars fails the strict zod schema and the whole retrieval trace vanishes on the turn that needed it. `kb_retrieval.rs:243-246`, `src/types/conversation.ts:396` | Frontend | wrong-result |
| 16 | The retrieval eval measures the search-page service, not chat retrieval, and CI never runs it; today's `fused_search` change is unmeasured. `examples/retrieval_eval/production.rs:14`, `.github/workflows/eval-integrity.yml` | Search | wrong-result |
| 17 | `# comment` lines in code files and inside markdown fences become section headings, splitting code into tiny chunks. `embedding_input.rs:52, 222, 282` | Ingestion | wrong-result |
| 18 | XLSX drops every numeric cell; PPTX's `<a:t` prefix match pulls table XML into text. `xlsx.rs:71,93`, `pptx.rs:82,94` | Ingestion | wrong-result |
| 19 | Tool schemas are counted nowhere in the token budget; on 2k–4k sidecar windows the prompt can exceed `n_ctx`. `chat.rs:1077-1084`, `tool_loop.rs:1243-1263` | Conversation | wrong-result |
| 20 | Running out of tool rounds (5, deep research included) fails the turn and throws away everything gathered. `tool_loop.rs:168, 253, 936-952` | Conversation | wrong-result |

## 3. Findings by area

### 3.1 Ingestion and indexing

The blob, GC and index plumbing is careful. The extractors are where content is lost.

| Finding | Where | Fix |
|---|---|---|
| DOCX line-by-line parser (fix-first 1) | `extraction/docx.rs:90-121` | Cursor scan matching exactly `<w:t>` / `<w:t ` as `pptx.rs:88` does, or quick-xml; test with a real .docx |
| `# ` comment lines as headings (17) | `embedding_input.rs:52, 222, 282` | Track code fences in `structure_spans`; no heading detection for `text/x-*` |
| XLSX numbers dropped, PPTX table XML (18) | `xlsx.rs:71,93`, `pptx.rs:82,94` | Rows as `header: value` like the CSV extractor; exact tag match; read `ppt/notesSlides` |
| Web imports not published to the live index (7) | `web/services/ingestion/service.rs:181` | Publish chunks and invalidate the query cache as `IndexFileUseCase::publish_document` does |
| HTML strip adds no separators; headings lost; numeric entities undecoded | `extraction/html.rs:83-101` | `\n` for block tags, space otherwise, `h1–h6` → `#`, decode `&#NNNN;` |
| Attaching a file already attached to another chat leaves it owned by the first chat; the second chat cannot see it after turn one | `start_file_import.rs:363-365` | Re-stamp the owner on the Duplicate path |
| Renaming a note in Finder deletes it (Remove event fires, new file skipped as id mismatch) | `vault/watcher.rs:260, 293` | Before deleting, look for a file whose front-matter id matches |
| Text, HTML and code must be UTF-8; Windows-1252 and UTF-16 are rejected | `extraction/text.rs:37`, `html.rs:40` | Read bytes, honour BOM, lossy fallback |
| Directory walk follows symlinks (loop → stack overflow) and aborts on the first unreadable folder; runs sync on an async worker; walks `.git` and `node_modules` | `index_directory.rs:233, 252` | walkdir without following links, skip and log folder errors, `spawn_blocking` |
| `chunking_policy_version` does not exist; libraries are never re-chunked. `Document::from_file` sentence-chunks and fills `has_code`/`section`, then `prepare_structured_with_spans` throws all of it away, so `has_code` is always false | `index_file.rs:405-421`, `chunking_strategy.rs:44` | Store a policy version per document; delete the sentence pass |
| 30 s PDF timeout does not stop the blocking thread; retries stack threads | `extraction/pdf.rs:66` | Scale with page count; cancel flag in the page loop |
| Quadratic batch polling (`get_batch_job` loads every item twice per item); `index_file.rs:576` loads every vector blob for chunk ids; `embedding/generation.rs:37-50` embeds one chunk per batch | as listed | Status-only and ids-only queries; batch embeds in `prepare` |
| Audio skips the 50 MB cap on the file and directory import paths | `content_extraction_adapter.rs:118` | Apply the cap |

Known items still present: file read three times, CAS copy without re-verify, zip extractors without an uncompressed cap, watcher/writeback warn-only, rename leaves the old name in the prefix, `find_all()` for ids, OCR no-op. Fixed: reindex validates content; first-load index goes through `open_or_rebuild`.

### 3.2 Search and retrieval

Space and attachment scoping hold on every live path. Fusion is one formula, but chat uses its own copy with different inputs, and nothing measures chat retrieval.

| Finding | Where | Fix |
|---|---|---|
| Eval measures the search page, not chat; CI runs only fixture checks (16) | `examples/retrieval_eval/production.rs:14,381`, `eval-integrity.yml:17-19` | Point the harness at `corpus_plan::retrieve`/`fused_search`; add a small-model recall gate |
| Small scopes force a full-index HNSW pass per query (pass 2 uses `count = index_size`; the early exit never fires at floor 0.15), under the state read lock | `usearch_index.rs:1013-1043, 408-410`, `corpus_plan.rs:685-690` | Count in-scope chunks first; exact search over the scope's keys when small |
| Chat's vector branch runs inline on the async worker, so BM25 does not actually run alongside it | `semantic_search.rs:140`, `corpus_plan.rs:707` | `spawn_blocking` as `hybrid_search.rs:139` already does |
| Every tool-loop search loads the whole document list and does a linear find per hit | `scoped_document_tools.rs:116-120` | Fetch only the hit ids or build a map once |
| Fusion inputs differ: chat floor 0.15 vs page 0.3; `normalize` vs `strict_tokenize`; compare has no keyword branch; chat hand-rolls RRF | `corpus_plan.rs:953-961, 688`, `di.rs:293`, `bm25.rs:99`, `compare/retrieval.rs:61` | One `fuse_ranked`, one floor constant, `fused_search` for compare |
| Compare falls back to a document's opening chunks on vector failure at `debug!` level, loading every chunk to keep a few | `compare/retrieval.rs:78-95` | `warn!`, mark the cell degraded, `LIMIT` |
| Search page returns fewer than asked when attachments match (filtered after LIMIT) and caches the short list for 10 min | `enrichment_service.rs:194`, `bm25.rs:73`, `hybrid/service.rs:852` | Filter `owner_conversation_id IS NULL` inside the SQL and the vector predicate |
| `HybridSearchUseCase` `space_id` path drops unfiled documents and is ignored by the vector branch; every caller passes `None` | `sqlite_text_search.rs:236-239`, `hybrid_search.rs:195-241` | Remove the parameter |
| `query_time_ms` measured before the search runs | `hybrid_search.rs:188` | Measure after |
| Legacy `ask_question` searches every space, no frontend caller | `ask_question.rs:255,389`, `src/lib/api.ts:1806-1823` | Delete |
| HyDE and classifier failures swallowed by `.ok()` with no log | `retrieval/pipeline.rs:519,525` | `warn!` first |

Known: sparse off, reranker off, reranker has no timeout. The >32k IN-list fix holds.

### 3.3 Conversation pipeline

Turn skeleton holds: `pending` user row, atomic `complete_turn`, `done` on emitter drop, every wait raced against deadline and cancel. Track A's changes broke nothing found, but tools on the sidecar make findings 6 and 19 reachable.

| Finding | Where | Fix |
|---|---|---|
| `pending` forever after a finalize failure or crash (2) | `chat.rs:1276-1292`, `chat/persistence.rs:108-142` | `fail_pending_turn` on any finalize error; startup `UPDATE … status='failed' WHERE status='pending'` |
| Malformed tool call kills the turn (6) | `llama_cpp/mod.rs:513-535`, `tool_loop.rs:303-313` | Return the bad call as a tool error result for the next round |
| Round limit fails the turn (20) | `tool_loop.rs:168, 253, 936-952` | Last round sends `tools: []` and "answer now"; scale rounds for deep research |
| Tool schemas outside every budget (19) | `chat.rs:1077-1084`, `tool_loop.rs:1243-1279` | Count `tool_specs` JSON once per turn and charge it |
| Redirects reach private addresses (9) | `web.rs:354-461, 1426-1444, 1486` | `redirect::Policy::custom` validating each hop; pin resolved IPs |
| One cookie jar across every turn and conversation; proxy URL with credentials logged at debug | `stealth.rs:424, 429`, `web.rs:354` | No cookie store or one per turn; redact the proxy URL |
| "Memory used" on each answer omits the conversation's own items and depends on parsing rendered prose | `chat.rs:1136-1149`, `render.rs:106-126` | Return used ids from `MemoryPlan` |
| Tool execution not raced against Stop or the deadline; Stop waits for the fetch (15 s slot + HTTP + 25 s browser fallback) | `tool_loop.rs:640-675` | `timeout(remaining)` + cancel `select!` like the LLM calls |
| `summary_pool_for` sizes summaries with `fixed = 0`; on small windows an accepted summary exceeds the next turn's pool → `incomplete_history` → refused turn | `compaction.rs:72-78` vs `budget.rs:165-189` | Pass a representative `fixed` or let the plan truncate |
| Pruning orders by `created_at, rowid` rather than `sequence` | `pruning.rs:112-116` | Order by `sequence` |

Known still present: settings `unwrap_or_default` per turn, `response.text()` no cap, len/4 tokens, content-edit trigger wipes memory, "remember" notes are `role=user`, space prompt drops global citation rules, in-turn compaction has no cancel token. Clean: telemetry opt-in only, no third-party calls, deep research honours depth/branch.

### 3.4 LLM engine and model management

Process lifetime is solid. Recovery and time are not.

| Finding | Where | Fix |
|---|---|---|
| Crashed sidecar never restarted (3) | `sidecar_manager.rs:2636-2650`, `model_cache.rs:40-50` | On `Terminated` after readiness, invalidate the role caches |
| 120 s non-stream cap (5) | `sidecar_client.rs:71, 465-474` | Use the caller's budget as-is; 120 s only when none is set |
| First token must arrive in 60 s (30 s legacy) with no prefill progress; times out on CPU-only Windows and 32k windows on base Macs | `sidecar_client.rs:62, 553, 596` vs `llama_cpp/mod.rs:157-161` | Send `return_progress: true` and treat progress frames as liveness, as the remote adapter does |
| Non-tool local models on the legacy decoder (4) | `factory.rs:488`, `tool_loop.rs:272-274`, `sidecar_client.rs:321-356` | Gate on `supports_typed_completions()` alone |
| Model over 70 % of VRAM gets the 8192 default instead of the 4096 floor (`checked_sub` → `None`) | `sidecar_manager.rs:291, 342-350` | Return `Some(AUTO_MIN_CONTEXT_SIZE.min(ceiling))` |
| Each sidecar sizes its context against the whole GPU budget; chat + a different utility GGUF on 16 GB swaps | `sidecar_manager.rs:269-291` | Subtract sidecars already in the registry |
| Tool support matched on the exact catalog file name; a catalog rename silently turns tools off for downloaded copies | `factory.rs:282-285`, `curated_models.rs:47-58` | Persist the capability on the downloaded-model row |
| Downloads never hash-verified; resume after the HF file changes splices two revisions | `download_model.rs:728`, `model_management.rs:468`, `engine.rs:372` | `If-Range` with ETag; verify LFS sha256 |
| Sidecar error bodies and SSE payloads copied verbatim into user-visible errors and logs | `sidecar_client.rs:486-491, 349-352` | Reuse `check_status` from llama_cpp; log only the parse error |
| Parallel tool calls replayed as N consecutive assistant messages (low confidence) | `llama_cpp/mod.rs:237-238` | Group into one assistant message |
| Intel-mac sidecar present but unpinned in the lock and mode 644 | `binaries/llama-server-x86_64-apple-darwin`, `scripts/llama-server.lock` | Add the sha, `chmod +x`, verify in CI |

Known: first-run RAM double count fixed; download stall timeout, `if let Ok(Some)` with no else, no cache_prompt/id_slot, single-instance guard all still present.

### 3.5 Data layer, security, platform

CSP blocks script, viewer iframes are sandboxed, the reader window has no IPC capability, nothing runs through a shell, restore refuses `..`.

| Finding | Where | Fix |
|---|---|---|
| Safety copy of a live WAL database (10) | `backup/adapter.rs:483-492` | `VACUUM INTO` as `create_backup` does, or `wal_checkpoint(TRUNCATE)` first |
| Corruption recovery leaves `-wal`/`-shm` (11); recovery is silent to the user | `connection.rs:92-104` | Move all three; surface it |
| Second instance kills the first's sidecar (12) | `main.rs:35`, `sidecar_manager.rs:1109-1134` | `tauri-plugin-single-instance` (or the declared, unused `fs4` lock); reap only sidecars whose parent is dead |
| Page-cache poisoning via the report URL (8) | `browser_reader.rs:130-137, 210`, `page_cache.rs:175` | Take `final_url` from the webview's real URL |
| Remote LLM auth headers in plaintext `settings.json`, sent to the webview on every `get_settings`, exported to any path the renderer names; `clear_all` skips them | `settings.rs:319-377`, `settings/repository.rs:1117-1126`, `security/mod.rs:173-177` | Keyring; redact in `get_all`/export; confine export to the exports path |
| Asset protocol scope is all of `$HOME` and `/Volumes` | `tauri.conf.json` | `$APPDATA/**` plus the library root; extend at runtime |
| Ad-hoc signing, no notarisation, no hardened runtime or entitlements | `tauri.conf.json` | Developer ID, entitlements, notarise in CI |
| Release builds log at `debug` into daily files that are never pruned | `tracing.rs:151, 229` | `info` in release; `max_log_files(7)` |
| Dead DB modules: `performance_indexes.rs`, `rebuild_fts5_index`, `DatabaseUtils`, uncompiled `fts5_migration.rs`, stray `001_create_tags_tables.sql`, `test_edge_cases.md` | as listed | Delete |
| Unused crates `memmap2`, `docx-rs`, `fs4`, `html5ever`, `ego-tree`; `md5` only in tests; `bundle:python` points at a missing script | `Cargo.toml:92-157`, `package.json` | Remove or use |
| Init migration was rewritten on 09-20 (commit df51b71d); any DB that applied the 09-16 version fails the sqlx checksum. `idx_memory_attributes_scope` leads with `scope`, so a space-delete cascade scans | `migrations/20260916000000_init_schema.sql`, `20260923000000:15` | Never again; add an index on `(space_id)` |

Known: `journal_size_limit` absent, `clear_all` discards results, every sqlx error → "Database error", `plugin_restore_backup` hard-codes `restored_count: 0`. Fixed: crash dir, rollback logging, Cmd-Q shutdown (not traced end to end). CI builds a release binary but never bundles, signs or notarises.

### 3.6 Frontend

Event subscriptions are sound. The defects are error paths and per-token re-renders.

| Finding | Where | Fix |
|---|---|---|
| Route fallback on "not found" (13) | `src/lib/api.ts:437-475` | Drop the chain, or match Tauri's exact unknown-command text and rethrow the first error |
| Retrieval trace dropped on long search errors (15) | `kb_retrieval.rs:243-246`, `conversation.ts:396` | `safe_truncate` in Rust or `.transform` in zod |
| Failed status poll drops attachments (14) | `ChatPanel.tsx:707-749` | `continue` on transient errors; null + toast on a real one |
| Every streamed token re-renders all 31 context consumers and every unmemoised, unvirtualised `Message` | `useConversationsController.ts:944-953, 1325`, `conversationsStore.ts:21-25`, `ChatPanel.tsx:1233`, `Message.tsx:77` | Streaming partials behind a zustand selector, `memo(Message)`, virtualise past ~50 (`@tanstack/react-virtual` is already a dependency) |
| Per-conversation query observers only grow; deleted conversations are refetched | `useConversationsController.ts:142-152, 410-424, 1259-1263` | Prune on delete; LRU cap |
| An open conversation the list filter hides cannot be deleted ("Conversation not found") | `useConversationsController.ts:1241-1245` | Accept the detail cache, or let the backend decide |
| Command-palette actions fail silently after the palette closes | `CommandPalette.tsx:150-179` | `toast.error` |
| Download listener never attached if the first list fetch fails; events between fetch and listen lost; a late `downloading` snapshot reverts a Completed row | `useDownloads.ts:86-96, 179-227` | Listen first; terminal-state guard |
| Composer blurs on every send and never refocuses; no drafting during a 30-minute turn; palette is not a modal dialog | `ChatPanel.tsx:198-200, 1322`, `CommandPalette.tsx:252-256` | `readOnly` or block submit only; refocus on the `isSending` edge; `Command.Dialog` |
| Dead: `progressStore.ts` (421 lines, no writer), `useProgressCleanup`, five unreferenced `EventSchemas` namespaces | `src/stores/progressStore.ts`, `App.tsx:29`, `events.ts:83-265` | Delete |
| Warm-up and role-refresh failures unreported (ApiResult never rejects; `try/finally` without catch) | `useDownloadedModels.ts:165-172`, `ModelRolesContext.tsx:60-70` | Check `ok`; add a catch |
| No tests for `useDownloadedModels`, `useIndexingStatusQuery`, `useJournalEntries`, `useSpaceEditor`, `useAggregatedDownloads`, `useModelCatalog`, the vault listeners, `api.ts` routing, `handleImportStagedFiles` | — | Add |

## 4. Not covered

Verification (`chat/verification/**`, logprobs fields, the frontend verification files) was mid-edit and excluded. Also skipped for budget: study/summaries/daily-notes callers, synthesis map/reduce, `/commands` and `@mention` parsing, history and attachment tools, page-cache eviction, the sparse service internals, the reranker model code, corpus_shape clustering quality, Ollama and cloud adapters beyond key headers, updates/health/huggingface adapters, download cancel paths, the 1,431-line init schema's triggers, `function_calling/test_custom_tool` (possible SSRF via user tools), Windows and Linux first-run paths, Journal, ReferenceInbox, Ingest, the FileBrowser store, TiptapEditor, and contrast tokens.

## 5. Suggested waves

1. **Correctness on the daily path** (fix-first 1–7, 13–15, 17–18): extractors, `pending` sweep, sidecar restart and timeouts, legacy decoder gate, malformed tool calls, web-import publish, the three frontend error paths. Independent files; four parallel tracks.
2. **Security and data** (8–12, cookie jar, plaintext headers, asset scope): web reader, web client, backup/connection, single instance, keyring.
3. **Measurement** (16): point the eval at chat retrieval and gate CI on it, then the rest of the Phase 1 measurement plan from the 09-24 roadmap.
4. **Performance and dead code**: the rest of §3, in any order.
