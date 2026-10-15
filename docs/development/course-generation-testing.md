# Course generation: testing and measurement

How the course pipeline described in [course-generation-pipeline.md](course-generation-pipeline.md) is tested, run live and measured. Every live run so far is recorded in [course-generation-history.md](course-generation-history.md). The wider Learning Studio release checks, including the small live lesson fixtures, are in [learning-studio-verification.md](learning-studio-verification.md).

There are three levels:

| Level | Model | Web | Database | Cost |
|---|---|---|---|---|
| Unit tests | Scripted fixtures (`ScriptedModel`, recorders) | Fixtures | In-memory or temporary SQLite | Minutes, free |
| Live tests | The real model server from the app settings | Real searches and page fetches | A new throwaway SQLite per run | 30 to 130+ minutes per course |
| Checker measurement | The judge under test | None | None (labelled pairs) | Minutes per checker |

## 1. Unit tests

Run from `src-tauri/`:

```bash
cd src-tauri
CARGO_INCREMENTAL=0 cargo test --lib features::learning -j 2
```

At `09d5a552` this runs 448 tests, all passing (414 at `62b3151f`). Use a narrower filter while working, for example `outline_linking`, `outline_review`, `outline_repair`, `generation::evidence`, `content_verification::research`, `evidence_paging`, `content_caps`, `patient_model`, `course_generation`. Keep `-j 2` and `CARGO_INCREMENTAL=0`: the build cache has filled the disk before, and many agents may share the machine. A fresh worktree needs `src-tauri/binaries/` (the sidecar stub) and `dist/` copied from another worktree before the lib test binary builds.

Where the tests live (paths under `src-tauri/src/features/learning/`):

| Area | Tests |
|---|---|
| Reference planning and gathering | `planning/outline_reference_tests.rs`, `references/web_research.rs` (inline) |
| Evidence linking | `planning/outline_linking/tests.rs` |
| Review protocol and scope | `planning/outline_review_tests.rs`, `planning/outline_review_scope_tests.rs` |
| Repair by kind, budgets | `planning/outline_repair_tests.rs`, `planning/outline_repair/tests.rs` |
| Outline loop end to end (scripted) | `lessons/course_generation_tests.rs`, `planning/generation_context_tests.rs` |
| Lesson evidence before writing | `lessons/generation/evidence_tests.rs`, `lessons/generation/tests.rs` |
| Reason-first calls and checkpoints | `lessons/structured_call_tests.rs` |
| No fixed caps: retrieval, catalog, web paging, research, assessments, tutor context | `content_caps_tests.rs` |
| Outline outage wait | `patient_model_tests.rs` |
| Evidence paging and deepening, subject test | `lessons/content_verification/evidence_paging_tests.rs`, `lessons/reference_collection/tests.rs` |
| General-knowledge gate | `lessons/content_verification/general_knowledge.rs` (inline), `content_verification/tests.rs` |
| Repair progress stop | `lessons/content_verification/repair_progress_tests.rs` |
| Claim extraction, coverage, checks, research, repair, resume | `lessons/content_verification/*_tests.rs`, `tests.rs`, and the `*_cases.json` fixtures beside them |
| Quote re-anchoring | `lessons/teaching/citation_tests.rs` |
| Retrieval | `lessons/reference_collection/tests.rs` |
| Job recovery | `lessons/generation_recovery_tests.rs`, `lessons/generation_jobs.rs` (inline) |

The Python measurement scripts have their own tests, not run in CI:

```bash
python3 -m unittest scripts/test_lesson_claim_pairs.py scripts/test_claim_checker_bakeoff.py
```

### Checks CI runs

From `.github/workflows/ci.yml`. Run them before handing over a change:

```bash
cd src-tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets
cd ..
bash scripts/check-repository-barrier.sh
bash scripts/check-rust-layer-boundaries.sh
cargo test --locked --manifest-path scripts/rust-architecture-check/Cargo.toml
```

`scripts/rust-architecture-check` enforces, among other rules, that no Rust file other than a `tests.rs` exceeds 800 lines (`MAX_LINES` in `src/ratchet.rs`). Move inline tests to a sibling `tests.rs` or split the module.

## 2. Live tests

All live tests are `#[ignore]` lib tests in `src-tauri/src/features/learning/lessons/`, compiled into the lib test binary. They are wired as `generation_jobs::live_lesson_tests` (`live_lesson_tests.rs`) with submodules `course_e2e` (`live_course_e2e.rs`), `small` (`live_small_lesson.rs`) and `claim_pairs` (`live_claim_pairs_eval.rs`).

### What a live run touches

- **Database:** a new SQLite file in the run's artifact directory, created with `create_new` and migrated from `migrations/`. The test refuses to run if the file exists. It never opens the user's library.
- **Model:** the llama.cpp server configured in the settings file named by `LATTICE_LLAMACPP_SETTINGS` (its `settings.llm` block). On Josh's machine this is his real remote Qwen3.8-27B llama-server with a 131k window. It is single-slot: while a run is going, the app's own chat queues behind it, and a run takes the server for hours.
- **Web:** real searches (DuckDuckGo, Bing) and real page fetches through `WebService`, with its cache in `<run>/web/`.
- **Disk:** every model call is recorded (`call-N.json`), so a run directory can reach hundreds of files.
- **Outages:** the test calls the outline service directly, without the app's `PatientModel` wrapper, so a 503 while the server reloads fails the outline stage at once (accept1 failed this way after 81 seconds). The lesson job inside the test runs through the job runtime and does defer transient errors. Check the server is up and not reloading before a run.

Run one live course at a time, and only one local llama-server at a time.

### Snapshot the test binary

Build once, then copy the binary into the artifacts so the run is pinned to known code and a rebuild cannot replace it mid-run:

```bash
cd src-tauri
CARGO_INCREMENTAL=0 cargo test --lib --no-run -j 2 2>&1 | grep 'Executable unittests src/lib.rs'
#   Executable unittests src/lib.rs (<target-dir>/debug/deps/lattice-<hash>)
RUN=../artifacts/course-e2e-$(date +%F)
mkdir -p $RUN/bin
cp <target-dir>/debug/deps/lattice-<hash> $RUN/bin/<label>-$(git rev-parse --short HEAD)
```

Name the binary `<label>-<short sha>`, as in `artifacts/course-e2e-2026-10-14/bin/final2-37c6ef57`. `artifacts/` is gitignored.

### The course end-to-end test

`course_e2e::live_topic_course_passes_review_and_prepares_its_first_lesson`.

| Variable | Meaning |
|---|---|
| `LATTICE_LLAMACPP_SETTINGS` | Settings JSON; the test reads `settings.llm`. The app's file is `$HOME/Library/Application Support/tech.lattice.app/settings.json` |
| `LATTICE_COURSE_REQUEST` | A `GenerateLearningProgramRequestDto` JSON. Must be topic-only (`documentIds` and `sourceUrls` empty) |
| `LATTICE_COURSE_ARTIFACTS` | A new directory for this run |
| `LATTICE_COURSE_MANUAL_REPAIRS` | Repair presses allowed after generation. Default 0, which proves generation alone yields an acceptable outline |

What it does and asserts:

1. Runs `service::generate_with_references_and_progress` with the real model and web, exactly as the app does.
2. Presses Repair (`outline_draft::repair_saved`) up to `LATTICE_COURSE_MANUAL_REPAIRS` times while the outline has not passed.
3. Asserts the outline review status is `Passed`.
4. Accepts the course (`service::accept`) and asserts it is `Active`.
5. Starts a lesson-preparation job for the first lesson of module 1 and runs it through the native job runtime with the production worker (`small::run_and_confirm`). Transient failures stay queued and are redelivered.
6. Asserts the job completed and, through a new database connection, that the lesson is `Ready` with blocks and questions.

Artifacts in the run directory:

| File | Written | Contents |
|---|---|---|
| `course.sqlite` | Always | The whole run: `learning_programs`, `learning_outline_drafts` (state JSON with review, receipts, plan), `learning_outline_checkpoints`, sources, `jobs` (including `staged_result_json` lesson drafts), `job_checkpoints` (keys `<lesson id>/<key>`), `learning_lesson_verifications` |
| `call-N.json` | Each model call | `input` (the messages), `answer`, `finishReason`, `inputTokens`, `outputTokens`, `firstTokenLogprobs` |
| `web/` | Always | The web service's cache |
| `outline-generated.json`, `outline-repair-N.json` | After generation and each manual repair | Review status, findings, note, sources, modules |
| `job-id.txt` | When the lesson job starts | The job ID |
| `result.json` | When the job ends | The job record: status, progress, error |
| `program.json`, `summary.json` | Only on success | The published program; wall time, completed model calls, IDs |
| `course-summary.json` | Only on success | Generation, acceptance and total seconds, manual repairs, source, module and lesson counts |

The log prints `LIVE COURSE generated: ...` after generation, `Live call N completed: <in> input / <out> output tokens, <finish reason>` per call, and `LIVE COURSE CONFIRMED: ...` on success.

### Running it: `scripts/live-course-e2e.sh`

```bash
scripts/live-course-e2e.sh <binary> <label> <requests-dir> [artifacts-root]
# example
scripts/live-course-e2e.sh artifacts/course-e2e-2026-10-15/bin/run1-abc1234 run1 evals/courses/requests
```

It runs every JSON in `<requests-dir>` one after another with one binary. Each request gets `<artifacts-root>/<label>-<request name>/` and a log beside it. `<artifacts-root>` defaults to `artifacts/course-e2e-<today>`. The script refuses to reuse an existing run directory. `<label>-summary.txt` records the binary's sha256, the commit, start and end times, exit code, minutes and the last result lines. Set `LATTICE_LLAMACPP_SETTINGS` to use another settings file and `LATTICE_COURSE_MANUAL_REPAIRS` to allow Repair presses.

Requests live in `evals/courses/requests/`:

| File | Depth | Minutes per session | Status |
|---|---|---|---|
| `1-dotnet-interview.json` | deep dive | 60 | The acceptance request |
| `2-postgres-performance.json` | course | 45 | Parked |
| `3-music-theory-production.json` | focused | 30 | Parked |
| `4-system-design-fundamentals.json` | deep dive | 60 | Parked |

To run only the acceptance request, point the script at a directory holding only `1-dotnet-interview.json` (on 2026-10-14 the other three were moved to `requests-later/` for this reason).

`1-dotnet-interview.json` is Josh's own request and describes his background, so it is gitignored and exists only in local checkouts (and in `artifacts/course-e2e-2026-10-14/requests/`). The repository is public; keep personal requests out of it.

### Inspecting a run

Never print a whole log or call file; they are large.

```bash
RUN=artifacts/course-e2e-2026-10-14/final2-1-dotnet-interview
grep -a -E 'LIVE COURSE|panicked at|Lesson preparation stage changed' $RUN.log | tail -20
jq '{status, error, progressMessage}' $RUN/result.json
grep -a -o 'Checking [0-9]* unsupported claims against their next page of evidence (page [0-9]*)' $RUN.log   # deepening rounds
sqlite3 $RUN/course.sqlite "select key, length(data_json) from job_checkpoints order by updated_at" | tail
sqlite3 $RUN/course.sqlite "select state_json from learning_outline_drafts" | jq '.review | {status, note, issues: (.issues|length)}'
# the largest requests
for f in $RUN/call-*.json; do print -r -- "$(jq -r '[.inputTokens, .outputTokens, .finishReason] | @tsv' $f) ${f:t}"; done | sort -n | tail -5
jq -r '.answer' $RUN/call-42.json | head -c 2000
```

### Retrying a failed lesson: how `lesson-fix-1` was run

`small::live_small_lesson_resumes_to_ready` continues a lesson job in a copied database through the same retry operation the UI uses (`LearningCurriculumRepository::retry_job`): it copies the failed job's checkpoints and drafts into a new job, then runs the production worker until it completes.

| Variable | Meaning |
|---|---|
| `LATTICE_LESSON_CALLS` | A directory holding `lesson.sqlite` and `job-id.txt`; new `call-N.json` files continue the existing numbering |
| `LATTICE_LLAMACPP_SETTINGS` | As above |

The job must be `Pending`, `Running` or `Failed`, and no other job of the program may be active. `lesson-fix-1` retried the failed lesson job of run `wp1-1` ("What's New in .NET 10 & ASP.NET Core 10", job `ffa5f37f`) with code that later became commit `2885fd55`. It reached Ready in 128 minutes (new job `7905f3ac`). The recipe:

```bash
A=artifacts/course-e2e-2026-10-14
cp -R $A/wp1-1 $A/lesson-fix-1          # keeps job-id.txt and the call files
mv $A/lesson-fix-1/course.sqlite $A/lesson-fix-1/lesson.sqlite
cd src-tauri
LATTICE_LLAMACPP_SETTINGS="$HOME/Library/Application Support/tech.lattice.app/settings.json" \
LATTICE_LESSON_CALLS=$PWD/../$A/lesson-fix-1 \
  <binary> live_small_lesson_resumes_to_ready --ignored --nocapture > ../$A/lesson-fix-1.log 2>&1
```

Always work on a copy. The test writes to the database it is given.

### Other live tests

These probe one stage with the real model. Each takes `LATTICE_LLAMACPP_SETTINGS` and an artifact directory in `LATTICE_LESSON_CALLS`, plus the inputs listed. The small-lesson fixtures and their commands are described in [learning-studio-verification.md](learning-studio-verification.md#small-live-lesson-publication-check).

| Test | Extra inputs | What it checks |
|---|---|---|
| `small::live_small_lesson_reaches_ready` | `LATTICE_LESSON_FIXTURE` (`live_fixtures/*.json`) | One small lesson from a fixed outline and source to Ready |
| `small::live_contract_fidelity_controls` | `LATTICE_FIDELITY_CASES`, `LATTICE_CHECK_BATCH` | Requirements versus empirical claims in fidelity and checks |
| `small::live_inventory_slots_keep_every_section` | `LATTICE_INVENTORY_SECTIONS` | Extraction keeps every section in one call |
| `small::live_review_schema_preserves_grounding` | `LATTICE_REVIEW_CASES` | Teaching review schema keeps grounding |
| `small::live_saved_inventory_revision_finishes` | `LATTICE_INVENTORY_BASIS`, `LATTICE_INVENTORY_DRAFT` | A recorded inventory revision completes |
| `live_batched_verification_matches_individual_checks_and_measures_cost` | `LATTICE_BATCH_BENCH_MODE` | Batched versus single claim checks |
| `live_coverage_mapping_recovers_existing_claims_without_inventing_missing_ones` | `LATTICE_MAPPING_FIXTURE` | Coverage mapping |
| `live_passage_coverage_preserves_complete_assertions` | `LATTICE_COVERAGE_CALL`, `LATTICE_COVERAGE_UNITS`, `LATTICE_COVERAGE_REQUIRED_MISSING` | Fidelity audit on a recorded call |
| `live_source_passage_judgments` | `LATTICE_CLAIM_CASES`, `LATTICE_CLAIM_POLICY`, `LATTICE_CLAIM_PROGRAM`, `LATTICE_LESSON_DATABASE` | Strict judge on captured evidence cases |
| `live_reference_selection` | `LATTICE_REFERENCE_CASES` (optional) | `select_references` on committed controls |
| `live_teaching_review_evidence_scope` | `LATTICE_TEACHING_REVIEW_CALL` | Teaching review on a recorded call |
| `live_review_findings_require_evidence` | `LATTICE_REVIEW_DRAFT`, `LATTICE_TEACHING_REVIEW_CALL`, `LATTICE_LESSON_DATABASE`, `LATTICE_REVIEW_PROGRAM` | Review findings are bound to evidence |
| `live_enqueue_saved_lesson_for_native_resume` | `LATTICE_LESSON_DATABASE`, `LATTICE_LESSON_JOB` | Enqueues a retry for the app's own worker to resume |
| `live_retry_saved_lesson_through_publication` | `LATTICE_LESSON_DATABASE`, `LATTICE_LESSON_JOB`, `LATTICE_LESSON_RECOVER_ORPHAN`, `LATTICE_REMOVE_RESEARCH_SOURCES` | Retries a failed job in a given database through publication |
| `claim_pairs::live_claim_pairs_eval` | see [below](#the-shipped-judge-live_claim_pairs_eval) | Scores the shipped judge on labelled pairs |

## 3. The acceptance bar

Josh's .NET request (`evals/courses/requests/1-dotnet-interview.json`, local only, a deep-dive, topic-only course) produces an outline that passes review and prepares lesson 1 to Ready, in **one clean run from scratch**, with no manual repairs:

```bash
mkdir -p /tmp/accept && cp evals/courses/requests/1-dotnet-interview.json /tmp/accept/
scripts/live-course-e2e.sh <binary> accept-1 /tmp/accept
```

The run passes when its log ends with `LIVE COURSE CONFIRMED` and `course-summary.json` exists. Not met as of 2026-10-15: accept2 (`f90f440c`) passed the outline and was stopped during lesson 1's evidence deepening after 3h20m; accept3 (`09d5a552`) is running. No run has yet reached the §16 general-knowledge gate.

Also record per run: wall time per stage, the outline's linked-item count, references gathered, claims checked, claims left unsupported, and the largest request in tokens.

## 4. Claim-checker measurement

The question: is the lesson judge a weak point, and would a second judge help? Measured on real lesson claims with labels the checkers never see.

### Building a labelled set: `scripts/lesson_claim_pairs.py`

Run from the repo root. In pipeline order:

1. **extract** pairs from a database with prepared lessons:
   `python3 scripts/lesson_claim_pairs.py extract --db <run>/lesson.sqlite --subject dotnet --out cands.jsonl --calls <run> --pool-out pool.jsonl [--approved 35 --related 10 --rejected 15 --pool 20 --seed N]`
   It reads the stored reports in `learning_lesson_verifications`. `approved` pairs use the passage the judge cited; `related` pairs use the retrieved passage it did not cite that shares the most words with the claim; `rejected` pairs (needs `--calls`, the run's `call-N.json` files) are claims the judge answered unsupported. Every passage must be a verbatim span of the stored source; others are dropped and counted. The judge's verdict is kept in `pipeline_verdict` and never shown to labellers.
2. **add-perturbed** minimal edits written by hand against pool passages:
   `add-perturbed --pool pool.jsonl --edits edits-<subject>.jsonl --out cands.jsonl` (each edit is `{pool_id, claim, intended_label, difficulty, note}`).
3. **blind**: `blind --pairs cands.jsonl --out-dir parts/ [--per-part 22]` writes shuffled `part-NN.txt` files with id, claim and passage only.
4. **Two independent labellers** label the parts under `LABEL-RULES.md` (three labels: supported, unsupported, contradicted). For w1 these were two models (Opus and Sonnet), each in a fresh context.
5. **assemble**: `assemble --pairs cands.jsonl --labels labels-A.jsonl labels-B.jsonl --out gold.jsonl --disagreements disagreements.jsonl` prints agreement and Cohen's kappa and keeps agreed labels.
6. **adjudicate** the disagreements into `adjudicated.jsonl`, then rerun `assemble ... --adjudicated adjudicated.jsonl`.
7. **spot-check for a human**: `spot-check --gold gold.jsonl --out page.html [--count 30]` writes a page for a person to relabel a sample blind; `compare --gold gold.jsonl --answers answers.json` reports agreement with the gold labels.
8. **Score** each checker (below).

**Adding a subject:** run a live lesson in that subject (a course run or `live_small_lesson_reaches_ready`), then `extract --subject <name>` from its database and call files, write `edits-<name>.jsonl` against its pool, and repeat steps 3 to 7. Keep one dataset directory per wave under `evals/grounding/`, with a dataset card. The w1 card is [evals/grounding/lesson-claims-w1/README.md](../../evals/grounding/lesson-claims-w1/README.md).

### The shipped judge: `live_claim_pairs_eval`

Scores the lesson's strict passage judge (`ClaimChecker`, `CheckPolicy::Strict`) exactly as lesson verification batches it. Each pair becomes one claim whose only evidence is its passage. The web challenge that follows a provisional verdict is not run, so this measures the passage judge alone.

| Variable | Meaning |
|---|---|
| `LATTICE_LLAMACPP_SETTINGS` | Settings whose `settings.llm` names the model to test (Josh's Qwen, or a settings copy pointing at another server such as Laguna) |
| `LATTICE_CLAIM_PAIRS` | The gold JSONL |
| `LATTICE_CLAIM_RESULTS` | Output JSONL; appended per batch, so a stopped run resumes |
| `LATTICE_CLAIM_CHECKER` | Name written into each row (default `lesson-judge`) |
| `LATTICE_CLAIM_BATCH` | Claims per batch (default 8, as in the pipeline) |

```bash
cd src-tauri
LATTICE_LLAMACPP_SETTINGS="$HOME/Library/Application Support/tech.lattice.app/settings.json" \
LATTICE_CLAIM_PAIRS=$PWD/../evals/grounding/lesson-claims-w1/gold-w1.jsonl \
LATTICE_CLAIM_RESULTS=$PWD/../evals/grounding/results/<date>-lesson-w1-<checker>.jsonl \
LATTICE_CLAIM_CHECKER=<checker> \
  <binary> live_claim_pairs_eval --ignored --nocapture
```

Rows: `id, checker, label, difficulty, verdict, reason, latency_ms, batch, calls`.

### Granite Guardian: `scripts/claim_checker_granite.py`

Granite Guardian 3.3 8B (Q8 GGUF at `~/.cache/lattice-eval-models/granite-guardian-3.3-8b-Q8_0.gguf`) served by the bundled `src-tauri/binaries/llama-server-aarch64-apple-darwin`. A typical start (the exact flags used on 2026-10-14 were not recorded):

```bash
src-tauri/binaries/llama-server-aarch64-apple-darwin \
  -m ~/.cache/lattice-eval-models/granite-guardian-3.3-8b-Q8_0.gguf --port 8094 -c 8192
python3 scripts/claim_checker_granite.py --url http://127.0.0.1:8094 \
  --dataset evals/grounding/lesson-claims-w1/gold-w1.jsonl \
  --out evals/grounding/results/2026-10-14-lesson-w1-granite-guardian-3.3-8b.jsonl [--think]
```

The script builds IBM's prompt by hand and posts it to the raw `/completion` endpoint with `n_probs` and temperature 0. It must not use the chat endpoint: llama-server's chat endpoint treats the final assistant message (here, the claim under judgement) as a prefill of the judge's answer. Granite answers "yes" when the text is ungrounded, so supported means "no"; `score_supported` is P(no) / (P(yes) + P(no)). It resumes by skipping IDs already in `--out`. `--think` uses the reasoning mode and names the checker `...-think`.

### MiniCheck and other local checkers: `scripts/claim_checker_bakeoff.py`

Runs one checker per process with local torch and transformers, no server. Use the eval venv at `~/.cache/lattice-eval-venv`:

```bash
~/.cache/lattice-eval-venv/bin/python scripts/claim_checker_bakeoff.py --checker minicheck-flan-t5-large \
  --dataset evals/grounding/lesson-claims-w1/gold-w1.jsonl --run-date 2026-10-14 [--device cpu --threads 4 --limit N]
```

Checkers: `minicheck-roberta-large`, `minicheck-flan-t5-large`, `hhem-2.1-open`, `lettucedetect-base`, `lettucedetect-large` (binary verdicts only). It writes `<date>-<checker>.jsonl` and `.meta.json` into `--out-dir`; rename to the `<date>-lesson-w1-<checker>` form for this set.

### Scoring: `scripts/claim_checker_bakeoff_score.py`

```bash
python3 scripts/claim_checker_bakeoff_score.py \
  --dataset evals/grounding/lesson-claims-w1/gold-w1.jsonl \
  --both qwen3.8-27b-lesson-judge,granite-guardian-3.3-8b \
  --json-out evals/grounding/results/2026-10-14-lesson-w1-scores.json \
  evals/grounding/results/2026-10-14-lesson-w1-*.jsonl     # not the .meta.json files
```

- `--both A,B` (repeatable) adds an AND gate: a claim passes only when both checkers say supported.
- **False approvals** (`false_approvals`): the checker said supported on a pair labelled unsupported or contradicted. Denominator: the not-supported pairs. This is the error that lets a wrong claim into a lesson.
- **False blocks** (`false_blocks`): the checker said not supported on a supported pair. Denominator: the supported pairs. This is the error that fails or rewrites a correct lesson.
- Also: accuracy, balanced accuracy with a 95% interval, precision, recall and F1 on supported, macro F1, recall for unsupported and contradicted, three-way accuracy, a confusion matrix, `unverified` count, latency, a best-threshold analysis, and breakdowns `by_subject` and `by_origin` (approved, related, rejected, perturbed).

## 5. Results on lesson-claims-w1

85 .NET pairs: **34 supported, 51 not supported** (40 unsupported, 11 contradicted). Labels by two blind model labellers, 94% agreement, kappa 0.90, five disagreements adjudicated. See the [dataset card](../../evals/grounding/lesson-claims-w1/README.md). FA = false approvals of the 51 not-supported pairs; FB = false blocks of the 34 supported pairs.

| Checker | FA | FB | Notes |
|---|---|---|---|
| Qwen3.8-27B (the shipped lesson judge) | 4/51 (7.8%) | 0/34 | Balanced accuracy 0.961. Best |
| Qwen AND Granite | 3/51 | 1/34 | A wash: one fewer approval, one new block |
| Granite Guardian 3.3 8B | 10/51 (20%) | 1/34 (3%) | Via raw `/completion` (see above) |
| Granite Guardian, think mode | 13/51 (25%) | 1/34 (3%) | Not in the scores file; computed from its JSONL |
| Laguna-XS-2.1 (Josh's utility model) | 16/51 (31%) | 1/34 (3%) | Qwen AND Laguna still has 4 FA: it caught none of Qwen's misses |
| MiniCheck Flan-T5-large | 15/51 (29%) | 4/34 (12%) | `~/.cache/lattice-eval-venv` |
| MiniCheck RoBERTa-large | 12/51 (24%) | 8/34 (24%) | Not in the scores file; computed from its JSONL |

Files: `evals/grounding/results/2026-10-14-lesson-w1-*.jsonl`, `*.meta.json` and `2026-10-14-lesson-w1-scores.json`.

**Conclusion: no second judge now.** No candidate catches the shipped judge's misses without adding blocks. The checker is not the weak point; generation grounding is. The labels are model labels; Josh's blind human spot-check is not done yet, and the set covers one subject.

### Qwen factual-accuracy sample

To see what the unsupported claims actually were, 40 claims the checker marked unsupported in the failed final2-1 .NET lesson were judged against world knowledge (by the lead agent, not by a source check):

| Judgement | Claims |
|---|---|
| Correct | 17 |
| Mostly correct (an overstated figure) | 2 |
| Overstated with an absolute ("always", "must") | 4 |
| Wrong (IMediator attributed to ASP.NET Core instead of MediatR) | 1 |
| Advice or opinion | 3 |
| About the lesson's own example code or exercises | 13 |

Most unsupported claims were true but unsourced, and a third described the lesson's own code, which no outside source can confirm. This motivated design §15 (gather evidence before writing and the stricter grounding rules) and §16 (label unsourced general knowledge instead of failing the lesson; classify lesson-internal statements as nonfactual).

## 6. Run history

Every live run of 2026-10-14 (baseline-1, wp1-1, wp3-outline-2, final-1 to final-4, final2-1 to final2-4, lesson-fix-1), with commit, outcome, minutes, key numbers, cause and the change that followed, is in [course-generation-history.md](course-generation-history.md). Add each new run there.
