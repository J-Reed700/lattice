# Claim judge prompt arms, 2026-10-29

Could a different prompt make the strict claim judge (`claim_verification`, the lesson check) right more often
on the local Qwen 3.8 27B, without costing Claude? Seven changes were each tried alone against today's prompt.
**None gains the 27B a single claim, so today's prompt stays.** The two changes aimed at the 27B's own errors
each trade about half a false support for half a false rejection.

## Arms

Each arm is one change from the shipped strict judge, at batches of 16 claims over a shared evidence bank, as
lesson verification sends them. The code is on branch `claims/field-by-field`, switched on by
`LATTICE_CLAIM_FORM`, `LATTICE_CLAIM_VARIANT` and `LATTICE_CLAIM_THINKING` in `live_claim_pairs_eval`.

| Arm | Change | Why it was tried |
|---|---|---|
| base | today's prompt | |
| fields | the answer as JSON fields in order (claimSays, passageSays, difference, sourcePassage, verdict) instead of three labelled lines | the comparison is written before the verdict, part by part |
| parts | the claim split into every factual part, each marked stated, missing or conflicts, before the verdict | claim splitting helped entailment models on WiCE (Kamoi et al., 2023); MiniCheck found almost no gain from atomic facts for GPT-4, and Hu et al. (NAACL 2025) found splitting trades what it catches against the noise it adds |
| closed-book | one more paragraph: base every verdict solely on the passages, as if you knew nothing else about the subject | Lee et al. (arXiv 2601.07506): reasoning judges override a supplied reference with what they know; this instruction cut those failures 9 to 16 points on GPT-4o |
| brief | the 1,100 words of rules (built up one fix at a time) rewritten as about 230 | IFScale (arXiv 2507.11538): models follow instructions less well as their number grows |
| named-parts | the rule "every factual part, including numbers, dates, names, quantities and negations" extended with "every qualifier or hedge (such as may, can, by default or some), every causal or logical link (such as so, because or therefore) and each clause the claim joins" | the 27B's false supports are claims true but for one hedge, one "so" or one extra clause |
| no-paraphrase | the sentence allowing "ordinary synonyms, faithful paraphrases and logically necessary consequences" removed | the same errors: the 27B reads an added consequence as a faithful paraphrase |
| think-on / think-off | the 27B asked with thinking on (it judges without), Haiku with thinking off (it judges with adaptive thinking) | a 2026 Qwen3 study (arXiv 2607.05861) found thinking caused errors on answers the model got right without it |

## Method

The real claims are `lesson-claims-w1/gold-w1.jsonl`: 85 .NET lesson claims against public Microsoft Learn
pages, each labelled by two blind labellers. The 27B gives the same verdict on the same input every time, but its
verdicts move with which claims share a batch. So every arm ran on the same five batch orders: the set's own
order (`r1`) and four seeded shuffles (`r2` to `r5`). Haiku and Sonnet vary run to run as well. Haiku ran the
set's order twice and the four shuffles. Sonnet ran the set's order twice and one shuffle. Each arm is compared
with base claim by claim on the orders both ran, with a 95% bootstrap interval over the 85 claims
(`scripts/score_claim_arms.py --paired`).

"Right" is whether the verdict lands on the right side of supported, which is what the provenance gate acts on.
A false support lets an unsupported claim into a lesson. A false rejection holds a true one back for review.

## Results

Today's prompt, averaged over its runs: the 27B 78.8 of 85 right (6.2 false supports), Haiku 78.2 (4.0),
Sonnet 82.0 (2.7).

Change against base, per claim, averaged over the orders both ran:

| Model | Arm | Orders | Right (95% CI) | False supports | False rejections |
|---|---|---|---|---|---|
| Qwen 3.8 27B | fields | 5 | +0.0 (−1.8 to +1.6) | −0.2 (−2.0 to +1.6) | +0.2 (+0.0 to +0.6) |
| Qwen 3.8 27B | parts | 5 | −1.2 (−2.8 to +0.4) | +0.8 (−0.6 to +2.4) | +0.4 (+0.0 to +1.0) |
| Qwen 3.8 27B | closed-book | 5 | −1.0 (−2.6 to +0.4) | +0.2 (−1.0 to +1.6) | +0.8 (+0.2 to +1.6) |
| Qwen 3.8 27B | brief | 5 | −1.2 (−2.6 to +0.2) | +0.2 (−0.8 to +1.2) | +1.0 (+0.2 to +2.2) |
| Qwen 3.8 27B | named-parts | 5 | +0.0 (−1.2 to +1.4) | −0.6 (−1.8 to +0.4) | +0.6 (+0.0 to +1.4) |
| Qwen 3.8 27B | no-paraphrase | 5 | +0.0 (−1.0 to +1.2) | −0.4 (−1.4 to +0.4) | +0.4 (+0.0 to +1.0) |
| Qwen 3.8 27B | think-on | 5 | −0.2 (−2.2 to +1.4) | +0.2 (−1.6 to +2.2) | +0.0 |
| Haiku 5.5 | fields | 6 | +2.0 (−0.0 to +3.8) | −0.2 (−1.7 to +1.3) | −1.8 (−3.2 to −0.8) |
| Haiku 5.5 | parts | 6 | +1.3 (−0.8 to +3.5) | +0.5 (−0.8 to +2.0) | −1.8 (−3.5 to −0.3) |
| Haiku 5.5 | closed-book | 6 | −0.5 (−2.2 to +1.0) | +0.5 (−0.5 to +1.7) | +0.0 (−1.2 to +1.3) |
| Haiku 5.5 | brief | 6 | +1.0 (−1.0 to +3.0) | +0.5 (−0.7 to +2.0) | −1.5 (−3.2 to −0.2) |
| Haiku 5.5 | think-off | 6 | −0.8 (−2.5 to +1.0) | +0.3 (−1.2 to +1.8) | +0.5 (−0.5 to +1.5) |
| Sonnet 5.5 | fields | 3 | −0.7 (−2.7 to +1.0) | +0.7 (−0.7 to +2.3) | +0.0 (−1.0 to +1.0) |
| Sonnet 5.5 | parts | 3 | +0.0 (−2.0 to +1.3) | +0.3 (−1.0 to +2.0) | −0.3 (−1.0 to +0.0) |
| Sonnet 5.5 | closed-book | 3 | −0.7 (−2.3 to +0.7) | +0.3 (−1.0 to +2.0) | +0.3 (+0.0 to +1.0) |
| Sonnet 5.5 | brief | 3 | −1.7 (−3.7 to +0.0) | +0.7 (−0.7 to +2.3) | +1.0 (+0.0 to +2.3) |
| Sonnet 5.5 | think-off | 3 | −0.7 (−2.7 to +0.7) | +0.3 (−1.0 to +2.0) | +0.3 (+0.0 to +1.0) |

named-parts and no-paraphrase ran only on the 27B: they were to go to Claude only if they helped the 27B.

**Synthetic claims** (`claim-pairs-v1.jsonl`, 101 pairs): every arm on every model scored 99 to 101 with no
false support, so this set cannot separate arms.

## What the numbers say

- **No arm helps the 27B.** fields, named-parts and no-paraphrase come out even. The rest lose about a claim,
  closed-book and brief through false rejections whose intervals exclude zero.
- **The 27B's errors are the same few claims under every prompt.** Its false supports are claims true but for
  one piece: a hedged clause ("may need to be verified"), a "so" read as "and", a fact from outside the passage,
  or a quote taken from a passage that belongs to another claim. Its errors also include the two
  "ASP.NET Core Identity and the Microsoft identity platform serve different purposes" claims, which every model
  marks supported and the two labellers split on. Naming hedges and causal links in the rule, or removing the
  paraphrase allowance, turns a false support into a false rejection about as often as it fixes one.
- **On a single run, batching moved the 27B as much as any prompt did.** Today's prompt scored 77 to 80 over the
  five orders. The first single-order run of fields and brief showed +3. On five orders that is +0.0 and −1.2.
- **Haiku likes fields** (+2.0, the interval just touching zero), by rejecting fewer true claims. Sonnet does
  not (−0.7), and the 27B gains nothing.

## Decision

Under the rule that a prompt change ships only if it helps the local model and holds on Claude, nothing ships.
The variants stay on `claims/field-by-field` and are not merged.

## Cost

About $1.5 on Haiku and $7 on Sonnet, estimated from the runs' output tokens and the set's size (the eval does not
record billed input). The 27B runs on the remote llama-server. One 27B run lost its connection partway and was
resumed from the claims it had written.

## Records

- `results/2026-10-29-claim-arms/`: one file per model, set, arm and run, `<model>-<set>-<arm>-<run>.jsonl.gz`, one
  row per claim (id, label, verdict, the judge's reason, batch size, output tokens), in the order the claims were
  sent.
- `results/2026-10-28-claim-batches/`: today's prompt at batch sizes 1 to all, Haiku and the 27B,
  `<model>-<set>-b<N>.jsonl.gz`.
- `scripts/score_claim_arms.py [DIR]` prints every run's score. `--paired` prints the table above.
