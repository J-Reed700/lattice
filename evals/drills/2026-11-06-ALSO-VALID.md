# Code review drills: issues found in review only credit (2026-11-06)

Every result here is on `claude-haiku-5-5`, on the tree `f920d152` (main at `576183f9` with the branches then open, among
them `fix/drills-also-valid`). The first measurement is [2026-11-05-FIRST-MEASUREMENT.md](2026-11-05-FIRST-MEASUREMENT.md).

## What changed

A drill's key holds planted issues, which are proven, and issues a blind reviewer raised, which a judge only judged real.
In the first measurement those 54 entries were 43% of what the keys hold. 6 restated a planted issue and 3 were wrong, yet
they counted like planted issues, so a learner could be marked down for missing them.
Now a review-found issue only credits: raising it counts, leaving it costs nothing, and restatements of a planted issue
are dropped from the key.

## Grading

Review-found issues are now numbered last, which renumbers 6 of the 40 labelled cases, so the judge saw those issues in a
new order. `check_regression.py` against `results/claude-haiku-5-5/grading/baseline.json`:

| Run | Issues | Findings | Result |
|---|---:|---:|---|
| 1 | 122 of 124 (0.984) | 113 of 115 (0.983) | no regression |
| 2 | 122 of 124 | 113 of 115 | no regression |
| 3 | 122 of 124 | 112 of 115 | no regression |

The floors are 0.959 and 0.964. Of the 6 renumbered cases, 5 had no disagreement in any of the 3 baseline runs or these 3.
The sixth, `28-java-two-findings-that-each-say-half`, has one disagreement on the same issue in all six, so the
renumbering did not cause it. The baseline stays as it is, since the change was not meant to move these numbers.

## Building

32 drills, against the first measurement:

| | First measurement | This run |
|---|---:|---:|
| Drills built | 31 of 32 | 32 of 32 |
| Planted issues proven | 72 | 79 |
| Proven by a test | 31 | 32 |
| Proven by the judge | 41 | 47 |
| Dropped as unproven | 21 | 17 |
| Clearly real, read by hand | 68 of 72 | 75 of 79 |
| Blind review findings | 259 | 250 |
| The judge's false positives | 116 | 115 |
| Findings matching a planted issue | 78 | 87 |
| Valid extras | 65 | 48 |
| Entries added to keys | 54 | 32 |
| Median seconds a drill | 85 | 77 |

All 111 were read by hand. The 4 doubtful planted issues:

- `typescript-01`: a check-then-act claim on a synchronous function.
- `typescript-03`: a lock-order claim on the same kind of code.
- `go-06`: a slice growing in a test double.
- `python-02`: a class attribute nothing reads.

Of the 32 entries added, 2 restate another issue (`python-07` needless allocation, `javascript-03` a second growth) and 1
has a fix that only changes a comment (`javascript-05`). None is wrong.

There are fewer entries because more findings match planted issues, the judge rule on main keeps a claim of a different
problem from counting, and this change drops restatements. An entry can only credit a learner, so having fewer cannot lower
a score.

The bar is 95% of planted issues real. 75 of 79 clear is 94.9%, and 100% if the 4 doubtful all hold; the first measurement
was 94.4% to 100%.

## Records

- `results/claude-haiku-5-5/grading/also-valid-run-1.json`, `-2`, `-3`: the grading reports.
- `results/claude-haiku-5-5/built-drills-2026-11-06/drills.jsonl.gz`, `summary.json`: the build, one drill a row.
