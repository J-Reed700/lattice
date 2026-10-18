# Track E2: claim-checker bake-off

The judge is the strong part; the lexical gate in front of it is the weak part. Report: `evals/grounding/2026-10-12-CLAIM-CHECKER-BAKEOFF.md`.

## What changed (uncommitted)

- **New files:**
  - `evals/grounding/`: 101 labelled pairs, README, report, results, scores.
  - `crates/lattice-desktop/examples/claim_checker_eval.rs`: runs the shipped verifier.
  - `scripts/claim_checker_bakeoff.py`: runs one external checker per process.
  - `scripts/claim_checker_bakeoff_score.py`: the scorer.
  - `scripts/test_claim_checker_bakeoff.py`: tests for the scorer.
- **Visibility only:** in `chat.rs`, `mod verification` became `pub mod`. In `verification/mod.rs`, `GroundingVerifier`, `new`, `verify`, `GroundingReport` and `metadata_json` became `pub`. No logic changed.

## Bake-off

101 pairs: 40 supported, 36 unsupported, 25 contradicted.

| checker | bal. acc (95% CI) | prec. supported | contradictions caught | ms/pair |
| --- | --- | --- | --- | ---: |
| Lattice, lexical only | 0.55 (0.49–0.60) | 0.42 | 2/25 | <1 |
| Lattice, lexical + 9B judge | 0.83 (0.76–0.89) | 0.68 | 19/25 | 2,063 |
| MiniCheck-Flan-T5-Large | 0.91 (0.85–0.96) | 0.86 | 22/25 | 180 |
| MiniCheck-RoBERTa-Large | 0.86 (0.79–0.93) | 0.85 | 22/25 | 111 |
| LettuceDetect-large (base) | 0.80 (0.79) | 0.65 | 12/25 | 136 |
| HHEM-2.1-open | 0.76 | 0.58 | 9/25 | 49 |

- **The gate:** 23 pairs (23%) never reached the judge. 17 of those are false, and the strong-overlap rule in `needs_judge` passed them; they are 17 of Lattice's 18 false passes. The rule only checks the claim for numbers, dates and negations, so a claim that drops the passage's "not", or swaps an entity, gets through.
- **The judge:** right on 65 of 66 definite verdicts, each with the exact three-way label. It declined 12 more at its 0.75 confidence floor, and its top label was right on 11 of those.
- **Significance:** only MiniCheck-Flan-T5 beats Lattice significantly (exact McNemar p = 0.043).
- **Porting:** every candidate needs a classification head; HHEM and MiniCheck-T5 also need T5.

**Recommendation:**
- Keep the judge.
- Change the gate (not done here; measure it with this harness first).
- Port MiniCheck-RoBERTa only to replace the gate. It catches 15 of the 17 and is the cheapest port.
- Skip HHEM and LettuceDetect.
- No thresholds were touched.

## Verified

- **Python:** 13 of 13 unit tests pass.
- **Rust:** `cargo check --all-targets` and `cargo fmt --check` are clean.
- **Dataset:** the scorer confirms every passage is verbatim from the v3 library.
- **Judge setup:** a probe confirmed thinking was off, and none of the 81 calls returned reasoning.
- **Run hygiene:** server killed (`pgrep` empty); external checkers spot-checked.

## Not verified, and risks

- **Labels:** one annotator, on a synthetic, adversarial set.
- **Thinking was off, set by server flag.** Lattice sends no switch, so the shipped judge may think. That was not measured.
- **Budget:** each pair got the full 90 s, where chat shares it across a whole answer.
- **Not covered:** utility-model judge, Candle-port latency, transformers 4.x parity.
- **Setup files:** I copied the gitignored sidecar binary in and created an empty `dist/`. The venv and HF cache (6.7 GB) are in `~/.cache/lattice-evals/`.

## Suggested commit

```
Add a claim-checker bake-off over 101 labelled pairs

The grounding checker had never been measured. Add synthetic
claim/passage pairs from the v3 library, an example that runs the
shipped GroundingVerifier over them, and scripts that run and score
MiniCheck, HHEM-2.1-open and LettuceDetect beside it.

The judge is right on 65 of 66 pairs it rules on; 17 of 18 false
passes come from the lexical shortcut that never asks it. The
verification module becomes pub; no checker logic changes.
```
