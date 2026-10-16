# Fix: the working summary moved on every turn past the window

Branch `llm/stable-summary-window`. Written 2026-10-13 against the integration branch at 0f19c85a; ported 2026-10-15 onto main after #46 (summary kept after a delete), #57 (the `lattice-core` crate) and #60 (no content caps). The design record is `docs/design/2026-09-19-conversation-memory.md` §24.8.

Defect: `track-b1.md`, "Summary stability". Consolidation refolded the summary after every answer, so it always ended at the watermark. Once older turns had left the window, the summary (message 2) and the window's first turn both changed on every turn. llama-server reuses its KV cache only up to the first changed token, so each turn re-read the whole conversation. The test `a_refolded_summary_ahead_of_the_history_breaks_the_shared_prefix` recorded it.

## The rules

**Coverage.** The summary's end is tracked apart from the extraction watermark. `MemorySnapshot::summary_through_sequence` (added by #46, read through a join on the summary row's `up_to_message_id`) gives it; `summary_covers_through()` reads it as a position: zero without a summary, the watermark for a boundary that cannot be placed.

**Anchoring** (assembler step 5, `context_assembler/anchor.rs`):

1. Candidates that reach the first message and fit the recent pool are carried whole, and the summary stays out. B1's rule, unchanged.
2. Otherwise, with a usable summary that fits its pool and knows where it ends, the window opens at the first message after the summary's end. It never reaches further back, even with room.
3. When those turns no longer fit, the window starts later, filled newest first as before. The plan then reports `summary_refold`: where the window started, and where the summary should end. It reports the same when the candidates begin past the summary's end, and when there is no summary at all.
4. The requested end sits inside the window. The newest half of the window's tokens stays verbatim, moved forward so the next window opens on a user turn; when the candidate list ended the window, half its messages too.
5. The eviction order is unchanged. The refold request is measured after final overflow eviction.

**Folding** (compaction job, `memory/job.rs` `FoldTarget`):

1. Consolidation after an answer extracts on every run. It folds only when the turn's plan asked, and only while the request still applies (`SummaryRefold::is_needed`). Two turns that ask before the first refold lands cause one refold.
2. `/compact` and the automatic compaction a turn waits on fold everything they extract, as before.
3. A fold covers the messages after the summary's end through its target, clamped to what extraction accepted. Source below the watermark is paged back with #46's bounded refold read, and consolidation takes further pages while its deadline allows.
4. A summary a delete or correction dropped covers nothing, so the next fold starts from the first message. Consolidation waits for a prompt to ask for it rather than folding to the watermark, which would end the summary on the newest answer and leave the next window holding only the new question.
5. A commit that does not fold carries the summary row into the new memory revision.

## What changed (paths under `src-tauri/`)

- `crates/lattice-core/src/domain/conversation/memory.rs`: `MemorySnapshot::summary_covers_through`, `SummaryRefold { window_start_sequence, through_sequence }`, `SummaryRefold::is_needed`.
- `crates/lattice-core/src/application/services/context_assembler/anchor.rs` (new): `recent_window` (the anchored step 5) and `summary_refold` (gap detection and the half-window target). `mod.rs` prices the summary before the window and calls both; `plan.rs` adds `ContextPlan::summary_refold`.
- `src/features/conversation/memory/job.rs`: `FoldTarget` (`for_run`, `below_watermark`, `end`); `summary_resume_point` reads `summary_covers_through`. `job/attempt.rs` reads the refold range only for a fold, folds only to `fold_end`, calls the summarizer only when the summary moves, and reports the summary in force. `job/reads.rs` `read_refold` takes the fold's end instead of the watermark.
- `src/features/conversation/memory/mod.rs`: `CompactionRequest::summary_refold`, module doc.
- `src/features/conversation/repository/memory/writes.rs`: a commit without a summary update sets the summary row's `memory_revision` to the new revision.
- Plumbing from the plan to consolidation: `chat/turn/assemble.rs` (`PlannedRequest`, plan log line), `chat/turn/generate.rs` (`GeneratedAnswer`), `chat/turn/finalize.rs`, `chat/persistence.rs` (`FinishedAnswer`), `chat/ports.rs` (`ChatPolicy::consolidate_after_turn` takes the request), `di/chat_runtime.rs`, `chat/test_runtime.rs`, `compaction.rs`. A resumed research turn passes `None` (`chat/turn/mod.rs`).

## Tests

- `context_assembler/tests/prefix.rs`: `a_refolded_summary_ahead_of_the_history_breaks_the_shared_prefix` is gone; new `consecutive_turns_past_the_window_share_every_message_up_to_the_newest_answer`, `the_summary_refolds_when_the_window_start_moves`, `candidates_that_start_past_the_summary_ask_for_a_refold`. The shared `snapshot()` helper places the summary's end at zero so it anchors nothing a test passes in unless the test says where it ends.
- `memory/tests.rs`: new `consolidation_extracts_without_touching_the_summary_when_no_refold_is_asked`, `consolidation_refolds_the_summary_over_the_turns_the_window_dropped`, `a_refold_never_runs_past_what_extraction_accepted`. #46's refold and forget tests that exercised a fold through consolidation now ask for it (`refolding(n)`), since consolidation no longer folds unasked; what each one checks is unchanged.
- `repository/tests/memory.rs`: `the_snapshot_reports_where_the_summary_ends_and_a_commit_without_one_keeps_it`.
- Domain `memory/tests.rs`: `a_refold_is_needed_only_while_the_summary_ends_before_the_window_gap`, `the_summary_covers_through_its_boundary_or_trusts_the_watermark`.

## Not verified

- No live run and no llama-server, so no `cache_n` measurement. The prefix property is shown on serialized plans only.
- Answer quality with the shorter windows is unmeasured. No eval was run.
- The live consolidation path through `consolidate_after_turn` runs only in the app. The fake runtime ignores the request.

## Risks

- **Less verbatim history.** Between refolds the window holds between half and all of the recent pool.
- **`/compact` and automatic compaction shrink the window.** They fold through the extraction boundary as before, and the window now opens after it. The eval harness compacts that way on every cycle, so its bounded-arm numbers may move when a fixture does not fit whole.
- **A dropped summary waits one turn.** After a delete or a correction drops the summary, the next turn that needs older history runs without a summary and asks for the fold; consolidation after it folds. #46 refolded on the consolidation right after the delete.
- **Pre-existing summaries end at their last consolidated answer.** Pre-release data, so there is no migration.
- **A refold can be skipped once.** A consolidation with nothing new to extract does not refold; the next turn whose window still has to move asks again.
- **Sequence gaps.** `is_needed` and the unread-candidates check compare sequences. A deleted message right after the summary's end can trigger one unneeded refold. It never skips a needed one.
