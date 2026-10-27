# Reading ahead of the groups, and the chat slot hold: timing (2026-10-27)

Local Qwen3.5-9B Q4_K_M, 16,384-token window, one slot, llama.cpp build 11669 on an RTX 4070 Laptop GPU,
every layer offloaded, started with the app's own cache flags (`--ctx-checkpoints 32 --checkpoint-min-step 1024
--cache-reuse 256`, and llama-server's default `--cache-ram` of 8 GiB). Code: `chat/tool_loop/reading/groups.rs`
(`read_ahead`), `llm/scheduler` (`hold`).

## Why

Qwen 3.5 is a hybrid model: its linear-attention layers keep a recurrent state that cannot be trimmed, so
llama-server resumes a prompt only from a checkpoint saved near the end of one it has read (about one batch,
512 tokens, before the end). Every reading group's prompt is the turn's message with a different group after
it. The first group's checkpoint sits inside that group's text, past the point where the second group's prompt
differs, so the second group, and every one after it, fell back to an early checkpoint (2,550 tokens in) and
read the message again: about 9,200 tokens a group. The grouped-reading A/B
(2026-10-26) did not see this: it asked for one token on the message before each arm, which left a checkpoint
where the groups diverge.

## Method

`scripts/replay_slot_hold.py`, on six saved reading prompts (three long questions, three short ones; the
2026-10-23 answer-coverage runs). The turn is main's grouped reading, groups of four, sent back to back. The
background request is another question's first group, cut at 256 output tokens. Every message opens with a fresh
nonce, so no request finds its prompt in the server's RAM cache, and each arm starts after a background request.

- **COLD**: the groups as main sends them.
- **WARM**: one token asked on the message without its reading instruction first (`read_ahead`).
- **-FREE**: a background request in every gap between the turn's requests, as a course build or a grounding
  backlog would take the slot without the hold. Without `-FREE`, the hold is in place: background work waits.

Turn time is the reading alone; the answer round after it is not timed.

## Results

| Arm | Median turn s | Total turn s | Prompt tokens processed | Requests | Background s inside the turn |
|---|---|---|---|---|---|
| COLD | 40.2 | 252 | 301,418 | 37 | 0 |
| **WARM** | **24.2** | **145** | **118,148** | 43 | 0 |
| COLD-FREE | 94.8 | 589 | 301,497 | 37 | 316 |
| WARM-FREE | 83.0 | 545 | 118,142 | 43 | 382 |

Per turn, COLD/WARM: median 1.73x (1.24 to 2.26x). WARM-FREE/WARM: median 3.70x (2.54 to 4.71x).

- Reading ahead cut the prompt tokens processed by 61% and the reading's median time by 40%. On a long
  question with eight groups: 77,378 tokens and 55.7 s, against 21,868 and 24.6 s.
- Background work in the gaps re-read nothing: `-FREE` processed the same prompt tokens as its arm without it.
  llama-server's RAM cache (`--cache-ram`) saves the slot's prompt with its checkpoints and restores it after
  the background request. What the turn loses is the wait behind each background request.
- So the hold's gain is that wait, not the cache: under a steady background load the reading took 3.7x as long
  (median) without it. A light load, one background request now and then, costs one request's time.

## Decision

Read ahead on every backend that keeps read prompts (`LLMPort::keeps_read_prompts`: llama-server); a provider
that bills each request is not sent the extra request. Keep the slot hold.

## Not measured

The answer round after the reading: its prompt also differs from the groups' just after the message, so it reads
from the read-ahead's checkpoint too. Prompts in the learning pipeline that share a head (a course's sources)
and differ after it may re-read the head the same way on a hybrid model.

Records: `results/chat/qwen3.5-9b-local/2026-10-27-slot-hold-timing.jsonl.gz`.
