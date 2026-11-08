# Drills in the course: the measurement

Every result here is on `claude-haiku-5-5`, on a copy of the app's data that holds two courses of 9 lessons each. All 18
lessons are outlines, so every drill's brief draws its evidence from passages retrieved from the course's sources. The
bars are those of section 9 of `docs/design/2026-11-07-drills-in-the-course.md`. Grading ran on `6dce47f45`, and nothing
it covers has changed since. Each build ran on the commit named in its table, and each one fixed what the build before
it showed.

| Section 9 asks | Bar | Result |
|---|---|---|
| Does grading agree with a careful human on reviews whose keys hold lesson issues? | at least 0.9 | issues 33 of 33 in all six runs; findings 34 or 35 of 35 (0.971 to 1.000) |
| Is every drill about its lesson? | every drill | code 18 of 20, passages 9 of 10: missed |
| Do the lesson links shown break the sentences they cite? | at least 95% | 14 of 16 (87.5%), 15 of 16 (93.8%) with one arguable: missed |
| Are the planted issues real? | at least 95% | code 62 of 68 clearly real (91.2%), 65 of 68 (95.6%) with the arguable and doubtful; passages 26 of 26 |
| Lesson issues each drill kept, lessons with none, lesson checks failed | reported | 16 in 20 drills; 7 of 18 lessons got none; 18 of 34 checks failed |

The two code drills that are not about their lessons are on lessons where the work is done by people, not programs. The
code fit check, added after these builds, turned down both lessons in each of three runs of a replay, and kept every
lesson labelled as teaching something code can show. It has not run in a build.

## Grading

The lesson set: 10 reviews of code whose keys cite a lesson's sentences. Each is labelled to the rule that a finding
which names the broken idea in the lesson's words is a find. Two rounds of three runs, on the same executable, labels
and prompt:

| Round | Run | Issues | Findings | Not graded |
|---|---|---|---|---|
| 1 | 1 | 33 of 33 | 34 of 35 (0.971) | 0 |
| 1 | 2 | 33 of 33 | 34 of 35 (0.971) | 0 |
| 1 | 3 | 33 of 33 | 34 of 35 (0.971) | 0 |
| 2 | 1 | 33 of 33 | 35 of 35 | 0 |
| 2 | 2 | 33 of 33 | 35 of 35 | 0 |
| 2 | 3 | 33 of 33 | 34 of 35 (0.971) | 0 |

Every issue agreed in every run: the 16 found, the 7 half found and the 10 missed. Two findings disagreed, each in 2
runs of 6:

- Case 07, finding 0 is the lesson rule's own risk. The finding says the lesson's idea, that requests which do not
  depend on each other start together, of a function with a single request whose every later line uses its result. The
  label is a false positive; the judge matched it to the lesson's issue.
- Case 06, finding 2 is a real defect of the clean code that the key does not hold: a word count that splits on one
  space. The label is valid extra; the judge called it a false positive.

The six runs are the set's baseline, `results/claude-haiku-5-5/grading-lesson/baseline.json`.

The first 40 cases, on the same executable, against the baseline of 2026-11-05:

| Run | Issues | Findings | Not graded |
|---|---|---|---|
| 1 | 122 of 124 (0.984) | 113 of 115 (0.983) | 0 |
| 2 | 121 of 124 (0.976) | 113 of 115 (0.983) | 0 |

Neither run is a regression. The judge's only change puts the cited sentences under an issue that has a lesson link, and
no issue of this set has one, so these runs measure noise. Case 28 disagrees in every run, as before. These were right
in every baseline run and wrong here, read by hand:

- Case 15, issue 0, in both runs: a flag argument called a smell with no word on why it matters. The label is half
  found, the judge said found, and the case's own notes call it the least certain label. One to watch.
- Case 06, finding 0 (run 1): a false claim of two calls, called a valid extra.
- Case 30, issue 0 and finding 2 (run 2): on the read error's line, a remark that the reviewer does not like how the
  file is read, which names no cause. Labelled half found; the judge said missed, and called the finding a false
  positive.
- Case 31, finding 0 (run 2): a false claim about `split_once`, which splits at the first `=` only, called a valid
  extra.

The passage set was not re-run. No passage issue carries a lesson link (no passage build made a lesson check), so its
prompt has not changed.

## Code drills

20 drills per build, spread over the two courses' 18 lessons, so the first lesson of each course got two. All at the
Standard size: 80 lines and 4 patterns.

| Build | Commit | Built | Planted | Proven by a test | Proven by the judge | Dropped | Added by the blind review | Lesson checks | Failed | Lesson issues kept | Seconds a drill |
|---|---|---|---|---|---|---|---|---|---|---|---|
| e | `d9825dc62` | 19 of 20 | 76 | 52 | 14 | 10 | 40 | 31 | 10 | 21 | 147 (113 to 205) |
| f | `192fa97e0` | 20 of 20 | 80 | 59 | 9 | 12 | 26 | 34 | 18 | 16 | 139 (98 to 197) |

f differs from e only in the lesson check's text. e's drill 04 failed as go-08 did in the first measurement: one bad
edit ("`before` is not in" the file), asked again and bad again.

An earlier build, on `6dce47f45`, planted no lesson issue in 20 drills. The planter was asked for as many lesson issues
"as the lesson really supports" and for none when nothing the evidence teaches is at work, and it took the second branch
every time. It is now asked where the code follows the lesson, and to plant there first. e is the first build after
that.

**About its lesson.** f: 18 of 20. Drill 16 is partly about its lesson: the lesson is a table of terms to say aloud, and
one of its rows shows in code. Drill 20's lesson is about speaking, not building, and its drill is the module's router.
e missed on the same two lessons.

### Lesson links

Before a drill is stored, each lesson link is checked against the sentences it cites, and only a link that passes is
shown. Every check of f was labelled by hand before its verdict was read. A link is genuine when the sentences state the
rule the edit breaks and the clean code follows. It is arguable when they state it only in spirit, such as a constant
delay against "retries with backoff". It is not genuine when the bug is real but the sentences do not teach it.

| Label | Checks | Passed and shown |
|---|---|---|
| Genuine | 16 | 14 |
| Arguable | 5 | 1 |
| Not genuine | 13 | 1 |
| **All** | **34** | **16** |

Shown: 14 of 16 genuine (87.5%), or 93.8% with the arguable one. Build e, with the old text, showed 21: 13 genuine, 6
arguable and 2 not (61.9%, or 90.5%). The new text asks for its reasons first: what the sentences say, what the code
before the edit does at the place of the edit, and what the edit changes there. It names its answers for what they ask.
The old names were `breaksIt` and `cleanFollowsIt`, and the model read the "it" as the edit: each of e's four false
fails said in its reason that the clean code follows the sentences, then answered that it does not.

- The false pass: the cited sentence names validation as a topic, and the edit rejects the twelfth month. The check's
  own reason says the sentence says nothing about months, then passes it on the word: a topic named, not a rule taught.
- The two false fails: in one, the sentences state an order (process, then save the position) that the edit reverses,
  and the check wanted them to name the loss. In the other, re-read after the verdicts, the edit breaks a type contract
  rather than the idea it cites, so the check may be right; the blind label stands in the tally.

The text was chosen on a replay of e's checks, three runs each, written on drills 02 to 13 and held out on 14 to 20.
Held out, the old text passed 16 of 18 genuine, 3 of 6 not genuine and 7 of 9 arguable; the new one 15 of 18, 0 of 6 and
5 of 9. With the new answer names and the old text, all 6 not-genuine checks held out passed: the strict wording is what
holds precision.

### Lesson issues kept

f kept 16 lesson issues in 20 drills: 9 drills kept none, 6 one and 5 two, the most a drill of 4 patterns plants. 7 of
the 18 lessons got none. Of the 18 failed checks, 12 were on links labelled not genuine, 4 arguable and 2 genuine, so
most failures are the check doing its job. e kept 21 in 19 drills (6 none, 5 one, 8 two), and 5 lessons got none.

### The planted issues, read by hand

f proved 68. 62 are clearly real, 1 arguable, 2 doubtful and 3 not real as stated: 91.2% clearly real, 95.6% with the
arguable and doubtful. e proved 66: 62, 1, 1 and 2 (93.9% to 97.0%). The standard is the first measurement's. Clearly
real means the defect is in the code, and the consequence follows when the code runs as the drill shows, or, for a new
helper, when it is called as its name says. Doubtful means the consequence needs code the drill does not have, or the
drill's own text undercuts it. Not real as stated means the defect the pattern names is not in the code at all.

| Drill | Issue | Verdict | Why |
|---|---|---|---|
| f-07 | planted-4 | arguable | a minimum of 100 items before new labels count is a defensible design |
| f-09 | planted-1 | doubtful | `0.1 + 0.7`: its own consequence says 0.8 still compares right |
| f-11 | planted-1 | doubtful | late-binding closures in a local list nobody reads: the stated consequence cannot happen |
| f-12 | planted-2 | not real as stated | an MD5 "integrity digest" with no key: anyone who tampers can recompute it |
| f-15 | planted-1 | not real as stated | missing CSRF protection in a message consumer: no browser, no cross-site request |
| f-18 | planted-1 | not real as stated | an open redirect on a value nothing redirects to |
| e-01 | planted-2 | arguable | a swallowed parse error that sends the item to review, which fails safe |
| e-09 | planted-1 | doubtful | `0.7 + 0.1`: one value is affected, and the harm is speculative |
| e-12 | planted-2 | not real as stated | MD5 picking one of 12 partitions: any hash can be steered there |
| e-20 | planted-1 | not real as stated | a timing-unsafe comparison of a value that is not a secret |

All five not real as stated are security patterns asked of code with no surface for them. A test that kills the plant
proves the edit changes behaviour, not the consequence it claims. As in the first measurement, some real plants look
planted: new helpers nothing calls (an HTML helper in a schema module, a path joiner, a pickle loader, random tokens, a
TLS context with verification off), `__import__`, `self.__dict__.setdefault`, `is not ''`, `os.popen`.

Some pattern names do not fit their issue: `wrong_operator` for a missing or an extra condition, `stale_state` for a
missing counter update, `mass_assignment` for sending more fields than the purpose needs, a swallowed exception for a
missing audit entry. Four key items of 200 in e and f cite brief line numbers the learner never sees ("L19 and L582").

### The issues the blind review added, read by hand

f added 26:

| Verdict | Count |
|---|---|
| Valid | 18 |
| Arguable | 3 |
| Restates a planted issue | 4 |
| Valid, with a wrong fix | 1 |

The wrong fix uses `setdefault`, which returns the in-flight marker it has just stored, so the fix raises the first time
it sees any key. One restatement proposes deleting the field the plant stopped reading, a harmful fix. As in the first
measurement, a restatement counts against a learner who misses it, until review-found issues only credit.

### The code fit check

Until the code fit check, a code drill decided from its own outcome whether its lesson fits code. A drill that kept no
lesson issue recorded the lesson as having nothing code can show, and the app stopped offering it for code. In f that
was 7 of 18 lessons, and 5 of those 7 teach what code can show; only 3 of the 7 had kept none in e as well. One drill's
outcome is not the lesson's fit.

Now the model is asked before the brief, from the lesson's module, title and objective, whether a program can follow or
break what the lesson teaches. The verdict is stored with its reason and model, and the lesson is not asked about again
until its module, title or objective, or the check's text, changes. A lesson it turns down gets no code drill, and is
still offered for passages. A replay on 42 lessons, three runs each: the 18 above and 24 written for the replay. Each
was labelled by hand before any verdict was read: 28 teach something code can show, 10 do not, 4 are arguable.

| Text | Teach: kept | Do not: turned down | Arguable: kept |
|---|---|---|---|
| First | 81 of 84 | 30 of 30 | 2 of 12 |
| Shipped | 84 of 84 | 30 of 30 | 3 of 12 |

The first text turned down one lesson in all three runs. It is a closing lesson that has the learner draw and defend a
design, and the check read it as a speaking exercise, though its drill in f kept two genuine lesson links. The shipped
text judges the design such a lesson names, not the drawing or the speaking. Of the 18 real lessons it kept the 16
labelled as teaching what code can show, and turned down the lessons of drills 16 and 20, three times each.

### The rating of a lesson

A graded drill rates its lesson, and the lesson's schedule moves only when there is a rating. A code drill rated its
lesson by the issues with a lesson link, so a drill that kept none, 9 of 20 in f, left its lesson never drilled. Now
such a drill rates its lesson by every rated issue, as a passage drill does, since its code was written on the lesson's
subject.

## Passage drills

| Build | Commit | Built | Issues | Dropped | Seconds a drill | About its lesson |
|---|---|---|---|---|---|---|
| e | `d9825dc62` | 10 of 10 | 30 | 2 | 78 (49 to 107) | 6 yes, 1 partly, 3 no |
| g | `d7b1d0501` | 10 of 10 | 27 | 4 | 85 (55 to 154) | 9 yes, 1 partly |

Both builds drilled the same 10 lessons with the same patterns. In e, three drills explained a passage that does not
teach the lesson. Twice it was the learner's own course request, which the course keeps among its sources, and once a
paper's section on another subject. The passage check, new in g, asks the model before the explanation whether the place
teaches the lesson, and a place it turns down gives way to the next. In g it made 50 checks for the 10 drills, each
labelled by hand before its verdict was read:

| Label | Checks | Passed |
|---|---|---|
| Teaches the lesson | 14 | 9 |
| Arguable | 2 | 1 |
| Does not | 34 | 0 |

No place that does not teach the lesson passed. Five that do were turned down, four of them from one paper the check
read as being on a neighbouring subject, a defensible narrow reading. The one drill that is partly about its lesson
passed on the arguable place: the learner's own notes, 4 of 12 sentences on the lesson's subject. One lesson took 27
checks, 26 of them no. Its sources are mostly the learner's request and notes, and overlapping places were checked one
after another.

Planted issues, read by hand: 26 of 26 real in g, and 28 of 28 in e. The blind review added 1 in g, valid, and 2 in e, 1
real and 1 arguable. Of g's 4 drops, 3 are fair. The fourth dropped a real error because the source check read its
sentence without the scope set by the sentence before. e had one false drop too: a plant that turned "X, and Y" into "X
because Y", a link the source never makes. Two misquoted figures give themselves away: a count changed beside a list
that still holds the old number of items.

## Evidence

A code drill's brief cites evidence units, which are sentences from the lesson's sources. Sources hard-wrapped at a
fixed width used to be cut into fragments at every line break. They are read as flowed text since `d9825dc62`. The
briefs of the 19 drills e built hold 11,875 units, where the briefs of the same lessons in the build before held 13,792.
Fragments fell from 33.7% to 24.3% of the units, and whole sentences rose from 49.6% to 60.1%. What is left as
fragments: chunk edges cut mid-word, page chrome, reference lists, tables, and list items without a stop.

## The proof run

This ran in the test build of `6dce47f45`, on a copy of the app's data with Learning on `claude-haiku-5-5`, driven
through Practice › Review drills:

- A code drill of the current lesson was ready after 148 s. It was graded in 5 s ("Graded by claude-haiku-5-5"), and its
  reveal showed 7 issues, 1 with the lesson's sentences. The lesson then showed as due again the next day.
- A passage drill was ready after 392 s, 300 of them in the pick; later commits brought the pick down to seconds. It was
  graded in 3 s and showed 5 issues. It explained the learner's course request, which the passage check now turns down.

## What this did not measure

- The code fit check in a build: only the replay above.
- Prepared lessons. All 18 were outlines, so every brief's evidence was retrieved passages, about 700 units a drill.
- The 27B, or any local model.
- A second blind reviewer, so how complete the keys are is unknown.
- Learners.

## Not fixed here

- The lesson check passes a sentence that names a topic without teaching the rule.
- Security patterns are planted into code that has no surface for them: all five not real as stated.
- Brief line numbers appear in the learner's text: 4 of 200 key items.
- A review-found issue that restates a planted one counts against the learner: 4 of 26 in f.
- Evidence units cross chunk edges, and a chunk that starts inside a code block reads its closing fence as an opening
  one.
- Some pattern names do not fit their issue.
- The passage check asks again about places that overlap ones it has turned down.
