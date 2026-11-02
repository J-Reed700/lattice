# Two sentences in the answer prompts (2026-10-31)

Claude Haiku 5.5 through the Messages API, and Qwen3.8-27B on the remote llama-server the app is set to. Code:
`chat/tool_loop/reading.rs`.

**One sentence has evidence and one does not.** The language sentence took Haiku's German-only answers from 4 of 5 to
0 of 5. The notes sentence showed no effect, because the replay never reproduced the leak it is aimed at. Both stay,
because the whole run held: 50 of 50 on the 27B, read by hand, against 48 before.

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

The two sentences go on every answer, so a replay of a few rounds is not enough. All fifty questions on the 27B were
the check, against its last whole run. This one is also the first whole run with #257 to #262 all merged, so it is not a
clean test of the wording alone. Each run also numbers the documents differently, and one or two questions either way
is noise.

| 27B, all 67 turns | Last whole run | With the two sentences |
|---|---:|---:|
| Correct, read by hand | 48 of 50 | **50 of 50** |
| The script's check, as now scored | 48 | 50 |
| Answer time, median | 31.9 s | 34.6 s |
| Answer time, 95th percentile | 57.7 s | 76.6 s |
| Turns whose evidence was rated insufficient | 7 | 2 |

- **It held.** Neither sentence cost a question. The two wrong answers of the last run (`dp-mt-runbook-ticket` named the
  wrong owner, `dp-mt-encryption-versions` never named the Gidgegate exception) are both right here. That gain is within the noise and
  cannot be credited to the wording.
- **The answers came in English.** Both turns of `dp-mt-plant-room-key` are English and keep the key's location and the
  signature rule, though the second reads awkwardly.
- **Two script failures were the script's.** It failed the Marbleau refusal and the refusal of Ilse Brantgard's mobile
  number. Both decline plainly, in wording `EXTRA_ABSTENTION_RE` lacked ("none of these returned", "no mobile number ...
  is given"), so both phrases were added. Rescoring moved nothing else that this change touched. The last run's row
  above reads 48 instead of the 49 first recorded, because the invented-quote check added since catches
  `dp-mt-runbook-ticket`.
- **The 95th percentile rose by 19 s, and this run cannot say why.** Main changed too, so it is not put down to the
  wording.

### What reading the answers found

- **One continuation said its quotes back.** `dp-mt-combine-versions` turn 3 ended in the two quotes the continuation
  prompt had just listed, raw, one of them a superseded "14 hours" with nothing to say so. That is 1 of 11 continuation
  rounds, against 0 of 7 in the last run and 0 of 14 in the replay. One case cannot show the notes sentence caused it.
  `passages_added` now counts such a reply as adding nothing, so the answer stays as it was. This needs no prompt change
  and no rerun: a unit test covers it.
- **Working still shows at the start of two answers.** `dp-long-allotment-agreement` opens "The search confirms the six
  obligations already identified ... I'll now give the answer", and `dp-long-choir-minutes` opens "The full document
  confirms exactly six ...". The last run had one of these, also on the choir minutes, so this comes from the round after
  a further search or document read, not from these sentences. It is not fixed here.
- **The opening the writing instruction forbids survives.** `dp-paraphrase-monitor` opens "The only passage bearing on
  your question is [1]", and `dp-paraphrase-clock` ends by explaining that [3] and [12] are only titles.

Record: `results/chat/qwen3.8-27b-remote/2026-11-01-shipped-path-wording.jsonl.gz` (answers and scores).
