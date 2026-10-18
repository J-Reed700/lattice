# Claim-verification pairs

`claim-pairs-v1.jsonl` holds 101 labelled claim/passage pairs for measuring a
claim checker: the component that decides whether one sentence of an answer is
backed by the passage it cites. It exists so that Lattice's checker (the
lexical pre-check plus the logprob judge) can be compared with dedicated
models on the same items. The comparison is in
[`2026-10-12-CLAIM-CHECKER-BAKEOFF.md`](2026-10-12-CLAIM-CHECKER-BAKEOFF.md).

## The pairs are synthetic

Every passage is a verbatim span of a document in
[`../retrieval/synthetic-library-v3.json`](../retrieval/synthetic-library-v3.json),
the invented vault of Rowan Selke at Quillon Grid. Nothing in it is true of the
world, so no checker can lean on what it already knows. The scorer refuses a
pair whose passage is not an exact substring of the document it names.

The claims were written by hand for this dataset on 2026-10-12 (Track E2 of the
LLM-stack upgrade program), one passage at a time, to exercise a named failure
mode. They are not model answers. Each label was set by the same author, with a
one-line `note` giving the sentence it rests on or the fact that is missing or
contradicted. No second annotator has reviewed them. The retrieval protocol's
rule applies: before any number here is quoted as a release claim, a person
should re-label a sample blind and publish the agreement rate.

## Labelling rule

Entailment from the passage alone. Outside knowledge never counts, for or
against.

| label | meaning |
| --- | --- |
| `supported` | Every factual part of the claim (numbers, dates, units, names, negations, scope) is stated in the passage or follows from it necessarily. A faithful paraphrase is supported. So is arithmetic on stated values (`cp-062`). |
| `contradicted` | The passage states something incompatible with the claim, or explicitly rules it out. |
| `unsupported` | Neither: the passage is silent on at least one part of the claim. A claim that is true in the world but absent from the passage is unsupported. So is one clause backed and another missing. |

The verdict vocabulary is the retrieval protocol's
([`EVALUATION_PROTOCOL.md`](../retrieval/EVALUATION_PROTOCOL.md)) without
`nonfactual`: every claim here is a factual assertion.

Some calls are deliberately close, and the note says why:

- A value carried across a scope boundary is unsupported, not contradicted.
  For example, the Tuesday window when the passage assigns only Thursday
  (`cp-020`), or the other ring's hold time (`cp-015`).
- Notice periods are minimums, so contradicted claims about them state the
  period itself (`cp-051`, `cp-057`). "May prevent renewal by giving 60 days"
  would be literally compatible with a 60-hour minimum.
- "There is no other way round it" makes an invented exception contradicted
  (`cp-032`).
- "Only this one has that milestone" makes the same date on another Garnetline
  contradicted (`cp-085`).

## Format

One JSON object per line:

```json
{"id": "cp-031", "claim": "...", "passage": "...", "document_id": "policy-snapshot-retention-v3",
 "label": "contradicted", "difficulty": ["unit_drift", "lexical_trap"], "note": "24 working days ..."}
```

Claims are single lines. A harness turns each into an answer paragraph ending
in a `[1]` citation of the passage, and a marker covers only its own line.

`difficulty` is a list of tags:

| tag | pairs | what it probes |
| --- | ---: | --- |
| `verbatim` | 10 | near-copies; the easy end |
| `paraphrase` | 24 | same meaning, different words |
| `number_drift`, `date_drift`, `unit_drift` | 8, 3, 3 | one number, date or unit changed (seconds to minutes, working to calendar days) |
| `distractor_numbers` | 17 | the passage holds several numbers; the claim uses the right one or a neighbour's |
| `negation`, `reversal` | 10, 3 | meaning flipped with or without a negation word |
| `wrong_entity` | 5 | the right fact attached to the wrong person, place or thing |
| `partial` | 17 | one clause supported, one missing (always unsupported) |
| `multi_clause` | 2 | one clause right, one contradicted |
| `multi_sentence` | 7 | two sentences, judged together |
| `world_knowledge` | 9 | true or plausible in the world, absent from the passage |
| `causal_addition` | 5 | an unstated "because" between stated facts |
| `scope`, `hedge` | 3, 2 | over-generalised scope or quantifier |
| `lexical_trap` | 16 | high word overlap with a different meaning |
| `table`, `arithmetic` | 9, 1 | answer in a table cell; a stated product |
| `noisy_passage` | 3 | OCR-damaged scan with page furniture |
| `cross_lingual` | 4 | Spanish or German passage, English claim |

Labels: 40 supported, 36 unsupported, 25 contradicted, over 37 passages
(137 to 1,458 characters, median 617) from 36 documents.

## Running the bake-off

```text
# Lattice's checker, lexical pass only (no server)
cargo run --example claim_checker_eval -- --lexical-only \
    ../evals/grounding/claim-pairs-v1.jsonl ../evals/grounding/results/2026-10-12-lattice-lexical.jsonl

# Lattice's checker with the judge, against a llama-server on :8092
cargo run --example claim_checker_eval -- --judge-url http://127.0.0.1:8092 \
    ../evals/grounding/claim-pairs-v1.jsonl ../evals/grounding/results/2026-10-12-lattice-checker.jsonl

# One external checker per process (torch + transformers + the checker's package)
"$EVAL_PYTHON" scripts/claim_checker_bakeoff.py --checker minicheck-roberta-large

# Score everything, checking every passage against the library
python3 scripts/claim_checker_bakeoff_score.py --library evals/retrieval/synthetic-library-v3.json \
    evals/grounding/results/2026-10-12-*.jsonl
```

The scorer's tests: `python3 -m unittest discover -s scripts -p 'test_claim_checker_bakeoff.py'`.

## Pairs from your own lessons: `lesson-claims-<date>.jsonl`

Every verdict a user gives on a claim the lesson checker flagged is a
labelled example of that checker (design
[`2026-10-16-lesson-claim-review.md`](../../docs/design/2026-10-16-lesson-claim-review.md)
§15), kept in the app database's `learning_checker_examples`:

| User action on a flagged claim | `label_source` | Exported `label` |
| --- | --- | --- |
| Confirm (the claim is true as written) | `user_confirm` | `supported` |
| Remove, or Edit the sentence | `user_remove`, `user_edit` | the checker's verdict (`unsupported` or `contradicted`) |

Accepting a general-knowledge proposal is not scored. Undoing a verdict
deletes its example. Retrieval misses (`retrieval_miss`, recorded when added
evidence verifies a claim the collection could not have supported before)
and examples without passages are not exported: no passage was judged.

`CheckerExamples::export` (in
`src-tauri/src/features/learning/lessons/claim_reviews/checker_examples.rs`,
behind the `export_learning_checker_examples` command) writes them in this
file's format, one pair per example: `id`, `claim`, `passage` (the passages
the checker compared, joined by a blank line), `label`, `difficulty` (the
label source) and `checker_model`. Save the file here as
`lesson-claims-<date>.jsonl` and score any checker on it with
`live_claim_pairs_eval` (see
[`course-generation-testing.md`](../../docs/development/course-generation-testing.md)):

```bash
cd src-tauri
LATTICE_LLAMACPP_SETTINGS="$HOME/Library/Application Support/tech.lattice.app/settings.json" \
LATTICE_CLAIM_PAIRS=$PWD/../evals/grounding/lesson-claims-<date>.jsonl \
LATTICE_CLAIM_RESULTS=$PWD/../evals/grounding/results/<date>-lesson-claims-<checker>.jsonl \
LATTICE_CLAIM_CHECKER=<checker> \
  <binary> live_claim_pairs_eval --ignored --nocapture
```

These pairs are the user's own content: they are not synthetic and are not
committed unless the user chooses to.
