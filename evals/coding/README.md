# Coding session evals

What `docs/design/2026-11-08-coding-sessions-and-one-practice-area.md` section 9 asks to be measured before coding
sessions ship. All are ignored tests that make real model requests, on the Learning model the settings name (Haiku for
test runs). Every item and every `summary.json` names its model. Reading is by hand, in a dated write-up beside this
file, `2026-11-xx-CODING-SESSIONS.md`; one or two items' difference between runs is noise.

| Eval | What it answers | Bar | How it is read |
|---|---|---|---|
| Builds (`live_coding_build_eval`), 30 builds | Do problems prove within the build's re-ask budget? | at least 27 of 30 | by machine: `summary.json` counts the proven builds; their p50 and p90 wall time and model calls are recorded |
| Builds, the statement | Is the statement clear and does it match its cases? | at least 28 of 30 | by hand, from each `build.md` |
| Builds, Easy | Does "Easy" hold? | at least 27 of 30 | by hand, from each `build.md` |
| Builds, the tags | Are the pattern tags right? | at least 27 of 30 | by hand: each `build.md` shows every tagged pattern's name and catalogue cue beside the statement |
| Builds, naming | Does any statement name its pattern? | none of 30 | by machine: `named_in` on the statement; the title is checked the same way and reported apart |
| Builds, kills | Are at least `MIN_KILLED_MISREADINGS` (2) misreadings killed? | every proven build | by machine, per build |
| Hints (`live_coding_hint_eval`), 40 drafts | Does any hint contain code that passes the problem's cases? | none: one blocks the release | by machine: every code block of every stored hint runs as `solution.py` against all cases |
| Hints, on-rung | Is each hint what its rung is (Nudge, Idea, Plan, Line)? | at least 36 of 40 per rung | by hand, from each draft's Markdown |
| Hints, Line | Does the Line hint point at the line that is wrong? | at least 36 of 40 | by hand: the lines it points at are marked in the numbered draft beside a line diff of the draft against the reference |
| Sessions (`live_coding_session_eval`), 20 per setting | Is everything the counterpart says about the code or a run true? | at least 19 of 20 | by hand, from each reply beside the exact text the model saw |
| Sessions, clarifications | Do its answers to clarifying questions agree with the problem's clarifications? | at least 19 of 20 | by hand |
| Sessions, question rule | Does the interviewer's question rule hold in every reply? | every reply | by machine: the setting's `TurnPolicy::check` re-run on each stored reply with the observation it was made on |
| Sessions, Watched | What share of Checkpoint cues does it speak on, and do its interjections read as useful and realistic? | rate recorded; at least 16 of 20 interjections | the rate by machine, the reading by hand |
| Debrief, evidence | Is all evidence valid? | every item | by machine, checked independently of `debrief.rs` |
| Debrief, points | Are rows' points within one of a careful grader? | at least 85% of rows | by hand, from each session's debrief beside its steps |
| Latency | p50 and p90 for a counterpart turn, a hint per rung, a run by scope, after a warm-up run, and finish | recorded; p50 of a warm Run examples | by machine, in `summary.json` |

The pattern-rating rule is covered by unit tests, not by these evals.

## What the evals use

- The app's own code: the builds go through `GeneratedProblems`, and the sessions through `CodingSessions` with
  `LiveAids`, the debriefer and `start_coding_run`, over an in-memory database with a generic program and the embedded
  Python runtime. Only the output code is the eval's own.
- The builds carry no lesson, unlike the app's, which take one from the course. Build `i` is Python, Easy,
  `CodingTask::Problem`, with the one pattern `PROBLEM_PATTERNS[i % len]`, so the catalogue is covered in order.
- Evals 2 and 3 read the proven problems of a builds run, so run the builds first.

## Run them

Set `LATTICE_LLAMACPP_SETTINGS` to the settings file the tests read, with `LATTICE_USE_LEARNING_MODEL=1` so they use the
Learning model it names, as the app does. Set `LATTICE_HOME` to a scratch folder so a test process never touches a real
home. `LATTICE_CODING_EVAL_OUT` is the output root; each eval writes under `<root>/<eval>/`.

| Variable | Used by | Meaning |
|---|---|---|
| `LATTICE_LLAMACPP_SETTINGS`, `LATTICE_USE_LEARNING_MODEL` | all | the model |
| `LATTICE_CODING_EVAL_OUT` | all | the output root |
| `LATTICE_CODING_EVAL_COUNT` | builds | the number of builds (default 30) |
| `LATTICE_CODING_EVAL_PROBLEMS` | hints, sessions | the `builds` output folder of a run of the builds eval |
| `LATTICE_CODING_EVAL_HINT_DRAFTS` | hints | the number of wrong drafts (default 40); at most ceil(drafts / 10) come from one problem |
| `LATTICE_CODING_EVAL_SESSION_PROBLEMS` | sessions | the number of problems, five learner levels each (default 4, which is 20 scripts per setting) |

Builds. Writes `<root>/builds/<nn>/build.json` and `build.md` for each build, `problem.json` (the stored form) for each
proven one, and `<root>/builds/summary.json`: the model, proven out of N, p50 and p90 of wall time and of model calls per
build, the statements and titles that name a pattern, the builds that kill too few misreadings, and the outcome per
pattern. A build's Markdown has the statement, every case, the clarifications, each tagged pattern's name and cue, the
numbered reference, each misreading with its numbered code and the cases that kill it, and the target complexity.

```sh
LATTICE_CODING_EVAL_OUT=run-1 LATTICE_LLAMACPP_SETTINGS=settings.json LATTICE_USE_LEARNING_MODEL=1 \
  cargo test -p lattice-desktop --lib live_coding_eval::builds::live_coding_build_eval -- --ignored --nocapture --test-threads=1
```

Hints. Each killed misreading's code is a wrong draft, spread over at least 10 problems, put through a Coached session
and asked for a hint at each rung. Writes `<root>/hints/<nn>.json` and `.md` per draft and `summary.json`: per rung, the
hints given, the errors and p50 and p90 ms, and any hint whose code passes every case, which is a release blocker.

```sh
LATTICE_CODING_EVAL_OUT=run-1 LATTICE_CODING_EVAL_PROBLEMS=run-1/builds LATTICE_LLAMACPP_SETTINGS=settings.json LATTICE_USE_LEARNING_MODEL=1 \
  cargo test -p lattice-desktop --lib live_coding_eval::hints::live_coding_hint_eval -- --ignored --nocapture --test-threads=1
```

Sessions. Five scripted learner levels, strong to weak, each run in Coached, Mock and Watched. Script text comes only
from the problem's own data. Writes `<root>/sessions/<setting>-<nn>.json` and `.md` per session and `summary.json`:
per setting the sessions, replies, silences and errors; for Watched, the Checkpoint cues and how many it spoke on; the
question-rule failures and invalid evidence items; and the latency of `respond`, `ask_hint`, `run` and `finish`.

```sh
LATTICE_CODING_EVAL_OUT=run-1 LATTICE_CODING_EVAL_PROBLEMS=run-1/builds LATTICE_LLAMACPP_SETTINGS=settings.json LATTICE_USE_LEARNING_MODEL=1 \
  cargo test -p lattice-desktop --lib live_coding_eval::sessions::live_coding_session_eval -- --ignored --nocapture --test-threads=1
```
