# Code review drill evals

What `docs/design/2026-11-04-code-review-drills.md` section 6 asks to be measured before the drills are trusted, and what
was measured. Results live under `results/<model>/`, and every result names its model.

| Eval | What it answers | Bar | How it is read |
|---|---|---|---|
| Grading (`live_grading_eval`) | Does the judge agree with a careful human on 40 labelled reviews? | at least 0.9 on issues and on findings | by machine, and every disagreement by hand |
| Build (`live_build_eval`) | Are the planted issues real? | at least 95% | by hand: nothing scores it |

Write-ups are dated files beside this one. The first is [2026-11-05-FIRST-MEASUREMENT.md](2026-11-05-FIRST-MEASUREMENT.md).

## Run them

Both are ignored tests that make real model requests. Set `LATTICE_HOME` to a scratch folder so a test process never
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

Build, one JSON and one Markdown file per drill, and a summary:

```sh
LATTICE_DRILL_EVAL_OUT=build LATTICE_DRILL_EVAL_LANGUAGES=python,javascript,typescript,go LATTICE_DRILL_EVAL_COUNT=8 \
  cargo test -p lattice-desktop --lib live_build_eval -- --ignored --nocapture --test-threads=1
```

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
