# Where Lattice stands, and the road to a 2026-grade release

Date: 2026-09-24. Branch: `conversation-memory-release-readiness` (42 commits ahead of `main`, 16 files still uncommitted).

## 1. Verdict in one paragraph

The architecture is current. The retrieval core, the evidence-first chat surface, the provenance-backed memory and the archived web evidence are all designs a 2026 team would recognise as the right shape, and the retrieval half has real numbers behind it. The app works in daily use. What is not current is the **default configuration** and the **measurement**: the shipped local model runs with tool calling off, the reranker and sparse branch are off, token budgets are estimated as characters divided by four, the grounding checker is a lexical pass plus a one-window judge, and only retrieval has an eval that a change can move. The remaining work is optimisation and fixes: turn on, measure, and finish the modern things already built.

## 2. Scorecard

| Area | State | Grade | Evidence |
|---|---|---|---|
| Retrieval core | Hybrid dense + lexical, one canonical weighted RRF, 480-token boundary-aware chunks, Qwen3 embeddings, exact scan under 20k vectors, index manifest so startup loads instead of rebuilds | **Current** | v3 recall@5 0.776 / nDCG 0.693 (`evals/retrieval/2026-09-19-V3-MODERNIZATION-RESULTS.md`) |
| Retrieval defaults | Reranker off (only MiniLM downloadable), sparse branch off (`SPARSE_BRANCH_ENABLED = false`), no neighbour chunks, no lost-in-the-middle ordering, `document_evidence` always empty | **2023 defaults** | `features/search/di.rs:42`; `rag-sota-audit-2026-09-22` |
| Agentic loop | Tool loop, agentic search and history tools exist, but `supports_tool_calling` is hard-coded `false` for the bundled sidecar, so the default install never runs them. "Deep research" is a fan-out of searches and page fetches, not a loop | **Built, switched off** | `features/llm/engine/factory.rs:477` |
| Context engineering | Bounded memory with byte-range provenance; assembler with eviction; but tokens are `len / 4`, no `/tokenize`, no `cache_prompt` / `id_slot`, prompt prefix ordering not designed for KV reuse | **Half** | `factory.rs:469`, `llama_cpp/mod.rs:277` |
| Grounding check | Lexical overlap + utility-LLM judge; verdicts stored per sentence with evidence quote and method; sampling now pinned to temperature 0. Still one window per source, unjudged claims flipped to unsupported, no `unverified` state, no eval set | **Rudimentary, proposal written** | `docs/design/2026-09-24-grounding-verifier-modernization.md` |
| Chat surface | Claim marks, evidence margin, docked reader, turn record with steps and timing, composer `/` and `@`, export. Ahead of most products | **Current** | `docs/design/2026-09-19-chat-research-workbench.md` |
| Web evidence | Pages archived at citation time, page cache, article extraction, hidden-webview fallback, text-fragment open links | **Current** | `docs/design/2026-09-22-citable-web-evidence.md` |
| Evaluation | Retrieval evals v1–v3 with a CI integrity check. **No end-to-end answer eval**, no grounding-verdict eval set, memory evals all `#[ignore]` and never run with weights | **Retrieval only** | `evals/retrieval/`, `src-tauri/tests/conversation_memory_evals.rs` |
| Ingestion | Text, DOCX/ODT/XLSX/PPTX, PDF text layer, HTML, transcription. OCR is a no-op port; no layout or table extraction | **2022** | `application/ports/ocr_port.rs` |
| Local inference | llama-server sidecar pipeline with pinned, verified binaries; auto `n_parallel`; `--ctx-size 32768`; no `--jinja`, no flash-attention flag, no draft model | **Solid, conservative** | `sidecar_manager.rs` |
| Robustness | Lints deny panics; two panics and the >32k-document failure fixed; ~20 spawned loops still die silently; unbounded RAM on index rebuild; no single-instance guard | **Good core, known list** | `rust-robustness-audit-2026-09-19` |
| Release | CI runs check/test/clippy/fmt/bindings but never `tauri build`; ad-hoc signing (`signingIdentity: "-"`); no updater plugin; no Intel-mac sidecar; candle CPU-only off macOS | **Not releasable** | `.github/workflows/ci.yml`, `src-tauri/tauri.conf.json` |

## 3. Game plan

Each phase has an exit criterion. Phases 2 through 5 are independent of each other and can run as parallel Opus tracks once Phase 0 and 1 are done (see the ownership pattern in `docs/design/2026-09-19-chat-research-workbench.md` §2).

### Phase 0. Land the branch (a day)

1. Commit the 16 modified files (judge concurrency, sampling override, verification settings, Tauri version bump) and open the PR to `main`.
2. Confirm in the log that the judge line shows `judged == requested` inside the 90 s budget on a normal turn.

Exit: the branch is merged.

### Phase 1. Measure the answer, not just the retrieval (1 week)

1. **Answer eval set**: 60–100 questions over the v3 fixture vault with gold answers and gold chunk ids. Score citation precision/recall against gold chunks, and answer correctness with a strong cloud judge through the existing cloud adapter (dev-only, never in the product).
2. **Grounding eval set**: 150 hand-labelled (claim, passage, verdict) triples drawn from real turns in `conversation_messages.metadata.verification`. This is Phase 1 of the verifier proposal and gates every verifier decision.
3. **Memory eval**: run the ignored `conversation_memory_evals` with weights and record the numbers.
4. Wire all three into `evals/` with the same integrity check the retrieval evals have.

Exit: three numbers on `main` that a change can move. Without this, every later phase is an opinion.

### Phase 2. Turn on the agentic loop for local models (1 week)

1. Add a tool-support field to the model catalog and make `supports_tool_calling` read it. Qwen3.5 models support function calling under llama.cpp with `--jinja`; pass it in `build_server_args`.
2. Bring the tool loop's `semantic_search` to parity with first-pass retrieval (same fusion weights and k).
3. Stop the intent classifier from disabling vault search when it wants the web; web should add to, not replace, the vault.
4. Make deep research a real loop: plan, search, read, decide whether to search again, with the turn record already able to show rounds.

Exit: a local-only install answers a multi-hop question with two or more tool rounds visible in the turn record, and the answer eval does not regress.

### Phase 3. Grounding verifier, phases 2 and 3 of the proposal (1 week)

Utility model only, no new weights:

1. Per-claim evidence: top 3 windows per cited source by embedding similarity, verdict is max over windows, min over sentences.
2. Per-claim yes/no with a document-first prompt and `n_probs`, so a verdict carries a probability and the UI can show confidence.
3. Add `unverified` to the Rust enum, the zod enum in `src/types/conversation.ts`, and `ClaimHoverCard`. Unjudged claims keep their lexical verdict with `method: lexical` instead of flipping to unsupported.
4. Move verification off the turn's critical path: emit `done` when the answer finishes, patch verdicts when the judge finishes.

Exit: verdict agreement with the Phase 1 grounding set at or above 0.8 balanced accuracy, and "Not found in the cited passage" false-negative rate under 10 percent on that set. Only then decide whether a dedicated checker (LettuceDetect in candle) is worth adding.

### Phase 4. Token-accurate context and KV reuse (3–4 days)

1. Real token counts: llama-server `/tokenize` behind a small cache, or the GGUF tokenizer in-process. Replace every `len / 4`.
2. Prompt layout for cache hits: system prompt, then space/document context, then history, then the new turn, so the sidecar's prefix cache holds across turns. Set `cache_prompt` and a stable `id_slot` per conversation.
3. Lost-in-the-middle ordering: strongest evidence first and last. Wire `document_evidence` so evidence-first eviction runs.
4. Delete the settings nothing reads (`chunk_size`, `chunk_overlap`, `hybrid_search_alpha`, `recency_boost`, `doc_support_*`).

Exit: measured time-to-first-token on turn 5 of a long conversation drops, and the RAG budget never overflows the window (log the real count against the limit).

### Phase 5. Retrieval wave 2 (1 week)

1. Re-benchmark reranking with a modern reranker over a deep pool (k=50): Qwen3-Reranker-0.6B and bge-reranker-v2-m3, on v3. Ship whichever wins if it wins by more than 0.02 nDCG at under 300 ms median.
2. Decide the sparse branch on v3 numbers; delete it if it does not pay.
3. Neighbour chunks in reading order for the top hits.
4. Re-chunk already-indexed libraries when `chunking_policy_version` changes.
5. Page the index rebuild so a 100k-chunk library does not hold ~1 GB in RAM.

Exit: v3 nDCG above 0.72 or a written note saying why it is saturated.

### Phase 6. Ingestion that reads what people actually store (1–2 weeks)

1. OCR through a real port: Apple Vision on macOS, Tesseract sidecar elsewhere. Scanned PDFs are the single biggest class of silently empty documents.
2. Layout-aware PDF: detect tables and keep rows together as chunks. Evaluate `pdfium` bindings before writing a parser.
3. Uncompressed-size caps on zip-based formats and a byte cap on web responses (from the robustness audit).

Exit: a scanned PDF and a table-heavy PDF in the v3 fixture retrieve as well as their text-layer twins.

### Phase 7. Releasable (1 week, can overlap with 5 and 6)

1. CI runs `tauri build` on macOS, Windows and Linux.
2. Real code signing and notarisation on macOS; the Tauri updater plugin with a signing key.
3. Single-instance guard, so a second launch does not kill the first one's sidecar.
4. Intel-mac sidecar in `scripts/llama-server.lock`, and a `cuda` cargo feature so Windows/Linux embedding is not CPU-only.
5. Supervise the remaining ~20 spawned loops and finish the robustness list.

Exit: a signed build installs on a clean machine of each platform and updates itself.

## 4. What not to do

Research from the 09-22 audit still holds: skip semantic and proposition chunking, GraphRAG-style graph building, HyDE as a default, and fixed Self-RAG or CRAG pipelines. Do not add a second resident model until the Phase 1 grounding set says the utility model is not enough; the machine carries one 9B utility model plus the chat model and that is the memory ceiling. 

## 5. Sequencing summary

| Order | Phase | Why here |
|---|---|---|
| 1 | 0 Land the branch | Later phases branch from `main` |
| 2 | 1 Measure | Every later phase needs a number to move |
| 3 | 2, 3, 4 in parallel | Independent code areas: llm engine, verification, context assembly |
| 4 | 5, 6, 7 in parallel | Search, indexing, and build pipeline do not overlap |

Estimated calendar: six to eight weeks with parallel Opus tracks on the independent phases, under the token-cap and no-sub-agent rules from `rust-robustness-audit-2026-09-19`.
