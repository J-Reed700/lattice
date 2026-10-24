# Eval baseline sweep

`scripts/baseline-sweep.sh` sets a new baseline for every eval suite with one command. It runs each suite's established procedure, as that suite's own README describes it, on the current commit. It adds no eval logic of its own: every number comes from the suite's existing runner and scorer.

```sh
export LATTICE_JUDGE_URL=…          # the remote judge's OpenAI-compatible base URL
export LATTICE_JUDGE_MODEL=…        # the model name judge requests carry
export LATTICE_JUDGE_WINDOW=…       # the judge's context window in tokens
export LATTICE_JUDGE_AUTH='Header-Name: value'   # only if the server needs it
scripts/baseline-sweep.sh           # everything, resumable
scripts/baseline-sweep.sh --list    # steps, what each needs, state, minutes
scripts/baseline-sweep.sh --only chat-answers,chat-score
```

## What a baseline is

A baseline is one commit's numbers on every suite, committed next to the earlier runs. A later change (a candidate) is measured with the same procedure and compared with it using the suite's paired comparison. A comparison is fair only if the baseline is consistent, so the sweep:

- refuses to start on a dirty tree, records the commit in `artifacts/baseline/<date>/commit`, and refuses to resume on another commit;
- builds the binaries once, at the start, and runs every step on those copies (`bin/` with `SHA256SUMS`, checked again on every resume). The lib test binary is built with `cargo-slot cargo test --lib --no-run -j 6` from `crates/lattice-desktop`, as the course README does;
- uses one local model (Qwen3.5-9B, weights hash checked by each runner) and one judge for the whole sweep;
- stops if HEAD moves or the tree changes during the sweep, since the scorers are read from the tree.

A judge's pass rate is a smoke signal until a person has checked a sample of its verdicts ([calibration](../../evals/retrieval/EVALUATION_PROTOCOL.md#how-good-is-the-judge)). A baseline is the reference point for a comparison. It is not a release claim.

## Before you start

- A clean tree at the commit to measure.
- `crates/lattice-desktop/binaries/llama-server-aarch64-apple-darwin`. A fresh worktree needs `crates/lattice-desktop/binaries/` copied in from the main checkout.
- The 9B weights at `~/.cache/lattice-evals/models/Qwen3.5-9B-Q4_K_M.gguf`, and the embedding model's GGUF at `~/.cache/lattice/models/qwen3-embedding-0.6b/Qwen3-Embedding-0.6B-f16.gguf`, with its `tokenizer.json` beside it.
- Port 8089 free. The sweep starts the 9B itself and refuses to start if anything else is listening there. Do not run the chat eval, the course e2e test or another sweep at the same time; the sweep holds `artifacts/baseline/.lock`.
- The app's settings file (`~/Library/Application Support/tech.lattice.app/settings.json`, or `LATTICE_APP_SETTINGS`). The claim step copies it.
- Disk space for an `evalopt` build next to the debug build, and for the course run (about 4M input tokens of model calls kept per lesson).

## Environment

| Variable | Meaning | Default |
|---|---|---|
| `LATTICE_JUDGE_URL` | The remote judge's OpenAI-compatible base URL | required by chat-score, claims, course-score |
| `LATTICE_JUDGE_MODEL` | The model name judge requests carry | required by the same steps |
| `LATTICE_JUDGE_WINDOW` | The judge's context window in tokens | required by claims and course-score |
| `LATTICE_JUDGE_AUTH` | One `Header-Name: value` line, sent only to the judge | none |
| `LATTICE_JUDGE_TAG` | Short judge name used in result file names | the model's file name in lower case, without `.gguf` |
| `LATTICE_MEMORY_URL`, `LATTICE_MEMORY_MODEL`, `LATTICE_MEMORY_AUTH` | The remote chat model the memory release baseline runs on | the judge's URL, model and auth |
| `LATTICE_MEMORY_CONCURRENCY` | Memory cells in flight. Keep it at or below the server's free slots | 2, as on 2026-09-26 |
| `LATTICE_MEMORY_CONTEXT_TOKENS` | The window the memory harness budgets against | 16384, as on 2026-09-26 |
| `LATTICE_APP_SETTINGS` | The settings file the claim step copies | the app's own |

No committed file names a server address or key. The script never prints the auth values. The claim step's settings copy holds the header; it is written with mode 600 under `artifacts/` and deleted when the step ends.

## The steps, in order

| # | Step | What it runs | Model | Last known duration |
|---|---|---|---|---|
| 0 | `build` | `cargo test --lib --no-run`, `cargo test --test conversation_memory_evals --no-run`, `cargo build --example shipped_path_eval`, and the `evalopt` build of `retrieval_eval`. Copies all four into `bin/` | none | not recorded |
| 1 | `retrieval-v3` | `LATTICE_EMBED_GGUF=<the GGUF> retrieval_eval --mode production --top-k 10 --compression none` on synthetic-library v3, scored with `rag_eval.py --k 5` | embedding model only | Embedding the corpus is "about seven minutes of a run" ([2026-09-19 results](../../evals/retrieval/2026-09-19-V3-MODERNIZATION-RESULTS.md#re-running)). The sweep's embedding cache starts empty |
| 2 | `retrieval-terse` | `terse_slice.py check` and `build`, the same retrieval run on the slice, then `terse_slice.py compare` against step 1's run | embedding model only | With the cache filled, a repeat run "takes about fifteen seconds" (same source) |
| 3 | `chat-answers` | Starts the 9B from `shipped-path-recipe-9b-local.json`'s `server.launch` (16K). Runs `shipped_path_eval` on `desktop-path-v1`, stops the server, and gzips the call log | local 9B | 2026-10-20, 38 items: 76 min. 2026-10-13, 34 items: 46 min (`started_at`/`finished_at` in each run's `.run.json`). Indexing v3 takes "about 5.5 minutes" of that ([shipped-path baseline](../../evals/retrieval/2026-10-12-SHIPPED-PATH-BASELINE.md#reproduce)) |
| 4 | `chat-score` | `shipped_path_eval.py score` with the remote judge | remote judge | not recorded |
| 5 | `memory` | `release_baseline_covers_every_family_baseline_and_ablation`, three repeats | remote chat model | 3 h 20 min on the remote Qwen3.8-27B, 2 of 4 slots, concurrency 2 ([memory design §24.6](../design/2026-09-19-conversation-memory.md)) |
| 6 | `claims` | `live_claim_pairs_eval` (Lattice's own checker) on `lesson-claims-w1/gold-w1.jsonl` with the remote judge, then `claim_checker_bakeoff_score.py` | remote judge | 2026-10-14: 85 pairs in 11 batches. The batches' `latency_ms` in `2026-10-14-lesson-w1-qwen3.8-27b-lesson-judge.jsonl` sum to about 50 s. Wall time not recorded |
| 7 | `course-run` | Starts the 9B from `course-eval-9b-local.json`'s `server.launch` (32K). Runs `scripts/course-eval.sh run … base --lessons m1.l1` on the snapshotted lib binary, then stops the server | local 9B | No 9B figure yet: "the first run sets it" ([course README](../../evals/courses/README.md#before-a-run)). On the remote 27B an outline took 10 to 32 min and a lesson 2 to 3.5 h, so the set with one lesson each took 16 to 28 h |
| 8 | `course-collect` | `course_eval.py collect` | none | not recorded |
| 9 | `course-score` | `course_eval.py score` with the remote judge and its window, then `summarize` | remote judge | not recorded |

`--list` shows each step's minutes once it has run. Copy them into the history rows, so the next sweep has figures to quote.

**Why this order.** The build comes first so that every step runs the same code. Retrieval needs no model server and is the cheapest step, so a broken build or index fails within minutes instead of after a day. Each suite then generates and is graded in turn. The local 9B runs only during its two generation steps (chat at 16K, course at 32K, each from its own recipe's launch line), so only one local server is ever up. It is stopped before any step that does not use it. Memory and claims need only the remote server. The course eval goes last because it is by far the longest step: it should not hold up the other suites, and it can run unattended overnight.

## Resuming

Run the same command again. Each step leaves `done/<step>` when it succeeds and is skipped after that. A failed step leaves `failed/<step>`; the sweep records it, moves on, and retries it on the next run. A step whose input step is not done is reported as `BLOCKED`. Ctrl-C stops the sweep and the 9B server, and the next run picks up from there. The sweep sets no time limit of its own; the memory harness's two are explained under [Choices the sweep makes](#choices-the-sweep-makes).

To resume on another day, pass `--date <the sweep's date>`. Without `--date`, the script finds an unfinished sweep of the same commit, names it, and refuses to start a second sweep by accident.

How each step resumes:

| Step | On a rerun |
|---|---|
| `retrieval-*` | Runs again; the embedding cache makes this fast |
| `chat-answers`, `chat-score` | Start over. The runner and scorer never overwrite, so an interrupted attempt's files are moved to `partial/` first |
| `memory` | Keeps every completed cell. The sweep collects the `run_complete` records from all earlier traces into `memory-resume.jsonl` and passes it as `LATTICE_EVAL_RESUME_TRACE` |
| `claims` | Continues from the last finished batch: the results file is appended per batch |
| `course-run` | `course-eval.sh` skips requests with `done.json` and continues the rest from their checkpoints. The step is done when every request has `done.json` or was skipped as optional |
| `course-collect` | Starts over; an earlier partial results directory is moved to `partial/` |
| `course-score` | Skips items already scored |

A memory run that reaches its end but fails §17.4 gates counts as done. The failures are the baseline's result, and the step log lists them.

## What the sweep writes

Under `artifacts/baseline/<date>/` (gitignored):

| Path | Holds |
|---|---|
| `commit`, `complete` | The commit measured; `complete` once every step is done |
| `done/`, `failed/` | One marker per step, with its commit, start, finish and minutes |
| `logs/<step>.log` | Each step's output, including the exact commands; the 9B's own output goes to `logs/<step>.server.log` |
| `bin/`, `bin/SHA256SUMS` | The four snapshotted binaries, named `<name>-<short sha>` |
| `outputs.txt` | The repository paths the sweep writes; only these may be dirty on a resume |
| `retrieval/`, `embed-cache/` | Retrieval run files before they are copied into `evals/`, and the embedding cache |
| `memory-traces/`, `memory-resume.jsonl` | The memory traces. **They contain conversation text: never commit them** |
| `course-eval/`, `course-web-traces/` | The course run directory, and the web trace this baseline recorded |
| `partial/` | Outputs of interrupted attempts, set aside |

## What to commit

At the end, the sweep prints the files it wrote into the tree and the history to update. For a date `D` and a judge tag `J`:

| Suite | Result files | History |
|---|---|---|
| Retrieval | `evals/retrieval/results/D-v3-BASELINE-run.jsonl`, `-metrics.json`, `-terse-run.jsonl`, `-terse-metrics.json` | No dedicated table. Add a row to the production-mode table (`\| Run \| recall@5 \| …`) in [evals/retrieval/README.md](../../evals/retrieval/README.md#production-mode-now-runs-chats-search-2026-10-09), and record the terse gap under [Terse slice of v3](../../evals/retrieval/README.md#terse-slice-of-v3-2026-10-19) |
| Chat | `evals/retrieval/results/chat/qwen3.5-9b-local/D-shipped-path-desktop-path-v1.jsonl`, `.jsonl.run.json`, `.jsonl.calls.jsonl.gz`, `.judge-J.scored.jsonl` with its `.manifest.json` and `.summary.json` | No table: the folder is the per-model history ([protocol](../../evals/retrieval/EVALUATION_PROTOCOL.md#against-the-bundled-sidecar-instead-of-a-remote-server)). Compare with the previous run using `shipped_path_stats.py compare` ([protocol](../../evals/retrieval/EVALUATION_PROTOCOL.md#is-the-candidate-better)) |
| Memory | `evals/conversation-memory/results/<model>/D-release-baseline.md`, written by hand from the printed summary in `logs/memory.log` with a header like the 2026-09-26 files. `<model>` is the served model's folder, e.g. `qwen3.8-27b-remote` | No table: one file per run per model ([results README](../../evals/conversation-memory/results/README.md)) |
| Claims | `evals/grounding/results/D-lesson-w1-J-lesson-judge.jsonl`, `D-lesson-w1-scores.json` | The Results table in [lesson-claims-w1/README.md](../../evals/grounding/lesson-claims-w1/README.md#results) and the table in [course-generation-testing.md §5](course-generation-testing.md#5-results-on-lesson-claims-w1) |
| Course | `evals/courses/results/qwen3.5-9b-local/D-course-eval-v1-base/` | The scored eval table in [course-generation-history.md](course-generation-history.md#scored-eval-runs) |

Never commit anything under `artifacts/`, the memory traces, or the claim step's settings copy (it is already deleted).

## Choices the sweep makes

These are the places where a suite's docs leave something open or conflict with the sweep's rules:

- **Memory runs the release baseline on the remote model.** The latest memory baselines (2026-09-26) are the remote Qwen3.8-27B release baseline and its rerun. The design calls the 9B runs "a harness and pipeline check and the floor for the local tier, not a release baseline" (§24.5).
- **Memory keeps two time limits.** The harness has no unlimited setting for `LATTICE_EVAL_REQUEST_TIMEOUT_SECS` (with llama.cpp this becomes the app's per-request stall timeout) or `LATTICE_EVAL_COMPACTION_DEADLINE_SECS`. The sweep sets both to 900 s, the 2026-09-26 values, so the run is comparable with that baseline. Removing them would need a harness change. They are the only model-call limits in the sweep.
- **The claim judge comes from a settings copy.** `live_claim_pairs_eval` reads `settings.llm` from `LATTICE_LLAMACPP_SETTINGS`. The testing doc allows "a settings copy pointing at another server", so the sweep copies the app's settings and sets `llamaCpp.url`, `model`, the auth header and `contextWindow` from the judge's variables. Every other setting stays the app's own.
- **The course baseline records its own web trace.** The README says a baseline replays "against an empty trace", and the recipe's shared `artifacts/course-eval/traces` may already hold pages. The sweep therefore sets `LATTICE_COURSE_WEB_TRACE` to `artifacts/baseline/<date>/course-web-traces`. A candidate replays that same directory by setting the variable to it.
- **Course scoring runs without `--claims`.** The README marks it optional. Run `course_eval.py score <results> … --claims` afterwards to judge every claim as well.
- **The external claim checkers (Granite, MiniCheck, Laguna) are not run.** The sweep measures Lattice's own checker only.
- **Earlier result files count as dirty.** Run records such as the shipped-path `.run.json` list the result files earlier steps wrote as dirty. The sweep checks that nothing else in the tree is.

## Where the details are

| Suite | Procedure |
|---|---|
| Retrieval | [evals/retrieval/README.md](../../evals/retrieval/README.md): Production mode, Terse slice of v3; the `evalopt` build is in [2026-10-12-RERANKER-FTS-HEADER-FUSION.md](../../evals/retrieval/2026-10-12-RERANKER-FTS-HEADER-FUSION.md#reproducing) |
| Chat shipped path | [EVALUATION_PROTOCOL.md](../../evals/retrieval/EVALUATION_PROTOCOL.md) (Generate answers with provenance; Against the bundled sidecar; Shipped-path statistics) and [2026-10-12-SHIPPED-PATH-BASELINE.md](../../evals/retrieval/2026-10-12-SHIPPED-PATH-BASELINE.md#reproduce) |
| Conversation memory | The module comment of `crates/lattice-desktop/tests/conversation_memory_evals.rs` (every variable), and [the memory design](../design/2026-09-19-conversation-memory.md) §17 and §24.6 |
| Lesson claim checker | [evals/grounding/README.md](../../evals/grounding/README.md) (Pairs from your own lessons), [lesson-claims-w1/README.md](../../evals/grounding/lesson-claims-w1/README.md), and [course-generation-testing.md §4](course-generation-testing.md#4-claim-checker-measurement) |
| Course eval | [evals/courses/README.md](../../evals/courses/README.md) and [course-generation-testing.md §7](course-generation-testing.md#7-scored-course-eval) |
