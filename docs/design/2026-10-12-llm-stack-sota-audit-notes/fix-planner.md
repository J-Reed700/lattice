# Fix: the corpus planner on a 9B with a 379-document vault (2026-10-13)

Defect §2 of `evals/retrieval/2026-10-12-SHIPPED-PATH-BASELINE.md`. The planner answered 0 of 16 requests: 12 were refused as larger than the 16,384-token window and 4 timed out at 20 s. One timeout stood the planner down for 10 minutes, so 26 of 41 turns never asked, and the corrective retry could never replan.

Worktree `/Users/josh/Code/lattice-fix-planner`, branch `fix/planner`, uncommitted. Paths below are under `crates/lattice-desktop/src/features/conversation/chat/retrieval/`.

## What was wrong, from the request log

The 16 planner requests in `2026-10-13-shipped-path-desktop-path-v1.jsonl.calls.jsonl.gz` carried 51,917 to 61,148 characters each. They showed 73 to 86 of 379 documents, at 700 to 716 characters a row. Those rows were the first 80-odd files in filename order, whatever the question. The budget came from a chars/4 estimate capped at 16,000 tokens. It did not count the UUIDs at their real cost: Qwen's tokenizer splits digits one by one, so a UUID is about 30 tokens.

The same log gives the reference machine's speeds. Prefill ran at a median 296 tokens/s (156 to 404, 92 calls over 500 prompt tokens). Generation ran at a median 14.3 tokens/s (238 calls).

## What changed

- **Token budget** (`corpus_plan/prompt.rs`, new).
  - `build` (line 243) sizes the prompt against two limits. The hard limit is the window less `PLANNER_OUTPUT_TOKENS` (384, line 29) and `TEMPLATE_FRAMING_TOKENS` (32, line 33). The soft limit is the prompt target from `health`.
  - Rows are chosen on the port's calibrated estimate. The finished prompt is then counted with llama-server's own tokenizer (`count_tokens_exact`, `POST /tokenize`, LRU-cached). If the count reads high, rows are chosen again at the measured rate, at most twice.
  - Without a tokenizer, or when it fails, only 4/5 of the window is spent (`spendable`, line 54).
  - A prompt that does not fit the window is never sent. It first retries with no rows, then returns an error and the turn plans locally.
  - The request now sets `max_output_tokens` (`corpus_plan.rs:566`).
- **Short handles** (`prompt.rs`, `render` at line 150). Rows are named `d1`, `d2`, ... in reading order and books `g1`, ... instead of UUIDs. The answer's handles are mapped back to real IDs before validation (`corpus_plan.rs:610-621`). Anything else passes through, and `validate` drops what is not in the catalog. Four openings now cost about 12 output tokens instead of about 120, which saves about 8 s at 14 tokens/s.
- **Leaner rows** (`prompt.rs:38-47`). The opening is clipped to 300 characters (it was 600), with up to 8 headings (it was 24). Conversation is limited to a fifth of the target, at most 512 tokens (`recent_history`, line 139).
- **Shortlist** (`corpus_plan/shortlist.rs`, new).
  - `candidate_documents` (line 44) runs the same fused hybrid search first-pass retrieval uses (`fused_search`, 40 passages). It searches the request alone and, when there is conversation, the request plus 12 informative terms from it. The two rankings are interleaved.
  - `shortlist` (line 145) orders the catalog for the planner (rule below).
- **Timeout policy** (`corpus_plan/health.rs`, new). This replaces the 10-minute `PLANNER_STOOD_DOWN_AT` static.
  - `PlannerHealth` (line 43) is process-wide and holds the prompt target (`PLANNER_PROMPT_TOKENS` 2,560, floor 1,024).
  - `PlannerTurn` (line 122) is per turn and carries the candidates and a timeout flag.
- **Planner entry points** (`corpus_plan.rs`).
  - `plan` (434) and `plan_correction` (458) take a `&PlannerTurn`.
  - `plan_with` (508) checks the turn and the health, builds the shortlist prompt, logs `Corpus planner prompt` (rows shown, tokens, exact or not, limit), and records a timeout, a window refusal (`is_window_overflow`, 503) or an answer.
  - One sentence in `PLANNER_SYSTEM` (line 96) says a catalog smaller than `catalog_total` is a search shortlist.
  - The comments at lines 31-40 and 91-93 are updated, and a doc comment that had drifted onto `PLANNER_TIMEOUT` is back on `CORRECTION_INSTRUCTION`.
  - The file went from 919 to 867 lines.
- **Turn wiring** (`kb_retrieval.rs`).
  - One `PlannerTurn` per turn (line 144).
  - `planned` (511) runs the shortlist search, then the planner, and falls back to `fallback_with_catalog` on any failure.
  - `shortlist_candidates` (558) searches once per turn.
  - `corrective_pass` (583) gives way only when this turn already timed out or the planner is standing down (line 600).
  - The rule that the planner runs only when a utility model is assigned is unchanged.

## The shortlist rule and cap

1. Take the candidate documents best first. Each hit brings its reading-order neighbours, the documents either side of it in its book (`source_context` group, in catalog order) or its filename series (`numbered_series_prefix`).
2. For a request to learn, start at the beginning, or get an overview (`whole_document_intent`), the first hit in each book or series also brings that book's front matter: the introduction, the first chapter, and the first document that is not a contents page or change log.
3. A catalog of 20 documents or fewer is shown whole, ranked rows first. With no ranking at all (search failed or empty) the catalog keeps its old order.
4. Rows are taken from the front until the token budget or `SHORTLIST_CAP` (20) runs out, then shown in catalog order.

`ordered_learning_plan` and `validate` still receive the whole catalog. A start-at-the-beginning plan therefore finds the introduction and first chapter from every document in the book, including ones the planner was not shown. A test checks this.

**Token arithmetic for the default target (2,560 prompt tokens).**

| Part | Tokens |
|---|---|
| `PLANNER_SYSTEM` (1,519 characters) | about 380 |
| JSON frame and the request | about 60 |
| Conversation excerpt | 0 to 512 |
| Each row (median 424 characters re-rendered from the log's catalog) | about 110 to 120 |
| Rows that fit | 15 to 17 with no conversation, about 13 with a full excerpt |

**Time on the reference machine.**

- Prefill: 2,560 tokens at 296 tokens/s is 8.6 s.
- Writing: a full plan (three queries, four handles, pretty-printed JSON) is about 100 tokens, 7 s at 14.3 tokens/s. A typical plan is half that.
- Total: about 15.6 s at worst against the 20 s timeout. That leaves about 4 s for a slower prefill or a queue wait.
- The window is not the binding limit at 16K. 2,560 + 384 + 32 tokens fits any window of 3K or more.
- The cap of 20 rows binds only when rows are unusually short.

**After a timeout.** The target halves, down to the 1,024 floor. A floor prompt is about 3.5 s of prefill plus up to 7 s of writing.

## Timeout policy, by failure kind

- **Window overflow.** This cannot happen while the window the port reports is right, because the prompt is counted against it before sending. If the server still refuses (`exceed_context_size`), the window report was wrong. The target halves, and there is no stand-down, since a refusal costs no time.
- **Timeout.**
  - The next attempt's prompt halves (2,560 to 1,280 to 1,024).
  - The turn that waited does not ask the planner again, so a turn pays at most one 20 s timeout. That turn's corrective retry keeps its first-pass evidence.
  - Later turns, and their corrective retries, plan at the smaller size.
  - Only a timeout at the 1,024-token floor stands the planner down, for 2 minutes (`PLANNER_STAND_DOWN`). That covers the first pass and the retry.
- **Answer in time.** An answer ends any stand-down. An answer within 10 s doubles the target back toward 2,560. A slower answer keeps the size that worked, so the planner does not swing between a size that times out and one that barely does not.
- **Other failures** (unparseable JSON, no queries, a cancelled turn): no change to health. The turn falls back as before.

## Verified

All commands ran from `crates/lattice-desktop` with `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2`, `jobs = 2`.

- `cargo check --lib`: clean, no warnings.
- `cargo test --lib -- conversation::chat::retrieval`: 98 passed, 0 failed, 1 ignored (the live test that needs a model).
  - That includes 13 new tests in `corpus_plan/budget_tests.rs`: token budget, shortlist and cap, handles, learning plans, and the timeout policy.
  - It also includes the existing planner, summary and correction tests, which now pass a `PlannerTurn`.
  - The test stubs estimate at 4 characters a token and count "exactly" at 3.5, the gap that caused the overflows.
- New tests cover these cases:
  - **Large vault.** A 379-document vault with UUIDs stays within 2,560 exactly-counted tokens. It shows hits and neighbours, not `Record 000`, and no UUIDs. The tokenizer is asked, and `max_output_tokens` is 384.
  - **Shortlist order.** Hits come first, then their neighbours, and the cap of 20 binds when rows are cheap.
  - **Handles.** `d2` and `D1` map back to real IDs, and an unknown ID is dropped.
  - **Learning plans.** A learning request's shortlist carries the introduction and chapter 100 of the hit's book. The validated plan also opens chapter 200, which the planner was never shown.
  - **Small window.** A 2,048-token window on an estimate-only port keeps the server-counted prompt plus 384 output tokens inside the window.
  - **No fit.** A 600-token window sends nothing and learns nothing.
  - **Timeout.** A timeout halves the next prompt with no stand-down, and the next turn's prompt is smaller.
  - **Corrective retry.** The retry does not call the planner on the turn that timed out. It replans on the next turn.
  - **Floor and recovery.** Only a floor timeout stands down, and for 2 minutes (paused tokio clock). A quick answer earns back room and a slow one keeps it.
  - **Window refusal.** It shrinks the prompt without a stand-down.
  - **Bad JSON.** It still falls back to the local plan.
- `cargo fmt`, then `rustfmt --check --edition 2021` on every touched file: clean.
- `bash scripts/check-rust-layer-boundaries.sh`: clean. The first run flagged two new `features::search` references in `shortlist.rs`. They now come through the parent module's existing imports, so the `conversation -> search` count is unchanged.
- `bash scripts/check-repository-barrier.sh`: clean.
- `cargo clippy --lib`: clean, no warnings or errors.

## Not verified

- **No live run.** No server or model was started. "Corpus-planner requests answered N of 16" and the retrieval p95 need the coordinator's re-run of the shipped-path eval.
- **Row cost is estimated.** It was not measured with Qwen's tokenizer, since nothing could be downloaded or run. It comes from character counts and the log's 400 refusals (under 3.8 characters a token).
- **Shortlist quality is untested on real search.** It was not measured on the v3 set, and the tests use a fixed ranking.
- **Lint and test targets.** `cargo clippy --all-targets`, which CI runs, was not run (not allowed here). Integration tests were not run.
- **Concurrent tokenize.** It is not confirmed that `/tokenize` answers while the single slot is busy. llama-server tokenizes in the HTTP handler, and `memory_context.rs` already relies on that every turn.

## Risks

- **The planner only knows what local search found.** If search misses a document, the planner cannot open it, though its queries still search the whole scope.
- **Extra searches.** Each planned turn costs one extra hybrid search, or two with conversation. Each search embeds its query.
- **Handles.** A 9B may answer with a document name instead of a handle. Such IDs are dropped, as invalid IDs were before.
- **Less detail per row.** Openings are clipped to 300 characters and headings to 8, so the planner has less to tell similar documents apart.
- **Tuned for the reference machine.** The 2,560 target suits that machine. A faster machine is not given more room; the target never grows past the default.
- **Shared health.** The planner health is process-wide and shared between the first pass and the correction. A model switch inherits a shrunken target until a quick answer restores it.
- **Overflow detection.** `is_window_overflow` recognises only llama-server's `exceed_context_size_error`. Other providers' overflow errors just fall back, as any error does.
- **Unchanged by design:** the planner and the corrective retry still run only when a utility model is assigned (`kb_retrieval.rs`, the `utility_llm_loader` branch). With the install-default roles, neither runs. Changing that is a product decision outside this task.
- **Not addressed:** defect §5. A `corrective_search` step is still recorded when the retry gives way.

## Suggested commit

```
Shortlist and token-budget the corpus planner so a 9B can answer it

The planner was sent the first eighty-odd catalog rows in filename order,
sized on chars/4 with UUIDs at full price, so on a 379-document vault every
request overflowed the 16K window or missed its 20 s timeout, and one
timeout switched planning and the corrective retry off for ten minutes.

It now sees a shortlist: the documents a hybrid search ranks for the
request, their reading-order neighbours, and for learning requests the
front matter of their books, named by short handles. The prompt aims at
2,560 tokens, is counted with the server's tokenizer, and is never sent
past the window. A timeout halves the next prompt; only a timeout at the
1,024-token floor stands the planner down, for two minutes. A turn pays
for at most one timeout, and later turns' corrective retries still
replan.
```
