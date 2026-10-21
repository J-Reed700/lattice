# Course eval: scoring course and lesson generation against a baseline

Status: design, 2026-10-20. Branch `feat/course-eval` at main `4b44a339`.
Builders: three work packages, section 7. Nothing here is implemented yet.

Course generation (outline → review → lessons with claim verification) is the
most-changed part of the app and has never been scored. The only check is the
live pass/fail acceptance run in
[course-generation-testing.md](../development/course-generation-testing.md).
Chat answers, retrieval, memory and the claim checkers each have a scored eval
with committed baselines and exact paired statistics. This design gives
courses the same: a recipe-driven run, mechanical and judged scores, a
record/replay of the web so two runs see the same pages, and `compare` with a
`can't tell from noise` verdict, reusing the chat eval's code where it fits.

## 0. Decisions at a glance

| Question | Decision |
|---|---|
| Where the harness lives | A new `#[ignore]` lib test beside `live_course_e2e` (`lessons/live_course_eval.rs`), run from the snapshotted test binary like every live test. Not an `examples/` binary: the pipeline entry points it needs are crate-private. |
| Unit of comparison | The *item* `<request>/<stage>/<criterion>`, one judged criterion of one artifact; plus the *slot* `<request>/outline` and `<request>/<lesson>` (all its criteria and mechanical checks pass). `compare` runs on both; the slot verdict is the one quoted. |
| Headline numbers | (1) slots passed (mechanical and judged), (2) lessons that reached Ready, (3) share of lesson claims supported by a cited passage (and general-knowledge share), (4) wall minutes per lesson. |
| Web | Record/replay at the `WebServiceTrait` port with a per-request trace; misses counted and reported, never hidden. Traces are not committed (page text, size); their index hash is. |
| Model sampling | The app's shipped settings, merged with the recipe's patch as the chat eval does. Sampling noise stays and the exact paired test absorbs it. |
| Judge | An independent OpenAI-compatible endpoint named on the `score` command line, identity recorded in the manifest, key from `LATTICE_JUDGE_AUTH`; same code as the chat judge (`scripts/shipped_path_eval.py`). |
| Statistics | `scripts/shipped_path_stats.py compare`, `calibration-sample`, `calibration-score` unchanged; the course scorer writes rows in the shape they read. One three-line extension to `rubric_lines`. |
| Requests | The 3 committed topic-only requests, a terse variant of each (6 committed), plus Josh's local `.NET` request when present (7). Default stage set: outline plus lesson `m1.l1`. |
| Results | `evals/courses/results/<model>/<date>-course-eval-v1-<label>/` committed: manifest, outline and lesson text, metrics, scored rows, summary. Raw run dirs (`course.sqlite`, `call-N.json`, traces) stay under gitignored `artifacts/course-eval/`. |
| Docs | New `evals/courses/README.md` (protocol and layout); new §7 in `course-generation-testing.md`; every run gets an entry in `course-generation-history.md`. |

## 1. What is measured, per stage

The pipeline stages are O1–O6 (outline) and L1–L14 (lesson) in
[course-generation-pipeline.md §2](../development/course-generation-pipeline.md#2-the-stages).
Everything below is per request; lesson rows are per prepared lesson slot.

### 1.1 Mechanical checks (code decides)

Written by the Rust harness into `metrics.json` (section 6.2), rolled up by
the Python scorer. "Source" says where the fact comes from.

Outline (after `service::generate_with_references_and_progress`, no Repair
presses):

| Key | Meaning | Source |
|---|---|---|
| `outline.review.status` | `passed` / `needs_repair` / `unchecked` | `LearningOutlineReviewDto.status` |
| `outline.review.issues` | open findings at the end | `review.issues.len()` |
| `outline.review.repair_passes` | automatic O5–O6 passes | `review.repair_passes` |
| `outline.references.count` | sources saved with the draft | `program.sources.len()` |
| `outline.references.domains` | distinct registrable hosts, and `max_share` of one host | source URLs |
| `outline.links.items`, `outline.links.linked` | outline items and those with a covering citation | `OutlineDraft.citation_receipts` (`covers == true`) and `LinkLog` |
| `outline.citations.verified`, `outline.citations.removed` | as the review DTO counts them | `review.citations_verified`, `review.citations_removed` |
| `outline.quotes.checked`, `outline.quotes.unmatched` | author quotes that are not a verbatim span of their source | `planning/outline_evidence::quote_checks(candidate, sources)` |
| `outline.shape.modules`, `outline.shape.lessons` | counts | program summary |
| `outline.shape.session_fit` | lessons whose `estimated_minutes` ≤ `minutes_per_session` | lesson DTOs vs request |
| `outline.cost.seconds`, `outline.cost.calls`, `outline.cost.input_tokens`, `outline.cost.output_tokens`, `outline.cost.peak_input_tokens` | wall time and model calls | `ObservedModel` counter at stage boundaries; tokens summed from `call-N.json` by the scorer |
| `outline.resumed` | true when the run was resumed through `repair_saved` after an interruption (section 3.4) | harness |

Lesson, per selector (e.g. `m1.l1`):

| Key | Meaning | Source |
|---|---|---|
| `lesson.outcome` | `ready` / `failed` / `interrupted`, with `error` text | job record `status`, `error` |
| `lesson.deferrals` | transient deferrals the runtime redelivered | count of `Pending` sightings in `phases.jsonl` after the first `Running` |
| `lesson.shape.blocks`, `lesson.shape.kinds`, `lesson.shape.questions` | counts and the block kinds present | `LearningLessonDto` |
| `lesson.citations.source_ids`, `lesson.citations.unresolved` | source ids on blocks and questions, and those not among the program's sources | lesson DTO vs `verification_sources` |
| `lesson.claims.total` | findings in the stored report | `learning_lesson_verifications.report_json` → `findings[]` |
| `lesson.claims.supported`, `.unsupported`, `.contradicted`, `.unverified` | verdict counts | `findings[].verdict` |
| `lesson.claims.general_knowledge` | unsupported findings the gate kept | findings with `verdict == unsupported` and a `resolution` (builder confirms the serde shape in `general_knowledge.rs`) |
| `lesson.claims.quotes_unmatched` | Supported findings whose `supporting_quote` is not a verbatim span of any of their `evidence[]` passages | report, re-checked by the harness (the pipeline enforces this; a non-zero here is a regression) |
| `lesson.claims.issues` | `issues[]` at publication | report |
| `lesson.examples.run`, `lesson.examples.failed` | code examples executed and failed | report `executions[]` |
| `lesson.rounds.repair`, `lesson.rounds.deepening` | repair rounds and evidence-deepening rounds | phase changes to `Repair`; progress messages matching `(page N)` |
| `lesson.cost.seconds`, `.calls`, `.input_tokens`, `.output_tokens`, `.peak_input_tokens` | as for the outline | as above |
| `lesson.cost.phase_seconds` | seconds per `LearningGenerationPhase` (References … Publishing) | `phases.jsonl` |

Slot-level mechanical pass (used by `--outcome mechanical|both`):

- outline slot: `status == passed` and `quotes.unmatched == 0`;
- lesson slot: `outcome == ready` and `citations.unresolved == 0` and
  `claims.quotes_unmatched == 0` and `claims.unverified == 0`.

### 1.2 Judged checks (a model decides, each with a rubric)

One judge call per criterion per artifact, greedy, schema-bound (`verdict`
pass/fail + one-sentence reason), exactly `judge_request` in
`scripts/shipped_path_eval.py`. The judge sees the request, the artifact
rendered as markdown (section 6.3), and the criterion's rubric lines. The
rubric texts are in Appendix A, verbatim; they are the contract and carry the
id `lattice-course-judge/1`.

| Item id | Criterion |
|---|---|
| `<r>/outline/O1` | Goal coverage: every `outline_must_cover` topic is a module or lesson objective |
| `<r>/outline/O2` | Fits prior knowledge: nothing in `prior_knowledge_excludes` is taught from scratch; nothing the learner lacks is assumed |
| `<r>/outline/O3` | Progression: each lesson builds on earlier ones; prerequisites come before use |
| `<r>/outline/O4` | Session fit: each lesson's objective is achievable in `minutes_per_session` |
| `<r>/outline/O5` | Specificity: objectives are concrete and assessable |
| `<r>/<lesson>/L1` | Teaches rather than lists: explanations give mechanisms, reasons and examples |
| `<r>/<lesson>/L2` | Objective met by the content |
| `<r>/<lesson>/L3` | Worked example complete and correct |
| `<r>/<lesson>/L4` | Practice doable from the lesson alone and matched to the objective |
| `<r>/<lesson>/L5` | Questions test the objective; one defensible correct answer; plausible distractors |
| `<r>/<lesson>/L6` | Level pitched to the stated prior knowledge |
| `<r>/<lesson>/L7` | Coherent: sections connect, recap matches, no internal contradictions |
| `<r>/<lesson>/claim/<unit>-<n>` | Claim correctness: the statement is true to the judge's knowledge (secondary; section 1.3) |

A lesson that did not reach Ready has no artifact: its L1–L7 items are
`graded: false`, `judge_pass: false`, as the chat scorer treats a turn with no
answer. They fail whoever grades them and form a known stratum in calibration.

### 1.3 Headline and secondary

Headline, in this order, all in `summary.md`:

1. **Slots passed** (`both`): `<n>/<N>` outline slots and `<n>/<N>` lesson
   slots whose mechanical checks and every judged criterion pass. This is the
   number `compare` gives a verdict on.
2. **Lessons Ready**: `<n>/<N>` lesson slots with `outcome == ready`, and the
   failure reasons for the rest.
3. **Grounding**: supported claims / total claims, general-knowledge share,
   contradicted count, summed over prepared lessons, with the judge-corrected
   claim-correctness rate once a calibration slice exists.
4. **Cost**: median and maximum wall minutes per outline and per lesson; model
   calls and input tokens per lesson.

Secondary: every other mechanical key, the per-criterion pass table, and the
claim-correctness judge. Claim correctness is a world-knowledge judgement, the
opposite of the labelling rule in `evals/grounding/lesson-claims-w1` (which
judges from the passage alone); it answers "does the lesson teach true
things", a different question from "did the pipeline ground them". A small
judge is weak at it, which is why its rate is only quoted after calibration.

## 2. Reproducibility

### 2.1 What the code gives today

- Reference captures go through `WebService::read_page_as(url,
  PageRead::Reference, Want::Live)`: "never served from or written to the chat
  cache" (`web/services/web.rs`). The `<run>/web/` directory the live test
  creates is the chat page cache and holds nothing the course pipeline reads
  twice. The lesson worker in `run_and_confirm` even uses a fresh `tempdir`.
- Frozen snapshots (`web/services/snapshot/`) capture cited chat pages as inert
  HTML for the reader; they are not on the course path.
- So there is no replay today. Two runs with no code change search a live
  index and fetch live pages, and the outline differs before any model call.

### 2.2 Decision: a traced web port

Every learning entry point takes `&dyn WebServiceTrait`
(`lattice_core::application::ports::web_service`); the lesson worker takes
`Option<Arc<dyn WebServiceTrait>>` (`research_web`). That port is the seam.

`TracedWeb` (new, test-only: `lessons/live_web_trace.rs`) implements
`WebServiceTrait`, wraps `Arc<dyn WebServiceTrait>` (the real `WebService`)
and a trace directory, and runs in one of three modes:

| `LATTICE_COURSE_WEB_MODE` | Hit | Miss |
|---|---|---|
| `live` | no trace is read or written | — |
| `replay` (default) | serve the stored entry | go live, store the entry, count a filled miss |
| `replay-strict` | serve the stored entry | return `AppError::InvalidInput("course eval: no trace entry for …")`, count a failed miss |

A baseline is recorded by running `replay` against an empty trace. The
candidate replays the same trace; its new searches (the model's queries are
sampled text, so some are new) are filled live and counted. "Miss" is a
first-class result: `web-trace-report.json` lists them, and `summary.md`
prints `web: 412 hits, 9 misses filled, 0 failed` per request. A run with many
misses is a run whose web input drifted, and the reader can see it.

Why fill rather than fail: a candidate that plans one new query would
otherwise never finish, and an eval that cannot finish measures nothing.
Why not return empty results on a miss: an empty search silently changes the
pipeline's path (the query is "skipped"), which is exactly the hidden drift
the trace exists to expose. `replay-strict` exists for an offline check that
a trace is complete.

Trace layout, one per request id, shared by every run of that request:

```
artifacts/course-eval/traces/<request-id>/
  index.jsonl               one line per entry: {"key","method","summary","sha256","bytes","recorded_at"}
  entries/<key>.json        {"method","key","input":…,"ok":true,"output":…}
                            or {"method","key","input":…,"ok":false,"error":"<Display>","variant":"<AppError variant>"}
```

Keys: `key = sha256(method + "\n" + canonical input)`, where canonical input
is the serde JSON of the port's input with keys sorted and null fields
removed. `search_web` keys the whole `WebSearchInput` (query, kind, subject,
identifier, ecosystem, site, after); fetches key the URL (and `as_of` day when
given). Pagination is inside `search_through_sourcing`, so one `search_web`
call is one entry. All port types derive `Serialize`/`Deserialize`
(`contracts/web.rs`), so entries are plain JSON.

Method handling in `TracedWeb`:

- `search_web`, `fetch_url_content`, `fetch_url_content_as_of`,
  `fetch_reference_page`: traced.
- `fetch_reference_content`: implemented as `self.fetch_reference_page(url)
  .map(|p| p.output)`, so a page is one entry whichever method asked.
- `validate_url`: pure; always delegated.
- `follow_links`: `Lead` is not serializable and no learning code calls it;
  delegated live and counted under `untraced_calls`.

Errors: a recorded failure replays as `AppError::InvalidInput(<recorded
message>)`. This is the variant the real service uses for an unusable page
("did not provide enough static reference text") and it is not in
`JobFailure::is_transient` (`Network`, `ServiceNotAvailable`,
`RateLimitExceeded` are), so a replayed run never defers on a page that failed
when the baseline ran. Search failures are logged and skipped by
`web_research::search_need` whatever the variant.

Traces are not committed: a request's trace holds the full text of 20–60
reference pages plus research fetches (tens of MB), it is third-party page
text in a public repository, and it is machine-local by nature. The results
manifest records `web.trace.index_sha256` (sha256 over the sorted
`key sha256` lines of `index.jsonl` at the end of the run) and
`entries_at_start`, so two results say whether they replayed the same trace
and how much one added. Josh's iCloud backup folder covers `artifacts/`.

### 2.3 What stays non-deterministic, and how the statistics carry it

- **Model sampling.** The run uses the app's shipped sampling (temperature,
  top-p, top-k from `LLMSettingsDto` defaults) merged with the recipe patch,
  as the chat recipe does. Users run that pipeline, so the eval scores it. Two
  runs of the same code therefore differ; `compare` is an exact paired test
  that reports `can't tell from noise` unless the discordant items are
  lopsided enough (section 5). A recipe may set `settings.llm.temperature: 0`
  with `topK: 1` for a sensitivity run; that is a different arm and is named
  in the manifest, never silently.
- **Trace misses** change the web input; they are counted and listed.
- **Server state**: the model server's version, context size and slot count
  are probed at start (`/v1/models`, `/props` when offered) and recorded.
  Weights are hashed when the recipe names a local GGUF; a remote model's
  revision is operator-supplied (`model_revision`), as the chat protocol says.
- **Clock-dependent pipeline behaviour** (source `acquired_at`, `after` dates
  the planner writes): recorded, not controlled.

## 3. Cost and scale

### 3.1 Requests

`evals/courses/course-eval-v1.json` lists seven request ids:

| id | file | shape | status |
|---|---|---|---|
| `postgres-performance` | `requests/2-postgres-performance.json` | full | exists |
| `music-theory-production` | `requests/3-music-theory-production.json` | full | exists |
| `system-design-fundamentals` | `requests/4-system-design-fundamentals.json` | full | exists |
| `postgres-performance-terse` | `requests/2t-postgres-performance-terse.json` | terse | new (WP-C) |
| `music-theory-production-terse` | `requests/3t-music-theory-production-terse.json` | terse | new (WP-C) |
| `system-design-fundamentals-terse` | `requests/4t-system-design-fundamentals-terse.json` | terse | new (WP-C) |
| `dotnet-interview` | `requests/1-dotnet-interview.json` | full | local only, `optional: true` |

Terse variants are written the way the chat eval's `synthetic-library-v3-terse`
was: the same learner, typed the way people type (goal ≤ 12 words, prior
knowledge one clause, same minutes and depth). Each terse item carries the
same `outline_must_cover` and `prior_knowledge_excludes` as its full twin, so
the outline judge asks the same questions of both. The `.NET` request stays
gitignored (it describes Josh); the set marks it optional and the harness
skips a missing optional request with a line in `run.json`.

### 3.2 Stages per request

Every request: the outline (O1–O6, no Repair presses). Then the lessons named
by the recipe's `lessons` list, default `["m1.l1"]` (module 1, lesson 1: the
acceptance test's lesson). Each selected lesson is one `LessonPreparation`
job with one lesson id, run to completion before the next, so each has its
own timing, outcome and resume point. `lessons: []` is the outline-only tier.

### 3.3 Expected wall time (from the run history)

Facts from `course-generation-history.md` and `course-generation-testing.md`,
all on the remote 27B:

- Outline to a review verdict: 10, 12, 16, 27, 32 minutes (wp3-outline-2,
  final-3, final-2, final-1, wp1-1); 816 s for wp1-1.
- Lesson to Ready: 128 minutes (lesson-fix-1; 213 recorded calls in its log,
  3.99M input tokens, peak request 95,396 input tokens). Lessons that failed
  or were stopped ran 132 minutes (final2-1) and 202 minutes (accept2, stopped
  in evidence deepening). The testing doc's working figure is 2–3 hours.

So on the 27B: outline tier for 7 requests ≈ 1.5–3.5 h; outline plus one
lesson each ≈ 7 × (0.25–0.5 h + 2–3.5 h) ≈ 16–28 h, unattended. The judge
adds, per lesson, 7 calls of ~15k tokens and ~70 claim calls of ~300 tokens;
on a local 9B that is minutes. No figure exists for the 9B as the course
model; the first 9B run establishes it and is written into the history.

### 3.4 Unattended, resumable, one at a time

`scripts/course-eval.sh run <recipe> <label>` is the driver (WP-A):

- takes `artifacts/course-eval/.lock` with `lockf -t 0`; a second run refuses
  to start. The model server is single-slot; the chat eval must not run at the
  same time, and the doc says so;
- runs under `caffeinate -i`; stdout/stderr per request to `<run>/<id>.log`;
- for each request in set order: skip when `<run>/<id>/done.json` exists;
  otherwise invoke the binary with the environment in section 6.1; a non-zero
  exit is recorded in `<run>/<id>/exit.json` and the loop continues to the
  next request;
- never applies a wall-clock limit to anything. A stuck request is stopped by
  Josh (Ctrl-C or `kill`), which is the user Stop the rules allow.

The Rust test resumes from what is on disk when `LATTICE_COURSE_RUN` exists:

| On disk | Action |
|---|---|
| no `course.sqlite` | fresh run: create the database, migrate, generate |
| program exists, outline `Passed`, not accepted | accept, then lessons |
| program exists, outline `Unchecked`/`NeedsRepair` | continue through `outline_draft::repair_saved` (the app's own Repair continuation, which reuses receipts); set `outline.resumed = true`. The slot still counts as passed only if `Passed` results, and the summary shows it was resumed. Drafting itself saves nothing until the draft exists, so an interruption during O1–O3 starts the outline over |
| job for a selector exists, `Pending`/`Running`/`Interrupted` | re-register the worker and dispatch; the runtime redelivers from checkpoints |
| job `Failed` with a transient error text | `retry_job` as `live_small_lesson_resumes_to_ready` does, then run |
| job `Failed` otherwise, or `lesson-<sel>.json` exists | recorded; move on |

A server restart mid-lesson is a transient error: the job runtime defers and
redelivers (`lesson_job_config()` backoff), and the outline waits through
`PatientModel`, which the live test already wraps around the model (the
testing doc's note that it does not is stale; the code at `4b44a339` does).

## 4. Server-agnostic: the recipe

`evals/courses/recipes/course-eval-9b-local.json` (WP-C) follows
`evals/retrieval/shipped-path-recipe-9b-local.json` field for field where the
fields mean the same thing:

```json
{
  "schema": "course-eval-recipe/1",
  "name": "course-eval-qwen3.5-9b-local",
  "results_model": "qwen3.5-9b-local",
  "requests": "evals/courses/course-eval-v1.json",
  "lessons": ["m1.l1"],
  "server": {
    "url": "http://127.0.0.1:8089",
    "model": "Qwen3.5-9B-Q4_K_M.gguf",
    "gguf": "~/.cache/lattice-evals/models/Qwen3.5-9B-Q4_K_M.gguf",
    "gguf_sha256": "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8",
    "launch": "crates/lattice-desktop/binaries/llama-server-aarch64-apple-darwin -m ~/.cache/lattice-evals/models/Qwen3.5-9B-Q4_K_M.gguf --port 8089 --host 127.0.0.1 --no-webui -ngl 99 --ctx-size 32768 -np 1 --jinja",
    "expected_ctx": 32768,
    "expected_slots": 1,
    "thinking": "off"
  },
  "settings": { "llm": { "contextWindow": 32768 } },
  "web": { "mode": "replay", "traces": "artifacts/course-eval/traces" }
}
```

`--ctx-size 32768`, not the chat recipe's 16384: the lesson writer's window
preflight needs about 20k tokens of room (the 9B at a saved 8,704 context
could not write lessons; see the lesson-window note in the testing doc once
WP-C adds it). The number is the recipe's, recorded in the manifest, not a
constant in code.

`course-eval-27b-remote.json` differs only in `server`:

```json
"server": {
  "url_env": "LATTICE_COURSE_SERVER_URL",
  "auth_env": "LATTICE_COURSE_SERVER_AUTH",
  "model": "Qwen3.8-27B",
  "model_revision": "operator-supplied: fill in the served weights' hash or tag",
  "expected_ctx": 131072,
  "expected_slots": 1,
  "thinking": "off"
}
```

Rules the harness enforces:

- `url` or `url_env` (never both); `auth_env` names a variable holding
  `Header-Name: value`, sent only to the model server and never written to a
  result, exactly as `LATTICE_JUDGE_AUTH` is handled. No URL of a remote
  server is written to a committed result; the manifest records
  `server.identity` (requested model, `/v1/models` listing, probed context and
  slots) and `server.url_sha256`.
- `gguf` present → hashed at start and must equal `gguf_sha256` (as the chat
  example does); absent → `model_revision` is required.
- The model is `LlamaCppLlm::new(&settings)` where `settings` starts from the
  shipped `LLMSettingsDto` defaults, sets `provider: Llamacpp`,
  `llama_cpp.{url, model, auth_header_name, auth_header_value}`,
  `context_window: expected_ctx`, then merges `settings.llm` with the chat
  recipe's `merge` semantics and refuses unknown keys. `server.thinking` must
  agree with the resulting thinking fields (`answerThinking`,
  `thinkingOverride`; builder confirms the serde names) or the run refuses to
  start, so the manifest never says "off" for a run that thought.
- Nothing names Josh's server, its hardware or its window. The configured
  window is the contract, as everywhere else in the app.

The judge is not in the recipe. `course_eval.py score --endpoint URL --model
NAME [--window N]` names it per scoring run; `judge_identity` from
`shipped_path_eval.py` records what answered, including whether it is the
model that wrote the course (`judges_own_answers`), the weakest setup.

## 5. Statistics

### 5.1 Comparing two runs

Rows the scorer writes (section 6.2, `scored.jsonl` and `slots.scored.jsonl`)
carry `query_id`, `judge_pass`, `mechanical_checks_passed` and `turn_scores`
with one turn, which is all `shipped_path_stats.compare` reads. So:

```sh
python3 scripts/shipped_path_stats.py compare \
  --baseline evals/courses/results/qwen3.8-27b-remote/2026-10-22-course-eval-v1-base/slots.scored.jsonl \
  --candidate evals/courses/results/qwen3.8-27b-remote/2026-10-25-course-eval-v1-cand/slots.scored.jsonl \
  --outcome both [--json report.json]
python3 scripts/shipped_path_stats.py compare --baseline …/scored.jsonl --candidate …/scored.jsonl --outcome both
```

Two levels, both reported, with a stated role each:

- **Slot level** (`slots.scored.jsonl`, up to 14 pairs for 7 requests × 2
  slots): the conservative test. Slots are independent artifacts. With one
  run per arm the exact McNemar test cannot call a difference on fewer than
  six discordant slots all one way (p = 2/64); a change that flips every
  lesson from failed to Ready is callable, most others return `can't tell from
  noise`. This is the verdict a history entry or a product claim quotes.
- **Criterion level** (`scored.jsonl`, ~84 pairs plus claims): the screening
  test. Criteria of one artifact are not independent (a lesson that fails
  flips seven items at once), so its p-value overstates the evidence; it is
  quoted as direction and diagnosis ("L1 and L7 improved on five of seven
  lessons"), never as the call.

Claims are not paired across runs: a claim exists only in the lesson that
made it. Claim rates are reported per run with their exact intervals, and the
judge's error on them is corrected by calibration (5.2). A run may be
repeated; `compare` accepts several runs per arm and uses pass shares.

`compare` already lists items missing from one run (a lesson selector a
smaller outline does not have, an optional request absent on one machine)
instead of counting them. `ALPHA` stays 0.05 and the "fewest to call" line is
printed as it is for chat.

### 5.2 Calibrating the judge (PPI) with a blind human slice

Reused unchanged:

```sh
python3 scripts/shipped_path_stats.py calibration-sample \
  RUN/items.json RUN/answers.jsonl RUN/scored.jsonl RUN/calibration-labels.json [--half-width H] [--seed N]
# Josh fills human_verdict (pass/fail on the rubric alone), then:
python3 scripts/shipped_path_stats.py calibration-score RUN/calibration-labels.json [--judge OTHER/scored.jsonl]
```

The same two commands on `RUN/claims.items.json`, `RUN/claims.jsonl`,
`RUN/claims.scored.jsonl` calibrate the claim-correctness judge. The sample is
stratified by the judge's verdict, ungraded items (failed lessons) are a known
stratum, and `calibration-score` gives the judge's false-pass and false-fail
rates and the PPI-corrected pass rate with exact hypergeometric intervals,
as the protocol describes. Labelling rows are blind (artifact, criterion,
rubric; no verdict, no stratum, random order).

Two fits to note: the labelling file's `estimated_minutes` counts the lesson
text once per item, so it overstates the time for a lesson's seven items,
which a person reads once; and `rubric_lines` in `shipped_path_eval.py`
renders chat rubric keys, so WP-B adds an early return for a `lines` key
(`{"lines": ["…"]}` → those lines) and nothing else changes.

## 6. Outputs

### 6.1 Running

```sh
# 1. snapshot the test binary (as the testing doc already says)
cd crates/lattice-desktop && CARGO_INCREMENTAL=0 cargo test --lib --no-run -j 2 2>&1 | grep 'Executable unittests'
mkdir -p ../../artifacts/course-eval/bin && cp <target>/debug/deps/lattice-<hash> ../../artifacts/course-eval/bin/base-$(git rev-parse --short HEAD)

# 2. run every request of the set, resumable, one at a time
scripts/course-eval.sh run evals/courses/recipes/course-eval-9b-local.json base \
  --binary artifacts/course-eval/bin/base-<sha> [--requests evals/courses/course-eval-v1.json] [--only postgres-performance,…]
#   writes artifacts/course-eval/<date>-base/<request-id>/…

# 3. collect, score, summarize
python3 scripts/course_eval.py collect artifacts/course-eval/<date>-base \
  --out evals/courses/results/qwen3.5-9b-local/<date>-course-eval-v1-base
python3 scripts/course_eval.py score evals/courses/results/qwen3.5-9b-local/<date>-course-eval-v1-base \
  --endpoint http://127.0.0.1:8090 --model <judge model> [--window 32768] [--claims]
python3 scripts/course_eval.py summarize evals/courses/results/qwen3.5-9b-local/<date>-course-eval-v1-base
```

Environment the driver sets for the Rust test (all read by
`course_eval::live_course_eval`):

| Variable | Meaning |
|---|---|
| `LATTICE_COURSE_RECIPE` | recipe JSON (section 4) |
| `LATTICE_COURSE_REQUEST` | one request JSON (existing `GenerateLearningProgramRequestDto`, topic-only) |
| `LATTICE_COURSE_REQUEST_ID` | the set's id for this request (directory and item names) |
| `LATTICE_COURSE_RUN` | run directory; may exist (resume) |
| `LATTICE_COURSE_LESSONS` | comma-separated selectors (`m1.l1`); empty = outline only; overrides the recipe |
| `LATTICE_COURSE_WEB_MODE`, `LATTICE_COURSE_WEB_TRACE` | section 2.2; override the recipe's `web` block |
| `LATTICE_COURSE_SERVER_URL`, `LATTICE_COURSE_SERVER_AUTH` | as named by the recipe's `url_env`/`auth_env` |

`LATTICE_LLAMACPP_SETTINGS` is not read: the recipe is the settings. The
acceptance test `live_topic_course_passes_review_and_prepares_its_first_lesson`
and `scripts/live-course-e2e.sh` are unchanged in behaviour.

### 6.2 Files

Raw run directory, gitignored, `artifacts/course-eval/<date>-<label>/<request-id>/`
(written by the Rust test; the driver adds `*.log`, `exit.json`):

| File | Written | Contents |
|---|---|---|
| `request.json`, `recipe.json` | start | copies, so the directory is self-describing |
| `course.sqlite` | start | the whole run, as today |
| `call-N.json` | each model call | as today (`input`, `answer`, `finishReason`, `inputTokens`, `outputTokens`, `firstTokenLogprobs`) |
| `outline-generated.json` | after generation | as today, plus `linkLog` and `quoteChecks` |
| `outline.json` | outline done | `{"status","issues":[…],"repair_passes","sources":[{"id","title","url"}],"modules":[…with lessons {id,title,objective,estimated_minutes}],"project":…}` |
| `phases.jsonl` | every change while a job runs | `{"at","selector","status","phase","message","calls"}` (polled every 5 s with the job status, as `run_and_confirm` polls today) |
| `jobs/<selector>/job-id.txt`, `jobs/<selector>/result.json` | job start / end | the job record |
| `lesson-<selector>.json` | lesson done | `{"selector","lesson_id","outcome","error","lesson": LearningLessonDto,"report": report_json,"sources":[{"id","title","url","content_sha256"}]}` |
| `metrics.json` | updated after each stage | the section 1.1 keys, nested `{"outline":{…},"lessons":{"m1.l1":{…}}}` |
| `web-trace-report.json` | end of each stage | `{"mode","trace_dir","entries_at_start","entries_at_end","hits","misses_filled","misses_failed","untraced_calls","by_method":{…},"missed":[{"method","summary"}]}` |
| `run.json` | start, updated at end | `{"schema":"course-eval-run/1","request_id","recipe_sha256","recipe":…,"binary_sha256","commit","dirty_files","server":{"identity","url_sha256","expected_ctx","thinking","gguf_sha256"|"model_revision"},"settings":<the merged LLM settings minus auth>,"web":{"mode","trace_index_sha256","entries_at_start"},"started_at","finished_at","resumed":bool,"stages":{…outcome per stage}}` |
| `done.json` | all selected stages finished (pass or fail) | `{"finished_at","outline":"passed|needs_repair|unchecked","lessons":{"m1.l1":"ready|failed|…"}}` |

Committed results directory,
`evals/courses/results/<results_model>/<date>-course-eval-v1-<label>/`
(written by `course_eval.py`):

| File | By | Contents |
|---|---|---|
| `run.json` | collect | run-level manifest: the per-request `run.json`s, the set's sha256, `scripts/course_eval.py` sha256, harness file hashes (the three Rust files), `artifacts_dir`, `calls_index_sha256` (sha256 of sorted `call-N.json` hashes per request) |
| `requests/<id>/outline.json`, `lesson-<sel>.json`, `metrics.json`, `web-trace-report.json`, `phases.jsonl` | collect | copied from the run dir (lesson JSON keeps the lesson and report; `call-N.json` and `course.sqlite` are not copied) |
| `items.json`, `answers.jsonl` | collect | the materialized question set and "answers" (rendered artifacts), chat shapes (section 5.2) |
| `claims.items.json`, `claims.jsonl` | collect | the same for claims |
| `scored.jsonl`, `scored.jsonl.manifest.json` | score | per-item rows: `{"query_id","request","stage","lesson","criterion","judge_pass","mechanical_checks_passed","turn_scores":[{"judge":{"pass","reason","graded"}}],"answer_sha256"}`; manifest in the chat scorer's shape with `settings.judge.identity` and the rubric id |
| `slots.scored.jsonl` | score | per-slot rows, same fields, `query_id` = `<id>/outline` or `<id>/<sel>` |
| `claims.scored.jsonl` | score `--claims` | per-claim rows; `reason` begins `FALSE:` or `UNSURE:` on a fail |
| `summary.json`, `summary.md` | summarize | section 1.3 headline, per-criterion table, per-request table, cost, web hits/misses, the `compare` command to run against the previous run in the same model folder |
| `calibration-labels.json`, `claims.calibration-labels.json` | `calibration-sample` | when drawn |

Rough size per run: outline and lesson JSON a few hundred KB per lesson
(the report carries evidence passages); the call files (4M tokens per lesson)
are the reason raw run dirs stay out of the repo. Compare with the chat eval,
which commits `.calls.jsonl.gz` for 34 short turns.

### 6.3 Rendering an artifact for the judge and the labeller

One deterministic markdown rendering per artifact, used as the item's
`answer` for all of its criteria (so a person reads a lesson once) and hashed
into `answer_sha256`:

- Outline: `# <title>`, the request (goal, prior knowledge, minutes per
  session, depth), then per module `## Module n: <title>`, summary, outcomes
  as a list, lessons as `- Lesson n.m: <title> (<estimated_minutes> min) —
  <objective>`, and the project milestone if any.
- Lesson: `# <title>`, `Objective: …`, the request's prior knowledge and
  minutes, then per block `## [<kind>] <title>` and the body verbatim (rubric
  lines for practice blocks), then `## Questions` with each prompt, its
  options lettered, and `Correct: <letter>` plus the explanation (L5 needs
  the key; the others ignore it).

The score step refuses, rather than truncates, an item whose prompt exceeds
the judge's window: `--window` (or `n_ctx` from the server when it lists it)
against a character-based token estimate; refused items are `graded: false`
with reason `too long for the judge window` and counted in the summary.

### 6.4 Documentation updates

- `evals/courses/README.md` (new, WP-C): the protocol for this eval: the
  request set and its fields, the recipe, commands, file layout, the two
  comparison levels and what each may claim, calibration, and the rule that
  every run is committed here and listed in the history.
- `docs/development/course-generation-testing.md` §7 "Scored course eval"
  (WP-C): the run procedure, pointing at the README; §2's stale "the live test
  does not wait out outages" line corrected; the 9B lesson-window note.
- `docs/development/course-generation-history.md` (WP-C adds the rule; the
  coordinator adds entries): each eval run gets a row with label, commit,
  binary, model, requests, slots passed, lessons Ready, claim rates, minutes,
  web hits/misses, and the `compare` verdict against the previous run.

## 7. Work packages

Three packages, disjoint files, buildable in parallel against the shapes in
sections 2.2 and 6.2. Builders run only `cargo check --lib --tests` through
`~/.local/bin/cargo-slot` (the harness is `#[cfg(test)]` code, which a plain
`cargo check` does not compile), write tests and never run them; the
coordinator runs the gates once at the end. Files stay under 800 lines
(the ratchet exempts `*_tests.rs` by name, but keep every new file under the
limit anyway). No new crates, no new Python packages (stdlib `urllib`, `json`,
`hashlib`, as the existing scripts).

### WP-A: harness (Rust + driver script)

Owns:

- `crates/lattice-desktop/src/features/learning/lessons/live_course_eval.rs` (new)
- `crates/lattice-desktop/src/features/learning/lessons/live_course_eval_tests.rs` (new; unit tests)
- `crates/lattice-desktop/src/features/learning/lessons/live_web_trace.rs` (new)
- `crates/lattice-desktop/src/features/learning/lessons/live_web_trace_tests.rs` (new)
- `crates/lattice-desktop/src/features/learning/lessons/live_lesson_tests.rs` (edit: add `#[path = "live_course_eval.rs"] mod course_eval;` and `#[path = "live_web_trace.rs"] mod web_trace;`)
- `crates/lattice-desktop/src/features/learning/lessons/live_course_e2e.rs` (edit: make `record` and `review_status` `pub(super)` so the eval reuses them; no behaviour change)
- `crates/lattice-desktop/src/features/learning/lessons/live_small_lesson.rs` (edit: split `run_and_confirm` into `run_job(pool, options, model, job_id, directory, observe: &dyn Fn(&LearningGenerationJob))` that polls, records and returns the final job, and the existing `run_and_confirm` that calls it and keeps its asserts and files; same signature as today)
- `scripts/course-eval.sh` (new driver, section 3.4)

Builds:

1. `TracedWeb` (section 2.2): `new(inner: Arc<dyn WebServiceTrait>, mode, trace_dir) -> Result<Self>`, the `WebServiceTrait` impl, `report() -> TraceReport` (serializes to `web-trace-report.json`), `index_sha256()`. Entry files are written atomically (write to `.tmp`, rename). Keys as specified; canonical JSON = `serde_json::Value` with null fields removed, keys sorted (a `BTreeMap` round-trip).
2. `CourseRecipe` (section 4): deserialize, resolve `~` and repo-relative paths like the chat example's `resolve`, build `LLMSettingsDto`, merge the patch with unknown-key refusal, hash the GGUF when named, probe `/v1/models`, produce `run.json`'s `server` block without the URL or auth.
3. `live_course_eval` test: resume logic of section 3.4; `ObservedModel` from the parent module as the recorder; `TracedWeb` around `WebService::new(<run>/web)` for the outline and as `research_web` for the worker; `run_job` with an observer that appends to `phases.jsonl`; `metrics.json` after each stage; `lesson-<sel>.json` from a reopened connection (lesson DTO via `LearningRepository::get`, report from `learning_lesson_verifications`, sources via `verification_sources`); `done.json`. Never asserts; records outcomes. The quote checks call `planning/outline_evidence::quote_checks` and the supporting-quote check uses `outline_citations::exact_source_quote` or a plain substring search over the finding's `evidence[]` text (builder picks the one that matches how `supporting_quote` is validated in `content_verification`).
4. `scripts/course-eval.sh`: section 3.4 and 6.1; zsh like `live-course-e2e.sh`; `run` subcommand only; prints the collect command at the end.

Tests it writes (not runs):

- `live_web_trace_tests.rs`: record through `MockWebService` (`features/web/mocks/web.rs`) then replay with the inner set to a mock that errors on every call, asserting identical outputs and zero misses; a miss in `replay` fills and counts; a miss in `replay-strict` returns `InvalidInput` and counts; an error entry replays as `InvalidInput` with the recorded text; the key is stable across field order and ignores nulls; `fetch_reference_content` and `fetch_reference_page` share one entry; `index_sha256` changes when an entry is added.
- `live_course_eval_tests.rs`: recipe parsing (url vs url_env exclusivity, missing `model_revision` without `gguf`, unknown settings key refused, thinking disagreement refused); lesson selector parsing (`m1.l1`, bad forms); `metrics.json` extraction from a fixture `report_json` and lesson DTO (verdict counts, general-knowledge count, unmatched supporting quote detected, unresolved source id detected); resume state decision table (section 3.4) from a fixture of program status and job status.

Done when: `cargo-slot cargo check --lib --tests` in `crates/lattice-desktop`
is clean; the files above exist with the names and signatures here; the
driver's `--help` matches section 6.1; the coordinator can point the test at
a recipe and a request and see the section 6.2 files appear.

### WP-B: scorer (Python)

Owns:

- `scripts/course_eval.py` (new)
- `scripts/test_course_eval_rag_eval.py` (new; the name must match CI's `python -m unittest discover -s scripts -p 'test_*rag_eval.py'`)
- `scripts/shipped_path_eval.py` (edit: `rubric_lines` returns `list(rubric["lines"])` when that key is present; nothing else)

Builds `course_eval.py` with subcommands:

- `collect <run-dir> --out <results-dir>`: reads every `<request-id>/` with a
  `run.json`; refuses an existing `--out` (never overwrites, as the chat
  harness); copies the section 6.2 files; sums tokens from `call-N.json` by
  stage using the call indexes in `metrics.json`; renders artifacts (6.3);
  writes `items.json` (ids and rubric lines from Appendix A plus the set's
  `outline_must_cover`/`prior_knowledge_excludes` for O1/O2), `answers.jsonl`,
  `claims.items.json`, `claims.jsonl`, and the run-level `run.json`.
- `score <results-dir> --endpoint URL --model NAME [--window N] [--claims]`:
  imports `judge_request`, `judge_headers`, `http_judge`, `judge_identity`,
  `parse_verdict`, `JUDGE_SCHEMA` from `shipped_path_eval`; system prompt
  `COURSE_JUDGE_SYSTEM` (Appendix A); writes `scored.jsonl`,
  `slots.scored.jsonl`, `claims.scored.jsonl`, and the manifest via
  `shipped_path_eval.manifest`-compatible fields with `rubric:
  lattice-course-judge/1`. Resumes by skipping ids already scored (append,
  as `live_claim_pairs_eval` does). No time limit on a grade.
- `summarize <results-dir>`: `summary.json` and `summary.md` (section 1.3),
  the per-criterion table, per-request table, cost, web hits/misses, and the
  `compare` line against the most recent earlier results dir in the same
  model folder when one exists.
- `render-labels <calibration-labels.json> --out page.html` (optional, if
  time allows): a blind page like `lesson_claim_pairs.py spot-check`.

Tests it writes (`unittest`, fixtures built in code, no network): rendering
is deterministic and includes every block and question; items carry the
Appendix A lines verbatim and the per-request `must_cover` lines; an
unprepared lesson yields `graded: false` items; token sums by call index
range; slot rows are the conjunction of their items; `scored.jsonl` rows
satisfy `shipped_path_stats.compare` and `calibration_sample` (call them on
fixture rows, as `test_shipped_path_stats_rag_eval.py` does); `rubric_lines`
with `lines` returns them and without it is unchanged; a prompt over
`--window` is refused, not truncated; `collect` refuses an existing output.

Done when: `python3 -m unittest scripts/test_course_eval_rag_eval.py` (run
by the coordinator) passes; `collect` on a fixture run dir the test builds
produces every file in section 6.2; `shipped_path_stats.py compare` on two
fixture `scored.jsonl` files prints a report.

### WP-C: dataset, recipes, docs

Owns:

- `evals/courses/course-eval-v1.json` (new; section 3.1 shape, Appendix B)
- `evals/courses/requests/2t-postgres-performance-terse.json`, `3t-music-theory-production-terse.json`, `4t-system-design-fundamentals-terse.json` (new)
- `evals/courses/recipes/course-eval-9b-local.json`, `course-eval-27b-remote.json` (new)
- `evals/courses/README.md` (new)
- `evals/courses/results/.gitkeep`
- `docs/development/course-generation-testing.md` (edit: §7, the stale outage line, the 9B window note; the requests table gains the terse rows)
- `docs/development/course-generation-history.md` (edit: the rule for eval-run rows and the column set)

Builds: the files above. `outline_must_cover` and `prior_knowledge_excludes`
for the three committed requests and their terse twins are written from the
request texts (3–5 topics each, concrete enough to judge); the
`dotnet-interview` item is `optional: true` with empty lists for Josh to fill,
since its request is not in the repository. A terse request keeps
`minutesPerSession`, `courseDepth`, empty `documentIds`/`sourceUrls`.

Tests: none in code; the README's commands must match sections 6.1 and 5
exactly, and the set file must validate against Appendix B (WP-B's `collect`
reads it; a mismatch surfaces at integration).

Done when: the files exist; `jq . evals/courses/course-eval-v1.json` parses;
the README reads as the protocol a stranger could follow; the testing doc §7
links it.

### Interfaces and order

- WP-A → WP-B: the raw run directory (section 6.2 first table) and
  `metrics.json` keys (section 1.1). WP-B builds its fixtures from this
  design, not from WP-A's code.
- WP-C → WP-A, WP-B: the set file (Appendix B) and recipe (section 4).
- All three start at once. Integration: the coordinator runs WP-B's tests,
  `cargo test --lib` for the new unit tests, the ratchet and layer checks,
  then one outline-only run on the 9B as the smoke, then the first full
  baseline.

## 8. Rules checked against this design

- **No arbitrary caps or timeouts**: the harness, driver and judge wait as
  long as the servers take; 5-second polling is pacing; the judge's window
  check refuses rather than cuts; the only stop is Josh's.
- **Server-agnostic**: the recipe and environment name the servers; results
  carry identities and hashes, never addresses; nothing is sized to a machine.
- **No shims, no migrations**: the eval opens a fresh database per run and
  reads tables as they are; no schema change.
- **No paid services, anonymity**: searches and fetches go through the app's
  own `WebService`; the trace is a local file.
- **cargo check only** for builders: all Rust is `#[cfg(test)]`, checked with
  `--lib --tests`; Python has no build step.
- **800 lines, layers**: new Rust files are test modules under
  `features/learning/lessons`, depend on `lattice_core` ports and on the
  `WebService` construction the sibling live tests already do; the ratchet
  skips `#[cfg(test)]` items and the layer check sees no new production edge.
- **Extends existing harnesses**: `live_course_e2e` helpers and
  `run_and_confirm` are reused; statistics are `shipped_path_stats.py` and
  `exact_stats.py` unchanged; the judge code is `shipped_path_eval.py`'s.

## 9. Decisions left to Josh

1. **Commit traces?** This design keeps page text out of the repository and
   commits only the trace index hash. Committing traces would let a fresh
   clone replay a baseline exactly, at tens of MB per request of third-party
   text in a public repo.
2. **Sampling arm.** Shipped sampling is the default arm. A greedy arm
   (`temperature 0, topK 1`) would turn the eval into a near-pure code-change
   detector at the cost of measuring a pipeline users do not run. Both are
   one recipe field away; the question is which one the history quotes.
3. **Claim-correctness judge.** It costs ~70 short calls per lesson and is
   only trustworthy after a human slice; keep it (`--claims`), or drop it and
   rely on the pipeline's own grounding verdicts plus the existing
   `lesson-claims-w1` checker measurement.
4. **Commit call files?** The chat eval commits gzipped calls; a course run's
   are ~4M tokens per lesson. This design leaves them in `artifacts/` with a
   hash in the manifest.

## Appendix A: rubric texts (`lattice-course-judge/1`)

System prompt for every course item (`COURSE_JUDGE_SYSTEM`):

> You grade one artifact of a learning app, an outline or a lesson, against
> one criterion written by the person who built the test. The learner's
> request is given: their goal, what they already know, and the minutes they
> have per session. Judge only the criterion you are given, from the artifact
> and the request. Do not reward length. Do not penalise a lesson for what a
> later lesson in the outline will cover. Return JSON with verdict "pass" or
> "fail" and a one-sentence reason that names the part of the artifact that
> decided it.

Each item's user message: `Request:` (goal, prior knowledge, minutes per
session, depth), `Criterion:` (the lines below), `Artifact:` between `<<<`
and `>>>` (section 6.3 rendering).

Outline criteria:

- **O1 Goal coverage.** Lines: "Each of the following topics must be the
  stated objective or outcome of at least one module or lesson, not merely
  mentioned in passing:" followed by one line per `outline_must_cover` entry.
  "Pass only if every topic is covered. If the list is empty, pass only if
  the modules and lessons together plainly achieve the learner's goal as
  written."
- **O2 Fits prior knowledge.** "The learner already knows the following; no
  lesson may teach any of it from scratch (a short recap inside a lesson is
  fine, a lesson devoted to it is not):" one line per
  `prior_knowledge_excludes` entry. "No lesson may assume knowledge the
  learner says they lack. Pass only if both hold."
- **O3 Progression.** "Lessons are ordered so that each one uses only what
  earlier lessons or the learner's prior knowledge provide. Fail if any
  lesson relies on a concept first taught in a later lesson, or if modules
  are an unordered list of topics with no build-up."
- **O4 Session fit.** "Each lesson's objective is achievable, by this
  learner, in one session of the stated minutes, given its estimated minutes.
  Fail if any lesson's objective bundles several outcomes that could not fit,
  or if a lesson's estimate exceeds the session length."
- **O5 Specificity.** "Every lesson objective names a concrete thing the
  learner will be able to do or explain, stated so that a quiz could test it.
  Fail if any objective is a topic label ("Indexes") or a vague aim
  ("understand performance")."

Lesson criteria:

- **L1 Teaches rather than lists.** "The explanation blocks explain how and
  why: mechanisms, causes, consequences, with at least one concrete example
  per main idea. Fail if the lesson is mostly bullet lists of facts,
  definitions without reasons, or restated headings."
- **L2 Objective met.** "After reading the lesson, the learner could do what
  the objective states. Fail if a part of the objective is never taught or
  only named."
- **L3 Worked example.** "The worked-example block walks through a complete,
  specific case step by step, and each step is correct for the lesson's
  subject as far as you can tell. Fail if the example is missing, generic,
  skips the step the objective is about, or contains an error you can
  identify."
- **L4 Practice.** "The practice blocks can be completed using only what this
  lesson and the learner's prior knowledge provide, and completing them
  exercises the objective. Fail if practice needs tools or facts the lesson
  never gave, or practises something else."
- **L5 Questions.** "Each question tests the objective; exactly one option is
  defensibly correct given the lesson; the other options are plausible to a
  learner who missed the point, not obviously wrong or jokes; the marked
  correct answer is the correct one. Fail if any question breaks any of
  these."
- **L6 Level.** "The lesson is pitched to the learner's stated prior
  knowledge: it does not re-teach what they know, and it does not assume
  vocabulary or skills they said they lack without introducing them. Fail if
  either happens in more than a passing sentence."
- **L7 Coherence.** "Sections follow from one another, the recap matches what
  was taught, and no section contradicts another. Fail on a contradiction, a
  recap that lists things not taught, or a section that belongs to a different
  lesson."

Claim criterion (`--claims`):

- **C1 True statement.** System prompt variant: "You judge one factual
  statement taken from a lesson. Use your own knowledge of the subject. Return
  pass when the statement is true as written, allowing ordinary simplification
  for a learner. Return fail when it is false, misleadingly overstated, or
  when you do not know; begin the reason with FALSE: or UNSURE: accordingly."
  User message: `Subject: <course goal>`, `Statement: <<< … >>>`.

## Appendix B: `course-eval-v1.json`

```json
{
  "schema": "course-eval-requests/1",
  "name": "course-eval-v1",
  "items": [
    {
      "id": "postgres-performance",
      "request": "requests/2-postgres-performance.json",
      "shape": "full",
      "outline_must_cover": [
        "reading EXPLAIN and EXPLAIN ANALYZE output",
        "choosing indexes (B-tree, composite, partial) for a query",
        "diagnosing and fixing a slow query in a production database"
      ],
      "prior_knowledge_excludes": ["everyday SQL: joins, GROUP BY, subqueries", "using an ORM"]
    },
    {
      "id": "postgres-performance-terse",
      "request": "requests/2t-postgres-performance-terse.json",
      "shape": "terse",
      "outline_must_cover": ["…same as the full twin…"],
      "prior_knowledge_excludes": ["…same…"]
    },
    { "id": "dotnet-interview", "request": "requests/1-dotnet-interview.json", "shape": "full",
      "optional": true, "outline_must_cover": [], "prior_knowledge_excludes": [] }
  ]
}
```

`request` paths are relative to the set file. `shape` is recorded in every
item row so the summary can split full from terse. Item ids are the first
path element of every `query_id`, so they must not contain `/`.

Terse twin example, `2t-postgres-performance-terse.json`:

```json
{
  "goal": "postgres query tuning: explain plans, indexes, slow queries",
  "priorKnowledge": "know SQL, never tuned a database",
  "minutesPerSession": 45,
  "documentIds": [],
  "sourceUrls": [],
  "courseDepth": "course"
}
```
