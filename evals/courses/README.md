# Course eval

A scored, repeatable eval of course and lesson generation. It runs a fixed set of topic-only course requests through the real pipeline (outline, then the first lesson of module 1), records everything, scores each result with mechanical checks and a judge, and compares a candidate run against a baseline with the same statistics as the chat eval. The design, with the reasons behind each choice, is [docs/design/2026-10-20-course-eval.md](../../docs/design/2026-10-20-course-eval.md). How the course pipeline is tested in general is [docs/development/course-generation-testing.md](../../docs/development/course-generation-testing.md).

## What is scored

Each request gives one **outline slot**, each selected lesson one **lesson slot**, and each selected lesson one **quiz slot** (`<request>/<lesson>/quiz`). An outline or lesson slot passes when its mechanical checks pass and every judged criterion passes; a quiz slot passes when its mechanical checks pass and at least 90 % of its items pass every judged criterion, since a bank, unlike a lesson, tolerates dropping a few items.

- **Mechanical checks** are decided by code and written by the harness into `metrics.json`: the outline's review status and open findings, references and their domains, linked outline items, verified and removed citations, unmatched quotes, shape and session fit, cost; for a lesson, its outcome, grounding verdicts, quotes and sources. The full key list is design §1.1.
- **Judged criteria** are one judge call each, with the rubric texts of design Appendices A and C (rubric id `lattice-course-judge/2`). Outline: O1 goal coverage, O2 fits prior knowledge, O3 progression, O4 session fit, O5 specificity. Lesson: L1 teaches rather than lists, L2 objective met, L3 worked example, L4 practice, L6 level, L7 coherence. L5 (questions) was retired in `/2`: the lesson's questions are quiz items now, judged by Q2 to Q4. A lesson that did not reach Ready has no artifact, so its items are ungraded and fail, and its quiz slot has no items and fails.
- **The quiz stage** ([quizzes design](../../docs/design/2026-10-23-quizzes-and-tests.md) §7.1) reads each Ready lesson's bank: the harness's `quiz-<sel>.json` (`course-eval-quiz-bank/1`, keys included, eval only), or, when none was recorded, the lesson's own reviewed questions as single-choice items. Mechanical item checks go into `metrics.json` under `lessons.<sel>.quiz`: format validity for each of the eight formats, leaks (the key in the stem, a shared 4-word run, a distractor quoted in the stem, a matched pair naming itself), length bias (the key is the unique longest option by more than 60 % of the median length), duplicate options, duplicate and near-duplicate items (stem Jaccard 0.8), explicit key/explanation conflicts ("the correct answer is C" when the key is B), true/false absolutes the lesson does not state, option letters in explanations (reshuffling breaks them), anchors and verbatim quotes, the format mix, and how often the key is the longest option against chance. The harness's own numbers (target and bank size, validation pass rate, key review first pass / after repair / dropped, calls, seconds) pass through. Then one judge call per item and criterion that applies: Q1 answerable from the lesson, Q2 exactly one correct key (a blind solve; code compares the answer with the key), Q3 plausible distractors (choice items, and matching with a distractor), Q4 no leak, Q5 keyed fairly (short answer, cloze, explanation). Q1 and Q2 see the item as a learner does; Q3 to Q5 see the key. The quiz judge must not be the model that wrote the bank: `score` refuses the stage unless the judge is another model, or `--allow-self-judge` records the exception in the manifest.
- **Claim correctness** (optional, `--claims`) asks the judge whether each lesson claim is true. It is secondary and only quoted after a calibration slice (below).

The headline in `summary.md`, in order: slots passed (outlines, lessons, quizzes), lessons Ready, grounding (supported claims, general-knowledge share, contradicted count), cost (minutes per outline and per lesson, calls and input tokens per lesson), quizzes (items passing every criterion, mechanical hits, the key-longest share against chance, anchored items). Tables follow for the quiz criteria and the banks. When the previous run was judged under another rubric version, the compare section says so: its lesson slots still held L5 and it had no quiz slots.

## The request set

`course-eval-v1.json` lists the requests, in the order they run.

| Field | Meaning |
|---|---|
| `schema` | `course-eval-requests/1` |
| `name` | The set name. It appears in every results directory name |
| `items[].id` | The request's id. It is the first part of every item id, so it must not contain `/` |
| `items[].request` | A `GenerateLearningProgramRequestDto` JSON, relative to the set file. Topic-only: `documentIds` and `sourceUrls` empty |
| `items[].shape` | `full` or `terse`. The summary splits results by it |
| `items[].outline_must_cover` | Topics O1 checks the outline covers, as a module or lesson objective |
| `items[].prior_knowledge_excludes` | Things the learner already knows, which O2 checks the outline does not teach from scratch |
| `items[].optional` | `true` when the request file may be absent. The run skips a missing optional request and says so in `run.json` |

| id | Request | Shape | Depth | Minutes |
|---|---|---|---|---|
| `postgres-performance` | `requests/2-postgres-performance.json` | full | course | 45 |
| `music-theory-production` | `requests/3-music-theory-production.json` | full | focused | 30 |
| `system-design-fundamentals` | `requests/4-system-design-fundamentals.json` | full | deep dive | 60 |
| `postgres-performance-terse` | `requests-terse/2t-postgres-performance-terse.json` | terse | course | 45 |
| `music-theory-production-terse` | `requests-terse/3t-music-theory-production-terse.json` | terse | focused | 30 |
| `system-design-fundamentals-terse` | `requests-terse/4t-system-design-fundamentals-terse.json` | terse | deep dive | 60 |
| `dotnet-interview` | `requests/1-dotnet-interview.json` | full | deep dive | 60 |

A terse request is its full twin typed the way people type: a goal of 12 words or fewer, prior knowledge in one clause, the same minutes and depth. Each terse item carries the same `outline_must_cover` and `prior_knowledge_excludes` as its twin, so the outline judge asks both the same questions.

`dotnet-interview` describes Josh, so its request file is gitignored and exists only in local checkouts. Its item is `optional` with empty lists; fill them locally before scoring its outline, or its O1 and O2 have nothing to check.

Rules for the lists: three to five topics each, taken from the request's own text, each concrete enough that a judge can say yes or no from the outline. Once a baseline has been committed against a set, do not edit its lists or requests: the questions would change under the old results. Make a new set file (`course-eval-v2.json`) instead.

## The recipe

A recipe names the model server, the settings and the web mode. Two are committed in `recipes/`:

- `course-eval-9b-local.json`: the bundled `llama-server` with Qwen3.5-9B on `127.0.0.1:8089`. `server.launch` is the command that starts it.
- `course-eval-27b-remote.json`: a remote server named only through environment variables.

| Field | Meaning |
|---|---|
| `schema` | `course-eval-recipe/1` |
| `name` | The recipe's name |
| `results_model` | The folder under `results/` this model's runs go in |
| `requests` | The set file, relative to the repository root |
| `lessons` | Lesson selectors to prepare after the outline, in order. `m1.l1` is module 1, lesson 1. `[]` runs the outline only |
| `embedding_gguf`, `embedding_model_id` | Required. The embedding model's GGUF file, with its `tokenizer.json` beside it (`~/` is the home directory, anything else is relative to the repository root), and its catalog id. A llama-server is started for it the way the app does (`LATTICE_LLAMA_SERVER`, else the bundled build), the same fields as the chat recipe. Lessons search each course's evidence index with it, as the app does; a recipe without one is refused, since a run without it would not measure the shipped path. `run.json` records the model's id, its artifact identity and its dimension |
| `server.url` or `server.url_env` | The server's address, or the name of a variable that holds it. Exactly one of the two |
| `server.auth_env` | Optional. The name of a variable holding one `Header-Name: value` line. It is sent only to the model server and never written to a result |
| `server.model` | The model name sent in requests |
| `server.gguf`, `server.gguf_sha256` | The weights file and its hash. The run hashes the file at start and refuses a mismatch |
| `server.model_revision` | Required when there is no `gguf`: the served weights' hash or tag, filled in by whoever runs the server |
| `server.expected_ctx`, `server.expected_slots` | The window and slot count the server must report |
| `server.thinking` | Whether the model thinks (`off` in both recipes). It must agree with the merged settings or the run refuses to start |
| `settings.llm` | Changes to the app's default LLM settings, in the settings file's names. An unknown key is refused |
| `web.mode`, `web.traces` | Web replay mode and the trace directory (below) |

The configured window is the contract. A recipe for another server changes `expected_ctx` and `settings.llm.contextWindow` to what that server serves. A lesson needs about 20k tokens of window to be written, so a recipe below that measures a failure the window causes, not the pipeline.

No committed file names a remote server's address or key. Results record the server's identity (requested model, its `/v1/models` listing, probed window and slots) and a hash of the URL.

## Before a run

- **One run at a time.** The driver takes `artifacts/course-eval/.lock` and refuses to start while another run holds it. The model server is single-slot: do not run the chat eval, the course e2e test or the app's chat against it at the same time.
- **Start the server.** For the 9B recipe, run its `server.launch` line from the repository root. For the remote recipe, set `LATTICE_COURSE_SERVER_URL` and, if the server needs it, `LATTICE_COURSE_SERVER_AUTH`, and fill in `model_revision`.
- **Plan the time.** On the remote 27B an outline took 10 to 32 minutes and a lesson about 2 to 3.5 hours, so the full set with one lesson each is 16 to 28 hours, unattended. The outline-only tier is 1.5 to 3.5 hours. No figure exists yet for the 9B as the course model; the first run sets it.
- **Disk.** Every model call is kept in the raw run directory, about 4M input tokens per lesson.

## Running

Step 1 runs in `crates/lattice-desktop`; steps 2 and 3 run from the repository root. `<target>` is the target directory the first command prints.

```sh
# 1. snapshot the test binary (as the testing doc already says)
cd crates/lattice-desktop && CARGO_INCREMENTAL=0 ~/.local/bin/cargo-slot cargo test --lib --no-run -j 6 2>&1 | grep 'Executable unittests'
mkdir -p ../../artifacts/course-eval/bin && cp <target>/debug/deps/lattice-<hash> ../../artifacts/course-eval/bin/base-$(git rev-parse --short HEAD)

# 2. run every request of the set, resumable, one at a time
scripts/course-eval.sh run evals/courses/recipes/course-eval-9b-local.json base \
  --binary artifacts/course-eval/bin/base-<sha> [--requests evals/courses/course-eval-v1.json] [--only postgres-performance,…]
#   writes artifacts/course-eval/<date>-base/<request-id>/…

# 3. collect, score, summarize
python3 scripts/course_eval.py collect artifacts/course-eval/<date>-base \
  --out evals/courses/results/qwen3.5-9b-local/<date>-course-eval-v1-base
python3 scripts/course_eval.py score evals/courses/results/qwen3.5-9b-local/<date>-course-eval-v1-base \
  --endpoint http://127.0.0.1:8090 --model <judge model> [--window 32768] [--claims] [--allow-self-judge]
python3 scripts/course_eval.py summarize evals/courses/results/qwen3.5-9b-local/<date>-course-eval-v1-base
```

The label (`base` above) names the run: use `base` for a baseline and `cand` for a candidate, or any short word. The judge is named per scoring run, not in the recipe. It can be any OpenAI-compatible server; the manifest records what answered and whether it is the model that wrote the course, which is the weakest setup. `score` refuses an item too long for the judge's window rather than cutting it, and the summary counts refused items. `score` resumes: run it again and it skips items already scored.

The driver runs the snapshotted binary's ignored test `course_eval::live_course_eval` once per request, with these variables:

| Variable | Meaning |
|---|---|
| `LATTICE_COURSE_RECIPE` | The recipe JSON |
| `LATTICE_COURSE_REQUEST` | One request JSON |
| `LATTICE_COURSE_REQUEST_ID` | The set's id for this request |
| `LATTICE_COURSE_RUN` | The request's run directory; may exist (resume) |
| `LATTICE_COURSE_LESSONS` | Comma-separated selectors; empty is outline only; overrides the recipe |
| `LATTICE_COURSE_WEB_MODE`, `LATTICE_COURSE_WEB_TRACE` | Override the recipe's `web` block |
| `LATTICE_COURSE_SERVER_URL`, `LATTICE_COURSE_SERVER_AUTH` | As named by the recipe's `url_env` and `auth_env` |

`LATTICE_LLAMACPP_SETTINGS` is not read: the recipe is the settings.

### Stopping and resuming

Nothing in the run has a time limit. To stop a stuck run, press Ctrl-C or `kill` it. To resume, run the same `scripts/course-eval.sh run` command again: requests with a `done.json` are skipped, an outline left unchecked continues through the app's own Repair continuation (and is marked `resumed` in the summary), a lesson job continues from its checkpoints, and a lesson that failed on a transient error is retried. A request that exits non-zero is recorded in `<request-id>/exit.json` and the run moves on to the next one. Each request's output goes to `<request-id>.log`.

### Web replay

Two runs only measure a code change if they read the same pages. The web is traced per request id, under `web.traces`, and shared by every run of that request:

| `web.mode` | Stored entry | No stored entry |
|---|---|---|
| `live` | not used | goes live, nothing is stored |
| `replay` (default) | served | goes live, is stored, counted as a filled miss |
| `replay-strict` | served | fails the call, counted as a failed miss |

A baseline is recorded by running `replay` against an empty trace. A candidate replays the same trace; its new searches are filled live and counted. `summary.md` prints the hits and misses per request. A run with many misses read different pages from its baseline, so its comparison says less. Traces stay in `artifacts/` and are not committed; the manifest records the trace index hash.

## Files

The raw run directory, `artifacts/course-eval/<date>-<label>/<request-id>/`, is gitignored. It holds `request.json` and `recipe.json` copies, `course.sqlite` (the whole run), `call-N.json` per model call, `outline-generated.json` and `outline.json`, `phases.jsonl` (lesson job progress), `jobs/<selector>/`, `lesson-<selector>.json`, `metrics.json`, `web-trace-report.json`, `run.json` and, when every selected stage has finished, `done.json`.

The results directory, `results/<results_model>/<date>-course-eval-v1-<label>/`, is committed:

| File | Written by | Contents |
|---|---|---|
| `run.json` | collect | Run manifest: each request's `run.json`, the set's hash, the scorer and harness file hashes, and a hash over the call files |
| `requests/<id>/` | collect | `outline.json`, `lesson-<sel>.json`, `lesson-<sel>.md` (the lesson as the judges read it), `quiz-<sel>.json` (when recorded), `metrics.json`, `web-trace-report.json`, `phases.jsonl` |
| `items.json`, `answers.jsonl` | collect | The questions and the rendered outlines and lessons the judge reads |
| `claims.items.json`, `claims.jsonl` | collect | The same for claims |
| `quiz.items.json`, `quiz.jsonl` | collect | One item per bank item with one turn per criterion, the lesson texts once each, and the items with their keys as a labeller reads them |
| `scored.jsonl`, `scored.jsonl.manifest.json` | score | One row per criterion, and the judge's identity |
| `quiz.scored.jsonl`, `quiz.scored.jsonl.manifest.json` | score | One row per quiz item, one turn score per criterion; the manifest records the judge's independence from the course model |
| `slots.scored.jsonl` | score | One row per slot (`<id>/outline`, `<id>/<sel>`, `<id>/<sel>/quiz`) |
| `claims.scored.jsonl` | score `--claims` | One row per claim |
| `summary.json`, `summary.md` | summarize | The headline, per-criterion and per-request tables, cost, web hits and misses, and the `compare` command against the previous run in the same model folder |
| `calibration-labels.json`, `claims.calibration-labels.json` | calibration-sample | When a human slice is drawn |

`call-N.json` files and `course.sqlite` are not copied; they stay in the raw run directory.

## Comparing two runs

```sh
python3 scripts/shipped_path_stats.py compare \
  --baseline evals/courses/results/qwen3.8-27b-remote/2026-10-22-course-eval-v1-base/slots.scored.jsonl \
  --candidate evals/courses/results/qwen3.8-27b-remote/2026-10-25-course-eval-v1-cand/slots.scored.jsonl \
  --outcome both [--json report.json]
python3 scripts/shipped_path_stats.py compare --baseline …/scored.jsonl --candidate …/scored.jsonl --outcome both
```

Both levels are reported, and they may claim different things:

- **Slot level** (`slots.scored.jsonl`, up to 14 pairs) is the verdict. Slots are independent, so its p-value holds. With one run per arm it cannot call a difference on fewer than six discordant slots all one way; most changes will read `can't tell from noise`. This is the line a history entry or a product claim quotes.
- **Criterion level** (`scored.jsonl`, about 84 pairs plus claims) is for direction and diagnosis only, such as "L1 and L7 improved on five of seven lessons". Criteria of one lesson are not independent (a lesson that fails flips seven items at once), so its p-value overstates the evidence and is never quoted as the call.

Claims are not paired across runs, since a claim exists only in the lesson that made it; claim rates are reported per run with their intervals. Items missing from one run (a lesson a smaller outline does not have, an optional request absent on one machine) are listed, not counted. `compare` accepts several runs per arm.

## Calibrating the judge

A judge's pass rate means little until a person has checked a sample of its verdicts. Draw a blind slice, label it, and score the judge against it:

```sh
python3 scripts/shipped_path_stats.py calibration-sample \
  RUN/items.json RUN/answers.jsonl RUN/scored.jsonl RUN/calibration-labels.json [--half-width H] [--seed N]
# Josh fills human_verdict (pass/fail on the rubric alone), then:
python3 scripts/shipped_path_stats.py calibration-score RUN/calibration-labels.json [--judge OTHER/scored.jsonl]
```

`RUN` is a results directory. The labelling file shows the artifact, the criterion and the rubric, in random order, without the judge's verdict; fill each `human_verdict` from the rubric alone. `calibration-score` gives the judge's false-pass and false-fail rates and the corrected pass rate with exact intervals. The same two commands on `RUN/claims.items.json`, `RUN/claims.jsonl` and `RUN/claims.scored.jsonl` calibrate the claim judge, and on `RUN/quiz.items.json`, `RUN/quiz.jsonl` and `RUN/quiz.scored.jsonl` (written to `RUN/quiz.calibration-labels.json`) the quiz judge; a quiz item passes for the person when every one of its criteria holds, and its lesson is in `requests/<id>/lesson-<sel>.md`. A lesson's seven items share one lesson, so the time estimate in the labelling file counts the lesson seven times; a person reads it once.

## Grading set

The quiz stage asks whether the questions are good; the grading set asks whether the app grades answers to them correctly and says why ([quizzes design](../../docs/design/2026-10-23-quizzes-and-tests.md) §4.3a and §7.2). It lives in `grading/`:

| File | Contents |
|---|---|
| `sets.json` | The registry (`quiz-grading-sets/1`): each set's rows file, bank file and, once frozen, their hashes and baseline |
| `bank-v1.json` | The fixed bank (`quiz-grading-bank/1`): three subjects, at least one not about software (music theory, Postgres, system design, the subjects of the committed requests), the source passages by version id, and items in the eval item shape documented at the top of `scripts/course_eval_quiz.py`, each with its `subject`. Explanation items carry a `reference_answer`; every item has a lesson block or a source quote found verbatim in its source |
| `quiz-grading-v1.jsonl` | The rows: one learner answer each |

| Row field | Meaning |
|---|---|
| `id` | Unique, no `/` |
| `item` | The bank item answered |
| `persona` | `correct`, `near_miss`, `partial`, `wrong`, `off_topic`, `empty` or `contesting` |
| `author` | `hand`, or `model:<name>` for a persona row; a model row needs `"reviewed": true`, set by the person who read it |
| `response` | Exactly the form's field for the format: `selected_indices`, `text`, `blank_responses`, `matches` or `ordered_values` |
| `contest_note` | On every contesting row and only there. Only model-graded answers can be contested |
| `expected` | `{"path": "deterministic", "points": p}` by section 3's arithmetic, or `{"path": "model"}` for an explanation or a short answer that matches no variant. `validate` recomputes it and refuses a mismatch |
| `generator` | Persona rows: the writing model, seed, temperature and prompt hash |

The deterministic rows double as fixtures for the engine's unit tests: the normalisation they assume is trim, case, diacritics (NFKD), English leading article, thousands commas (`1,200`), then punctuation and symbols to spaces (a decimal point between digits stays) and whitespace collapsed; a numeric key accepts a number with one trailing unit variant within an absolute tolerance. A short answer that matches no variant and is not empty goes to the model, a wrong number included.

**Rules.** Rows and bank items may be added or corrected until the baseline is scored and committed. Then `freeze` records the hashes in `sets.json`, and `validate`, which CI runs through `scripts/validate_rag_evals.py`, fails if either file changes. After that a new row, a correction or a new bank item goes into a new version: copy the files to `quiz-grading-v2.jsonl` (and `bank-v2.json` if the bank changes), register it, and never edit v1. Verdicts and points are labelled blind first; reasons only after, so the grader's reason is never in view while the verdict is decided. Persona rows come from `scripts/course_eval_personas.py` against the eval lessons' real banks once those items are in the bank file; a person reviews every one before `merge` takes it.

**Grades.** The grader's output on a set is one `quiz-grading-grades/1` file, written by the live hook from the app's own `grade_open` and equivalence judge with the recipe's model:

```json
{"schema": "quiz-grading-grades/1", "set": "quiz-grading-v1", "rows_sha256": "…", "bank_sha256": "…",
 "grader": {"model": "…", "model_revision": "…", "rubric_version": "…", "recipe_sha256": "…", "commit": "…"},
 "grades": [{"row_id": "…", "pass": "first", "seed": 0, "status": "graded", "error": null,
             "points_earned": 2.0, "points_possible": 4.0, "grade_status": "provisional",
             "rationale": {"summary": "…", "criteria": [{"criterion_id": "c1", "title": "…", "verdict": "partly_met",
                           "points": 1.0, "max_points": 2.0, "what_was_missing": "…", "your_words": ["…"],
                           "full_credit_includes": "…", "source_quote": "…", "source_version_id": "…"}],
                           "grader_model": "…", "rubric_version": "…", "graded_at": 0},
             "raw": {"…": "the model's output before the app's checks, same shape"},
             "response_to_note": null}]}
```

Every model-graded row has one `first` grade (`status` `couldnt_grade` with its `error` when the grader failed), deterministic rows may have one for the exactness check, and every contesting row has three `regrade` entries (seeds 0 to 2) graded with the note as data and a `response_to_note`. `rationale` is what the learner is shown; `raw`, when recorded, is what the checks below read, since the shown rationale has already had its bad spans dropped.

**Running.**

```sh
python3 scripts/course_eval_grading.py validate
python3 scripts/course_eval_personas.py generate quiz-grading-v1 --endpoint URL --model NAME \
  --out artifacts/course-eval/grading-draft.jsonl [--items ID,…] [--personas P,…] [--per-item N]
#   a person reads the draft, fixes a persona or answer where the model missed, sets "reviewed": true
python3 scripts/course_eval_personas.py merge artifacts/course-eval/grading-draft.jsonl quiz-grading-v1
python3 scripts/course_eval_grading.py score quiz-grading-v1 GRADES.json \
  --out evals/courses/results/<results_model>/<date>-quiz-grading-v1-<label>
python3 scripts/course_eval_grading.py label RESULTS        # Josh: human_verdict and human_points, blind
python3 scripts/shipped_path_stats.py calibration-score RESULTS/calibration-labels.json
python3 scripts/course_eval_grading.py reasons RESULTS      # Josh: human_reason on the marked-down rows
python3 scripts/course_eval_grading.py report RESULTS [--labels BASELINE/calibration-labels.json]
python3 scripts/course_eval_grading.py freeze quiz-grading-v1 --baseline RESULTS   # once, after the baseline
```

`score` writes `items.json`, `answers.jsonl` and `scored.jsonl` (with its manifest) in the chat eval's shapes, one row per model-graded answer: the grader is the judge, and full credit is a pass. `label` draws every graded row through `calibration_sample` unchanged (`--half-width 0`) and adds `human_points`; a row the grader couldn't grade is a known stratum and needs no label. A later run of the same set is scored against the baseline's labels with `--labels`, as `calibration-score --judge` does. `rationale.jsonl`, `deterministic.json` and a copy of the grades file sit beside them.

**What `summary.md` reports**, in order: agreement with the person on pass/fail, false-pass and false-fail rates and the PPI-corrected pass rate, with exact intervals; mean absolute point error against `human_points`; the rationale checks on every graded row (your_words and source quotes verbatim after whitespace normalisation, points agreeing with the verdict, a `what_was_missing` wherever points were lost, `full_credit_includes` reproducing the reference answer, meaning the whole of it or half its word trigrams, and markdowns without a reason, counting those the app showed); the share of marked-down answers whose reason the person accepts (`right`, against `verdict_right_reason_wrong` and `reason_fabricated`; one fabricated reason fails the grader on this eval whatever its verdicts); contest stability (regrades that change the verdict on an unchanged answer, limit 10 % across three seeds, with flips toward the person's label, rows whose seeds disagree and replies to the note counted beside it); and deterministic rows the app graded differently from the arithmetic. A set not yet frozen is marked a draft and is not quoted.

## Recording a run

Every scored run is committed here under `results/<results_model>/` and gets a row in the scored eval table of [docs/development/course-generation-history.md](../../docs/development/course-generation-history.md): label, commit, binary, model, requests, slots passed, lessons Ready, claim rates, minutes, web hits and misses, and the `compare` verdict against the previous run.
