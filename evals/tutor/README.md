# Tutor evals

Scripted learners talking to a real tutor chat, end to end, on a synthetic course. Design:
[`docs/design/2026-10-28-tutor.md`](../../docs/design/2026-10-28-tutor.md) §4. Run it when the tutor's
prompt, tools or `TurnSourcePort` change, not on a schedule.

| File | What it is |
|---|---|
| `course-v1.json` | A course on Tidewell, a job queue that does not exist, so a claim can only come from its passages. Three sources, three modules, six cards. |
| `sessions-v1.json` | The chats. A `say` turn is fixed text; a `play` turn is the learner played by the model under test, answering the tutor's last reply rightly or with a stated misconception. |
| `recipe.example.json` | A recipe for a server on this PC. Copy it and point it at your server; keep a recipe with a remote URL out of the repo. |
| `results/<model>/` | Saved runs. Each row names the model the server said it served. |

## What a run is

`crates/lattice-desktop/examples/tutor_eval` builds `Container::new` on a fresh database, activates the
embedding model through the Settings command, seeds a fresh copy of the course per session through the app's
repositories, indexes it with the real embedding model, makes a tutor chat as the Tutor tab does, and sends each
turn through `run_headless_turn`. So `ChatRuntime::turn_source`, `CourseTurn`, the course's tools, the reading
rounds and the grounding check are the shipped code. After each turn the row keeps what Recall and the Notebook
hold, read through their own repositories.

```text
cd crates/lattice-desktop
cargo run --example tutor_eval -- RECIPE_JSON OUT.jsonl [--only ID,ID] [--keep-data]
python -X utf8 scripts/tutor_eval.py score evals/tutor/course-v1.json OUT.jsonl OUT.summary.json
```

`LLAMA_AUTH_HEADER_NAME` and `LLAMA_AUTH_HEADER_VALUE` carry a server's header, which is sent upstream only and
never recorded. `OUT.jsonl.calls.jsonl` has every request verbatim. `LATTICE_LLAMA_SERVER` names the llama-server
for embeddings, else the build in `crates/lattice-desktop/binaries/`.

The played learner's call goes straight to the server, between tutor turns, so a run's times are not the app's.
Measure speed with a replay, not with this.

## What the scorer checks, and what it cannot

`scripts/tutor_eval.py score` refuses rows that name no model or two, then reports what §4 asks from the rows and
the database: cited numbers exist, the share of claims the grounding check supported, a question within three
turns of teaching, a session with due cards opening with `due_reviews`, the modules those cards span, reviews that
moved a card's schedule, and each session's `expect` block.

Those checks are an upper bound. They cannot tell a good question from a bad one or a true sentence from an
uncited false one, so read every reply before quoting a score.

The rest of §4 lives in the tests: `prompts_outside_the_tutor_are_unchanged` pins the other prompts by digest, and
the explorer's tests cover the port.
