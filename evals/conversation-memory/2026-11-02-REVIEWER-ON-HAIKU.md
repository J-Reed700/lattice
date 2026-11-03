# The memory reviewer on Haiku (2026-11-02)

Claude Haiku 5.5 through the Messages API, sent as `features/llm/cloud.rs` sends the reviewer's call: adaptive
thinking, the verdict schema as its output format, no sampling settings. Haiku is the verifier model in the app's
settings. Code: `conversation/memory/prompts.rs`, `conversation/memory/verify.rs`.

## Why

#280 grouped the reviewer's rules by verdict and had the first rule that applies decide. It was measured on the 9B (19
of 30 transitions right before it, 28 after) and the 27B (30 of 30), but not on Haiku.

## Method

`scripts/replay_memory_review_ab.py --claude claude-haiku-5-5`, which now sends its requests through
`scripts/replay_claude.py`. The six scenarios ran five times each, under the prompt before #280 (`8426b641^`) and
main's, each with and without the added open-matter question. Cost: $0.04.

## Results

Transitions given the verdict their scenario needs, of 30:

| Arm | Haiku 5.5 |
|---|---:|
| Before #280 | 23 |
| Before #280, with the question | 21 |
| **#280, as shipped** | **25** |
| #280, with the question | 24 |

#280 helps Haiku too. Under it, every case is right except two:

| Scenario | Needs | Before #280 | #280 |
|---|---|---|---|
| clean correction | supported | 2 of 5 | 4 of 5 |
| partial revocation | supported | 1 of 5 | 1 of 5 |

The question arms lost three verdicts outright: the reply held only the first addition's.

## What reading the reasons found

Haiku's reasons on those cases point at the payload, not at the rules. "The replacement label repeats the retired 'Do
not deploy anywhere', which the newer passage does not state." The script builds a transition as `verify.rs` does:
an `OwnedReview` whose `label` is the existing item's, sent as `proposed_label`. So a supersede asks the reviewer to
accept the old wording as what the later passage proposes. The prompt itself says a superseded item's evidence "was
refreshed to the later wording". Haiku reads the label literally and refuses. The 27B passed the same requests 30 of
30.

That is a defect in the app's request, and it is fixed separately, test first, and measured again on all three models.

## What this does not show

- **Six scenarios.** They are the ones written for the conflict case, not a sample of real memory changes.
- **One model.** Sonnet and Opus were not run.

Records: `results/claude-haiku-5-5/2026-11-02-reviewer-rules.jsonl.gz`.
