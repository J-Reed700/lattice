# The memory reviewer's transition label (2026-11-02)

Claude Haiku 5.5 through the Messages API, sent as `features/llm/cloud.rs` sends the reviewer's call, and Qwen3.8-27B
(Q5_K_S) on the remote llama-server. Code: `conversation/memory/verify.rs`, `conversation/memory/prompts.rs`.

## What changed

`verify.rs` built a transition's review with the retired item's own label, and `render_verifier_prompt` sent it as
`proposed_label`. A correction ("Update: deploy only to staging, after approval") reached the reviewer as proposing "Do
not deploy anywhere". `2026-11-02-REVIEWER-ON-HAIKU.md` found it in Haiku's reasons.

A transition's review now sends the label of the replacement candidate the transition names as `proposed_label`, and
`null` when it names none (a `resolved` transition with nothing in its place). `existing_item_label` and
`existing_item_evidence` are still the retired item's, and additions are unchanged. The design (section 6.1) has a
transition identify the item it retires, its state, and a replacement candidate when relevant; the replacement is what
the later passage has to support, and `apply.rs` already treats the two as one claim.

The reviewer contract moved to `memory-verifier/2026-11-02.11` (`memory-validator/6+memory-verifier/2026-11-02.11`),
because committed memory records which contract reviewed it. The prompt text did not change.

Two tests in `conversation/memory/tests.rs` run a pass through the real job with a fake model and read the reviewer's
prompt. On the old code both failed, with the retired item's label in `proposed_label`; they pass now, with the rest of
the memory module (58 tests).

## Method

`scripts/replay_memory_review_ab.py`, with the transition built as the fixed app builds it: it replaces c1 with c2, so
`proposed_label` is c2's and `existing_item_label` is c1's. Otherwise as in the Haiku write-up: the six scenarios, five
seeds each, under the prompt before #280 (`8426b641^`) and main's, each with and without the added open-matter question.
Haiku cost $0.03. The 27B ran at temperature 0.2 with `cache_prompt` off.

## Results

Transitions given the verdict their scenario needs, of 30. A reply that leaves the transition out is counted apart from
a wrong verdict.

| Arm | Haiku, old request | Haiku, fixed | 27B, fixed |
|---|---|---|---|
| Before #280 | 23 right, 7 wrong | 29 right, 1 left out | 30 |
| Before #280, with the question | 21 right, 8 wrong, 1 left out | 29 right, 1 left out | 30 |
| **#280, as shipped** | **25 right, 5 wrong** | **28 right, 2 left out** | **30** |
| #280, with the question | 24 right, 4 wrong, 2 left out | 30 | 30 |

Under #280, per scenario:

| Scenario | Needs | Haiku, old request | Haiku, fixed | 27B, fixed |
|---|---|---|---|---|
| clean correction | supported | 4 of 5 | 4 of 5, 1 left out | 5 of 5 |
| partial revocation | supported | 1 of 5 | 5 of 5 | 5 of 5 |
| moved on | unsupported | 5 of 5 | 4 of 5, 1 left out | 5 of 5 |
| the other three | | 5 of 5 each | 5 of 5 each | 5 of 5 each |

Haiku's 24 wrong verdicts are gone: every transition verdict it returned after the change was the one the scenario
needs (116 of 116). The 27B was 30 of 30 on #280's rules before, and is 30 of 30 on every arm now (120 of 120).

## What reading the reasons found

All 116 of Haiku's transition reasons and all 120 of the 27B's were read by hand.

- **Haiku, old request.** All 24 wrong verdicts blamed the label ("the replacement label repeats the retired 'Do not
  deploy anywhere', which the newer passage does not state"), and six right ones complained of it ("though the proposed
  label oddly repeats the old value").
- **Haiku, fixed.** None mentions the label. The partial revocation reasons read the passage: "explicitly replaces the
  blanket prohibition with the staging-after-approval rule", "keeps the production prohibition". The clean correction
  reasons say the date moved from 12 to 19 August.
- **27B, fixed.** All 120 are accurate readings of the passages, and none mentions the label. One is loose on a
  conflict case ("the old certainty remains valid") with the right verdict.

The four replies Haiku left a transition out of are the same short object,
`{"verdicts":[{"id":"addition:c1","reason":"","verdict":"supported"}]}`: three of 38 output tokens and one of 273, all
`end_turn`. The old run had three such replies, so the label does not explain them. The app records an omitted id as
ambiguous and marks the review degraded, so none of them retires anything.

## What this does not show

- **Six scenarios.** They are the ones written for the conflict case, not a sample of real memory changes. In each, the
  transition replaces a candidate from the same response; a transition on a committed item takes the same path in
  `verify.rs` but was not replayed.
- **A hand-built request.** The script mirrors `verify.rs`. The app's own request is pinned by the Rust tests, and the
  two were compared by reading, not by diffing a captured prompt.
- **The 27B before.** Only #280's 30 of 30 for its own rules is on record; the other three arms were not run on the old
  request.
- **Two models.** Sonnet, Opus and the 9B were not run.
- **The kind.** A transition's review still carries the retired item's `kind`. A replacement of another kind, such as a
  constraint replaced by an open question, was not replayed.

Records: `results/claude-haiku-5-5/2026-11-02-transition-label.jsonl.gz`,
`results/qwen3.8-27b-remote/2026-11-02-transition-label.jsonl.gz`.
