# Track B1: byte-stable prompt prefix

Branch `llm/prefix-stability` (from 70cc8c92), uncommitted.

## What changed

- **`context_assembler/render.rs`:** new `FinalUserMessage`. It holds recall, recorded requirements, supporting material, `Today is YYYY-MM-DD, Weekday.`, then `[current message]` with the request bytes unchanged. `MEMORY_USE_POLICY` now says only `[current message]` is live.
- **`context_assembler/mod.rs`:** the order is now policy, summary, verbatim turns, then the one final message. Recent turns are chosen before the summary. Overflow eviction works on the message structure: evidence, then recall, then summary, then the oldest turns. This fixes a bug: the old code matched an `[older passages` header that is never rendered, so recall was never evicted. `ContextRequest` gains `recent_reaches_start` and `today` (the clock is injected).
- **`chat/memory_context.rs`:** `RecentCandidates.reaches_start` is `from == 0 && !has_more`. `build_memory_plan_for_query` now takes `today`.
- **`chat/turn/assemble.rs`:** reads the local date once per turn. The memory-off path also puts the date line in the final message.
- **Tests:** new `tests/prefix.rs` (4); `tests/eviction.rs` (3 moved); updated render, assembler and `memory_context` tests (+1).

## Summary stability: it was not stable

The summary is rolling:
- `persistence.rs:126` triggers consolidation after every answer (`compaction.rs:190`).
- Maintenance processes everything up to the last completed answer (`memory/job.rs:332`) and refolds the summary on each commit (`job.rs:523`).
- The summary row is a single `UNIQUE` upsert per conversation (`repository/memory.rs:910`).

Rendering is deterministic, but message 1 still changed every turn from turn 2 on.

**Fixed:** the summary is omitted while the window holds the first message (it only repeated verbatim turns), so conversations that fit are append-only.

**Still open:** once older turns leave the window, the refolded summary and the sliding window start both move every turn. The test `a_refolded_summary_ahead_of_the_history_breaks_the_shared_prefix` records this. The fix is outside B1:
- track the summary's coverage separately from the extraction watermark (`MemorySnapshot`, `job.rs`);
- refold only when the window start moves;
- anchor the window there.

## Verified

- `cargo check --lib` → `Finished`, with no new warnings. It needs an empty, gitignored placeholder at `src-tauri/binaries/llama-server-aarch64-apple-darwin`.
- `cargo test --lib context_assembler` → `51 passed; 0 failed`. This includes `consecutive_turns_share_every_message_up_to_the_newest_answer`: turn N+1's serialized messages begin with N's up to N's last answer, even though recall, memory, evidence, the summary and the date all differ.
- `cargo test --lib -- conversation::chat::turn completion_input compaction memory_context` → `51 passed; 0 failed`.
- `rustfmt --check` passes on all touched files.
- Only `memory_context.rs` calls the assembler (Explorer shares that path); learning, daily notes, synthesis and handoff do not.

## Not verified

- No live `cache_n` measurement (llama-server was not allowed for this track).
- No `cargo check --all-targets` or clippy.
- Model behaviour with the sectioned final message is untested.

## Risks and findings

- Recall, memory and evidence moved from separate user messages into the final one. The policy's live-instruction wording changed with them.
- `memory/mod.rs:207` gives compaction calls `cache_key = conversation_id`. Consolidation is therefore routed to the chat's slot and overwrites its cached prefix between turns. A `memory:{id}` key would avoid that.
- A resumed research turn replays its saved date.
- The shared target dir rebuilds across worktrees (12-16 min/run).

## Suggested commit

```
Keep the prompt prefix stable across turns

llama-server reuses its KV cache only up to the first changed token, and on
hybrid Qwen 3.5 an early change rolls back to a distant checkpoint. Recall,
recorded requirements and evidence sat ahead of the verbatim history, so
every turn re-read the whole conversation. Move them and a new date line
into one final user message after the history, evidence before question.
Omit the rolling summary while the window holds the first message, since
consolidation rewrites it after every answer.
```
