# Lesson-claim pairs, wave 1 (.NET)

`gold-w1.jsonl` holds 85 labelled claim/passage pairs for measuring a claim
checker on the claims Lattice's lesson pipeline really makes. It is the
lesson-side companion to
[`../claim-pairs-v1.jsonl`](../claim-pairs-v1.jsonl), which is synthetic. Here
the passages are real web pages, so a checker can lean on what it already
knows, and the labelling rule says it must not.

## Provenance

- **Claims and passages** come from the verification report of the
  lesson-fix-1 .NET lesson ("What's New in .NET 10 & ASP.NET Core 10", the
  first lesson of run wp1-1, retried to Ready), a live run under
  `artifacts/course-e2e-2026-10-14/lesson-fix-1` (gitignored, so the run is
  not in the repo). `scripts/lesson_claim_pairs.py extract` reads the stored
  report. Each checked claim kept the passages it was judged against and the
  passage the lesson judge cited.
- `origin` says how a pair was built:

  | origin | meaning |
  | --- | --- |
  | `approved` | the claim with the passage the lesson judge cited as support |
  | `related` | the claim with the retrieved passage it did not cite that shares the most words with it, a realistic near miss |
  | `rejected` | a claim the judge answered unsupported, with the permitted passage that shares the most words with it (from the recorded `call-N.json` files) |
  | `perturbed` | a deliberately altered claim, written against a pool passage |

- Every passage is a verbatim span of the stored source text. A pair that is
  not is dropped at extraction. Passages include page chrome (links, headings).
- `pool.jsonl` is the extracted pool of 25 approved pairs offered for
  perturbation. `edits-dotnet.jsonl` holds the 25 minimal edits written against
  it, each with an intended label, difficulty tags and a note.
  `add-perturbed` joins the two by `pool_id`.
- The lesson judge's verdict is kept in `pipeline_verdict` for analysis. It
  is never shown to a labeller. Perturbed pairs have none.

## Labelling

[`LABEL-RULES.md`](LABEL-RULES.md) is the rule given to perturbation writers
and labellers alike. In short:

- Judge entailment from the passage alone. Outside knowledge never counts,
  for or against.
- Labels are `supported`, `unsupported` and `contradicted`, as in v1.
- A value carried across a scope boundary is unsupported, not contradicted,
  unless the passage says it applies only there.
- Evaluative or advisory wording ("significant", "best practice") is a
  factual part. It needs support in the passage.
- Over-generalised scope is unsupported unless the passage states an
  exception that rules it out, which makes it contradicted.
- An unstated causal link between separately stated facts is unsupported.
- Page chrome is still passage text and may support a claim.
- Difficulty tags come from a fixed list (the v1 list plus the same
  definitions for `partial`, `world_knowledge`, `lexical_trap` and
  `distractor_numbers`).

Two model labellers worked blind, from `scripts/lesson_claim_pairs.py blind`
output (id, claim and passage only, shuffled). The brief names them Opus and
Sonnet. The label files do not record which model wrote which, so the
A = Opus, B = Sonnet assignment rests on the file names and the brief.

I recomputed agreement from `labels-A-w1.jsonl` and `labels-B-w1.jsonl`:

| measure | value |
| --- | --- |
| items labelled by both | 85 |
| three-way agreement | 80 of 85 (94.1%) |
| Cohen's kappa, three-way | 0.903 |
| supported vs not supported agreement | 81 of 85 (95.3%) |

The five disagreements were decided by an adjudicator and are in
`adjudicated.jsonl` (id, label, note, difficulty). The adjudicated note
ends in "Adjudicated." These match the 94% and 0.90 reported earlier.

## Label distribution

From `gold-w1.jsonl`. All 85 pairs have `subject` = `dotnet`.

| label | pairs |
| --- | ---: |
| supported | 34 |
| unsupported | 40 |
| contradicted | 11 |
| not supported (unsupported + contradicted) | 51 |

| origin | pairs | supported | unsupported | contradicted |
| --- | ---: | ---: | ---: | ---: |
| approved | 35 | 30 | 5 | 0 |
| perturbed | 25 | 0 | 14 | 11 |
| rejected | 15 | 0 | 15 | 0 |
| related | 10 | 4 | 6 | 0 |

The data says 34 supported and 51 not supported. The scores file agrees: its
false-approval rates use 51 as the denominator (4 of 51 = 0.078).

## Files

| file | what it holds | rows |
| --- | --- | ---: |
| `gold-w1.jsonl` | the dataset: id, claim, passage, document_id (page URL), label, difficulty, note, subject, origin, source_title, pipeline_verdict | 85 |
| `pool.jsonl` | approved pairs offered for perturbation, with the lesson id and the judge's reason | 25 |
| `edits-dotnet.jsonl` | perturbed claims: pool_id, claim, intended_label, difficulty, note | 25 |
| `labels-A-w1.jsonl` | labeller A's blind labels: id, label, difficulty, note | 85 |
| `labels-B-w1.jsonl` | labeller B's blind labels, same fields | 85 |
| `adjudicated.jsonl` | final labels for the pairs A and B split on | 5 |
| `LABEL-RULES.md` | the labelling rule | 42 lines |

## Regenerate or extend

`scripts/lesson_claim_pairs.py` builds every file here, in the order
`extract`, `add-perturbed`, `blind`, `assemble`, then `spot-check` and
`compare`. A new subject needs a Lattice database holding prepared lessons
with stored verification reports, and ideally a directory of recorded
`call-N.json` lesson-judge calls for the `rejected` pairs. Run the same steps
with a new `--subject` and a new edits file.

The full procedure, including the shipped-judge eval
(`live_claim_pairs_eval`) and Granite, is in
[`docs/development/course-generation-testing.md`](../../../docs/development/course-generation-testing.md#4-claim-checker-measurement).

## Caveats

- Every label was set by a model. Josh's blind human spot-check has not been
  done yet. Until it is, do not quote a number from this set as a release
  claim.
- One subject (.NET), and the claims come from one lesson. Results may not
  carry to other subjects or to claims in other styles.
- 85 pairs is small. The balanced-accuracy intervals in the scores file are
  wide, and one pair moves a rate by 2 points (one of 51).
- The adjudicator and the labellers are models of the same families as some
  checkers under test.
- `perturbed` pairs were written to be hard, so they are over-represented
  among not-supported items.

## Results

Run on 2026-10-14, in
[`../results/`](../results/). Each `.jsonl` has one line per pair.

| file | what it holds |
| --- | --- |
| `2026-10-14-lesson-w1-qwen3.8-27b-lesson-judge.jsonl` | the 27B lesson judge, three-way verdicts |
| `2026-10-14-lesson-w1-granite-guardian-3.3-8b.jsonl` | Granite Guardian 3.3 8B, no-think |
| `2026-10-14-lesson-w1-granite-guardian-3.3-8b-think.jsonl` | Granite Guardian 3.3 8B, thinking mode (not in the scores file) |
| `2026-10-14-lesson-w1-laguna-xs-2.1.jsonl` | Laguna XS 2.1, batched judge with reasons |
| `2026-10-14-lesson-w1-minicheck-flan-t5-large.jsonl` | MiniCheck Flan-T5 Large |
| `2026-10-14-lesson-w1-minicheck-roberta-large.jsonl` | MiniCheck RoBERTa Large (not in the scores file) |
| `2026-10-14-lesson-w1-minicheck-*.meta.json` | model revision, parameters, device, load time (two files) |
| `2026-10-14-lesson-w1-scores.json` | every metric for the checkers above and three AND gates |

FA = false approvals: the checker said supported on a not-supported pair.
FB = false blocks: the checker said not supported on a supported pair. Rates
are FA / 51 and FB / 34. Balanced accuracy is the mean of recall on the
supported class and on the not-supported class. A gate "X AND Y" approves only
when both approve.

| checker | FA | FA rate | FB | FB rate | balanced accuracy |
| --- | ---: | ---: | ---: | ---: | ---: |
| qwen3.8-27b-lesson-judge | 4 | 0.078 | 0 | 0.000 | 0.961 |
| granite-guardian-3.3-8b | 10 | 0.196 | 1 | 0.029 | 0.887 |
| laguna-xs-2.1 | 16 | 0.314 | 1 | 0.029 | 0.828 |
| minicheck-flan-t5-large | 15 | 0.294 | 4 | 0.118 | 0.794 |
| judge AND granite | 3 | 0.059 | 1 | 0.029 | 0.956 |
| judge AND laguna | 4 | 0.078 | 1 | 0.029 | 0.946 |
| judge AND minicheck-flan-t5 | 3 | 0.059 | 4 | 0.118 | 0.912 |

How to run the scorer and the checkers is in the
[v1 card](../README.md#running-the-bake-off).
