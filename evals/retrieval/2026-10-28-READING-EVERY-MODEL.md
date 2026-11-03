# Grouped reading on every model: the wording, not the group size (2026-10-28)

> Corrected 2026-11-02: the Haiku, Sonnet and 9B rows were scored against garbled text. See the correction at the end.

Qwen3.5-9B Q4_K_M (sha256 `03b74727…`) on this PC's llama-server (RTX 4070 Laptop GPU, 32,768-token window,
the app's flags); Qwen3.8-27B on the llama-server the app is set to; Claude Haiku 5.5 and Sonnet 5.5 through
the Messages API. Code: `chat/tool_loop/reading/groups.rs`, `chat/prompting.rs`, `llm/cloud.rs`.

## Why

The 2026-10-26 A/B chose groups of four passages on the 9B. Run against Claude, the same reading fell apart:
Haiku as the app sends it today noted 115 of 300 facts. Its replies answered the question instead of writing
the lines, or wrote the lines under headings, so most labels had no line. The instruction began "Before
answering…" and came right after the answer template's closing "Answer:", so the model was cued to answer
twice. Smaller groups made it worse (89 at two passages), which ruled out capacity.

## Method

`scripts/replay_grouped_reading_ab.py` on the same 60 readings and 300 fact passages as 2026-10-26 (Sonnet:
the 28 readings of the `-r1` runs, 108 facts). Claude requests are built the way `llm/cloud.rs` builds them
(`--claude`; `--thinking off` sends the model's own off form). The script reads in the shipped wording with the
template's trailing "Answer:" taken off; `--group-instruction FILE` runs another wording, and `--keep-answer-cue`
leaves the cue in, as the old arms had it. (These runs used an earlier copy of the script, where dropping the cue
was the flag.)

- **Old**: main's wording ("Before answering…") after the "Answer:" cue.
- **Draft**: "Do not answer my question yet…", the cue dropped.
- **New**: the draft, asking for each line to start with its label in square brackets as shown, and ending
  "Write only these lines: your reply ends with the line for [N]."

## Results

Facts noted (a verified copy from the passage that states each fact):

| Model | Wording, thinking | 2 | 4 | 8 | 16 | All at once |
|---|---|---|---|---|---|---|
| 9B (this PC) | old | | 277 | | | 284 |
| 9B (this PC) | **new** | | 280 | | | **294** |
| 27B | old | 295 | 298 | 287 | 287 | 287 |
| 27B | draft | 299 | 298 | 299 | 299 | 300 |
| 27B | **new** | | | | | **299** |
| Haiku | old, thinking (as sent today) | 89 | **115** | 219 | 230 | 224 |
| Haiku | old, off | 144 | 201 | 242 | 250 | 269 |
| Haiku | draft, off | 288 | 288 | 289 | 287 | 289 |
| Haiku | draft, thinking | 284 | 288 | 280 | 289 | 285 |
| Haiku | **new**, off | | 291 | | | **291** |
| Sonnet (/108) | old, thinking | | 108 | | 108 | 108 |
| Sonnet (/108) | draft, off | | 107 | 108 | 108 | 108 |
| Sonnet (/108) | **new**, off | | | | | **108** |

All at once took 60 requests where four to a group took 448.

- The wording, not the group size, decided Claude's results: with the new wording every size noted the same,
  and the smallest groups were never better.
- The draft made the 27B write an answer after its lines on 15 of 60 readings. The new wording's closing
  sentence stopped that (none), with 21% less output.
- With everything at once, the new wording is the best or equal on every model. On the 9B it beats the old
  wording at both sizes on the same machine (294 against 284 and 277).
- Thinking did not change what Claude noted. It cut the lines that are not facts (Haiku in the draft wording
  at four: 404 with thinking off, 173 of them whole passages, against 11 with it on). With everything at once,
  which ships, there are few either way (31 with the new wording, thinking off), and thinking took two to
  three times as long.

## Decision

Read every passage in one group in the new wording, split only when the window cannot hold it, with the
"Answer:" cue taken off before any reading. Reading requests ask for no thinking, which the Claude adapter
sends as each model's off form where it has one.

## What this does not show

The same 9B file and old wording at four noted 294 on a Mac (16k window, Metal) and 277 here, so the machine or
build moves the result by itself; every comparison above is on one machine. Opus 5.5 and Fable, which cannot
turn thinking off, were not run. Facts are planted details scored by spelling, as before.

Records: `results/chat/<model>/2026-10-28-reading-*.jsonl.gz`.

## Correction (2026-11-02): the Haiku, Sonnet and 9B runs read the logs as cp1252

The replay opened the recorded logs with Python's default encoding. On this PC that is cp1252, so in the Haiku, Sonnet and 9B runs every non-ASCII character reached the model, and the scorer, garbled: "£4,680.83" as "Â£4,680.83", "—" as "â€”". A line that copied the garbled text was credited. A line that wrote the real character, as Claude often did, was scored a miss. The 27B runs read the logs as UTF-8, and none of their lines holds a garbled character, so their rows stand.

Re-scored, a missed fact counts as noted when its line is an exact copy of the real passage (reported → re-scored):

| Model | Wording, thinking | 2 | 4 | 8 | 16 | All at once |
|---|---|---|---|---|---|---|
| 9B (this PC) | old | | 277 → 294 | | | 284 → 295 |
| 9B (this PC) | new | | 280 → 292 | | | 294 → 298 |
| Haiku | old, thinking (as sent) | 89 → 92 | 115 → 119 | 219 → 220 | 230 → 232 | 224 → 224 |
| Haiku | old, off | 144 → 147 | 201 → 204 | 242 → 247 | 250 → 254 | 269 → 270 |
| Haiku | draft, off | 288 → 294 | 288 → 294 | 289 → 297 | 287 → 295 | 289 → 298 |
| Haiku | draft, thinking | 284 → 286 | 288 → 290 | 280 → 282 | 289 → 292 | 285 → 287 |
| Haiku | new, off | | 291 → 295 | | | 291 → 298 |
| Sonnet (/108) | old, thinking | | 108 → 108 | | 108 → 108 | 108 → 108 |
| Sonnet (/108) | draft, off | | 107 → 108 | 108 → 108 | 108 → 108 | 108 → 108 |
| Sonnet (/108) | new, off | | | | | 108 → 108 |

What changes:

- The wording result holds, and on Haiku it is larger. The draft and new wordings note 294 to 298 at every size. The old wording, with thinking as sent, notes 92 to 232.
- The 9B at four in the old wording re-scores to 294, the Mac's figure. The 17-fact gap was this bug, not the machine or build. The first paragraph of "What this does not show" is wrong.
- On the 9B the new wording is about even with the old: 298 against 295 all at once, and 292 against 294 at four. All at once in the new wording is still the best or equal on every model, so the decision stands. The results bullet saying it beats the old wording at both sizes on the 9B no longer holds.
- Once re-scored, thinking lowered Haiku's counts in the draft wording by 2 to 15 (286 to 292, against 294 to 298). The reported counts showed no difference.

These figures re-score the saved replies; nothing was rerun. They assume that a model which wrote the real character would also have copied it from clean text.

`scripts/` now opens every text file as UTF-8. `test_text_encoding_rag_eval.py` reads a log through an `open()` that defaults to cp1252, and it fails if any script touches a text file without naming an encoding. The only other committed result holding garbled text is Haiku's `2026-10-28-condense-16k-off` run. Its four lost facts are plain-ASCII terms, so its counts stand.
