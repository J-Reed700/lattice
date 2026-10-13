# Integration notes: tracks A, B1, B2, D, F, H

These edits are unstaged, so `git diff` shows only the integration work on top of the six staged tracks. Nothing is committed.

## Wiring

1. **Thinking on the answer round.**
   - `tool_loop.rs:246` reads `answer_thinking` through `container.settings()`.
   - When it is on, `:343` calls the new `final_round::ask_for_answer_without_tools` (`final_round.rs:45`). That round sends no tools and no `tool_choice`. When it is off, B2's keep-tools round is unchanged.
   - `:351` sets `ThinkingMode::for_answer` on every round.
   - B2's retry is kept in both cases.
   - Tests are at `final_round.rs:351` and `loop_tests.rs:396`, and `FakeRuntime` now has an `answer_thinking` field.
2. **Markers.** The retry and the notes request send `ThinkingMode::Off` (`final_round.rs:57`, `research_notes.rs:166`). No `integration:` markers remain.
3. **MTP.** `factory.rs:227` sets `config.mtp_capable = chat_model_uses_mtp_draft(model_path)`.
4. **B1.** `memory/mod.rs:211` now keys memory calls `memory:{conversation_id}`. `handoff.rs` keeps the chat's own key on purpose.
5. **Router default.** It is now `qwen3.5-4b-q4_k_m` (`settings.rs:994`; the id is at `curated.rs:306`). The validation hints (`settings_port.rs:570`, `validation.rs:351`) and `latticeApiMock.ts:104` are updated to match. No test asserted the old id.
6. **F.** Its two modules: 127 passed, 1 ignored (live). Memory design §15 (`2026-09-19-conversation-memory.md:664`) now describes the schema-only approach.
7. **`preserve_order` is enabled** (`Cargo.toml:63`). The lock gains `indexmap 2.14.0`, which was already resolved.
   - The audit found nothing that depends on sorted keys:
     - no code iterates a `Map` expecting sorted keys;
     - `query_cache` sorts explicitly;
     - `IndexMap` equality ignores order;
     - no `HashMap` becomes a `Value` on a prompt or hash path.
   - Two side effects:
     - Digests of `json!` values, such as `content_verification` reuse, change once.
     - `Map::remove` becomes `swap_remove`, which is still deterministic.
8. **H.** Compiles alongside the other tracks; no edits.
9. **Bindings.** Already up to date; no regeneration was needed.

Empty, git-ignored placeholders were created: `src-tauri/binaries/llama-server-aarch64-apple-darwin` and `dist/`.

## Commands

All cargo commands ran with `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2`.

- `cargo check --lib`: clean, no warnings.
- `cargo test --lib -- llm_port llama_cpp conversation::chat context_assembler learning::lessons::content_verification learning::lessons::course_generation_tests models::curated settings conversation::memory engine::factory features::learning features::ocr --test-threads=4`: **1096 passed, 0 failed, 26 ignored**. This was one run, covering the brief's batch and F's re-run, plus the files I touched and `preserve_order`.
- `cargo run --features bindings-export --bin export_bindings -- --check`: up to date.
- `npx tsc --noEmit -p .`: clean.
- `npx vitest run` on `AITab.test.tsx`, `ThinkingSection.test.tsx` and `computeModelFit.test.ts`: **40/40**.
- `rustfmt --check` on the touched files: clean.

## Follow-ups

- **B1 summary watermark:** refold only when the window start moves.
- **D context sizing:** `kv_cache_bytes_per_token` counts about 4x too much for Qwen hybrids and stops at Gemma 4's per-layer KV-head array.
- **H OCR marker:** nothing marks a chunk as OCR-derived; adding one needs a migration.
- **E1 tool definitions:** `budget.rs` reserves no room for them.
- **`preserve_order` coverage:** the full run should watch tests outside this filter for assertions that depend on key order.
- **Thinking answers:** they re-read the whole prompt on the last round. That is by design.

# Pass 2: final wiring, verification and commits (2026-10-13)

All cargo commands ran with `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2` and the worktree's `jobs = 2`, one at a time. No llama-server ran and nothing was downloaded.

## Wiring

1. **Settings migration (A and C).** The version-2 migration (`file_store.rs`, `release_untouched_answer_sampling`) moves answer sampling still on the old defaults to the card profile: top-p 0.9 → 0.8, top-k 40 → 20, repeat penalty 1.1 → 1.0. Each value moves only if it is exactly the old default. Presence penalty 1.5 fills in on read through its serde default. Test: `version_one_sampling_defaults_migrate_to_the_card_profile`, next to C's.
   - **Departure from the brief:** the brief listed temperature 0.9 as an old default. The old temperature default was 0.7, the same as the new one, so temperature is left alone. The 0.9 was top-p.
2. **E1 compiles.** `cargo check --example shipped_path_eval` was clean with no changes.
   - The recorder proxy does not assert on the body. It adds `enable_thinking: false` only when a request has no switch.
   - A now sends the switch on every request, so the proxy forwards it untouched, records it under `chat_template_kwargs`, and logs `thinking_switch_added: false`.
   - The recipe's `thinking: "off"` is now a no-op.
3. **E2 compiles.** `cargo check --example claim_checker_eval` was clean with no changes.
4. **C and H DI.** `cargo check --lib` was clean. No wiring test enumerates bindings beyond the lib suite, and the lib suite passed.
5. **Markers.** `grep -rn "integration:" src-tauri/src src` returns one hit: the doc comment `HuggingFace integration:` in `features/huggingface/mod.rs`, which is already on main and is not a marker.

## Fixes found by the full run

- **Clippy.** CI runs `cargo clippy --all-targets`, which denies `indexing_slicing`. It found five unchecked indexes:
  - `tool_loop/calls.rs` (`batch_end`, `prepare`);
  - `tool_loop/research_notes.rs`, twice;
  - `examples/shipped_path_eval/main.rs`.

  They now use `get()`. Five test-only lints were also fixed: two `expect_err`, a needless clone, a `vec!` and a default-then-assign.
- **Snapshots.** With `preserve_order` on, six insta snapshots changed key order:
  - `audit_events_test__audit_result_variants`;
  - five `indexing_output_test__*`.

  Each new snapshot was parsed and compared as JSON, and each is equal to the old one. They were accepted by moving the `.snap.new` files into place, because cargo-insta is not installed. The two targets were then re-run on their own.

## Verification (run once, in order)

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo check --all-targets` | clean, no warnings (only the build script's empty-sidecar notice) |
| `cargo clippy --all-targets` (CI's form; CI does not pass `-D warnings`) | 5 denies, fixed; then 0 warnings, so `-D warnings` would also pass |
| `cargo test --lib --bins --tests --no-fail-fast` (CI's targets; doctests are off in Cargo.toml) | lib 4238 passed, 0 failed, 67 ignored; integration 257 passed, 6 failed (snapshot key order) |
| `cargo test --test audit_events_test --test indexing_output_test` (re-run after accepting) | 17 + 12 passed |
| `npx tsc --noEmit -p .` | clean |
| `npx vitest run` | 248 files, 2079 tests passed |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | 99 OK |
| `python3 scripts/validate_rag_evals.py` | OK (v2, v3, v1-answers, desktop-path-v1) |
| `python3 -m unittest src-tauri/scripts/test_verify_llama_binaries.py` | 101 OK, 1 skipped |

The bindings check was not re-run: pass 2 changed no exported type.

## Commits

Each track commit carries that track's own version of its files, applied from its patch, so the integration edits land in the wiring commit.

- Files shared between tracks go in full with the first batch:
  - `curated.rs` with D;
  - `llm_port.rs` and `settings.rs` with A;
  - `evals/retrieval/README.md` with C;
  - `chat.rs` with E1.
- C's commit carries the final settings migration, including the sampling move.
- `Cargo.toml` and `Cargo.lock` are split:
  - F's commit holds only the `preserve_order` change;
  - H's commit adds its macOS-only dependencies.
- The track-*.md reports go in the docs commit with the other notes, not in the track commits.

Commit subjects, in order:
1. Refresh the chat catalog to pinned, current-generation GGUFs
2. Switch thinking explicitly and sample each mode by the model card
3. Keep tools on the last round, run reads together, condense research
4. Keep the prompt prefix stable across turns
5. Measure retrieval defaults on v3 and keep only what held up
6. Reason before the schema in learning; drop prompt format demands
7. Read scanned PDF pages with PaddleOCR-VL in its own llama-server
8. Measure the shipped chat path with a headless runner and rubric set
9. Add a claim-checker bake-off over 101 labelled pairs
10. Wire the LLM-stack tracks together
11. Record the 2026-10-12 LLM-stack audit and track notes

## Still open

The pass 1 follow-ups still stand, along with the following:
- **E1's three defects:**
  - the budget cannot trim evidence at 8K;
  - the planner never plans on a 9B;
  - intent sends vault questions to the web.
- **E1 baseline:** it was measured on 70cc8c92. Re-running it on this branch is the measurement that shows what the program changed.
- **Track G:** the MTP speed experiment is still running separately.
- **Main moved during this pass.** `origin/main` is now d1fb4c4b (#42, which split 31 backend files and 9 React components). A test merge (`git merge-tree`) conflicts in four files:
  - `contracts/settings.rs` (3 hunks);
  - `curated.rs` (1 hunk);
  - `ollama_client.rs` (1 hunk);
  - `TuningTab.tsx` (2 hunks).

  #42 moved those files' tests into new files, so the branch's test edits there need to move too. Main was not merged here, because a merge needs its own verification run.
