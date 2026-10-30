# The handoff note's rules after the transcript (2026-10-30)

Qwen3.5-9B Q4_K_M on this PC's llama-server (RTX 4070 Laptop GPU, 16,384-token window, the app's flags);
Qwen3.8-27B on the remote llama-server the app is set to; Claude Haiku 5.5 through the Messages API, as the app
sends it. Code: `conversation/handoff.rs`. This is finding 17 of the 2026-10-28 prompt audit.

## Why

"Continue in new chat" writes a handoff note that becomes the new conversation's only memory of the old one.
The request put the purpose, the four headings, the rules and the 450-word limit before the transcript and
nothing after it (`GroundedRequest::closing` was empty). So the last thing the model read was the
conversation's last message, often a question.

## Method

`scripts/replay_handoff_ab.py`. It builds 20 synthetic conversations from `desktop-path-v1`: each strings
together 10 to 20 items' exchanges (16 to 30 messages, 7,000 to 13,000 characters) with the answers the
2026-10-22 shipped-path run gave them, then ends on another question with no answer. Each request is rendered as
`summary_request` renders it (task, transcript, closing), with the app's sampling (temperature 0, top-k 1,
thinking off). A note is scored on:

- its four headings, in order, with nothing before the first;
- the rubric facts it keeps, of those the transcript's answers stated (469 in all);
- whether the unanswered last question is listed under Open threads, found by a key term for each question;
- its length.

Arms:

- **BEFORE**: the prompt as it was.
- **AFTER**: one sentence of purpose before the transcript; "Now write the handoff note for the conversation
  above…" and every rule after it.
- **AFTER_Q**: AFTER, plus "Its last message may be a question nobody has answered yet. Do not answer it: put it
  under Open threads."

## Results

| Model | Arm | All four headings | Last question under Open threads | Facts kept | Median words |
|---|---|---|---|---|---|
| 9B | BEFORE | 20 | 20 | 262/469 | 327 |
| 9B | AFTER | 20 | 18 | **393/469** | 368 |
| 9B | **AFTER_Q** | 20 | **20** | **372/469** | 344 |
| 27B | BEFORE | 20 | 20 | 451/469 | 475 |
| 27B | AFTER | 20 | 20 | 449/469 | 462 |
| 27B | **AFTER_Q** | 20 | 20 | 453/469 | 459 |
| Haiku | BEFORE | 20 | 20 | 449/469 | 570 |
| Haiku | AFTER | 20 | 20 | 453/469 | 566 |
| Haiku | **AFTER_Q** | 20 | 20 | 458/469 | 527 |

Facts against BEFORE on the 9B, paired by conversation (95% bootstrap): AFTER +131 [+48, +222], with 10
conversations better and 5 worse; AFTER_Q +110 [+22, +206].

- The audit expected notes that answer the last question instead of summarizing. Every note in every arm had
  its four headings with nothing before them. The 9B failed differently: with the rules first, its notes dwelt
  on the conversation's first topic, the one in its title, and dropped most of the rest.
- The 27B and Haiku kept about 96% either way.
- Every arm on the 27B and Haiku runs past the 450-word limit when a conversation holds 15 to 30 facts.

## Decision

AFTER_Q. It keeps the 9B's gain, loses nothing on the 27B or Haiku, and lists the pending question on every
model. The notes path (`notes_task`, used when a transcript does not fit the window) gets the same shape:
purpose before the part, instructions after it. It is not measured here.

## What this does not show

The conversations are question-and-answer sessions over one synthetic library, short enough for the 9B to
summarize in one request. Long real conversations take the part-by-part notes path. One sample per
conversation and arm; the 27B and 9B run at temperature 0.

Records: `results/<model>/2026-10-30-handoff-replay.jsonl.gz` (Haiku cost $0.10).
