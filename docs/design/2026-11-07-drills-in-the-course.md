# Review drills in the course: every drill is built from one lesson

Date: 2026-11-07. Status: design. The ask: "Either allow the drills to plug into a course to form its drills around,
or move drills into the course like where tutor is. Whatever makes the most sense." The call made here: drills move
into the course, and each drill is built from one of its lessons. It builds on `2026-11-04-code-review-drills.md` and
`2026-11-05-spot-the-errors-drills.md`; the pipeline, the key, grading and the reveal stay as they are.

## 0. What a drill is built from today

- **No course reaches a code drill.** `review::start` gives code no domain and no course (`review.rs:45`). The clean
  stage then takes the least recently used of 38 hard-coded areas (`bank::DOMAINS`: "payments and refunds", "rate
  limiting", …). The original design meant the domain to be "the course's topic" (§2, step 1); the build never wired
  it.
- **The same patterns every time.** `targets()` is always called with no states (`review.rs:56`, `bank.rs:244`). The
  pattern schedule was step 5 of the drills design and was never built. So every drill of a size plants the first
  pattern of each family in catalogue order. Ratings go to `learning_drill_attempts.ratings_json` and are never read.
- **A passage drill knows its course but not its lesson.** The course comes from a dropdown, and the passage is the
  least-drilled paragraph of any of the course's sources.
- **It lives outside the course.** The section sits on the Studio landing page and opens at `/studio?drills=open`.

Nothing in a drill relates to what the learner is studying. The result is generic: idiomatic code about payments with
an off-by-one and a swallowed exception, whatever the course is.

## 1. The decision

**Drills move into the course.** The course workspace's Practice tab gets a third view, **Review drills**, beside
"Written practice" and "Code & simulations". It goes in Practice rather than a new top-level tab beside Tutor because
a drill is practice on the course's lessons, and Practice already heads its views with the module and lesson. The top
bar already holds seven tabs. The Studio landing section, its route and the course dropdown go.

**Every drill is built from one lesson.** That lesson is the next one the lesson schedule offers (§5), or the one the
learner picks. The lesson gives a code drill its subject and some of its defects, and gives a passage drill its
passage. Plugging drills into a course from the Studio would have needed the same per-lesson build. It would only
have added a course picker in a second place, and the drills would still have been away from the lessons they test.

The methods, each fitted to where it acts:
- **Erroneous examples of the lesson's own ideas.** A planted defect that breaks what the lesson teaches is an
  erroneous example of that idea. Finding and explaining it beats studying correct examples alone on delayed tests
  (Große & Renkl, 2007; McLaren, Adams & Mayer, 2015).
- **Retrieval practice and successive relearning per lesson.** A lesson comes back for another drill on its own FSRS
  schedule until its ideas are reliably found (Rawson & Dunlosky, 2011).
- **A pretest for a lesson not studied yet,** with the reveal as the feedback. Attempting first improves later
  learning when feedback follows (Richland, Kornell & Kao, 2009).
- **Interleaving within a drill.** Catalogue patterns keep alternating families (Rohrer & Taylor, 2007), and lesson
  issues sit beside catalogue issues, so the learner has to tell a broken idea from a generic defect.

## 2. The lesson brief

Code builds a brief of the lesson when the build starts and saves it in the first checkpoint (`CleanCode.brief`), so a
resumed build and every later stage see the same text:

- **The course:** title, goal and what the learner already knows (`prior_knowledge`).
- **The module:** title, summary and outcomes.
- **The lesson:** title and objective.
- **Evidence, numbered L1, L2, ….**
  - *A prepared lesson:* its teaching blocks (explanation, worked example, contrast, recap) cut into sentences with
    the core segmenter. A fenced code block counts as one unit. The check, practice and reflection blocks are left
    out because they are questions, not teaching.
  - *An outline lesson:* the course's source passages retrieved for the lesson, each under its source title. The query
    is the lesson's title and objective plus the module's title and outcomes, through `ReferenceCollection` as
    `snapshot_passages` does. It takes as much as the lesson's planned minutes take to read
    (`estimated_minutes` × 238 words a minute, the mean silent reading rate for non-fiction, Brysbaert 2019), within
    what the request's window leaves (`source_room`).

Both courses in the app today are outlines: 18 lessons, none prepared, sources only. So every brief there is source
passages until lessons are prepared. Everything in a brief is data, never instructions, as passages already are.

## 3. Code drills on a lesson

- **Language.** The learner chooses it, and the choice is kept per course. The default is the catalogue language that
  most of the fenced code in the course's prepared lessons is written in, otherwise the last one the learner used.
- **The writer** gets the brief instead of a domain. It writes realistic code in which the lesson's ideas are at work,
  used correctly. `DOMAINS`, `next_domain`, `least_recently_used` and `domain_uses` are removed.
- **Two kinds of planted issue.** A drill of n issues (3, 4 or 5, by size):
  - **Lesson issues:** up to ⌈n/2⌉ defects that each break something the lesson teaches. Each cites the evidence it
    breaks (`lessonSentences`: the evidence numbers) and names a catalogue pattern as its shape: the pattern its defect
    is an instance of, which the key, the proof and grading need. The planter plants as many as the lesson really
    supports, and none when the lesson teaches nothing code can show.
  - **Catalogue issues:** the other n − i, which are exactly the first n − i patterns of the schedule's ranking (§5), as
    today.

  `read()` checks the split. It accepts 0 to ⌈n/2⌉ lesson issues with valid, distinct sentence numbers, and requires
  the catalogue issues to be exactly that prefix of the ranking, with every issue of a different pattern. The schema
  gives `lessonSentences` the range of evidence numbers and gives `pattern` the material's whole pattern list.
- **The proof** of every issue is chosen by its pattern's family, unchanged. A proven lesson issue then gets a
  **lesson check**: a judge is shown the cited sentences, the clean code and the changed code, and answers with
  reasons first, then whether the change breaks what those sentences say and whether the clean code follows them.
  - It fails the check but passed its proof: it stays in the key as a plain issue with no lesson link.
  - It fails its proof: it is dropped, as today.
- **The key.** `KeyIssue` gets `lesson: Option<LessonLink { lesson_id, sentences }>`, a serde default, so old keys
  read as `None`. Each sentence keeps the title of the source it comes from (`LessonSentence { text, source }`),
  because an outline lesson's evidence comes from several sources. Code copies the sentences and their sources from
  the brief by number; the model never copies them. The drill row gets `lesson_id`.
- **Grading.** The judge's task shows the cited sentences under the issue's pattern ("From the lesson: …"). A finding
  that names the broken idea in the lesson's words is then found, not only one that names the pattern.
- **The reveal.** It shows "From the lesson", the sentences and their source, and a link to the lesson.

## 4. Passage drills on a lesson

The passage comes from the lesson:
- *A prepared lesson:* the sources its blocks cite.
- *An outline lesson:* the course's sources.

Either way the passages are ranked by the brief's query. The pick is the best-ranked passage no drill has used. Once
all have been used, it is the one used longest ago, because drilling a lesson again on the same passage is the point
of relearning. The explanation is written with the lesson's title as its topic. Clean review, planting and the source
check are unchanged. If the sources hold no passage for the lesson, that build fails with the reason, and the lesson
is recorded as having nothing to drill from in passages (§7).

## 5. What comes back when: two schedules

Both use recall's FSRS-6 `CardSchedule::review` with no target, and both are written once, in the transaction that
records the grade (`record_graded_in`). An attempt is graded only once (a regrade is only for one that couldn't be
graded), so no review log is needed. SQLite's `BEGIN IMMEDIATE` serialises concurrent gradings' read-modify-writes.

- **Patterns:** one schedule per material (`code:python`, `passage`) and pattern. Every rated issue rates its pattern,
  as `ratings()` does today. `targets()` reads these states.
- **Lessons:** one schedule per course and lesson. The issues tied to the lesson rate it: in code, those with a lesson
  link; in a passage drill, every rated issue. The rating is the floor of their mean, counting Again as 0 up to Easy as
  3. For example, found, found and missed gives Hard. A code drill with no lesson issue rates no lesson. The lesson is
  then recorded as having no code ideas (§7) and is not offered next for code. It stays offered for explanations and
  can still be picked. Without that record the lesson would stay "never drilled" and come first again every time.
- **Pending.** The next build is queued the moment a drill is taken, before it is graded. Without a pending state the
  next drill would repeat the same lesson and the same new patterns. A lesson or pattern is pending in three cases:
  a drill of it is submitted and waiting for its grade, it is ready on a shelf (any shelf of the course, for a lesson;
  any shelf of the material, for a pattern), or it belongs to the drill being taken as the next build is queued. An
  open attempt the learner walked away from is not pending, so its lesson comes back. A ready drill from before this
  change is never served again, so it holds nothing back.
- **The order** (pure functions in core, deterministic):

  | Rank | Lessons (`next_lesson`) | Patterns (`targets`, families interleaved as today) |
  |---|---|---|
  | 1 | due, earliest first | due, earliest first |
  | 2 | never drilled: the current lesson, then completed lessons in course order, then the rest in course order (a pretest) | new, in catalogue order |
  | 3 | rested, soonest due first | rested, soonest due first |
  | 4 | pending | pending |

  Pending is ranked last rather than left out, so a one-lesson course still gets drills. A pending pattern is planted
  only once every other pattern is, even when its family has not been picked in the round. A lesson recorded as not
  fitting the material is left out.

FSRS never schedules a lesson sooner than a day. For a course studied in a day, that means each lesson is drilled once
and then the weakest come back first. A course has no interview date today. When it has one, the lesson schedule
takes it as its target, as a recall deck does: gaps before the target shrink to a share of the time left (Cepeda et
al., 2008).

## 6. The bank, per course

- A **shelf** is a course and a material (code in a language, or passages). It keeps one ready drill. Taking it queues
  the build of the next-up lesson in the same transaction, as today, but only when no other drill of the shelf is
  ready or building. Builds of a shelf run one at a time under the job subject `drill:{program_id}:{material}`.
- **Asking for a lesson** takes the ready drill only if it was built for that lesson. Otherwise a build for the lesson
  is queued and the learner is told it is under way.
- **Drills from before this change** have no lesson and are never served again. Their attempts stay readable. A
  pending build queued before the change fails cleanly with a reason; its spec has no lesson.

## 7. Persistence

One migration:
- `learning_drills.lesson_id TEXT REFERENCES learning_lessons(id) ON DELETE SET NULL`, with an index for the shelf:
  `(program_id, material, language, status, created_at)`. It replaces the bank's older indexes by language, by course
  and by program. The domain-use index goes with `DOMAINS`, and an index on `lesson_id` serves deleting a lesson.
- `learning_drill_pattern_schedules(material, pattern_id, …)`: the `CardSchedule` fields, keyed by
  `(material, pattern_id)`. A row exists once a review has rated the pattern.
- `learning_drill_lesson_schedules(program_id, lesson_id, …)`: the same fields, keyed by `(program_id, lesson_id)`.
  The pair references the lesson within its course, so a lesson of another course cannot be recorded, and the row
  cascades with the course and the lesson.
- `learning_drill_lesson_fits(program_id, lesson_id, material, fits, basis, decided_at)` records whether a lesson can
  be drilled in code or passages.
  - It is written when a drill of the lesson is stored: code fits when it kept a lesson issue, and a passage always
    fits.
  - It is also written when a passage pick finds nothing for the lesson. A code brief with no evidence needs no
    record of its own: its drill is stored with no lesson issue, and that records that code does not fit.
  - `basis` hashes the evidence the decision rested on: the lesson's blocks when it is prepared, and the course's
    active source versions, which a prepared lesson's passages come from too. A record whose basis no longer matches
    is ignored. So preparing the lesson or changing a source asks the question again, and no retrieval runs to find
    that out.

`DrillSpec` gets `lesson_id` and `lesson_issues` (the most lesson issues to plant: ⌈n/2⌉ for code, 0 for a passage).
`patterns` becomes the ranked catalogue targets, n of them. `validate()` requires a course and a lesson for every new
spec.

## 8. Commands and the view

- `start_course_drill { programId, lessonId?, material: code{language} | passage, mode, lines, issues }` replaces
  `start_review_drill`. It answers as today: started, or building.
- `course_drills(programId)` gives:
  - the language suggestion;
  - the next lesson for code and for passages;
  - each lesson with its standing (due, new, rested, pending, no code ideas) and when it is due;
  - the course's recent attempts, to continue or open.
- Submit, result, regrade and `code_review_languages` are unchanged. `review_drill_courses` goes.
- **The view, Practice › Review drills:**
  - The chooser: the lesson ("Next: …" preselected, grouped by module, each lesson with its standing), what to review
    (code in a language, or an explanation of the lesson's sources), the size and the mode.
  - The course's recent drills.
  - The drill itself: today's review, grading and reveal screens, with the lesson named above them.

  The command palette's "Review drills" opens it for the last course opened.

## 9. Measured before it ships

On a copy of the app's data, on Haiku, with every result naming its model:

- 20 code and 10 passage drills across the two courses' lessons, every key issue read by hand.
- Bars:
  - every drill is about its lesson;
  - at least 95% of the lesson links shown really break the cited sentences;
  - at least 95% of planted issues are real, as before.
- Reported: how many lesson issues each drill kept (and how many lessons got none), and how often the lesson check
  failed.
- `live_grading_eval` is re-run, with fixtures that include lesson issues, at the old bar of 0.9 agreement or better.

## 10. Build order

Each step is test first, built by an agent and reviewed line by line, and ends with one integrated build:

1. **Core (pure):** `LessonLink` on `KeyIssue`; the lesson rating; `next_lesson`; pending in `targets`; the language
   of a course's code.
2. **Schedules:** the migration and repositories. Grading writes both schedules in its transaction, and the bank
   reads them for `targets` and `next_lesson`.
3. **Builds from the brief:**
   - the brief, frozen in the first checkpoint;
   - the writer and the planter for lesson issues, and the lesson check;
   - the judge's task, and the passage pick by lesson;
   - `DOMAINS` removed.
4. **The bank per course and the commands.**
5. **The view:**
   - Practice › Review drills, and the reveal's "From the lesson";
   - the palette entry;
   - the Studio section and its route removed.
6. **Measure (§9),** then a proof run in a test build on a copy of the data.

Calls for Josh. The defaults are what gets built unless he says otherwise:
- **Where:** Practice › Review drills (default), or a top-level Drills tab beside Tutor. Moving it is a one-line
  change.
- **The split:** up to half of a code drill's issues come from the lesson, rounded up (default).
- **Code review practicals:** the Practical tab's "code review" kind, which is never graded, stays as it is (default).
  The alternative is to make it open a review drill on the lesson instead.
