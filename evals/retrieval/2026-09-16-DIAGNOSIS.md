# Fresh diagnosis and grounding fixes — September 16, 2026

## What ran

- 38 fresh answer requests to the configured `Qwen3.8-27B-Q5_K_S.gguf` endpoint.
  These used the saved v1 Qwen retrieval rankings, full document context, and a
  2,048-token generation budget. Retrieval itself was not rerun.
- Four synthetic adversarial cases through the actual Rust `GroundingVerifier`
  and `LlamaCppLlm` adapter: supported claim, wrong number, negation, invented
  citation. The supported claim passed; the two contradictions were identified;
  the invented citation was rejected without a model call.
- 32 deterministic native grounding tests passed; the opt-in live test passed
  separately with all four cases. 49 Python eval tests passed, and all three
  retrieval corpora plus answer labels validated.

[Fresh answers and mechanical diagnostics](results/2026-09-16-fresh-answer-diagnostics.json)

[Live production-verifier results](results/2026-09-16-live-grounding-regressions.json)

The answer collection began with the legacy runner before the schema-2 harness
files became available in the working checkout. The archived diagnostic artifact
rescored those actual answer texts under schema 2. It does not invent an immutable
model revision, finish reason, usage record, or contemporaneous manifest that the
legacy runner did not capture. It is not a resumable schema-2 run.

## Diagnosis

The legacy runner reported 26/38 passes (68.4%). That is a mechanical pass rate,
not defensible answer accuracy. Examples of false failure signals:

- `do-not-retry-auth`: correctly refuses repeated authentication attempts, but
  misses the fixture's required literal `401`, which the question did not request.
- `write-timeout-safety`: cites the timeout document containing the answer but
  fails because the fixture requires an additional service-failure citation.
- `failed-model-migration`: answers that the previous generation stays active,
  but fails a requirement to mention separate generations or atomic activation.
- `unanswerable-japanese-holiday-calendar`: explicitly says the 2027 calendar is
  unavailable, but fails the rule forbidding all citations in an abstention.

Other patterns are similarly wording-sensitive. These observations do not amount
to independent human review of all claims, and no verified accuracy is reported.
The answer text did not justify changing the generator simply to please regexes.

## Production defects fixed

| Defect | Corrected behavior |
| --- | --- |
| Invalid citation numbers were discarded, enabling fallback to other evidence | Invalid and mixed valid/invalid citations remain unsupported; the judge cannot rescue them |
| Citation numbers were interpreted as array positions | Assigned citation IDs resolve to their actual sources, including reordered/sparse IDs |
| Empty token sets shifted evidence indices | Exactly one token set is retained per source |
| Short cited numeric claims were skipped | Cited claims are evaluated even below the general prose length threshold |
| A supported judge verdict survived an invented or missing quote | Positive verdicts require a quote found in the supplied evidence |
| Failed semantic judgments restored positive lexical guesses | Escalated claims stay unsupported when judging fails or times out |

The four initial adversarial regression tests failed on the original verifier
and passed after the fixes. Further tests cover short claims and empty sources.
These fixes change grounding indicators; they do not automatically rewrite a
model's answer. Uncited short prose and strong lexical matches remain limitations
of the heuristic prefilter. No-judge mode is still explicitly lexical.

Latency is informational by default. Retrieval comparisons accept an optional
`--max-latency-ratio`; answer policies can optionally set `max_p95_latency_ms`.
The default example policy imposes quality and evidence requirements only.

## Rerun the live verifier

```sh
LATTICE_LLAMACPP_SETTINGS='/path/to/app/settings.json' \
  cargo test --manifest-path src-tauri/Cargo.toml --lib \
  live_grounding_adversarial_regressions -- --ignored --nocapture
```

The live test sends synthetic claims only. It does not mutate the library or
conversation history. This is real production-adapter/verifier inference, not a
full desktop chat, retrieval, tool-loop, or independently reviewed user benchmark.
