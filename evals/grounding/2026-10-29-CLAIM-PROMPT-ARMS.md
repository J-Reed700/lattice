# Claim judge prompt arms, 2026-10-29

Could a different prompt make the strict claim judge (`claim_verification`, the lesson check) score higher
on the local Qwen 3.8 27B without costing Claude? Five changes from the research were each tried alone
against today's prompt, on three models and both claim sets. **None helps every model, so today's prompt
stays.** The 27B gains up to 3 claims from a change, but every change that gives it those 3 costs Sonnet
1 to 3, and Haiku cannot tell any arm from today's prompt.

## Arms

Each arm is one change from the shipped strict judge, at batches of 16 claims over a shared evidence bank,
as lesson verification sends them. The code is on branch `claims/field-by-field` (commit `fc3111b9`),
switched on by `LATTICE_CLAIM_FORM`, `LATTICE_CLAIM_VARIANT` and `LATTICE_CLAIM_THINKING` in
`live_claim_pairs_eval`.

| Arm | Change | Why it was tried |
|---|---|---|
| base | today's prompt, rerun to measure noise | |
| fields | the answer as JSON fields in order (claimSays, passageSays, difference, sourcePassage, verdict) instead of three labelled lines | the comparison is written before the verdict, part by part |
| parts | the claim split into every factual part, each marked stated, missing or conflicts, before the verdict | claim splitting helped entailment models on WiCE (Kamoi et al., 2023); MiniCheck found almost no gain for GPT-4 from atomic facts, and Hu et al. (NAACL 2025) and the alignment bottleneck paper (arXiv 2602.10380) found splitting a trade-off between what it catches and the noise it adds |
| closed-book | one paragraph more: base every verdict solely on the passages, as if you knew nothing else about the subject | Lee et al. (arXiv 2601.07506): reasoning judges override a supplied reference with what they know, and this instruction cut those failures 9 to 16 points on GPT-4o |
| brief | the 1,100 words of rules (built up one fix at a time) rewritten as about 230, without the counter-scenario add-ons | IFScale (arXiv 2507.11538): models follow instructions less well as their number grows, and favour the earlier ones |
| think-on / think-off | the 27B asked with thinking on (it judges without), Haiku with thinking off (it judges with adaptive thinking at effort low) | a 2026 Qwen3 study (arXiv 2607.05861) found thinking caused errors on answers the model got right without it |

## Results

Gate right is whether the verdict lands on the right side of supported or not, which is what the provenance
gate acts on. A false support passes an unsupported claim into a lesson. Haiku and Sonnet were run twice on
the real claims; the 27B once, because it repeats itself (below).

**Real lesson claims** (`lesson-claims-w1/gold-w1.jsonl`, 85 .NET claims): gate right / false supports

| Arm | Qwen 3.8 27B | Haiku 5.5 | Sonnet 5.5 |
|---|---|---|---|
| base | 77 / 8 | 81 / 2, 79 / 5 | **83 / 2, 83 / 2** |
| fields | **80 / 5** | 82 / 3, 79 / 4 | 82 / 3, 81 / 4 |
| parts | 76 / 8 | 82 / 3, 80 / 5 | |
| closed-book | 79 / 6 | 78 / 4, 79 / 4 | |
| brief | **80 / 5** | 81 / 3, 80 / 4 | 80 / 3, 82 / 3 |
| think-on (27B) / think-off (Haiku) | 78 / 7 | 79 / 3, 78 / 5 | |

**Synthetic claims** (`claim-pairs-v1.jsonl`, 101): every arm on every model scored 99 to 101 with no false
support, so this set cannot separate them.

Sonnet was run on base and the two arms that helped the 27B most.

## What the numbers say

- **Sonnet is at the ceiling on today's prompt.** In both runs its only misses are the two "ASP.NET Core
  Identity and the Microsoft identity platform serve different purposes" claims. Every model passes those on
  every arm, and the two blind labellers split on them (one supported, one unsupported, adjudicated
  unsupported), so the label is the doubt there, not the judge. Fields adds a false support on the hedged
  "EnableSensitiveDataLogging configuration may need to be verified" claim in both runs, and one more in the second; brief adds one on the
  "configuration or best practice can ensure" claim and rejects two supported ones.
- **Haiku cannot tell the arms apart.** Today's prompt alone moves between 79 and 81 and between 2 and 5
  false supports from one run to the next, and every arm sits within a claim of that.
- **The 27B's gain is three borderline claims, not a mechanism.** It gives the same verdict on all 85 claims
  when the run is repeated (today's base against the batch-16 run of 2026-10-28). Fields, brief and
  closed-book change different things, yet each fixes the same three claims: two that are true in the world
  but not in the passage (the `field` keyword "can break code" that uses it, the "smoother path" quote), and
  "configuration or best practice can ensure" redaction. Any change to the prompt tips those three. Parts
  and thinking break other claims.
- **Batch size moves the 27B as much.** On today's prompt the 27B scored 80 / 4 one claim at a time and
  80 / 5 with all 85 together, against 76 to 78 at 2 to 32 (2026-10-28, records below). So 80 is what the
  27B reaches on today's prompt with a different batching, and a three-claim move is within what batching
  alone does to it.

## Decision

Under the rule that a prompt change ships only if it helps or holds on both Qwen and Claude without raising
false supports, nothing ships. Fields and brief each help the 27B by three claims and cost Sonnet one to
three. The variants stay on their branch to be rerun.

What would change this: a larger real-claim set. At 85 claims a model's run-to-run noise (Haiku's is 2 to 3
claims) is as large as any arm's effect.

## Cost

About $0.4 on Haiku and $3 on Sonnet, estimated from the runs' output tokens and the sets' size (the eval does
not record billed input). One Haiku run lost 94 minutes to a stalled connection and was rerun. The 27B runs on
the remote llama-server.

## Records

- `results/2026-10-29-claim-arms/`: one file per model, set, arm and repeat, `<model>-<set>-<arm>-<repeat>.jsonl.gz`,
  one row per claim (id, label, verdict, the judge's reason, batch, output tokens).
- `results/2026-10-28-claim-batches/`: today's prompt at batch sizes 1 to all, Haiku and the 27B,
  `<model>-<set>-b<N>.jsonl.gz`.
- `scripts/score_claim_arms.py [DIR]` prints the table above from either folder.
