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
    ../../evals/grounding/claim-pairs-v1.jsonl ../../evals/grounding/results/2026-10-12-lattice-lexical.jsonl

# Lattice's checker with the judge, against a llama-server on :8092
cargo run --example claim_checker_eval -- --judge-url http://127.0.0.1:8092 \
    ../../evals/grounding/claim-pairs-v1.jsonl ../../evals/grounding/results/2026-10-12-lattice-checker.jsonl

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

## Page recall: does the cited passage reach the judge?

`live_page_recall_eval` (in
`crates/lattice-desktop/src/features/learning/lessons/live_page_recall_eval.rs`)
measures the page rule of
[`2026-10-19-claim-evidence-pages.md`](../../docs/design/2026-10-19-claim-evidence-pages.md)
(§9, "Eval") on a stored run, with no model. It reads what
`scripts/lesson_claim_pairs.py extract` reads: every
`learning_lesson_verifications.report_json` in the database, and the
program's saved source versions, ranked by keyword alone
(`ReferenceCollection::lexical`). The database is opened read-only.

For every `supported` finding it walks the claim's pages under the current
rule (ring 0 and ring 1 first, then the rest of the course, then the deeper
pages) as if no page settled the claim, and stops at the page that holds the
passage the stored judge cited. A section's citations are the passages the
stored report marks `cited`. Each finding is one JSON line in the results
file:

| field | meaning |
| --- | --- |
| `old_page` | passages and estimated tokens of the stored page that decided the claim (its page index is not stored) |
| `new_page` | the page (1 = first) that holds the cited passage under the new rule: a passage of its source that contains it, or covers its middle (the hit its context was widened around), since the new rule may draw the passage's edges a little differently; `null` when no page does |
| `first_overlapping_page` | the first page with a passage overlapping the cited one, even if it holds only part of it |
| `tokens_read_to_it` | estimated tokens of every page up to and including `new_page` |
| `pages` | per page: its ring (`stage`), passages, tokens, whether each ranking had a clear gap (`clear`, `none`, `-` for no candidates) and whether a counter page ran |
| `cited_mentions_subject` | whether the cited passage mentions the claim's subject; `false` is a passage the new rule never shows (decision D5) |
| `modelled_seconds` | with a pace given: `new_rule`, the modelled time to reach the cited passage, and `old_deciding_page`, the same for the stored deciding page alone |
| `skipped` | set when the cited passage's source changed after the run |

The last lines printed sum it up: how many cited passages fail
`mentions_subject_of`, how many findings reach the judge on each page, how
many never do, the old and new token totals and, with a pace given, the
modelled time totals and the page budget that pace gives.

### Modelled time

Every page a claim needs costs one more answer, so tokens read alone do not
say which rule is faster. Set `LATTICE_PAGE_RECALL_PACE` to
`"<prompt tokens/s>,<output tokens/s>,<answer tokens per claim>"`, read off
a run's log: the prompt rate from llama.cpp's `prompt_ms` and evaluated
prompt tokens, the output rate from its `tokens_per_second`, and the answer
size as a check's output tokens divided by the claims it answered. Each
finding the new rule reaches is then modelled as

    seconds = Σ page tokens ÷ prompt rate + pages × answer tokens ÷ output rate

over the pages up to the cited passage. The old rule's total uses each
finding's stored deciding page as one page, a lower bound, since what the
old rule read before that page is not stored. The summary line also prints
the page budget the app derives from the same pace,
`answer tokens per claim × prompt rate ÷ output rate` (design §13); the
eval's pages are not yet sized by it. Without the variable, no time is
modelled.

Run it on a copy of your database or the live one (it never writes):

```bash
cd crates/lattice-desktop
LATTICE_PAGE_RECALL_DB="<the app data folder>/lattice.db" \
LATTICE_PAGE_RECALL_WINDOW=<the run's configured context window> \
LATTICE_PAGE_RECALL_RESULTS=$PWD/../../evals/grounding/results/<date>-page-recall.jsonl \
LATTICE_PAGE_RECALL_PROGRAM=<optional program id> \
LATTICE_PAGE_RECALL_PACE="<optional prompt tok/s>,<output tok/s>,<answer tokens per claim>" \
  cargo test --lib live_page_recall_eval -- --ignored --nocapture
```

The window bounds only how many of a section's citations page 1 holds. The
results are the user's own content and, like `lesson-claims-<date>.jsonl`,
are not committed unless the user chooses to.
