# The claim checker remeasured, 2026-11-06

The 101 pairs of `claim-pairs-v1.jsonl` were last scored on the shipped checker on 2026-10-13. Prompts around it and the
schemas of the calls beside it have changed since, and #301 removed length limits on what models write. This run asks
whether any of it moved the checker: it did not.

Lexical pre-check plus the first-token logprob judge, on `Qwen3.5-9B-Q4_K_M.gguf`, on the tree `f920d152` (main at
`576183f9` with the branches then open). Windows, RTX 4070 Laptop, `--ctx-size 4096 -np 1`, the machine's load not held
steady. The run: `claim_checker_eval --judge-url`, scored by `scripts/claim_checker_bakeoff_score.py`.

| | 2026-10-13 | 2026-11-06 |
|---|---:|---:|
| Accuracy (binary) | 0.970 | 0.970 |
| Balanced accuracy | 0.967 | 0.967 |
| Three-way accuracy | 0.802 | 0.812 |
| False approvals | 1 | 1 |
| False blocks | 2 | 2 |
| Unverified | 15 | 14 |
| Median ms a pair | 2,926 | 2,672 |

The latencies are on different machines, so they say nothing about the code.

Three pairs changed verdict, all three labelled `contradicted`, none across the line between supported and not:

| Pair | 2026-10-13 | 2026-11-06 |
|---|---|---|
| `cp-025` | unsupported | contradicted |
| `cp-032` | unverified | contradicted |
| `cp-082` | contradicted | unsupported |

The lexical pre-check alone gives the same verdict on every pair as it did on 2026-10-13.

## Records

`results/2026-11-06-lattice.jsonl` and `results/2026-11-06-lattice-lexical.jsonl`. Each row names its model. Their checker
names carry the date, so the scorer can compare them with the earlier files:

```sh
python -X utf8 scripts/claim_checker_bakeoff_score.py --library evals/retrieval/synthetic-library-v3.json \
  evals/grounding/results/2026-10-13-lattice.jsonl evals/grounding/results/2026-11-06-lattice.jsonl
```
