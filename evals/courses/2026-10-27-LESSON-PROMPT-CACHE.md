# What a lesson's calls share, and what the prompt cache keeps of it (2026-10-27)

## Why

Lesson calls carry the course as their cache key: a llama-server sends them to the slot that last served the
course, and Claude is asked to cache them. Chat's grouped reading had been reading its whole message again for
every group on a hybrid model (`evals/retrieval/2026-10-27-READ-AHEAD-AND-SLOT-HOLD.md`), so the question was
what a lesson's calls actually share, and what each backend reuses of it.

## Method

One small lesson, `live_small_lesson_reaches_ready` with the `card_workflow.json` fixture, on the local
Qwen3.5-9B (32,768-token window, one slot, the app's cache flags, RTX 4070 Laptop GPU). The test records every call
(`call-N.json`). It ran through drafting, review, claim extraction and most of the claim checks (310 calls) and
was stopped there; the rest are more of the same steps.

- `scripts/lesson_prompt_prefixes.py`: for each call, the longest prefix it shares with an earlier call (what a
  cache can reuse as the prompt is laid out) and the share of its text already sent in one of the ten calls
  before it, anywhere (what a layout that put shared material first could reuse).
- The 9B server's log: tokens each request read and reused.
- `scripts/replay_lesson_cache.py`: the first 245 calls replayed to Claude Haiku 5.5 in order, as `cloud.rs` sent
  them (`MODE=app`) and with the breakpoints this change sets (`MODE=system`), with output capped at a few
  tokens. About $0.20 in all.

## What the calls share

| Calls | Prompt tokens | Prefix shared with an earlier call | Text repeated from the ten before | Step |
|---|---|---|---|---|
| 106 | 406,535 | 44% | 73% | claim checks |
| 47 | 65,981 | 51% | 65% | inventory fidelity audits |
| 5 | 69,758 | 3% | 38% | reference selection |
| 26 | 13,342 | 58% | 62% | claim classification |
| 33 | 9,285 | 93% | 92% | origin checks |

Calls of one step share their system prompt; their user message is a JSON payload that leads with the call's own
item (a claim, a passage, the topics) and carries the shared material (passages) after it. Two steps that
continue one exchange (the draft, then its structuring) share the whole earlier prompt. A step that works on the
same draft under another system prompt (the corrections) shares nothing as a prefix.

## What each backend kept

**The 9B llama-server** reused 197,436 of 971,488 prompt tokens (20%). Reading prompts took about 495 s and
writing answers 2,814 s (100,396 tokens), so prompt reading is about 15% of the lesson's model time; reusing all
the repeated text would save some 5%. (One request stalled for 2,351 s in prompt processing, the laptop sleeping
or throttling; it is left out of the 495 s.)

**Claude**, as the app sent it, wrote nearly every prompt to the cache and read almost none back:

| Breakpoints | Uncached | Written (1.25x) | Read (0.1x) | Input cost | Against no caching |
|---|---|---|---|---|---|
| top-level only, as sent | 21,103 | 926,793 | 24,893 | $0.1182 | 1.22x |
| **the system prompt; top-level only on a continued exchange** | 741,510 | 8,092 | 223,187 | **$0.0774** | **0.80x** |

The top-level breakpoint lands on the last block, after each call's own payload, so every call wrote an entry
no later call began with: a cache write on every call and a surcharge on the input. A breakpoint on the system
prompt, which every call of a step shares, is written once a step and read after that.

## Decision

`cloud.rs`: a cache-keyed Claude request puts its breakpoint on the system prompt, and the top-level one only
when it continues an exchange (an assistant turn, a tool call or result), which the next request repeats and
adds to: a chat round, a lesson re-ask. A one-off request still caches nothing. Lesson input to Claude costs a
third less (0.80x against 1.22x on these calls).

Not changed: the payload order. Putting the shared passages ahead of each call's own item would let the claim
checks reuse more (73% of their text repeats; 44% is a shared prefix), on Claude and on the local server, but
that reorders every lesson prompt the evals were measured on; on the 9B it would save about 5% of a lesson's
time.

Records: `measurements/2026-10-27-lesson-prompt-prefixes.txt`,
`measurements/2026-10-27-lesson-cache-haiku-as-sent.jsonl.gz`,
`measurements/2026-10-27-lesson-cache-haiku-system-breakpoint.jsonl.gz`.
