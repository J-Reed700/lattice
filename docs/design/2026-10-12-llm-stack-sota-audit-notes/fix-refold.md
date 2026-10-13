# Fix: the working summary moved on every turn past the window

Branch `fix/refold` (from the integration branch at 0f19c85a), uncommitted.

Defect: `track-b1.md`, "Summary stability". Consolidation refolded the summary after every answer, so it always ended at the watermark. Once older turns had left the window, the summary (message 2) and the window's first turn both changed on every turn. llama-server reuses its KV cache only up to the first changed token, so each turn re-read the whole conversation. The test `a_refolded_summary_ahead_of_the_history_breaks_the_shared_prefix` recorded it.

## The rules

**Coverage.** The summary's end is tracked apart from the extraction watermark. It is the sequence of the summary row's `up_to_message_id`, read through a join. No column and no migration: the row cannot outlive that message (foreign key plus the clamp trigger).

**Anchoring** (assembler step 5):

1. Candidates that reach the first message and fit the recent pool are carried whole, and the summary stays out. This is B1's rule, unchanged.
2. Otherwise, with a usable summary that fits its pool, the window opens at the first message after the summary's end. It never reaches further back, even with room. While the turns after the summary fit, each request repeats the previous one up to its newest answer, summary included.
3. When they no longer fit, the window starts later, filled newest first as before. The plan then reports `summary_refold`: where the window started, and where the summary should end. It reports the same when the candidates begin past the summary's end (the 64-message candidate cap left turns unread).
4. The requested end sits inside the window. The newest half of the window's tokens stays verbatim, moved forward so the next window opens on a user turn. When the candidate list ended the window, half its messages too. The next window starts half empty, so the following turns extend an unchanged prefix until it fills. Each refold breaks the prefix once.
5. The eviction order is unchanged. The refold request is measured after final overflow eviction, which can also move the start.

**Refolding** (compaction job):

1. Consolidation after an answer still extracts on every run. It folds the summary only when the turn's plan asked, and only while the request still applies (`SummaryRefold::is_needed`: the summary still ends before the message ahead of the window). Two turns that ask before the first refold lands cause one refold.
2. A refold covers the messages after the summary's end through the requested sequence, clamped to the new watermark. Messages below the watermark are paged back from the transcript.
3. `/compact` and the automatic compaction a turn waits on fold everything they extract, as before.
4. A commit that does not refold carries the summary row into the new memory revision.

## What changed (file:line, paths under `src-tauri/src/`)

- `domain/conversation/memory.rs:620` `MemorySnapshot::summary_through_sequence`. `:664` new `SummaryRefold { window_start_sequence, through_sequence }`, `:681` `is_needed`.
- `features/conversation/repository/memory.rs:299` snapshot reads `summary_text` and the boundary message's `sequence` through a join. `:953` a commit without a summary update sets the summary row's `memory_revision` to the new revision.
- `application/services/context_assembler/mod.rs:31` module doc. `:302` the summary is rendered and priced before the window. `:323` `whole_transcript_fits`. `:325` `anchored_after`. `:335` the window takes only candidates after it. `:373` step 6 reuses the priced summary. `:520` and `:555` `summary_refold`: gap detection and the half-window target. `:798` `is_user_turn`.
- `application/services/context_assembler/plan.rs:136` `ContextPlan::summary_refold`.
- `features/conversation/memory/mod.rs:449` `CompactionRequest::summary_refold`; module doc on extraction and the summary moving separately.
- `features/conversation/memory/job.rs:521` the fold target and fold window; the summarizer runs only with a target. `:729` `fold_source` pages messages below the watermark. `:843` `fold_target`. `:625` the commit log records where the summary now ends. `:646` the outcome reports the summary in force.
- Plumbing from the plan to consolidation: `chat/turn/assemble.rs:362, :489` (`PlannedRequest`), `:455` (plan log line), `chat/turn/generate.rs:18` (`GeneratedAnswer`), `chat/turn/finalize.rs:25, :110`, `chat/persistence.rs:73, :127`, `chat/ports.rs:178` (`ChatPolicy::consolidate_after_turn` takes the request), `di/chat_runtime.rs:154`, `chat/test_runtime.rs`, `compaction.rs:196`. A resumed research turn passes `None` (`chat/turn/mod.rs:373`).
- `docs/design/2026-09-19-conversation-memory.md`: §24.1 item 7 (deviation from §7.3 step 8) and new §24.8 (coverage, anchoring, refold rules, consequences).

## Tests

Replaced in `context_assembler/tests/prefix.rs`: `a_refolded_summary_ahead_of_the_history_breaks_the_shared_prefix` is gone. New:

- `:256` `consecutive_turns_past_the_window_share_every_message_up_to_the_newest_answer`. 40 long messages on an 8K window, summary through 24. Turns N and N+1 both open at message 25 with the summary at message 2, ask for no refold, and N+1 extends N up to its newest answer. The same request without the anchor starts before 25, so the anchor is what holds it.
- `:312` `the_summary_refolds_when_the_window_start_moves`. Summary through 4: the window no longer fits after it, starts later, and asks for a refold ending on an assistant message with at most half the window kept. With the refolded summary the next request opens right after it, asks for nothing, and the one after extends it.
- `:356` `candidates_that_start_past_the_summary_ask_for_a_refold`. Candidates 31 to 40, summary through 4: a refold is asked for, halved by messages (through 36, not 34).
- `memory/tests.rs:1524` `consolidation_extracts_without_touching_the_summary_when_no_refold_is_asked` (no summarizer call, summary and its end unchanged, watermark moves).
- `memory/tests.rs:1560` `consolidation_refolds_the_summary_over_the_turns_the_window_dropped` (reads messages 3 and 4 back from below the watermark, folds nothing the window still carries, ends at m4; a second request already satisfied makes no summarizer call).
- `memory/tests.rs:1648` `a_refold_never_runs_past_what_extraction_accepted` (work cap stops extraction at 4; the summary ends at 4, not 6).
- `repository/tests/memory.rs:466` `the_snapshot_reports_where_the_summary_ends_and_a_commit_without_one_keeps_it`.
- `domain/conversation/memory/tests.rs:1093` `a_refold_is_needed_only_while_the_summary_ends_before_the_window_gap`.

Unchanged and passing: the other prefix tests (B1's two, `the_summary_returns_when_the_window_no_longer_reaches_the_first_message`), the three eviction tests, and the memory_context tests that check the summary (one comment reworded).

## Verified

All cargo commands ran from `src-tauri` with `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2`.

- `cargo check --lib` → `Finished`, exit 0, no warnings.
- `cargo test --lib -- context_assembler conversation::memory conversation::chat::memory_context conversation::compaction conversation::chat::turn conversation::repository::tests::memory` → `test result: ok. 195 passed; 0 failed; 0 ignored; 4170 filtered out`. No warnings. Every new test above is in that run.
- `cargo fmt` → touched only my two test files (checked first with `cargo fmt -- --check`). Then `rustfmt --edition 2021 --check` on all 22 touched `.rs` files → clean.
- `bash scripts/check-repository-barrier.sh` → "Repository Barrier check: clean (272 production feature files)."
- `bash scripts/check-rust-layer-boundaries.sh` (with the shared target dir) → "Rust layer boundary check: clean (syntax-aware)." Feature edges still 345.
- `cargo clippy --lib` (after the format pass) → `Finished`, exit 0, no warnings or errors.

## Not verified

- No live run and no llama-server, so no `cache_n` measurement. The prefix property is shown on serialized plans only.
- Answer quality with the shorter windows is unmeasured. No eval was run.
- After the test run I moved one closure in `summary_refold` into the `is_user_turn` function, and `cargo fmt` reflowed two test files. Clippy compiled that code; the tests were not run again.
- No integration tests, no `--all-targets`. `tests/conversation_memory_evals.rs` was not compiled; it uses neither changed signature.
- The live consolidation path through `consolidate_after_turn` runs only in the app. The fake runtime ignores the request.

## Risks

- **Less verbatim history.** Between refolds the window holds between half and all of the recent pool. Before, it always filled the pool, overlapping the summary.
- **`/compact` and automatic compaction now shrink the window.** They fold through the extraction boundary as before (keep 4 messages by default), and the window now opens after it. The eval harness compacts that way on every cycle, so its bounded-arm numbers may move when a fixture does not fit whole.
- **Pre-existing summaries end at their last consolidated answer.** In a conversation that no longer fits its window, the first turns after this change carry few verbatim turns until the window refills. Pre-release data, so there is no migration; deleting the database or `conversation_summaries` rows avoids it.
- **A refold can be skipped once.** A consolidation with nothing new to extract (a queued run that an earlier run already covered) does not refold. The next turn whose window still has to move asks again, one sliding turn later.
- **Larger folds.** A refold folds half a window in one run instead of one turn per run. On a 128K model that is about four 16 KB fold steps. A refold that overruns the job's deadline also loses that run's extraction, as any summary failure did before.
- **Sequence gaps.** `is_needed` and the unread-candidates check compare sequences. A deleted message right after the summary's end can trigger one unneeded refold. It never skips a needed one.
- **Consolidation's cache slot.** Track B1 noted that compaction calls shared the chat's cache key; on this branch `Deadline::for_conversation` already uses `memory:{id}`, so refolds do not evict the chat's prefix.

## Suggested commit

```
Keep the summary and window start fixed until the window must move

Consolidation refolded the working summary after every answer, so once
older turns had left the window the second message of every request
changed and llama-server re-read the whole conversation each turn.

The summary's end is now read from its boundary message and tracked
apart from the extraction watermark. The assembler opens the window
right after it and keeps it there while the budget allows. When the
window has to start later, the plan asks for a refold that leaves the
newest half of the window verbatim, and the turn passes that request to
consolidation, which otherwise only extracts. Compact and automatic
compaction fold as before.
```
