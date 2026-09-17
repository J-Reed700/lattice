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
execution traces; do not substitute whole documents for the retrieved passages.

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
