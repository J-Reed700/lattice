# The shipped reading on Opus 5.5 (2026-11-02)

Claude Opus 5.5 through the Messages API, sent as `features/llm/cloud.rs` sends a reading. Code:
`chat/tool_loop/reading/groups.rs`, `llm/cloud.rs`, `scripts/replay_claude.py`.

## Why

2026-10-28 settled on one group in the new wording, measured on the 9B, the 27B, Haiku and Sonnet. Opus 5.5 was not
run, because it cannot turn thinking off: `claude_thinking_off` has no entry for it, so the app sends its readings with
adaptive thinking at low effort. The replay sent `disabled` to every model it had no entry for, so it could not send
what the app sends.

## Method

`scripts/replay_grouped_reading_ab.py --claude claude-opus-5-5 --thinking off --arms G64`, on the 2026-10-26 readings
and rubrics, with every passage in one group in the shipped wording. `replay_claude.py` now asks each model not to
think the way `cloud.rs` does: `disabled` for Haiku 5.5, `between_tools` for Sonnet 5.5, and adaptive thinking for a
model with no off form. Every request for no thinking also carries `output_config: {"effort": "low"}`. The app sends
that too, and the earlier replays did not.

The run stopped at its $6 budget after 48 of the 60 readings. The 12 not read are six of `rm-long8-r3` and the six of
`rm-short6-r1`.

## Results

| Model | Readings | Facts noted | Missed readings | Non-fact lines | of them whole passages | Median s/reading | Dollars |
|---|---:|---:|---:|---:|---:|---:|---:|
| Haiku 5.5 (2026-10-28, new, off) | the same 48 | 250/258 | 8 | 12 | 1 | — | — |
| **Opus 5.5** | 48 | **258/258** | **0** | 13 | 1 | 4.5 | 6.12 |

The long readings were 252/252 with no non-fact line, and the short ones 6/6.

Every non-fact line was read by hand, and each is a verbatim copy. Twelve are in `dp-buried-agreement-meeting`: the
synthetic agreement repeats "The parties shall attempt to resolve a dispute at contract-manager level before any
escalation." across its passages, and Opus copied it for each one. The question asks what must happen before anyone
goes to court, so the line does bear on it. The thirteenth is passage 10 of `dp-paraphrase-clock`, copied whole, as
Haiku also copied it. Haiku's other eleven fall elsewhere: five more in `dp-paraphrase-clock` and one in each of six
other readings.

The money went on cache writes: 573,272 of the 1,131,015 input tokens. The replay caches the turn's message as the app
does, but with one group nothing later in a replayed reading reads it back. In the app, the answer that follows does.

## What this does not show

- **48 of 60 readings.** The budget ran out before the last 12, so the short set has 6 readings here, not 12.
- **One run.** Fable 5 and 5.1 were not run.
- Facts are planted details scored by spelling, as before.

Records: `results/chat/claude-opus-5-5/2026-11-02-reading-new-off.jsonl.gz`.
