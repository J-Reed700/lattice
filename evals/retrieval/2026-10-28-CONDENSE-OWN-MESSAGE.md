# Condensing: the instruction inside the last result against a message of its own (2026-10-28)

Qwen3.8-27B on the llama-server the app is set to, and Claude Haiku 5.5 through the Messages API. Same turns,
rubrics and scoring as `2026-10-27-CONDENSE.md` (the 9B). Code: `chat/tool_loop/research_notes.rs`.

## Why

Condensing said its instruction after the last tool result, inside it, so a new user message would not change
how Qwen 3.5's template renders the turns before it. The 9B took that well (22 of 28 usable, the rest asked
again). The prompt audit (finding 5) flagged that a tool result is the least trusted input a model reads, so
Claude may not follow an instruction found there, and that notes which answer instead of condensing are accepted
as notes.

## Method

`scripts/replay_condense_ab.py`, arms **ONE** (as shipped: in the last result) and **ONE-USER** (a user message
of its own), each request otherwise the shipped one. Claude runs use the replay's `--claude` option, built the
way `llm/cloud.rs` builds a request. `--thinking adaptive` is what Claude gets today: condensing asks for no
thinking with `ThinkingMode::Off`, the default every request carries, which Claude does not read as "off".
`--thinking off` is shown for comparison.

## Results

**16k-size turns**, 60 turns (48 long questions with six facts each, 12 short with one or two), about 23,000
characters of results each:

| Model | Arm | Facts kept | with citation | Usable notes | Replies writing a call | After / before | Median s |
|---|---|---|---|---|---|---|---|
| 27B | ONE | 304/304 | 304 | 17/60 | 43 | 0.75 | 2.0 |
| 27B | **ONE-USER** | 304/304 | 304 | **58/60** | 0 | **0.18** | 14.2 |
| Haiku, thinking as sent | ONE | **200/304** | 200 | 32/60 | 0 | 0.47 | 2.5 |
| Haiku, thinking as sent | **ONE-USER** | **304/304** | 303 | **60/60** | 0 | **0.07** | 5.5 |
| Haiku, thinking off | ONE | 303/304 | 294 | 60/60 | 0 | 0.06 | 2.7 |
| Haiku, thinking off | ONE-USER | 301/304 | 297 | 60/60 | 0 | 0.06 | 2.7 |

**Packed turns**, 12 turns of four long questions asked together, 24 facts and about 115,000 characters of
results (some 30,000 tokens) each:

| Model | Arm | Facts kept | with citation | Usable notes | Replies writing a call | After / before |
|---|---|---|---|---|---|---|
| 27B | ONE | 270/288 | 270 | 3/12 | 9 | 0.76 |
| 27B | ONE-USER | 264/288 | 261 | 12/12 | 0 | 0.03 |
| Haiku, thinking as sent | ONE | 216/288 | 192 | 8/12 | 0 | 0.35 |
| Haiku, thinking as sent | **ONE-USER** | **288/288** | 286 | 12/12 | 0 | 0.03 |
| Haiku, thinking off | ONE | 288/288 | **216** | 12/12 | 0 | 0.03 |
| Haiku, thinking off | ONE-USER | 287/288 | 284 | 12/12 | 0 | 0.03 |

An unusable reply leaves the results as they were, so ONE's "facts kept" on the 27B counts mostly turns that
were never condensed.

- Haiku as it is sent today wrote nothing at all on 28 of 60 turns with the instruction in the result (an empty
  reply, so nothing was condensed), and the notes it did write lost 104 facts across 18 turns. As a message of
  its own, nothing was lost.
- With thinking off, Haiku followed it either way, but in the result it dropped a quarter of the citations on
  the packed turns (216/288); the facts were kept without the number of the source they came from.
- The 27B answered the instruction in the result with a tool call on 43 of 60 turns, which the app then asked
  again as a message of its own. Its notes from the result were barely shorter than the results (0.75); as a
  message of its own they were a fifth of them (0.18).
- On the packed turns, the 27B's notes left out two of four sources entirely on 2 of 12 turns (12 facts each),
  in both arms (ONE's one usable loss was the same). Haiku's did not. That is not about where the instruction
  is said, and is left open below.

## Decision

Say the condensing instruction as a user message of its own, for every model, and drop the second request that
asked again that way when a call came back. On llama-server this costs a read of the turn's results (the new
message changes how the template renders the turns before it), which the 27B already paid on 43 of 60 turns
through the second request, and on the 9B about 5 s at 16k and 24 s at 64k.

## What this does not show

The facts are planted details among unrelated text, scored by spelling. The 27B's whole-source losses on packed
turns are open: a check that asks again for sources the notes leave out would have to tell those from sources
the instruction asks it to drop because they do not bear on the question, which these turns cannot test (every
source here does). Opus 5.5 and Fable, which cannot turn thinking off, were not run; they get the message of
their own, the arm that lost nothing with thinking on.

Records: `results/chat/qwen3.8-27b-remote/2026-10-28-condense-{16k,packed}.jsonl.gz`,
`results/chat/claude-haiku-5-5/2026-10-28-condense-{16k,packed}-{adaptive,off}.jsonl.gz`.
