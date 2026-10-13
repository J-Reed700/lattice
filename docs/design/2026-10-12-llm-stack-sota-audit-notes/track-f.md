# Track F: reason before schema

## Survey (all send `json_schema` unless noted)

| Calls | Format demand (before) | Bound >2000 (before) |
|---|---|---|
| Outline, lesson, recall drafts | "Return only JSON…" + `format` examples | 5000, 3000 |
| Teaching review/repair, text edits, answer-key, finding check, outline review | "Return JSON with issues", "in the supplied schema" | none |
| Content verification (9) | "unit-N key", "in the supplied schema" | none |
| Diagnostic, assessment, practice, practical, study (13) | "Return (strict) JSON (only)", field lists | 8000, 6000, 262144, 12000 |
| Memory extractor, verifier | "Return only JSON matching the requested schema" | none |
| Memory summariser | prose, no schema | n/a |
| Intent | **no schema**; "Respond with strict JSON only" | n/a |

Finding: 11 learning bounds exceeded 2000, so llama.cpp dropped the whole grammar for lessons, assessments, practice, practical and recall drafts. Only the prompt text held their shape.

## What changed

- New `lessons/structured_call.rs` (+ `_tests.rs`) holds the one-step call (moved from `generation.rs`) and `complete_reasoned_json`.
  - Step 1: system + brief, task, no schema, chat sampling, capped at the answer's reservation.
  - Step 2: `[system, task, draft, "Step 2: give the final answer…"]`, with schema and greedy sampling. It opens with step 1's exact messages, so the server can reuse that prefix.
  - Falls back to one step when the window can't hold task + draft + answer, or the draft is empty or overflows.
  - Lesson jobs checkpoint the draft, so a resumed job skips step 1.
  - Progress and the log show the draft's token cost. Each step has its own 300 s budget.
- Knobs in `model_call.rs`:
  - `REASON_FIRST_AUTHORING = true`: outline draft, lesson, outline review, and the first attempt of the teaching review and answer-key check.
  - `REASON_FIRST_CLASSIFIERS = false`: finding check.
  - Corrections stay one-step.
- Format demands removed from about 20 learning prompts and from memory (versions bumped). Intent sends its schema and keeps the lenient parser.
- Bounds over 2000 dropped; the parsers already enforce those caps. Teaching-review policy is now v3.
- Tests: the memory bound test covers intent, a scan checks learning's literal bounds, and fixtures check every schema request for bounds and format demands.

## Verified

`cargo check --lib` and `--all-targets` pass with no warnings; `cargo fmt` is clean.

The single `cargo test --lib` run (learning, memory, intent): 378 passed, 12 failed. All 12 failures came from my fixture guard. It caught "Return JSON with checks" in the shared `claim_verification/batch.rs`, which is not my file. The guard now exempts that prompt; the fix compiles but was not re-run (one-run rule). To re-run:

`cargo test --lib -- features::learning::lessons::content_verification features::learning::lessons::course_generation_tests`

## Not verified, and risks

- No live model ran: quality gain, prefix reuse and draft lengths are unproven.
- Lesson tokens roughly double. On 32k windows, lessons will often fall back to one step.
- Greedy decoding of a long answer can loop.
- Step 1 needs thinking off (Track A). Both steps keep `reasoning_effort: "low"`, which Qwen 3.8 accepts.
- Without serde_json's `preserve_order`, schema keys reach llama.cpp in alphabetical order, so `correctIndex` is generated before `prompt`. Not addressed.
- I edited `memory/tests.rs` to extend the bound test. Memory design §15 still lists "Return only JSON".
- Git-ignored placeholders added: the sidecar file and `dist/`.

## Suggested commit

```
Reason before the schema in learning; drop prompt format demands

Asking for JSON in the prompt costs small models most of their
structured-output accuracy; the grammar does not. Authoring calls now
draft in prose, then answer under the schema from that draft. Format
demands leave learning, memory and intent prompts; intent sends its
schema. Bounds over 2000 made llama.cpp drop whole grammars, so those
caps move to the parsers.
```
