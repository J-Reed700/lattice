# Fix: the grounding judge gate

The lexical shortcut no longer certifies a cited claim on word overlap. Only a near-verbatim copy of its passage skips the judge now, and only if the copy cut nothing that matters from the sentence it copies. All 17 false passes the shortcut caused in the 2026-10-12 bake-off now reach the judge. Measured on the 9B: balanced accuracy 0.827 to 0.967, false passes 18 to 1. Branch `fix/grounding` in `../lattice-fix-grounding`, uncommitted.

## What changed

All in `crates/lattice-desktop/src/features/conversation/chat/verification/`.

- **`lexical.rs`**
  - `:28` `MIN_COPIED_TOKENS = 4` replaces `STRONG_MATCHING_TOKENS`. `STRONG_OVERLAP_RATIO = 0.55` is gone.
  - `:34` `COPIED_RUN_WORDS = 4`, the run length for the near-verbatim test.
  - `:57` new field `LexicalClaim::near_verbatim`. `contradiction_prone` (`:61`) now also holds when the copied sentence carries a marker the claim cut.
  - `:49` `overlap_ratio` is kept for tests and debugging. No decision reads it, so it carries `#[cfg_attr(not(test), allow(dead_code))]`.
  - `:120` `lexical_pass` computes the copy test for each cited claim with valid citations. Passage wording is built once per cited source.
  - `:163` `needs_judge`: the new rule, below.
  - `:212`, `:224` new `WEEKDAY_MARKERS` and `NUMBER_WORDS`, read only in the copied sentence.
  - `:263` `padded_lowercase`, split out of `is_contradiction_prone` (unchanged behaviour).
  - `:278` to `:423` `PassageWording`, `compare_wording`, `drops_a_marker`, `capitalised_words`, `lowercase_words`, `word_runs`.
- **`mod.rs`**: the module doc now describes the new gate and the unreached-claim rule (`:5`). `mod gate_tests` is declared under `cfg(test)` (`:30`). Tests below.
- **`gate_tests.rs`** (new): gate tests that read the dataset's own pairs from `evals/grounding/claim-pairs-v1.jsonl`.

Unchanged: `judge.rs`, the judge prompt, `MIN_VERDICT_CONFIDENCE`, the 90 s budget, concurrency, `evidence_for`, and the shared `evidence_retrieval` service. Uncited claims still never reach the judge. A citation to an unknown source still fails closed (`supported = false`, no evidence, no call).

## The new gate rule

A cited claim skips the judge only when all of these hold:

1. **Copy.** Every run of four consecutive words of the claim (lowercased, unstemmed, stopwords kept) occurs in one window of a cited passage. Coverage follows: every claim word is in that window. Order follows: a dropped, swapped or added word in the middle breaks every run that crosses it.
2. **Enough words.** The claim has at least 4 content tokens, and the lexical pass calls it supported.
3. **Claim side, unchanged.** The claim carries no digit, month or negation marker.
4. **Window side, new.** The passage sentences the claim copies (each sentence holding at least one claim run, split line by line) carry nothing the claim cut: no negation marker, digit, weekday, month, number word, and no capitalised word after the first that the claim lacks.

Rule 4 reads the copied sentences, not the whole 1,200-character window. Read literally, "the window holds a number or name the claim lacks" is true of almost every real window. On this dataset it would have sent the verbatim `cp-019` to the judge too, leaving no skip at all.

If the judge never answers a claim that rule 4 sent to it, the claim reads as unverified, not supported. It is `contradiction_prone`, and the existing unreached rule in `mod.rs` handles it. A claim escalated only for failing rule 1 keeps its lexical verdict when unreached, as the ambiguous band always did.

**On the 101 pairs**, a Python model of the rule (scratch only) and the Rust tests agree:

- **The 17 false passes:** all now fail rule 1. Dropped negation (`cp-025`, `cp-088`), swapped entity (`cp-004`, `cp-055`), reversal (`cp-018`, `cp-072`), the six `partial` pairs, scope (`cp-037`, `cp-047`), world knowledge (`cp-038`, `cp-098`) and the weekday swap (`cp-020`).
- **The 6 supported pairs that skipped before:** only `cp-019` (verbatim) still skips.
  - `cp-029` is two sentences. The first still skips. The second ("Recovery keys are escrowed centrally.") cut "without" and "45" from its sentence, so rule 4 sends it to the judge.
  - The four paraphrases (`cp-003`, `cp-013`, `cp-017`, `cp-079`) fail rule 1 and are judged.
- **Gold negatives:** no gold-negative pair skips the judge.

## Tests

New, in `gate_tests.rs`, one per failure category, using the dataset's pairs:

- `a_dropped_negation_reaches_the_judge` (`cp-025`, `cp-088`)
- `a_swapped_entity_built_from_the_passage_words_reaches_the_judge` (`cp-004`, `cp-055`)
- `a_reversal_reaches_the_judge` (`cp-018`, `cp-072`)
- `an_extra_unsupported_clause_reaches_the_judge` (`cp-012`, `cp-016`, `cp-045`, `cp-052`, `cp-067`, `cp-078`)
- `a_widened_scope_reaches_the_judge` (`cp-037`, `cp-047`)
- `world_knowledge_the_passage_never_states_reaches_the_judge` (`cp-038`, `cp-098`)
- `a_swapped_weekday_reaches_the_judge` (`cp-020`)
- `no_false_pair_in_the_dataset_is_settled_without_the_judge` (all 61 gold negatives)
- `a_near_verbatim_claim_still_skips_the_judge` (`cp-019`; `cp-029` sentence 1 skips, sentence 2 is judged)
- `a_copy_that_cuts_a_negation_number_date_or_name_reaches_the_judge`: copies cut from dataset passages that pass rule 1 and fail rule 4. They cut a "Do not" (`cp-013`), a "32 weeks" (`cp-045`), a "July" (`cp-016`), a "Thursday" (`cp-019`) and an "Ostrander" (`cp-067`).

New in `lexical.rs`:

- `a_copy_is_only_a_copy_when_its_words_keep_their_order`: total overlap, swapped owners, judged.
- `a_copied_sentence_is_read_for_what_the_claim_left_out`: each marker kind, plus the sentence-initial capital.

New in `mod.rs`, end to end through `GroundingVerifier`:

- `a_claim_that_keeps_the_passage_words_and_drops_its_negation_reaches_the_judge` (`cp-025`: one call, contradicted).
- `an_unreached_copy_that_cut_a_negation_is_not_checked_rather_than_supported` (unusable judge reply: unverified, `judge_failed`).

Renamed because the rule changed what they assert:

- `strong_overlap_skips_the_judge_but_a_number_does_not` is now `a_near_verbatim_copy_skips_the_judge_but_strong_overlap_or_a_number_does_not`. It now also asserts that 0.8 overlap without a copy is judged, and that a 3-token copy is judged.
- `a_plain_strongly_supported_claim_never_reaches_the_judge` is now `a_plain_near_verbatim_claim_never_reaches_the_judge`. Same input, which is a verbatim copy.

## Verified

- **Build and lint** (from `crates/lattice-desktop`, with `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2`):
  - `cargo check --lib`: clean. The first run warned that `overlap_ratio` was never read; fixed as above.
  - `cargo test --lib -- conversation::chat::verification`: 64 passed, 0 failed, 1 ignored (the live test).
  - `cargo fmt`, then `rustfmt --edition 2021 --check` on the three touched files: clean.
  - `cargo clippy --lib`: 0 warnings.
- **Boundary scripts:** `bash scripts/check-rust-layer-boundaries.sh` and `bash scripts/check-repository-barrier.sh` are both clean.
- **Order of edits:** two doc-comment edits in `lexical.rs` came after the test run. Clippy ran on the final text.
- **Clippy ran twice.** Another agent wrote to the same `clippy.log` name in the shared scratchpad and overwrote the first run's output. The second run is the one reported.
- **Example:** `cargo build --example claim_checker_eval` (dev profile), once. I copied the binary out of the shared target before running it, and checked it held this tree's new functions.

### Measurement on the 9B, 2026-10-13

Steps:

1. `pgrep -fl llama-server-aarch64-apple-darwin` listed nothing before the run.
2. Started `crates/lattice-desktop/binaries/llama-server-aarch64-apple-darwin -m ~/.cache/lattice-evals/models/Qwen3.5-9B-Q4_K_M.gguf --port 8092 --host 127.0.0.1 --no-webui -ngl 99 --ctx-size 4096 -np 1 --jinja`. No server-wide thinking flag.
3. Ran `claim_checker_eval --lexical-only` and `claim_checker_eval --judge-url http://127.0.0.1:8092` over `evals/grounding/claim-pairs-v1.jsonl`.
4. Killed the server. `pgrep` then counted 0 llama-server processes.
5. Scored with `python3 scripts/claim_checker_bakeoff_score.py --library evals/retrieval/synthetic-library-v3.json --json-out evals/grounding/results/2026-10-13-scores.json` on the two new result files.

| Lattice, lexical + 9B judge | 2026-10-12 | 2026-10-13 |
| --- | ---: | ---: |
| balanced accuracy (95% CI) | 0.827 (0.76–0.89) | 0.967 (0.93–1.00) |
| false passes | 18 | 1 (`cp-006`, which every checker passes) |
| pairs never judged | 23 | 1 (`cp-019`, verbatim) |
| judge calls | 81 | 106 |
| total time for the pass | 184 s | 315 s |
| recall, supported | 0.950 | 0.950 |
| 3-way accuracy | 0.703 | 0.802 |
| unverified (low confidence) | 12 | 15 |

- **Where the gain comes from:** the gate alone. The 78 pairs judged on 2026-10-12 got identical verdicts on 2026-10-13.
- **The 17 former false passes:** none is supported now. 10 have the exact label, 3 are unverified under the floor, and 4 have the other negative label.
- **Supported pairs:** the six the shortcut settled are still supported.
- **Extra calls:** 25. That is 22 pairs plus a second sentence in `cp-009`, `cp-039` and `cp-074`.
- **Time:** inflated by load from other agents' builds (1-minute load average 38 before, 70 after). The median call took 2.9 s against 2.3 s. At the old call time the pass would take about 240 s; that is an estimate, not a measurement.
- **Reasoning text:** none. All 106 calls finished with `stop`, none errored, and none returned reasoning text. Since PR #43 the client sends `enable_thinking: false` per request.
- **Against MiniCheck-Flan-T5-Large:** 0.967 against 0.913. Exact McNemar gives 7 against 1, p = 0.07.

Files: `evals/grounding/results/2026-10-13-lattice.jsonl`, `-lattice-lexical.jsonl` (identical verdicts to 2026-10-12, as expected), `2026-10-13-lattice.meta.json` and `2026-10-13-scores.json`. The report section is "Gate change, 2026-10-13" in `evals/grounding/2026-10-12-CLAIM-CHECKER-BAKEOFF.md`, with a pointer at the top of its Summary.

## Not verified

- **Held-out pairs.** The rule is generic, but its tests and this measurement use the same 23 pairs that motivated it. Treat 0.967 as optimistic.
- **Real answers.** Chat answers paraphrase, so on real turns almost every cited sentence will now reach the judge. More judge calls per turn means more background time on the shared slot, and more claims can hit the 90 s budget on a long answer. Not measured on real turns.
- **Integration tests and `--all-targets`** were not run, by the machine rules. The example was built once in the dev profile.
- **Judge model.** Only the 9B Q4_K_M judge was measured.

## Risks

- **More "not checked".** When the budget runs out, a claim sent to the judge only by rule 4 shows as unverified where it used to show as supported.
- **Over-escalation, the safe direction.** A faithful copy of part of a sentence that also holds a name, number or "not" is judged, for example `cp-029` sentence 2. That costs one call each.
- **Rule 4 is sentence-scoped.** A negation in the previous sentence that governs the copied one ("None of the following apply. Readings are reordered in place.") is not seen. Rule 1 does not catch it either. The judge reads the windows, so this is only a skip-path gap.
- **Capitalisation.** Headings that run into prose on one line, or German nouns, read as names. That only adds judge calls.
- **Test coupling.** `gate_tests.rs` reads `evals/grounding/claim-pairs-v1.jsonl` at test time, the way other tests read `evals/retrieval/` files. Editing those pairs can fail these tests. A v2 dataset should be a new file.

## Suggested commit

```
Ask the claim judge unless a claim copies its passage

The lexical gate settled any plain cited sentence with more than half
its words in the passage window, so a claim that kept the passage's
words and lost its meaning was certified supported without the judge.
On the 2026-10-12 claim-checker bake-off that was 17 of the checker's
18 false passes: dropped negations, swapped entities, reversals, extra
clauses, widened scope.

Now a claim skips the judge only when every run of four of its words
is in one window of a cited passage, and the sentence it copies carries
no negation, number, date or name that the claim cut. Everything else
cited goes to the judge, whose prompt, confidence floor, budget and
concurrency are unchanged. Gate tests use the dataset's own pairs.
```
