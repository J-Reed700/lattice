# Code review drill evals

What `docs/design/2026-11-04-code-review-drills.md` section 6 asks to be measured before the drills are trusted, and what
was measured. Results live under `results/<model>/`, and every result names its model.

| Eval | What it answers | Bar | How it is read |
|---|---|---|---|
| Grading (`live_grading_eval`) | Does the judge agree with a careful human on 40 labelled reviews? | at least 0.9 on issues and on findings | by machine, and every disagreement by hand |
| Grading on lessons (`live_grading_eval`, set `drill_grading_lesson`) | Does it agree on 10 reviews of code whose issues cite a lesson's sentences, where naming the broken idea in the lesson's words is a find? | at least 0.9 on issues and on findings | by machine, and every disagreement by hand |
| Build (`live_build_eval`, `live_passage_build_eval`) | Are the planted issues real? | at least 95% | by hand: nothing scores it |
| Build, the lesson | Is every drill about its lesson? | every drill | by hand, from each drill's brief and Markdown |
| Build, the lesson links | Does each lesson link really break the sentences it cites? | at least 95% of the links shown | by hand, from the sentences under each key issue and the lesson checks |

Write-ups are dated files beside this one. The first is [2026-11-05-FIRST-MEASUREMENT.md](2026-11-05-FIRST-MEASUREMENT.md).

## Run them

All are ignored tests that make real model requests. Set `LATTICE_HOME` to a scratch folder so a test process never
touches a real home. `LATTICE_LLAMACPP_SETTINGS` names the settings file the tests read, and with
`LATTICE_USE_LEARNING_MODEL=1` they use the Learning model it names, as the app does.

Grading, one report per run:

```sh
LATTICE_DRILL_GRADING_OUT=run-1.json cargo test -p lattice-desktop --lib live_grading_eval -- --ignored --nocapture --test-threads=1
```

The same eval runs over the cases on passages (27 reviews of explanations of an invented source, labelled to the judge's
rules for a passage; the bar of section 7 of `docs/design/2026-11-05-spot-the-errors-drills.md`) when
`LATTICE_DRILL_GRADING_SET=drill_grading_passage` is set. It reads `tests/fixtures/drill_grading_passage` instead, and
its report is compared only with another run of that set.

The lesson cases (10 reviews of code, each derived from a case of the first set, with a lesson link on one or two issues of
its key) run when `LATTICE_DRILL_GRADING_SET=drill_grading_lesson` is set. They are labelled to the judge's rules for code and
to the design's rule that a finding naming the broken idea in the lesson's words is found, not only one that names the
pattern. Their report is compared only with another run of that set; the bar is the old one, 0.9 agreement:

```sh
LATTICE_DRILL_GRADING_SET=drill_grading_lesson LATTICE_DRILL_GRADING_OUT=lesson-1.json \
  cargo test -p lattice-desktop --lib live_grading_eval -- --ignored --nocapture --test-threads=1
```

Build, one JSON and one Markdown file per drill, and a summary. Each drill is built from a lesson of a course, on a copy of
the app's data (never the live database: the drills are stored in it). `LATTICE_DB_COPY` is the copy and `LATTICE_COURSES` a
comma list of course ids. The drills are spread over the courses in turn and, in each course, over the lessons the app offers
for the material, in order. `LATTICE_DRILL_EVAL_LINES` and `LATTICE_DRILL_EVAL_PATTERNS` default to the app's Standard size
(code: 80 lines and 4 patterns; passage: 10 sentences and 3 patterns). Code takes each course's own language, or
`LATTICE_DRILL_EVAL_LANGUAGE` (one catalogue language) for every course. The design asks for 20 code and 10 passage drills
across the lessons of two courses:

```sh
LATTICE_DB_COPY=copy.sqlite LATTICE_COURSES=course-1,course-2 LATTICE_DRILL_EVAL_OUT=build-code LATTICE_DRILL_EVAL_COUNT=20 \
  cargo test -p lattice-desktop --lib live_build_eval::live_build_eval -- --ignored --nocapture --test-threads=1
LATTICE_DB_COPY=copy.sqlite LATTICE_COURSES=course-1,course-2 LATTICE_DRILL_EVAL_OUT=build-passage LATTICE_DRILL_EVAL_COUNT=10 \
  cargo test -p lattice-desktop --lib live_passage_build_eval -- --ignored --nocapture --test-threads=1
```

`summary.json` names the model and the courses and lists each drill with its course, lesson and counts. For code it totals the
lessons drilled, those none of whose drills kept a lesson issue, and the lesson checks made and failed. Nothing scores the build's
bars: read every key issue, and every lesson link against the sentences it cites, in the Markdown.

## Check a change for a regression

Run the grading eval on the changed code and compare its report with the baseline of the same model. Exit 0 is no
regression, 1 a regression, and 2 means the reports cannot be compared, for instance when the models differ:

```sh
python -X utf8 -I evals/drills/check_regression.py check evals/drills/results/claude-haiku-5-5/grading/baseline.json run-1.json
```

A model's answers vary, so a drop counts only past the noise: 2 items, or the spread of wrong items between the baseline's
runs when that is wider. An item every baseline run got right and this run got wrong is listed to read by hand. A change
that is meant to move the numbers ends with a new baseline, made from at least three runs:

```sh
python -X utf8 -I evals/drills/check_regression.py baseline run-1.json run-2.json run-3.json > baseline.json
```

The checker's own tests: `python -X utf8 -I evals/drills/test_check_regression.py`.
