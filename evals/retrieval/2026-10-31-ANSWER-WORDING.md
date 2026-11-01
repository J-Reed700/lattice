# Two sentences in the answer prompts (2026-10-31)

Claude Haiku 5.5 through the Messages API, and Qwen3.8-27B on the remote llama-server the app is set to. Code:
`chat/tool_loop/reading.rs`.

**One sentence has evidence and one does not.** The language sentence took Haiku's German-only answers from 4 of 5 to
0 of 5. The notes sentence showed no effect, because the replay never reproduced the leak it is aimed at. It stays only
if the whole run holds.

## Why

Reading all 50 Haiku answers (`2026-10-31-FOLLOWUP-SEARCH.md`) found two things the score does not show, and both start
in our own prompts.

- **Working shows on 5 turns.** "The other passage in my notes", and "should have been a hyphen". `CONTINUE_INSTRUCTION`
  and the writing instructions talk about "your notes", and the reader has never seen any.
- **`dp-mt-plant-room-key` answers an English question in German alone.** Its passage is German and the prompt says "in
  the passages' own words". The 27B did the same on an earlier build. It answers in English on the current one, but
  the sentence that allows it was unchanged.

## What changes

- `WRITE_INSTRUCTION` and `SEARCH_OR_WRITE_INSTRUCTION` end: "Write in the language of my question, translating any
  passage in another."
- `CONTINUE_INSTRUCTION` ends: "Do not mention your notes or correct them."

## Replay

`scripts/replay_recorded_turns.py wording`, on the requests recorded in the runs before this change. The counts below
come from reading the replies, because the script's flags are coarse: `leak` also matches "Note:", and `german` also
matches a quoted German sentence in an English answer.

| Language, `dp-mt-plant-room-key` | Replies | German alone, before | After |
|---|---:|---:|---:|
| Haiku | 5 each | 4 | **0** |
| 27B | 4 each | 0 | 0 |

| Notes, continuation rounds | Replies | Leaks, before | After | Hyphens, before | After |
|---|---:|---:|---:|---:|---:|
| Haiku | 3 each | 0 | 0 | 3 | 3 |
| 27B | 7 each | 0 | 0 | 5 | 4 |

- **The language sentence works on Haiku and costs the 27B nothing.** Every 27B reply kept the key's location and the
  signature rule in both arms.
- **The notes sentence is unproven.** No reply in either arm leaked, so the replay cannot show it helping. A hyphen is
  the continuation round saying it has nothing to add. Four against five is one reply, which is noise.
- **A different habit remains on the 27B.** Three of its four replies in each arm open by describing the passages ("The
  only passage bearing on your question is [1]"), which the writing instruction already forbids. Neither sentence is
  aimed at that.

## Records

- `results/chat/claude-haiku-5-5/2026-10-31-wording-replay.jsonl.gz` and the same name under
  `results/chat/qwen3.8-27b-remote/`.
- `2026-10-31-followup-markers-replay.jsonl.gz` in the same two folders: the replies behind the numbers in the commit that
  removed old citation markers from history (#260).

These ran from two working scripts, joined afterwards into `scripts/replay_recorded_turns.py`. Its job counts were
checked against the records with `--dry-run`, but it was not rerun against a model.

## Whole run

The two sentences go on every answer, so a replay of a few rounds is not enough. All fifty questions on the 27B are
the check, against its last whole run of 48.
