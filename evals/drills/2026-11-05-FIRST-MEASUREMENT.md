# Code review drills: the first measurement

Every result here is on `claude-haiku-5-5`. Grading ran on main `576183f9` and on the judge rule below; the build ran on
`576183f9`. This is the first time the drills were measured, so it is a baseline and shows no regression from anything.

| Design section 6 asks | Bar | Result |
|---|---|---|
| Does grading agree with a careful human? | at least 0.9 | 0.975 to 0.992 on issues, 0.982 to 0.991 on findings, with the rule below |
| Are the planted issues real? | at least 95% | 68 of 72 clearly real, 4 doubtful: 94.4% to 100%, read by hand |
| Is the key complete enough? | a second blind reviewer finds at most 1 more per drill | not measured |
| The same on the 27B | | not measured |

## Grading

40 labelled reviews. Agreement with the labels, per run:

| Prompt | Run | Issues | Findings | Not graded |
|---|---|---|---|---|
| Before the rule | 1 | 119 of 124 (0.960) | 110 of 115 (0.957) | 0 |
| Before the rule | 2 | 119 of 121 (0.983) | 110 of 112 (0.982) | 1 |
| With the rule | 1 | 118 of 121 (0.975) | 109 of 111 (0.982) | 1 |
| With the rule | 2 | 123 of 124 (0.992) | 114 of 115 (0.991) | 0 |
| With the rule | 3 | 121 of 124 (0.976) | 113 of 115 (0.983) | 0 |

Taken together, issues went from 0.971 to 0.981 and findings from 0.969 to 0.985.

**The rule.** Reading the disagreements showed the judge's instructions contradicting each other: a finding on an issue's
lines about a different problem is not about that issue, yet an issue is half found when a finding "blames something
else". Haiku read a false claim on an issue's line both ways, and one answer that credited a finding it had also called a
false positive was refused twice, so that review could not be graded. Now a wrong cause earns half credit only when the
finding describes the issue's own failure, and a claim of a different problem, true or false, earns the issue nothing. The
prompt holds no example from the labelled cases, so the eval still measures the rule and not a memory of its answers.

Runs wrong, per case:

| Case | What it tests | Before (of 2) | With the rule (of 3) |
|---|---|---|---|
| 11 | a false claim on an issue's line | 1 | 0 |
| 20 | false claims on the lines of two issues, deprecation claim | 2 | 2 |
| 20 | the same, redirect claim | 2 | 1 |
| 31 | three wrong claims, every issue missed | 2, one of them ungraded | 1 |
| 39 | a false alarm and a preference | 1 | 0 |
| 28 | two findings that each say half | 2 | 3 |
| 38 | a comment that names no cause | 0 | 1 |
| 34 | a general comment names the overflow | 0 | 1, ungraded |

The half-credit cases the rule could have hurt (02, 04, 13, 16, 22, 23, 24, 33) agreed in all three runs. Three things did
not go the rule's way:

- Case 38 flipped once: "add_event's default argument bothers me", which names no cause, is half found by label and came
  back missed. One run in three is not a pattern, so the wording stays.
- Case 34 was not graded once: the answer held no entry for any finding, twice. That is a different failure from the one
  the rule removed, which did not recur in three runs.
- Case 28 disagrees in every run, before and after. The rule does not touch it, and the case's own notes call its label
  ambiguous: it turns on whether two findings can add up to one.

## Build

32 drills asked for, 8 per language, of about 60 lines and 3 patterns each. About 84 seconds a drill, from 52 to 128.

| Language | Runner | Built | Proposed | Proven by a test | Proven by the judge | Dropped | Added by the blind review |
|---|---|---|---|---|---|---|---|
| Python | built in | 8 | 24 | 15 | 4 | 5 | 9 |
| JavaScript | built in | 8 | 24 | 16 | 3 | 5 | 21 |
| TypeScript | none | 8 | 24 | 0 | 17 | 7 | 13 |
| Go | none | 7 | 21 | 0 | 17 | 4 | 11 |
| **All** | | **31** | **93** | **31** | **41** | **21** | **54** |

`go-08` failed: "`before` is not in internal/report/export.go". One bad edit in one answer, asked again and bad again,
fails the whole drill. Its other two edits could have stood, as a dropped mutation does.

### The planted issues, read by hand

72 proven. 68 are clearly real. These 4 are doubtful:

| Drill | Issue | Why it is doubtful |
|---|---|---|
| python-08 | `security.sql_injection`, critical | It is a real splice into SQL, but on an empty in-memory database whose result nobody reads: nothing is there to reach |
| javascript-07 | `errors.retry_not_idempotent`, major | Real only for a store that keeps a server-side cursor, which this code does not show |
| typescript-08 | `correctness.float_for_exact_values`, major | The helper it changes looks unused |
| go-01 | `errors.swallowed_exception`, minor | The consequence is speculative |

Separately from whether they are real, some planted code does not look like code anyone wrote:
`__import__("sqlite3").connect(":memory:")` in a CSV export, `export_csv.__dict__.setdefault('sent', [])`, and
`if groups.get(note.recipient) !== []`. A reader can spot these by their shape, which trains less than a plausible bug.

### The issues the blind review added, read by hand

54 added, on top of the 72 planted, so 43% of what the drills' keys hold was never proven.

| Verdict | Count |
|---|---|
| Real | 36 |
| Arguable | 8 |
| A real problem on the wrong lines | 1 |
| Restates a planted issue | 6 |
| Wrong | 3 |

Restating a planted issue:

| Drill | Added | Planted | How it got there |
|---|---|---|---|
| python-07 | `review-1` | `planted-1` | The judge rightly called "no test covers this boundary" an extra, and the describer then wrote it up as the planted identity comparison on the same line |
| go-07 | `review-4` | `planted-1` | The same stale page size, on a line inside the plant |
| javascript-01 | `review-3` | `planted-1` | The same null dereference |
| javascript-06 | `review-3` | `planted-2` | The same unbounded retry |
| javascript-06 | `review-1`, `review-2` | `planted-2` | The swallowed errors are that retry's own lines, under another pattern |

Wrong:

| Drill | Added | Why it is wrong |
|---|---|---|
| javascript-02 | `review-2`, check-then-act | `reserve` is synchronous JavaScript with a synchronous store: nothing can run between the check and the save |
| javascript-04 | `review-3`, untested branch | It says the bad-cost paths and the constructor checks never run, and `test/edge_cases.mjs` in the same drill runs them |
| typescript-06 | `review-1`, dead code | The trailing `return false` is what the type checker needs, as its own text admits |

A real problem on the wrong lines: go-07 `review-3`, an unknown sort key falls through with no error. The defect is the
`switch` at lines 53 to 60. The entry points at line 44, because an entry's lines come from where its fix goes.

Arguable: javascript-02 `review-3` (authorisation may sit above this service), javascript-07 `review-1` (needs a store that
repeats a cursor), javascript-08 `review-1` (needs null rows), python-05 `review-2` (a `memoryview` of items wider than a
byte), go-06 `review-1` (since Go 1.23 an unstopped timer is collected), go-07 `review-1` (a real wrap whose outcome is an
empty page), typescript-03 `review-1` (its own text says it reveals nothing), typescript-04 `review-2` (a backward clock
that looks handled on purpose).

**What this means.** The design says such an issue joins the key as "also valid", "so a learner who finds it is credited".
The code counted them as planted issues instead: a miss lowered weighted recall and rated the pattern Again, so a learner
who found every proven bug could be told they did badly on the strength of the 9 entries above. That is a defect in the
drills, not a regression, and it is fixed separately.

## What this did not measure

- A second blind reviewer, so how complete the keys are is unknown.
- The 27B.
- Rapid mode, which is not built.
- Learners. Agreement here is with hand labels on written reviews, not with what helps anyone practise.
