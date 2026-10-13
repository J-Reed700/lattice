# Claim-checker bake-off, 2026-10-12

Item 13 of the LLM-stack audit asks whether Lattice's claim checker, a
lexical pre-check followed by a first-token logprob judge on the local model,
is the right one, or whether a dedicated grounding model would do better. This
report measures it against MiniCheck, HHEM-2.1-open and LettuceDetect on the
101 labelled pairs in [`claim-pairs-v1.jsonl`](claim-pairs-v1.jsonl)
([README](README.md)). It reports; nothing in the checker was changed.

## Summary

- **Lattice as shipped scores 0.827 balanced accuracy (95% CI 0.76–0.89).**
  MiniCheck-Flan-T5-Large scores **0.913 (0.85–0.96)** and is the only checker
  significantly better on the same pairs (exact McNemar, 18 vs 7 discordant,
  p = 0.043).
- **The judge is not the weak part. The lexical shortcut in front of it is.**
  The judge saw 78 pairs and gave 66 definite verdicts, 65 of them right,
  every one with the correct three-way label. It declined the other 12 under
  its 0.75 confidence floor. 23 pairs never reached it, and 17 of those 23 are
  false: the strong-overlap rule in `needs_judge` certified them as
  supported. **Those 17 are 17 of Lattice's 18 false passes.**
- MiniCheck-Flan-T5-Large flags all 17, and MiniCheck-RoBERTa-Large flags 15.
  HHEM-2.1-open and LettuceDetect are not better than Lattice here. HHEM finds
  9 of 25 contradictions.
- **Recommendation: keep the judge and the per-claim design, and change what
  decides whether the judge is asked.** This track changes no thresholds; the
  options are below. A learned pre-filter is worth porting only to replace
  that shortcut, and MiniCheck-RoBERTa-Large is the cheap one to try.

## Setup

| | |
| --- | --- |
| pairs | 101: 40 supported, 36 unsupported, 25 contradicted. Synthetic, written to stress named failure modes, one annotator. |
| Lattice | The shipped `GroundingVerifier::verify` via [`src-tauri/examples/claim_checker_eval.rs`](../../src-tauri/examples/claim_checker_eval.rs). Each pair is a one-paragraph answer ending in `[1]`, with the passage as source 1. Two runs: no judge (`lattice-lexical`) and with the judge (`lattice`). |
| judge model | unsloth Qwen3.5-9B Q4_K_M on the bundled llama-server (llama.cpp v0.6.0), Metal, `-ngl 99 --ctx-size 4096 -np 1 --jinja`. Greedy, as the default verification settings ask. |
| thinking | Off, set server-wide with `--chat-template-kwargs '{"enable_thinking":false}'`. Lattice's client sends no switch. A probe showed the rendered prompt ending in an empty `<think></think>`, and all 81 judge calls returned no reasoning text. |
| external | One process per checker on CPU with 4 threads (torch 2.14.1, transformers 5.19.0), via [`scripts/claim_checker_bakeoff.py`](../../scripts/claim_checker_bakeoff.py). Each used its publisher's decision rule: MiniCheck P(supported) > 0.5, HHEM score ≥ 0.5, LettuceDetect "any token hallucinated". Revisions are recorded in `results/*.meta.json`. |
| scoring | [`scripts/claim_checker_bakeoff_score.py`](../../scripts/claim_checker_bakeoff_score.py). Binary view, "supported" positive. Lattice's "unverified" counts as not supported. 95% bootstrap CI over pairs. |
| machine | Apple M3 Max, 36 GB |

## Results

| checker | acc | bal. acc (95% CI) | prec. supported | recall supported | recall unsupported | recall contradicted | 3-way acc | unverified | median ms/pair |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | ---: |
| Lattice, lexical only | 0.465 | 0.549 (0.49–0.60) | 0.422 | 0.950 | 0.194 | 0.080 | 0.446 | 0 | 0 |
| **Lattice, lexical + judge** | 0.802 | **0.827** (0.76–0.89) | 0.679 | 0.950 | 0.667 | 0.760 | 0.703 | 12 | 2,063 |
| MiniCheck-Flan-T5-Large | 0.911 | **0.913** (0.85–0.96) | 0.860 | 0.925 | 0.917 | 0.880 | – | – | 180 |
| MiniCheck-RoBERTa-Large | 0.871 | 0.863 (0.79–0.93) | 0.846 | 0.825 | 0.917 | 0.880 | – | – | 111 |
| LettuceDetect-large | 0.772 | 0.799 (0.72–0.87) | 0.649 | 0.925 | 0.806 | 0.480 | – | – | 136 |
| LettuceDetect-base | 0.762 | 0.786 (0.71–0.86) | 0.643 | 0.900 | 0.778 | 0.520 | – | – | 60 |
| HHEM-2.1-open | 0.713 | 0.758 (0.69–0.83) | 0.582 | 0.975 | 0.667 | 0.360 | – | – | 49 |

"Recall unsupported/contradicted" is the share of those pairs not called
supported. Only Lattice separates the two negative classes. Its exact recall
is 0.68 on contradicted and 0.44 on unsupported. The gap is mostly abstention:
8 of the 12 unverified pairs are gold unsupported.

Paired exact McNemar tests on binary correctness: MiniCheck-Flan-T5 vs Lattice
p = 0.043; MiniCheck-RoBERTa vs Lattice p = 0.23; Lattice vs LettuceDetect-large
p = 0.70; Lattice vs HHEM p = 0.14; MiniCheck-Flan-T5 vs MiniCheck-RoBERTa
p = 0.42. With 101 pairs, nothing else separates.

**Best macro-F1 thresholds.** These are chosen on the same 101 pairs, so they
are optimistic and are not a tuning recommendation:

| checker | threshold on P(supported) | macro-F1 | bal. acc | prec. supported |
| --- | ---: | ---: | ---: | ---: |
| MiniCheck-Flan-T5-Large | 0.778 | 0.937 | 0.934 | 0.947 |
| MiniCheck-RoBERTa-Large | 0.366 | 0.888 | 0.897 | 0.822 |
| Lattice (judge P(supported); 1.0 when not judged) | 0.293 | 0.822 | 0.852 | 0.690 |
| HHEM-2.1-open | 0.809 | 0.810 | 0.827 | 0.706 |
| LettuceDetect-large / base | 0.840 / 0.854 | 0.817 / 0.808 | 0.822 / 0.818 | 0.750 / 0.723 |

No threshold on the judge's probability can recover Lattice's false passes,
because those pairs were never judged.

## What reaches Lattice's judge

**23 of 101 pairs (23%) never reached the judge.** In each, the lexical pass
found strong overlap (ratio ≥ 0.55 with ≥ 4 matching tokens), and the claim
held no digit, month or negation marker, so `needs_judge` returned false.

Only 6 of those 23 are supported. The other 17 (11 unsupported, 6
contradicted) were certified supported without a model ever reading them. They
cover every vocabulary-preserving failure in the set:

- dropped negation (`cp-025`, `cp-088`);
- swapped entity (`cp-004`, `cp-055`);
- reversal (`cp-018`, `cp-072`);
- an extra unsupported clause (`partial`, 6 pairs);
- scope (`cp-037`, `cp-047`);
- world knowledge (`cp-038`, `cp-098`);
- the Tuesday/Thursday swap (`cp-020`).

The judge's own record on the 78 it saw: 81 calls (three pairs needed two),
all `finish_reason = stop`, no errors, a label as the top first token in 81 of
81. The median prompt was 473 tokens, the median reply 69 tokens, and a call
took 2.3 s. Its one false pass, `cp-006`, is shared with every external model
(below).

It declined 12 pairs under the 0.75 confidence floor (`low_confidence`), and
10 of those are gold negatives. On those 12, the judge's top label would have
been right 11 times. The one wrong top label was a "contradicted" on a
supported paraphrase (`cp-034`, at 0.49), which the floor kept from being
shown. On this set, the floor withheld 11 correct findings to avoid one false
accusation.

This set is adversarial, so the shortcut's error rate here (17 of 23) is not
its rate on real answers, where most strongly overlapping sentences are
presumably true. The mechanism is general, though. `is_contradiction_prone`
reads only the claim, so a claim that keeps the passage's words and loses its
"not" has nothing in it to escalate.

Accuracy by difficulty tag (pairs correct, binary):

| tag | n | Lattice lexical | Lattice | MiniCheck-T5 | MiniCheck-RoBERTa | HHEM | Lettuce-large |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| partial | 17 | 1 | 11 | 17 | 17 | 13 | 16 |
| distractor_numbers | 17 | 12 | 16 | 15 | 15 | 13 | 13 |
| lexical_trap | 16 | 1 | 10 | 13 | 13 | 8 | 7 |
| negation | 10 | 6 | 8 | 9 | 10 | 7 | 9 |
| world_knowledge | 9 | 5 | 7 | 9 | 9 | 8 | 9 |
| table | 9 | 6 | 9 | 8 | 6 | 7 | 5 |
| number_drift | 8 | 0 | 8 | 7 | 8 | 1 | 3 |
| wrong_entity | 5 | 1 | 3 | 5 | 4 | 3 | 3 |
| cross_lingual | 4 | 2 | 4 | 4 | 2 | 4 | 3 |
| unit/date drift | 6 | 0 | 5 | 5 | 4 | 3 | 3 |
| reversal + scope | 6 | 0 | 2 | 6 | 5 | 2 | 2 |

Where the judge reads the claim (numbers, units, dates, tables), Lattice is
at or tied for the top. Where the shortcut decides (partial, lexical traps,
reversals), it is the weakest of the six checkers.

## Four instructive disagreements

**1. A dropped negation never reaches the judge (`cp-025`, contradicted).**
Passage: "There is no other way round it: a local addendum cannot create one
and a manager cannot grant one." Claim: "A manager can grant an exception to
the full-disk encryption rule." Every content word is in the passage and the
claim has no "not", so Lattice certified it supported on overlap alone.
MiniCheck-Flan-T5 gives P(supported) 0.03 and LettuceDetect-large 0.16. HHEM
passes it at 0.73.

**2. A swapped entity, built entirely from the passage's words (`cp-004`,
contradicted).** Passage: "The Sepwick field crew owns the pager for
ingest-fanout; the Halloway reliability bench owns the underlying storage."
Claim: "The Halloway reliability bench owns the pager for ingest-fanout."
Overlap is total, so the judge is never asked. MiniCheck-Flan-T5 catches it
(0.07). MiniCheck-RoBERTa (0.96) and LettuceDetect-large (0.97) do not.

**3. An invented "because" fools everything (`cp-006`, unsupported).**
Passage: "Emeric Losk prefers to hold the change until Wednesday rather than
ship into a thin on-call week. The December drill found that the Merridew
platform team could not reach the vendor desk inbox from the field tablet at
all." Claim: Losk "wants to hold the change until Wednesday because the
December drill showed the vendor desk inbox was unreachable". Lattice's judge
said supported at 0.95 ("prefers to delay the change until Wednesday due to
findings from a December drill"). All five external runs scored it
0.92–1.0. A causal link between two stated facts is the hardest case in the
set: 3 of 5 for Lattice and both MiniChecks, 1 of 5 for HHEM.

**4. The judge does what the classifiers cannot: arithmetic and units
(`cp-062` supported, `cp-057` contradicted).** For "The sensor heads on
PO-63276 come to £168,883.11 in total" against a table of 33 × £5,117.67, the
judge answered supported (0.92) with the calculation as its reason. Every
external model rejected it (0.00–0.05). For "The written notice needed to
prevent renewal of the Aldersgate Print agreement is 60 days" against "Either
party may prevent renewal by giving 60 hours of written notice", the judge
answered contradicted (0.97) and named both values. Four of the five
external runs passed it (0.73–0.90); only LettuceDetect-base flagged it
(0.44).

## Latency

| checker | where | median per pair | p95 |
| --- | --- | ---: | ---: |
| Lattice lexical | in process | < 1 ms | 1 ms |
| Lattice judge | 9B Q4_K_M on Metal, one slot | 2,063 ms (judged pairs ≈ 2.3 s per call) | 3,066 ms |
| HHEM-2.1-open | CPU, 4 threads, PyTorch | 49 ms | 73 ms |
| LettuceDetect-base / large | CPU, 4 threads | 60 / 136 ms | 84 / 198 ms |
| MiniCheck-RoBERTa-Large | CPU, 4 threads | 111 ms | 166 ms |
| MiniCheck-Flan-T5-Large | CPU, 4 threads | 180 ms | 265 ms |

The whole Lattice pass took 184 s for 101 pairs. In chat, the judge runs after
the answer is on screen and shares the model's slot with the user's next turn.
A small classifier would run in process in about a tenth of the time, with no
slot contention. Python-reference latency is only a proxy for a Candle port.

## Porting cost

Lattice pins Candle 0.10.2. The brief's premise that the runtime supports only
BERT, Jina and Qwen3 needs a correction. The embedding loader
(`features/embedding/candle_service.rs`) already loads `bert`, `distilbert`,
`xlm-roberta`/`roberta`, `jina_bert`, `nomic_bert`, `modernbert` and Qwen3
encoders. The reranker loads a BERT cross-encoder with a one-logit
`classifier`, or Qwen3-Reranker. What none of them has is a pair- or
token-classification head with two labels, or any T5.

| candidate | architecture | parameters (measured) | in Candle 0.10.2 | Lattice today | work |
| --- | --- | ---: | --- | --- | --- |
| Lattice judge | the chat/utility LLM, one completion per claim | 0 new | – | ships | none |
| MiniCheck-RoBERTa-Large | RoBERTa-large `ForSequenceClassification`, 2 labels, 512 tokens | 355 M | `xlm_roberta::XLMRobertaForSequenceClassification` (same layout) | encoder loads as `roberta` | **small**: wire the upstream head, MiniCheck's sentence chunking with max over chunks; BPE `tokenizer.json` in the repo; weights only as `pytorch_model.bin` |
| MiniCheck-Flan-T5-Large | T5 encoder-decoder; P(supported) from the first decoded token | 783 M | `t5::T5ForConditionalGeneration` | no T5 | **medium**: T5 loader, SentencePiece, one decoder step, 3.1 GB fp32 |
| HHEM-2.1-open | Flan-T5-base encoder + two-way token head read at position 0 | 110 M | `t5::T5EncoderModel` | no T5 | **medium-small**: T5 encoder loader and a `Linear` head; fixed prompt template |
| LettuceDetect-base / large | ModernBERT `ForTokenClassification`, 8k context | 150 M / 396 M | `modernbert::ModernBert` and `ModernBertHead`; no token-classification wrapper | encoder loads as `modernbert` | **small**: per-token head and the span rule |

Licences per the model cards: MiniCheck and LettuceDetect MIT, HHEM-2.1-open
Apache-2.0. "Small" means days and "medium" about a week, each including a
parity test against the Python reference on these pairs.

## Recommendation

1. **Keep** the per-claim, no-silent-rewrite design and the LLM judge as the
   verdict-maker. On the pairs it reads, the judge is right 65 times in 66.
   It is the only checker that separates contradicted from unsupported, and
   the only one that handles arithmetic. Its reasons name both values.
2. **Change what decides whether the judge is asked. Measure that first with
   this harness; it is not done in this track.** The candidates, cheapest
   first:
   - Send every cited claim to the judge. On this set that is 23 more calls,
     about 2.3 s each on the 9B.
   - Let `is_contradiction_prone` fire when the cited window holds a negation
     or number that the claim lacks.
   - Replace the strong-overlap skip with a learned pre-filter that escalates
     anything it does not confidently support.
3. **Port only for that third option.** MiniCheck-RoBERTa-Large is the cheap
   port, because Lattice already loads RoBERTa encoders. It flagged 15 of the
   17 shortcut passes at 111 ms on CPU. MiniCheck-Flan-T5-Large is more
   accurate (17 of 17, best overall) but needs a T5 port. Neither should
   replace the judge, since both are binary and miss units and arithmetic.
   **Do not port** HHEM-2.1-open (contradiction recall 0.36) or LettuceDetect
   (0.48–0.52).
4. **Thresholds unchanged.** `MIN_VERDICT_CONFIDENCE` = 0.75 declined 12
   judged pairs. The judge's top label was right on 11 of them, and the floor
   prevented one false "contradicted". Whether that trade is worth it is a
   product decision. With 12 pairs it is a note, not a finding; re-measure it
   after the change in item 2.

## What this does not show

- **Labels.** The pairs are synthetic, fictional and adversarial, with one
  annotator and no blind second pass. Rates here, especially the shortcut's
  17 of 23, do not transfer to real answers.
- **Thinking was off.** The coordinator reports that llama-server turns
  thinking on when a request carries no switch, and Lattice's judge sends
  none. The shipped judge on this 9B may therefore think, and that would be
  slower and possibly different. It was not measured.
- **Budget.** Each pair got the judge's full 90 s budget. In chat that budget
  covers a whole answer. Here every call finished, so the budget never bit.
- **Window selection barely ran.** Passages are at most 1,458 characters (one
  or two windows), so Lattice's window selection hardly engaged. The external
  models read the whole passage and the whole claim, while Lattice splits
  sentences.
- **One judge configuration.** The judge was one model (9B Q4_K_M) with
  `--ctx-size 4096 -np 1`. A separate utility model was not tried.
- **Reference environment.** The external models ran on transformers 5.19,
  newer than they were trained with. HHEM warned about its config class name.
  Spot checks were sane (verbatim pairs 0.89–0.99), but there was no parity
  check against the authors' environment.

## Files

- `claim-pairs-v1.jsonl`, `README.md`: the dataset and its labelling rule.
- `results/2026-10-12-<checker>.jsonl`: per-pair verdict, P(supported),
  latency. Lattice rows also carry each sentence's verdict and every judge
  call (reply, label probabilities, tokens).
- `results/2026-10-12-<checker>.meta.json`: model revision, parameters,
  device, versions.
- `results/2026-10-12-scores.json`: every metric above, including by tag.
