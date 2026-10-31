# What a follow-up searches for (2026-10-31)

The remote Qwen3.8-27B through `shipped_path_eval`, with the reranker on this PC's RTX, on mains power, and the local
Qwen3.5-9B for the intent replays. Code: `chat/intent.rs`, `chat/retrieval/kb_retrieval.rs`,
`chat/retrieval/corpus_plan.rs`, `scripts/shipped_path_eval.py`.

## Why

The last run in [`2026-10-31-SEARCH-AFTER-READING.md`](2026-10-31-SEARCH-AFTER-READING.md) had two follow-ups wrong. One
quoted "page Anneke Voskuil and quote TCK-1836", a sentence in no document, and passed every check. With no planner a
follow-up searches its own words, and "Is there any way round it?" names nothing, so its search led with the handbook
and the data-export policy. A follow-up right in one run and wrong in the next was a matter of whether the writer
happened to search again.

## The change

The intent step already sees the last exchange and already says whether a message is a follow-up. It now also writes
the message as a question that stands alone, in the same call. On a follow-up the plan searches both, its own words and
that question, and the reranker reads both. The planner's own plan is unaffected and takes the question as one more
query. Any provider that leaves the field out searches as before.

The intent step now samples greedily. It was on the chat's sampling, temperature 0.7 and presence penalty 1.5, and a
penalty on repeating words is the wrong thing to ask of a rewrite that has to repeat names.

## Replaying all 67 intent calls

No build, about 2 s a call. Flags compared with the same model's old prompt.

| Model and wording | Follow-ups rewritten | False follow-ups | Flags moved | Median tokens |
|---|---:|---:|---:|---:|
| 27B, "...Otherwise empty." | 15/15 | 0 | 0/67 | 67, or 48 on a standalone message |
| 9B, "...Otherwise empty." | 14/15 | 2 | 4/67 | 67 |
| **27B, no empty option** | **15/15** | **0** | **0/67** | **67** |
| **9B, no empty option** | **15/15** | **0** | **1/67** | **66** |

The empty option was there to save 19 tokens on a message that already stands alone. On the 9B it left "Is there any
way round it?" with nothing, the one case the change exists for, and marked two standalone messages as follow-ups. Without
it the 9B matches the 27B. The cost is about 0.6 s on the 27B on a turn that ignores the result, because a rewrite is
used only when the message is a follow-up.

## What the search found

The expected document's rank across the 15 follow-ups, comparing the last run with this change. This takes no model
judgement, so it does not move with the noise between runs described in that write-up.

| Follow-up | Before | After |
|---|---:|---:|
| "What is its standing ticket, and if the problem hasn't cleared after the wait..." | 36 | 4 |
| "Is there any way round it?" | 19 | 3 |
| "Who owned the drift report action in that same week..." | 38 | 3 |
| "And on the Wendover Basin edge ring?" | 25 | 4 |
| "So compared with that first version, is the deadline longer or shorter now?" | 21 | 1 |
| "Is there anything I have to do to take it out?" | 19 | 1 |

Better on 12, level on 3, worse on none. Two of the three were already first and the third is a refusal with no expected
document. Across the 14 with one, the median rank went from 10 to 1.

## The twelve conversations

All twelve multi-turn conversations, 29 turns, read by hand.

| Run | Passing | Wrong by reading | Median seconds | p95 seconds |
|---|---:|---:|---:|---:|
| The last whole run, on these twelve | 10 | 2 | 28.7 | 35.5 |
| **Searching the standalone question too** | **12** | **0**, and one slip | **29.9** | **38.3** |

The two that were wrong are right. The runbook follow-up now quotes "page Marisol Quenby and quote TCK-1836" from its own
passage, and "Is there any way round it?" answers with the Gidgegate exception. Neither needed the writer to search
again. Two of the 29 turns called a tool, and the cost is about a second at the median.

One answer has a slip the checks do not see: `dp-mt-combine-renewals` says "more than three years later" beside its own
right "about 35 months". That is the model's date arithmetic, not its search; the last run was a day out on the same
turn.

## A check for quotes

`scripts/shipped_path_eval.py` now fails a turn with a cited quote of at least five words that no passage it read holds.
It compares letters and digits only, so a line break or OCR hyphen cannot fail it, and it splits at an ellipsis or a
marked edit such as "[must]". **It was written after seeing the answer it catches**, and the first version failed that
marked edit, which is corrected. Rescored, the four whole runs read 48, 44, 50 and 48. It flags one quote across all
four, the Voskuil sentence, so the last run now scores what reading it found.

## What this does not show

- **Twelve conversations, not fifty.** The change cannot touch a first turn's search, and no flag moved on the 27B in
  the replay, so the other 38 were not rerun.
- **Neither the 9B nor Claude has run this end to end.** The 9B was replayed on the intent step only. Haiku and Sonnet
  were not run at all.
- **The planner was not rerun.** With a utility model set, the question joins the planner's queries, which is unmeasured.
- **`dp-mt-plant-room-key`** answered an English question in German alone on the build before this wording. It answers in
  English here, but the sentence that allows it, "in the passages' own words", is unchanged.
- **Not a held-out score.** These are the same questions the wordings were tuned on.

Records: `results/chat/qwen3.8-27b-remote/2026-10-31-shipped-path-followup.jsonl.gz` (answers and scores),
`2026-10-31-intent-standalone-replay.jsonl.gz` (every arm of the replay), and `2026-10-31-shipped-path-last50.jsonl.gz`
rescored with the quote check.
