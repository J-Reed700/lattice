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

## Page recall: where does the cited passage reach the judge?

`live_page_recall_eval` (in
`crates/lattice-desktop/src/features/learning/lessons/live_page_recall_eval.rs`)
measures the two-pass claim pages of
[`2026-10-19-course-evidence-index.md`](../../docs/design/2026-10-19-course-evidence-index.md)
(§5, §7.1–7.2) on a stored run, with no model. It reads what
`scripts/lesson_claim_pairs.py extract` reads: every
`learning_lesson_verifications.report_json` in the database, and each
program's saved source versions. The database is opened read-only. From
those versions it builds a temporary course index in a scratch directory,
the same index a lesson job reads, then for every `supported` finding:

- asks whether **pass A** settles it: the passage the stored judge cited
  lies in the passages its own section cites (what the stored report marks
  `cited` for that unit);
- walks the claim's **pass B** pages to the end of its lists, as if no page
  settled it and the judge read each page whole, under each order of §5.2:
  `relevance` (the whole course by relevance, the app's setting),
  `lesson-sources` (the sources the lesson cites first, then the course)
  and `tie-break` (the course, the lesson's sources first on equal scores);
- finds the cited passage in pass B's fused list: a passage of its source
  that contains it, or covers its middle byte (the stored quote is the whole
  widened passage, and the index may draw its edges a little differently).

Each finding is one JSON line per order in the results file:

| field | meaning |
| --- | --- |
| `order` | `relevance`, `lesson-sources` or `tie-break` |
| `retrieval`, `embedding` | `hybrid`, `partial` or `lexical`, and the embedding model's identity when one was loaded |
| `page_budget` | the tokens a pass B page held, from the pace given; `null` when no pace was given and the window alone bounded each page |
| `settled_by_a` | the cited passage lies in the section's own citations |
| `rank` | the cited passage's position (1 = first) in pass B's fused list, every reach's list in turn; `null` when no list holds it |
| `page` | the pass B page (1 = first) that holds it; `null` when none does (the claim would have gone to web research) |
| `reach` | the reach that page was read from |
| `attribution_tokens` | estimated tokens of pass A's page |
| `tokens_read_to_it` | estimated tokens of every pass B page up to and including `page` |
| `modelled_seconds` | with a pace given, pass A's page and the pass B pages up to `page` read at the prompt rate, plus one answer each at the output rate |
| `list` | the run-out cost: passages, tokens and pages of every list read to its end (§5.4) |
| `research_seconds` | what researching one claim on the web costs, from `LATTICE_PAGE_RECALL_RESEARCH_SECONDS`; `null` without it |
| `depths` | with a research cost given, for a finding pass A did not settle: under each depth rule (`first page`, `break-even`, `list end`), whether the course `found` its support and the `pass_b_seconds` spent on it |
| `pages` | per page: its reach, passages, tokens, whether it reached its list's end and whether a counter query was fused |
| `cited_mentions_subject` | whether the cited passage mentions the claim's subject; `false` is a passage past every list's end |
| `skipped` | set when the cited passage's source changed after the run |

The last lines printed sum each order up: findings settled by pass A,
reached in pass B on each page, never reached, the median, p90 and total
tokens to the support, the modelled total minutes, the run-out lists'
length and token distribution, and how many cited passages fail the
subject test. The first line says which retrieval ran (hybrid with the
model's identity, or text only) and the page budget.

### Modelled time and the page budget

Every page a claim needs costs one more answer, so tokens read alone do not
say which order is faster. Set `LATTICE_PAGE_RECALL_PACE` to
`"<prompt tokens/s>,<output tokens/s>,<answer tokens per claim>"`, read off
a run's log: the prompt rate from llama.cpp's `prompt_ms` and evaluated
prompt tokens, the output rate from its `tokens_per_second`, and the answer
size as a check's output tokens divided by the claims it answered. The page
budget is then the app's own, `answer tokens per claim × prompt rate ÷
output rate` (`page_budget`), and each finding reached is modelled as

    seconds = (pass A tokens + Σ pass B page tokens) ÷ prompt rate
              + (pass A answer + pass B pages) × answer tokens ÷ output rate

Without the variable no time is modelled and pages are bounded by the
window alone, as the app does before a backend has reported its pace.

### How deep pass B reads (D5)

Set `LATTICE_PAGE_RECALL_RESEARCH_SECONDS` to what researching one claim
on the web costs, in seconds: a run's `Researched` claim events carry it as
`round_ms_per_claim`, each claim research round's time from its start to
its result divided by the claims it researched. Each finding pass A did
not settle is then read under the three depth rules, by the app's own rule
(`Depth::reads_next`) and page cost (`page_seconds`): the first page only;
break-even, which reads the next page while what pass B has spent past the
first page, with that page, stays within the research cost; and on to the
list's end. A page costs

    seconds = page tokens ÷ prompt rate + answer tokens ÷ output rate

and a claim whose support no page read holds goes to web research. One
line per order and rule sums it up: the claims found in the course, the
claims sent to web research, the modelled minutes with the research cost
counted for each claim sent, and the worst single claim's pass B minutes.
Without a pace no time is modelled, and break-even reads the first page
only, as the app does. Without the variable no rule is compared.

### Hybrid against text only

`LATTICE_PAGE_RECALL_EMBEDDING=<model directory>` loads the configured
embedding model through the app's own loader (the directory's artifact
identity is computed, as for any unregistered model) and the index is
searched by meaning and by words, fused. Without it the index is searched
by words alone. Run both and compare `rank`, `page` at 1 (recall at page
one), the reciprocal rank and the count never reached; the passages that
share no distinctive term with their claim are the first thing to look for
in the hybrid column.

Embedding a whole course takes as long as the app's own build (about six
passages a second on an Apple-silicon GPU). When the app has already built
the course's index with the same model, copy its `course-index` folder and
point `LATTICE_PAGE_RECALL_INDEXES` at the copy: the eval brings it current
instead of embedding it again. Never point it at the app's own folder; the
eval writes to the index it is given.

Run it on a copy of your database or the live one (it never writes to the
database):

```bash
cd crates/lattice-desktop
LATTICE_PAGE_RECALL_DB="<the app data folder>/lattice.db" \
LATTICE_PAGE_RECALL_WINDOW=<the run's configured context window> \
LATTICE_PAGE_RECALL_RESULTS=$PWD/../../evals/grounding/results/<date>-page-recall-<order>-<retrieval>.jsonl \
LATTICE_PAGE_RECALL_PROGRAM=<optional program id> \
LATTICE_PAGE_RECALL_ORDER=<relevance|lesson-sources|tie-break|all, default all> \
LATTICE_PAGE_RECALL_PACE="<optional prompt tok/s>,<output tok/s>,<answer tokens per claim>" \
LATTICE_PAGE_RECALL_RESEARCH_SECONDS=<optional seconds to research one claim on the web> \
LATTICE_PAGE_RECALL_EMBEDDING=<optional embedding model directory> \
LATTICE_PAGE_RECALL_INDEXES=<optional copy of the app's course-index folder> \
  cargo test --lib live_page_recall_eval -- --ignored --nocapture
```

Record each run's summary in [`results/README.md`](results/README.md). The
per-finding results are the user's own content and, like
`lesson-claims-<date>.jsonl`, are not committed unless the user chooses to.
