# Claim corroboration: lesson db8397ff ("Function Calling & Tool Schemas")

Eval inputs for one question: why does the lesson claim checker not accept
the official documentation the course has already saved (the OpenAI
function-calling guide, the Claude tool-use pages, Gemini's function-calling
page) as support, and leave 31 claims waiting for the user (30 queued and
1 open: 17 insufficient standing, 12 general-knowledge proposals, 1
contradicted, 1 unsupported, as of the 2026-10-21 copy)?

These pairs are the owner's own course content (real web pages). They are
not synthetic and are not committed unless the owner chooses to.

## How they were made (2026-10-21)

1. The app database was copied while the app ran
   (`sqlite3 lattice.db ".backup ..."`), and the course's passage index
   (`course-index/026020fa-.../passages.db`, identity
   `sha256:9a4938b7...`) was copied beside it. Nothing was written to either
   original.
2. For each waiting claim of lesson `db8397ff-60fd-474f-a1a5-6a889b530526`,
   `build.py` reads the stored page its last decision rests on
   (`learning_lesson_claims.evidence_json`) and recovers pass B's first page
   and the judge's answer to it from the job checkpoints. It recomputes the
   `claim-evidence-v2:` selection key (`evidence_selection::selection_key`)
   and the `claim-v1:` receipt key (`claim_receipt_key` over
   `ClaimChecks::key`). All 31 claims' pass B receipts were found. A
   claim's pass A page is its stored page minus pass B's page and minus the
   example runs. When that is empty, the claim takes the pass A passages
   the other waiting claims of its section show.
3. The official passages were found by text search over the course's saved
   versions. Each one is cut the way the course index cuts a hit: the index
   chunk holding the quote, widened by `verification_context`'s rule
   (1,700 characters each way, trimmed to a line edge within 300).
4. Labels are one reader's (Claude, 2026-10-21), by the rule in
   [`../README.md`](../README.md#labelling-rule): entailment from the
   passage alone. Each label has a `note` with the sentence it rests on or
   the part that is missing. `close_call` marks the labels a second reader
   could reasonably flip. Nobody has re-labelled them blind.
5. Run `python3 -I build.py --db <db copy> --index <passages.db copy>` to
   rebuild `pairs.jsonl` and `recall.jsonl` from `spec.json`. The spec holds
   every hand decision: which passage, its quote and its label.

## Files

| file | what |
| --- | --- |
| `spec.json` | the hand labels and passage choices |
| `build.py` | builds the two JSONL files from a database copy and an index copy |
| `pairs.jsonl` | 60 claim/page pairs for `live_claim_pairs_eval` |
| `recall.jsonl` | 8 claims, each with the official passages that should be found, for `live_page_recall_eval` (`LATTICE_PAGE_RECALL_TARGETS`) |
| `.gitignore` | keeps `pairs.jsonl`, `recall.jsonl` and the result rows out of the repository |

`pairs.jsonl`, `recall.jsonl` and the result JSONL rows hold real excerpts of
the owner's saved pages, so they stay local (owner's decision, claim sources
design §15): rebuild the two inputs with `build.py` from a database copy and
a course-index copy (step 5 above). The harnesses, `build.py`, `spec.json`,
this README and the result numbers below are committed.

### `pairs.jsonl`

One pair per line: `id`, `claim`, `passages` (a page: each `{label, text}`,
and the eval writes the label line above the text as the lesson check does),
`label`, `difficulty`, `note`, `kind`, `claim_id`, `source_version_ids`,
`pipeline_verdict` (the claim's verdict in the app, never shown to the judge).

| kind | pairs | what it measures |
| --- | ---: | --- |
| `seen` | 31 (16 supported, 15 unsupported) | the judge on the very page it last decided on, as it read it |
| `primary` | 16 (9 supported, 7 unsupported) | the judge on an official passage already in the course, labelled with its source |
| `primary_unlabelled` | 9 (supported) | the same 9 supported official passages without the `From "…" · domain · dated …` line |
| `framed` | 4 (1 supported, 3 unsupported) | the claim written as `Citations::label` writes a statement that names "Source 0": `Source "Tool Use & Function Calling — The LLM Stack" states: "…"` |

The seven `primary` pairs labelled unsupported are controls: a vendor-scoped
contrary passage (`cc-f8223602-claude-contrary`), name drift (`max_turns`
for `max_iterations`), a best practice for a "must", "most important" for
"entirely", JSON schema without "subset", and the Academy page that lists a
tool schema's parts without defining the term.

### `recall.jsonl`

One claim per line: `claim`, `unit`, `cited` (pass A's page, by source
version and byte range), `targets` (each official passage that states the
claim: its source version, the byte span of its `quote`, and the quote),
`researched` (the versions the claim's research round added; its live walk
read only these) and `in_research_scope` (whether any target is among them).

## Running them

Build once in the worktree (the private target dir is set in
`.cargo/config.toml`), then from `crates/lattice-desktop`:

```bash
# The judge on the pairs (Josh's remote Qwen3.8-27B, the live checker)
LATTICE_LLAMACPP_SETTINGS="$HOME/Library/Application Support/tech.lattice.app/settings.json" \
LATTICE_CLAIM_PAIRS=$PWD/../../evals/grounding/claim-corroboration-db8397ff/pairs.jsonl \
LATTICE_CLAIM_RESULTS=$PWD/../../evals/grounding/results/2026-10-21-claim-corroboration-qwen3.8-27b.jsonl \
LATTICE_CLAIM_CHECKER=qwen3.8-27b-lesson-judge \
LATTICE_CLAIM_BATCH=8 \
CARGO_INCREMENTAL=0 ~/.local/bin/cargo-slot cargo test -p lattice-desktop --lib -j 6 \
  claim_pairs::live_claim_pairs_eval -- --ignored --nocapture

# Where the course's hybrid ranking puts the official passages (no model calls;
# copy the course index first, never point at the app's own folder)
cp -c -R "$HOME/Library/Application Support/tech.lattice.app/course-index" /tmp/claims-course-index
LATTICE_PAGE_RECALL_DB=<the database copy> \
LATTICE_PAGE_RECALL_WINDOW=131072 \
LATTICE_PAGE_RECALL_PROGRAM=026020fa-3444-4442-bf96-7da0a1b933c6 \
LATTICE_PAGE_RECALL_TARGETS=$PWD/../../evals/grounding/claim-corroboration-db8397ff/recall.jsonl \
LATTICE_PAGE_RECALL_INDEXES=/tmp/claims-course-index \
LATTICE_PAGE_RECALL_EMBEDDING=<the embedding model GGUF the app uses> \
LATTICE_PAGE_RECALL_PACE="<prompt tok/s>,<output tok/s>,<answer tokens per claim>" \
LATTICE_PAGE_RECALL_RESULTS=$PWD/../../evals/grounding/results/2026-10-21-claim-corroboration-recall-hybrid.jsonl \
CARGO_INCREMENTAL=0 ~/.local/bin/cargo-slot cargo test -p lattice-desktop --lib -j 6 \
  page_recall::live_page_recall_eval -- --ignored --nocapture
```

The standing order (claim sources design §7, §14.2) walks each claim's
standing scope, the course's versions a stored judgement says stand on its
subject, read from the trust store with no model. It reads the database
through the app's own queries on a migrated scratch copy (the copy given is
never written), and the same index copy:

```bash
LATTICE_PAGE_RECALL_ORDER=standing \
LATTICE_PAGE_RECALL_DB=<the database copy> \
LATTICE_PAGE_RECALL_WINDOW=131072 \
LATTICE_PAGE_RECALL_PROGRAM=026020fa-3444-4442-bf96-7da0a1b933c6 \
LATTICE_PAGE_RECALL_TARGETS=$PWD/../../evals/grounding/claim-corroboration-db8397ff/recall.jsonl \
LATTICE_PAGE_RECALL_INDEXES=/tmp/claims-course-index \
LATTICE_PAGE_RECALL_EMBEDDING=<the embedding model GGUF the app uses> \
LATTICE_PAGE_RECALL_PACE="<prompt tok/s>,<output tok/s>,<answer tokens per claim>" \
LATTICE_PAGE_RECALL_RESEARCH_SECONDS=<research seconds per claim, from a run's log> \
LATTICE_PAGE_RECALL_TRUST_MODEL=<the model that judged the course's sources; default: the one with most ownership judgements> \
LATTICE_PAGE_RECALL_RESULTS=$PWD/../../evals/grounding/results/2026-10-21-claim-corroboration-recall-standing.jsonl \
CARGO_INCREMENTAL=0 ~/.local/bin/cargo-slot cargo test -p lattice-desktop --lib -j 6 \
  page_recall::live_page_recall_eval -- --ignored --nocapture
```

Each `standing` row adds `scope: { versions, owner_hosts, proxies_by_level,
model }`, or `scope: null` when the claim has no stored profile or its
subject no stored ownership judgement. The bar (claim sources design
§14.2): of the 6 claims with a stored scope (`cc-recall-11d420ee`,
`-b5c6269d`, `-56a1e15e`, `-7cf7155f`, `-0ef43f21`, `-69c1da48`, all
`insufficient_standing` in the copy), a target passage is on standing page
1 for at least 5, and within the break-even depth for all 6; the 2 without
a stored scope (`-e2e66bc4`, `-f8223602`) report `scope: null`. The
`relevance` order's numbers do not change. Not run yet: the coordinator
runs it and records the numbers below.

`LATTICE_LLAMACPP_SETTINGS` names a file shaped like the app's
`settings.json`: the eval reads its `settings.llm` block (the
`LLMSettingsDto`, with `llamaCpp`'s server URL, model and any auth header)
and nothing else. To reproduce the live checker it must name the model the
claims were checked with, `/models/Qwen3.8-27B-Q5_K_S.gguf`.

Without `LATTICE_PAGE_RECALL_EMBEDDING` the recall eval searches by words
only. Without `LATTICE_PAGE_RECALL_PACE` a page is bounded by the window
alone, not by the app's page budget, so page numbers count far more
passages than the app's one-to-two-passage pages. Give the pace from a run's
log to model the app's page size.

The recall eval writes one row per claim and order: `settled_by_a`, `rank`
(the best target's place in pass B's fused list), `page`, every target's
`rank` and `page`, `first_page` (what pass B's first page held instead) and
`in_research_scope`. Its last lines sum each order up.

## What the results would say

| hypothesis | confirmed if | refuted if |
| --- | --- | --- |
| Q2: the judge rejects official passages that state the claim | `primary` pairs labelled supported come back unsupported, while `seen` third-party pages are accepted | the 9 supported `primary` pairs come back supported (at the rate the judge reaches on `seen` supported pairs) |
| Q2: the source label sways the judge | `primary` and `primary_unlabelled` verdicts differ | they agree |
| Q2: citation framing makes the judge reject other sources | `cc-11d420ee-framed-book` supported and the three framed official pairs unsupported (correct behaviour for a statement that names its source) | the framed official pairs come back supported |
| Q1: the course ranking buries the official passages | under `relevance`, hybrid, with the app's pace, the targets' `page` is past 1 or null, with third-party pages in `first_page` | the targets are on page 1: the live miss is then the research-scoped page and the frozen first-page selection, not the ranking |
| Q1: research scoping hides them | `in_research_scope` is false for a claim whose target the course ranking finds on page 1 | the targets are in scope and still missed |

## Results (2026-10-21)

Judge: Josh's remote `Qwen3.8-27B-Q5_K_S`, batches of 8, 310 s for 60 pairs
(`../results/2026-10-21-claim-corroboration-qwen3.8-27b.jsonl`). Recall:
hybrid, embedding `sha256:421a27e5…` (llama.cpp GGUF), pace `850,11.3,50.6`,
page budget 3,806 tokens
(`../results/2026-10-21-claim-corroboration-recall-hybrid.jsonl`).

| kind | label → verdict |
| --- | --- |
| `seen` (31) | supported 14/16; unsupported 11/15 (4 accepted) |
| `primary` (16) | supported 6/9 (the 3 misses are all `close_call`); controls 7/7 not accepted (one as contradicted) |
| `primary_unlabelled` (9) | the same 6/9 as `primary`, pair for pair |
| `framed` (4) | 4/4 as labelled |

| order | first official passage on page |
| --- | --- |
| `relevance` | 2, 11, 17, 27, 37, 78; never for 2 of 8 (ranks 4, 21, 27, 36, 44, 79) |
| `lesson-sources` | 1, 6, 20, 23, 35, 40, 86; never for 1 |

Read against the hypotheses:

- Q2 refuted: the judge accepts official passages that state the claim at
  about its rate on supported pages it already sees, and the source label
  line changes no verdict. Framing behaves as designed.
- Q1 confirmed, and it is the ranking, not research scoping alone: across
  the whole course the official passage ranks 4th at best and 79th at worst,
  behind the book and third-party explainers that paraphrase the same text.
  With one or two passages a page and the walk ending at the first
  supported page, the judge never reads it. Only 1 of 8 claims has a target
  inside its research round's scope.
- The `seen` pages are judged a little leniently: 4 of 15 pairs labelled
  unsupported were accepted. These are the claims that end as
  `insufficient_standing` in the app (supported, by a source without
  standing).
