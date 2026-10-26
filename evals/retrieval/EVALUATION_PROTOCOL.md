# Evaluation evidence protocol

A green regression test is evidence about the harness. A strong retrieval score
is evidence about a particular corpus and search configuration. A product quality
claim additionally needs representative held-out tasks, the actual application
execution path, repeated runs, and reviewed answers. Keep those claims separate.

## Offline integrity checks

```sh
python3 -m unittest discover -s scripts -p 'test_*rag_eval.py'
python3 scripts/validate_rag_evals.py
```

The `Eval integrity` workflow runs both on pushes and pull requests without model
credentials. It protects grader semantics, fixture labels, artifact compatibility,
evidence binding, coverage, and statistical comparison behavior. It does not run
models or constitute a model-quality release gate.

## Paired retrieval comparison

```sh
python3 scripts/compare_rag_eval.py evals/retrieval/synthetic-library-v1.json \
  --baseline evals/retrieval/results/synthetic-v1-minilm.jsonl \
  --candidate evals/retrieval/results/synthetic-v1-qwen3.jsonl \
  --k 5 --samples 5000 --seed 2026 \
  --max-regression 0.02 --min-queries 5
```

A nonzero exit is an evidence failure, not a broken script. The historical Qwen
configuration improves retrieval but is much slower. Latency is informational
by default; explicitly add `--max-latency-ratio` to impose a latency gate. Each arm accepts multiple JSONL paths for repeated trials. Every
trial must cover every query exactly once. Trials are averaged within each query;
the bootstrap resamples paired queries rather than treating repeats as new data.
Reports include per-query changes, Recall/nDCG/MRR differences and percentile
95% intervals, dimension slices, p95 latency, and the fraction of queries with
full recall on every candidate trial. Bootstrap intervals are conditional on this
fixture population. Tiny or saturated datasets can yield degenerate intervals;
minimum sample gates and additional held-out tasks remain necessary. Dimension
intervals are exploratory, not simultaneous family-wise confidence guarantees.
Unanswerable queries are excluded from ranking metrics; evaluate their behavior
using retrieval abstention scoring and the answer review gate.

This bootstrap suits retrieval runs over the 194-query v3 fixture. It does not
suit the shipped-path answer set, which is far smaller; use the next section for it.

## Shipped-path statistics: comparing runs and calibrating the judge

`scripts/shipped_path_stats.py` (exact tests in `scripts/exact_stats.py`, tests in
`scripts/test_shipped_path_stats_rag_eval.py`) answers two questions a single
scored run cannot. Both are exact: on fewer than about 100 items, a bootstrap
interval or a test on raw judge scores calls differences that are noise, worst
of all when the judge looks nearly perfect (evalstats, arXiv 2609.35815).

### Is the candidate better?

```sh
python3 scripts/shipped_path_stats.py compare \
  --baseline evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-13-shipped-path-desktop-path-v1.scored.jsonl \
  --candidate evals/retrieval/results/chat/qwen3.5-9b-local/DATE-shipped-path-desktop-path-v1.scored.jsonl \
  [--outcome judge|mechanical|both] [--json report.json]
```

The question is the unit. Each question is paired with itself across the two
runs, and the report lists every paired outcome, the discordant counts (better,
worse), each run's pass rate, and an exact two-sided McNemar p-value (the sign
test on the discordant questions). Concordant questions carry no evidence about
the difference. The verdict is `better` or `worse` only when p ≤ `ALPHA`
(0.05, one comparison chosen before looking; see `exact_stats.py` for why no
extra margin is added), and `can't tell from noise` otherwise. The report also
says how many of the changed questions would have had to go one way for a call:
fewer than six changed questions can never reach 0.05, because six all going
one way is p = 2/64. Each arm accepts several runs of the same set; a question's
outcome is then its pass share across them, and ties drop out of the test.
Questions missing from a run, or whose turn count differs between runs, are
listed and left out, never silently counted.

The two 2026-10-13 arms (utility role and default roles) are 20/34 each, with
two questions better and two worse: `can't tell from noise`, as they should be.

### How good is the judge?

The judge's endpoint and model are independent of the answering model (`score
--endpoint URL --model NAME`), and the scored manifest records its identity under
`settings.judge.identity`: the model name requested, what the server lists at
`/v1/models`, and whether it is the model that wrote the answers. A judge server
that wants a key reads it from `LATTICE_JUDGE_AUTH` as `Header-Name: value`; it is
sent only to the judge and never written to a result. No time limit cuts a grade
off, since a shared remote may queue it behind other work. A judge grading
its own answers is the weakest setup. Greedy grading gives one verdict per turn;
repeating it gives the same verdict, so each turn is graded once. Whichever judge
is used, calibrate it against human labels:

```sh
python3 scripts/shipped_path_stats.py calibration-sample \
  evals/retrieval/desktop-path-v1.json RUN.jsonl RUN.scored.jsonl RUN.calibration-labels.json \
  [--half-width H] [--seed N]
# fill in human_verdict in RUN.calibration-labels.json, then:
python3 scripts/shipped_path_stats.py calibration-score RUN.calibration-labels.json \
  [--judge OTHER.scored.jsonl]   # another judge's scoring of the same answers
```

The sample is stratified by the judge's own verdict (judge pass, judge fail).
Items with a turn that produced no answer were never shown to the judge and fail
whoever grades them, so they form a known stratum that needs no labels. The size
comes from a precision target on the corrected pass rate: by default half the
run's own Wilson 95% half-width (`JUDGE_NOISE_SHARE`), because labels remove only
the judge's error, not the question-sampling error, and at one half the judge's
residual error widens the combined interval by about 12%. The plan assumes the
widest case, a judge no better than a coin inside each stratum, so it assumes
nothing about the judge being measured; a good judge gives a narrower interval
than planned. Allocation is proportional to stratum size. Rows are in random
order, so labelling a prefix and stopping still leaves a valid smaller sample.
The file records the sampled scored file's path and SHA-256, and scoring
refuses to run if that file has changed.

Rows are blind, like the review packets below: each shows every turn's question,
the rubric as the judge reads it, and the answer line by line, with no judge
verdict, reason or stratum, in random order so the strata do not show through.
Set `human_verdict` to `"pass"` when every turn meets its rubric and `"fail"`
otherwise, grading on the rubric alone.

Scoring looks the verdicts up instead. The sampled run's judge defines the
sampling strata; the judge being measured is that run's by default, or any other
scored run of the same answers given with `--judge` (the same 9B answers graded by
the 9B and by a 27B, say), checked by each item's answer hash. Each sampling
stratum is split by the measured judge's verdict into cells; inside a cell the
labelled items are still a simple random sample, so one labelled file scores
every judge of those answers with the same exact method.

The score step reports, with exact 95% intervals for this run's items: the
judge's accuracy on answered items, its false-pass rate (the share of judge
passes a human fails), its false-fail rate (the share of judge fails a human
passes), and the pass rate corrected for its errors by prediction-powered
inference. Because the measured judge's verdict is constant inside each cell,
the PPI estimate, mean(judge) + Σ_c (N_c/N) · mean_c(human − judge), reduces to
Σ_c (N_c/N) · (human pass share in cell c). Its interval combines each
cell's exact hypergeometric interval at α divided by the number of cells
not labelled whole (Bonferroni), so it holds at any sample size and does not
collapse to zero width when the judge agrees with every label. A cell
labelled whole is known exactly. The CLT interval PPI papers use is not
reported: at ten-odd labels per stratum it under-covers. These intervals are
about this run's items; whether a change helped is the paired test's question.

The 2026-10-13 utility-role run has a labelling file ready:
`results/chat/qwen3.5-9b-local/2026-10-13-shipped-path-desktop-path-v1.calibration-labels.json`
(17 of 20 judge passes, 9 of 11 judge fails; 3 unanswered need no label; about
28 minutes to label).

Choose budgets before inspecting candidate results. Compare identical corpora,
cutoffs, hardware, warmup policies, and concurrency. Randomize execution order.
The comparator hashes supplied artifacts but cannot establish how an old run was
collected or verify its hardware. Keep raw outputs and collection metadata.

## Generate answers with provenance

```sh
python3 scripts/chat_rag_eval.py \
  evals/retrieval/synthetic-library-v1.json \
  evals/retrieval/results/synthetic-v1-qwen3.jsonl \
  evals/retrieval/synthetic-library-v1-answers.json \
  /tmp/lattice-answers.jsonl \
  --config /path/to/provider.json --model-revision IMMUTABLE_WEIGHTS_SHA256
```

This is explicitly a **whole-document component harness**. It does not exercise
streaming, desktop tools, workspace authorization, extraction, or answer
verification. It must not be advertised as an end-to-end product benchmark.
A future application adapter must export actual supplied evidence and complete
execution traces; do not substitute whole documents for the retrieved passages. That adapter now exists for chat:
`crates/lattice-desktop/examples/shipped_path_eval` runs the desktop turn headlessly and
exports the evidence each turn supplied, every model request, the turn record
and the grounding verdicts. See
[`2026-10-12-SHIPPED-PATH-BASELINE.md`](2026-10-12-SHIPPED-PATH-BASELINE.md).

A sibling `.manifest.json` records hashes of the corpus, labels, retrieval input,
harness code, model revision and decoding settings; it also records the commit
and Python/platform environment. Model revision is operator supplied, not remotely
attested. Provider headers are never serialized. Endpoint identity is hashed.
Rows record answer/context hashes, usage when returned, completion reason and
request attempts. Latency includes retries. Non-normal completions fail the run;
they are not dropped to improve the average. Keep the partial run and diagnose
failures. `--resume` requires identical provenance and query selection; changing
weights, input, code, or settings requires a new output path. Existing files are
never overwritten. A partial final JSONL line is rejected; inspect the interrupted
write before repairing it. Simultaneous writers to one output are unsupported.

`--query` and `--limit` are debugging controls. Coverage always uses the entire
corpus denominator, and partial runs cannot pass a release gate.

### Against a cloud model

A recipe can name a cloud model in place of a server, as
`shipped-path-recipe-claude-opus-5-5.json` does. That recipe holds the window to 32,768
tokens: the app sizes evidence from the window, so a cloud model's full window makes every
turn several times as costly, and a held window keeps runs comparable to each other:

```json
"cloud": {"provider": "anthropic", "model": "claude-opus-5-5"}
```

The desktop reads the provider's key as it always does (`ANTHROPIC_API_KEY`, else
the key saved in Settings). The run starts the same recording proxy and points the
desktop's client at it through `LATTICE_ANTHROPIC_BASE_URL`, so every request is
still kept verbatim. The run manifest's `answering_model` names the model requested
and every name its replies were served under. No weights hash applies to a cloud model.

To grade with Claude, use `score --judge-provider anthropic --model claude-opus-5-5`,
keyed by `ANTHROPIC_API_KEY`. Claude rejects the sampling settings greedy grading
uses, so a Claude judge is not deterministic, and its manifest says so
(`judge.deterministic: false`).

Results are filed by model (`results/chat/<model>/`), and `shipped_path_stats.py
compare` refuses arms answered or judged by different models: a difference between
two models is not a difference a change made. To compare the models themselves,
pass `--across-models`, and the report says that is what it measured. To learn
whether a change helped on Claude, compare a Claude run before the change with one
after it.

### Against the bundled sidecar instead of a remote server

The harness only needs an OpenAI-compatible base URL, so the bundled
`llama-server` with a locally downloaded GGUF works and avoids queueing on a
shared single-slot remote. Same flags the app uses, plus a fixed port:

```sh
crates/lattice-desktop/binaries/llama-server-aarch64-apple-darwin \
  -m ~/.cache/lattice/models/qwen3.5-9b-q4_k_m/Qwen3.5-9B-Q4_K_M.gguf \
  --port 8089 --host 127.0.0.1 --no-webui -ngl 99 --ctx-size 16384 --jinja &
python3 scripts/chat_rag_eval.py evals/retrieval/synthetic-library-v1.json \
  evals/retrieval/results/synthetic-v1-qwen3.jsonl \
  evals/retrieval/synthetic-library-v1-answers.json out.jsonl \
  --config evals/retrieval/local-sidecar-provider.example.json \
  --model-revision "$(shasum -a 256 ~/.cache/lattice/models/qwen3.5-9b-q4_k_m/Qwen3.5-9B-Q4_K_M.gguf | cut -d' ' -f1)" \
  --delay 0
```

The 38-query v1 set takes about two minutes on an Apple Silicon laptop with the
9B model (`results/2026-09-25-synthetic-v1-qwen3-local-qwen3.5-9b-chat.jsonl`).
The remote Qwen3.8 27B is the chat model in daily use, but its single shared
slot can queue each request for minutes, so use the local 9B to iterate on the
harness and prompt and the 27B for the numbers that matter. Compare runs only
against the same weights hash; a 9B score is not a 27B regression signal.

Answer runs are kept as one history per model under `results/chat/`:
`qwen3.8-27b-remote/` and `qwen3.5-9b-local/`. Put a new run in the folder for
the weights it used, never across folders, and keep the `.manifest.json` beside
it. The conversation-memory suite (`crates/lattice-desktop/tests/conversation_memory_evals.rs`)
keeps the same split under `evals/conversation-memory/results/`; only the
printed summary goes there, since its traces carry conversation text.

### Score meanings and migration

Schema 2 deliberately retires the old claims `facts_correct` and
`citations_correct`. Their replacements are `pattern_checks_passed` and
`citation_structure_passed`. `mechanical_checks_passed` is a cheap diagnostic,
not a correctness verdict. Regexes cannot detect arbitrary contradictions or
unsupported additions. `answer_correct` is null until a human review is applied.
Historical JSONL files remain unchanged and cannot be resumed or presented as
verified accuracy. Regenerate manifested answers; do not invent provenance for
historical artifacts. Consumers of the old metric names need to migrate.

## Blinded, evidence-bound review

```sh
python3 scripts/review_rag_eval.py evals/retrieval/synthetic-library-v1.json \
  /tmp/lattice-answers.jsonl --export /tmp/lattice-review-packets.jsonl
```

Packets omit model identity and mechanical scores. Assign packets in randomized
order to reviewers who did not produce the answer. Each review is one JSONL
object with these fields, copied from its packet:
`rubric`, `query_id`, `answer_sha256`, `run_id`, `packet_sha256`.
Add `reviewer_type: "human"`, a stable `reviewer` identifier, boolean `complete`,
`correct`, `abstention_appropriate`, and a nonempty `rationale`.

Add `spans`, partitioning every non-whitespace character of the answer into
nonoverlapping ranges. Offsets are Python Unicode character offsets, start
inclusive and end exclusive. Separate each independently checkable assertion;
do not combine a supported claim and an unsupported claim into one favorable
judgment. Each span contains:

```json
{
  "start": 0,
  "end": 25,
  "verdict": "supported",
  "rationale": "The source states the same duration.",
  "citations_support_claim": true,
  "evidence": [{"document_id": "a", "quote": "14 days"}]
}
```

Allowed verdicts are `supported`, `unsupported`, `contradicted`, and `nonfactual`.
A supported claim requires a verbatim quote from an actually supplied document.
The reviewer must determine entailment, whether citations support the associated
claim, completeness, contradictions, and appropriate abstention. `nonfactual`
is for connective language or an abstention statement, never a way to exempt a
claim. Check hallucinated suffixes and uncited claims as carefully as cited ones.
The validator verifies quote locations and review coverage, not human honesty or
semantic truth. Model-generated judgments cannot stand in for human review.
For consequential release claims, independently double-review a sample and all
failures, adjudicate disagreements, and publish agreement rates. An automated
semantic judge should only be introduced after calibration against those labels.

```sh
python3 scripts/review_rag_eval.py evals/retrieval/synthetic-library-v1.json \
  /tmp/lattice-answers.jsonl \
  --expectations evals/retrieval/synthetic-library-v1-answers.json \
  --reviews /tmp/lattice-human-reviews.jsonl \
  --policy evals/retrieval/release-policy.example.json
```

The example policy is deliberately demanding and is **not a measured product
SLO**. Set a reviewed policy before a release run. Full answer and review coverage
is mandatory. Both overall and every dimension must meet sample size, Wilson 95%
accuracy lower-bound requirements. Latency is informational unless the policy
explicitly sets `max_p95_latency_ms`. Small fixtures should fail the
sample-size gate. Missing reviews produce null accuracy, never a passing zero
or a conveniently reduced denominator. Reports contain review and grader hashes.

## Evidence required for a product claim

Maintain a held-out, access-controlled set of representative user tasks, separate
from fixtures used to tune prompts or retrieval. Include document extraction,
workspace boundaries, conflicting versions, indirect prompt injection, multi-hop
answers, follow-up turns, multilingual tasks, inaccessible evidence, and long
files. Label expected evidence and acceptable outcomes before evaluating.
Record failures from production with consent and appropriate redaction; keep
near-duplicates in the same data split. Rotate exposed tasks and version every
label correction. Synthetic regressions remain useful but do not replace this.

Run the shipped application path, preserve tool and verification traces, measure
latency and resource cost, and repeat stochastic tasks under the same settings.
Publish all failures, corpus/slice sizes, uncertainty, comparisons, and limitations.
This repository currently supplies synthetic retrieval fixtures and a component
answer harness; no held-out customer benchmark, calibrated semantic judge, or
full desktop chat benchmark is claimed by these tools.
