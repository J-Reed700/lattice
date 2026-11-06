# Memory reviews that come back unread (2026-11-06)

Claude Haiku 5.5 through the Messages API, sent as `features/llm/cloud.rs` sends the reviewer's call, and Qwen3.8-27B
(Q5_K_S) on the remote llama-server. Code: `conversation/memory/verify.rs`. Replay: `scripts/replay_memory_review_hollow.py`.

## What it is

Some of Haiku's replies to the reviewer are one short object, and nothing else:

```json
{"verdicts":[{"id":"addition:c1","reason":"","verdict":"supported"}]}
```

Of 600 requests on main, 42 came back like this. All 42 name the first id and say "supported", and their reasons are
empty (38), `placeholder` (3) or `x` (1). Of 17 in a smaller run whose blocks were recorded, 14 hold one text block and 3
a thinking block and a text block. All 17 stop with `end_turn`, and most are 38 output tokens, so nothing cut them off. It
is valid against the schema: `reason` is a required string, and the empty string is one.

`verify.rs` kept each verdict's word and dropped its reason. So on a patch of one addition this counted as a complete review
that found the change supported, and memory committed it unread. On a larger patch the first addition was approved unread
and the rest stayed ambiguous, which holds the whole batch and repeats its extraction on a later attempt when any of them
is optional. `2026-11-02-TRANSITION-LABEL.md` saw these replies and noted that a left-out transition retires nothing. It
missed that the first addition gets approved.

## What does not fix it

Replies counted hollow when a requested id has no verdict with a reason, as `verify.rs` now counts them.

The reason's bound (`--arms now,bound,min,one,one_min`), 100 seeds a scenario:

| Arm | Hollow |
|---|---:|
| Schema as shipped | 42 of 600 |
| The reason's `maxLength: 512`, as before #301 | 74 of 600 |
| The reason's `minLength: 1` | 29 of 600 |
| One addition, as shipped | 15 of 300 |
| One addition, `minLength: 1` | 11 of 300 |

So removing `maxLength` did not cause this: the rate fell without it (p = .0023). `minLength` does not prevent it either,
because Claude's structured outputs accept the keyword without enforcing it.

The request's settings, 40 seeds a scenario (240 requests each):

| Arm | Hollow | Mean output tokens |
|---|---:|---:|
| Adaptive thinking, as shipped | 17 | 324 |
| The sentence "Return only JSON...", as before #43 | 21 | 315 |
| Effort medium | 22 | 322 |
| Effort high | 4 | 576 |
| Thinking disabled | 2 | 193 |
| Thinking disabled, effort low | 2 | 192 |
| Each id a required key of an object | 2 | 298 |

Three of these lower the rate, and each costs something:

- Effort high adds 78% to the output.
- A keyed schema turns the conflict's later statement "supported" 21 of 80 times, against none as shipped. Its transition
  stays right, but the addition beside it is approved.
- Thinking disabled is the cheapest, but it would change verdicts on every Claude model and was measured on one. Its false
  approvals on "moved on" lean upward (7 and 10 of 40 against 4 of 39, within noise).

## The fix

A verdict counts only with a reason: an empty reason or a single ASCII word is no verdict, and a reason in a script written
without spaces still is one. Of the 1,066 reviewer reasons stored from Haiku, the 27B and the 9B, this refuses only the 7
empty ones.

The reviews a reply did not read are asked for once more, on their own. What the second ask does not read stays ambiguous
and marks the review degraded, as a failed review always has. A reply that cannot be parsed is asked for again; a provider
failure is not, because the transport has already retried it. A second ask can only change what it asked about.

The contract moved to `memory-verifier/2026-11-06.13`, because committed memory records which contract reviewed it.

## The fix replayed

`--reask`, 50 seeds a scenario, 600 requests against main's prompt and schema:

| | Three reviews | One review |
|---|---:|---:|
| First replies hollow | 25 of 300 | 25 of 300 |
| Calls | 325 | 325 |
| Still unread after two asks | 4 | 1 |
| Transitions right | 296 | |
| Transitions wrong | 0 | |
| Transitions unread | 4 | |
| The addition supported | | 299 |
| The addition unread | | 1 |

So 45 of the 50 second asks read everything, what stays unread falls to 0.8% and ends ambiguous rather than approved, and
the cost is 8% more calls. A hollow reply now costs a call and approves nothing.

## The reviewer's schema after #301

`#301` removed `maxLength: 512` from the reviewer's reason. The six scenarios of `2026-11-02-REVIEWER-ON-HAIKU.md`, five times
in each arm:

| Model | Result |
|---|---|
| Qwen3.8-27B | 30 of 30 transitions right in every arm; the longest reason is 176 characters |
| Claude Haiku 5.5 | no transition read wrong; those it left unread are this document's subject |

So nothing relied on the bound.

## Left open

Thinking off for the reviewer gets about the same result as asking again without the extra calls. It is a fair thing to
measure separately on every Claude model the app offers, with conflicts and false approvals read by hand.

## Records

- `results/claude-haiku-5-5/2026-11-06-hollow-rate.jsonl.gz`: the reason's bound.
- `results/claude-haiku-5-5/2026-11-06-hollow-settings.jsonl.gz`: the request's settings.
- `results/claude-haiku-5-5/2026-11-06-hollow-reask.jsonl.gz`: the fix replayed.
- `results/claude-haiku-5-5/2026-11-06-reviewer-rules.jsonl.gz`, `results/qwen3.8-27b-remote/2026-11-06-reviewer-rules.jsonl.gz`:
  the reviewer's schema after #301.
