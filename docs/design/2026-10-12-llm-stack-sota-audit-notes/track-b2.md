# Track B2: tool loop

Uncommitted. Paths under `crates/lattice-desktop/src/features/conversation/chat/` unless noted.

## Changed
- `application/ports/llm_port.rs`: the `tool_choice` field and `ToolChoice` enum, byte-identical to the brief. The serde impls are hand-written; keep Track A's copy at integration.
- `tool_loop.rs`: 1,887 → ~670 lines, orchestration only. New in `tool_loop/`:
  - `final_round.rs`: the last round keeps its tools and sets `ToolChoice::None`. A reply that is only calls (native, JSON or `<tool_call>` text) gets one retry with no tools, then fails as before.
  - `calls.rs`: read-only calls run 4 at a time, results in call order; history, custom and unknown tools run alone.
  - `research_notes.rs`: past 60% of the window, results are condensed into notes, at most twice per turn and never on the last round.
  - also `generation.rs`, `clock.rs`, moved helpers, `loop_tests.rs`.

## Two departures from the brief, measured
Bundled server, Qwen3.5-9B Q4_K_M, ~3,800-token prompt. Cached / re-read tokens:

**Last round:**
- tools dropped: 0/3479 (7.9 s)
- tools kept, instruction as a new user message: 363/3450 (7.9 s)
- tools kept, instruction at the end of the last tool result: 3263/553 (1.45 s)

Qwen 3.5's template marks assistant turns after the last user message, so a new user message rewrites earlier turns. The instruction now ends the last result.

**Condensing:**
- keeping the latest round word for word: 289/1350 (3.3 s)
- condensing everything: 2415/646 (1.65 s); the next round re-read 150 tokens

Notes kept every fact and `[1]`, `[2]`, so everything is condensed. All variants answered correctly. My server is stopped.

## Verified
- `cargo check --lib` and `cargo check --lib --profile=test`: clean, no warnings.
- `cargo test --lib conversation::chat`, run once: **391 passed, 0 failed, 2 ignored**, including 62 tool-loop tests:
  - the last round keeps its tools, and retries without them
  - 3 reads overlap and come back in call order; never more than 4 at once
  - a call that may write runs alone
  - Stop works mid-batch
  - condensing gets past a full window
  - `[1]` and its step link survive condensing
  - refused notes fall back to the window guard
- rustfmt on every touched file.

## Not verified
- `cargo check --all-targets`.
- An app run: the field does nothing here until Track A maps it.
- Note quality on real research or on a 27B.
- Concurrency against real search providers.
- Cloud providers with an instruction inside a tool result.

## Integration markers
`// integration: thinking = ThinkingMode::Off (track A)` at `tool_loop/final_round.rs:46` (the retry) and `tool_loop/research_notes.rs:165` (the notes request). `reasoning_effort` is untouched.

## Risks
- 4 concurrent web searches may hit provider rate limits.
- An instruction in a tool result may carry less weight; the retry is the backstop.
- A last-round answer that opens with `{`, `[` or a code fence shows only once complete.
- `tool_execution_ms` is now wall time per round.

## Commit message
```
Keep tools on the last round, run reads together, condense research

Dropping the tools for the answer changed the prompt from its first
token, so the server re-read all of it. The last round now keeps them,
forbids calls, and puts its instruction at the end of the last result:
on Qwen3.5-9B that reused 3,263 cached tokens where a new user message
reused 363. A reply that is still only calls is retried once without
tools.

Read-only calls run four at a time with results in call order, so the
transcript stays deterministic; anything that may write runs alone.

Results past 60% of the window are condensed into notes (at most twice),
so research continues instead of stopping at the window.
```
