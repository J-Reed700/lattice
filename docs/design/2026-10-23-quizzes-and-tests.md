# Quizzes and tests: one assessment system, in every lesson, module and course

Status: design, 2026-10-23, written against main `c694fc79`. Nothing here is
implemented. Three builder tracks, section 8.

The owner's ask, verbatim: *"a really really beautifully architected
breathtakingly amazing quiz and test solution where each lesson there could be
like a quiz, module or something and it has its own dedicated space with
questions and then you can either have a textural answer. Maybe it's
multiple-choice there should be all types and then you can take it and then try
to pass and if you fail and repeat … integrated within this app."*

## 0. Decisions at a glance

| Question | Decision |
|---|---|
| Which system survives | The assessment engine (`features/learning/assessment/`: blueprints, immutable forms, private keys, autosave, idempotent submit, unseen-first retakes, rubric grading, evidence). It becomes *the* quiz system. |
| What is deleted | "Quick checks" end to end: `learning_questions`, `learning_answer_keys`, `learning_attempts`, `LearningQuestionDto`/`LearningAttemptDto` and friends, `submit_learning_attempt`, `AssessmentPanel.tsx`, `AttemptReview`, `moduleTestCoverage.ts`, `ModuleCheckpoint.tsx` (the only blueprint author today), the Practice → "Quick checks" and "Assessments" sub-tabs. Full list in section 6.6. |
| Where quizzes live | A new primary Studio tab, **Quizzes**, beside Lessons and Practice: one overview of every lesson quiz, module test and the course exam. The same quizzes are reachable from the end of each lesson card and from the module milestone strip. |
| Three kinds | **Lesson quiz** (purpose `checkpoint`), **Module test** (`module_test`), **Course exam** (`cumulative`). Any of them can be run in **Practice mode** (ungraded, feedback per item). `transfer` stays in the schema, unsurfaced. |
| Who writes blueprints | The backend, from the course: `scope → blueprint` is a pure function of the prepared lessons and module outcomes. The frontend never builds a blueprint again. |
| When items are made | A `quiz_bank` stage at the end of lesson preparation (after claim verification) authors the lesson's bank; module and course banks are prepared on first open, as a job with progress. Any bank can be topped up later. The lesson's six reviewed MCQs from the prose step seed its bank. |
| Question types (v1) | single choice, multi-select, true/false, short answer (text variants, numeric tolerance), cloze, matching, ordering, explanation (rubric). Section 3. |
| Grading | Deterministic for seven of eight types, with explainable partial credit. Explanation and short-answer near-misses are model-graded with a rubric, quotes, and honest uncertainty, as a stoppable job. A grader failure is "couldn't grade yet", never a fail. Section 4. |
| Pass and fail | Score ≥ pass mark (quiz 70 %, test 75 %, exam 75 %, editable per course). Pass/fail is state and feedback only; nothing locks. Best attempt counts as the status; latest is shown beside it. |
| Fresh retakes | The bank holds three forms' worth per requirement; retakes prefer unseen items, reshuffle option order, label any repeat in words, and offer "Prepare more questions". |
| Why it was marked wrong | Every answer that loses points says why. Model-graded answers carry a structured rationale: a plain summary, per-criterion verdict, what was missing, the learner's own words it rests on (verbatim, checked), what full credit includes, and the source passage. A grade the model cannot explain is "Couldn't grade yet", never "Not correct". Section 4.3a. |
| Contest | "I think I was right" records a note, triggers a blind regrade, and stores both verdicts. After an upheld regrade the learner may override; the override is recorded, shown on the score, and never becomes `observed` evidence. |
| Missed items | One click adds missed items to the FSRS recall deck as typed cards (Q/A or cloze), with the item as origin. |
| Storage | One new migration `20261024120000_quizzes_and_tests.sql`: widen the format and grade-status checks in place (the `writable_schema` procedure already used by `20261016100000`), add scope columns, a tagged `key_json`, option order, contests, overrides, bank jobs; drop the quick-check tables. No data migration of old attempts; prepared lessons' reviewed questions are carried into the bank in SQL. |
| Packs | v6: `program/attempts.json` and lesson `questions` leave the pack; the evidence section (which already carries blueprints, candidates, keys, forms, submissions) gains the new tables. Quizzes and attempts export and import. |
| Eval | The course eval gains a `quiz` stage per lesson (mechanical item checks + judged Q1–Q5), a grading set with a blind calibration slice (PPI), and partial-credit unit tests. No parallel harness. Section 7. |

## 1. Why the engine wins, and what the survey found

Two systems exist today, both two levels deep under Studio → Practice.

*Quick checks* (`src/features/learning/assessment/AssessmentPanel.tsx`,
`ProgramWorkspace.tsx:293`) show the six MCQs the lesson prose step authors
(`lessons/generation/lesson.rs:96`, `kind` practice/quiz/test, two each),
pooled per module for a "module test" (`moduleTestCoverage.ts`). They have no
pass mark, no verdict, and a retake shows the same six questions. Their one
real asset is the blinded answer-key review (`lessons/answer_review.rs`), which
independently solves each question and disputes the authored key.

*Assessments* (`features/learning/assessment/`, ~3.5 k lines) have everything a
test needs and nothing a learner can reach: the only creation path is
`ModuleCheckpoint.tsx`, which hard-codes a rubric, explanation and artifact
items, and `passingScore 0.7`. Four of five purposes always read "No accepted
form yet". The hub shows a percentage, no verdict.

Verified facts this design relies on:

- Keys never cross IPC: `LearningAssessmentCandidate.key` is internal, form DTOs
  carry prompts and options only (`dto/assessment.rs` header comment,
  `assessment_engine.rs:75–97`).
- Forms are immutable, responses are revisioned with `operation_id`
  idempotency, and `start_form` prefers unseen candidates and labels
  `previously_exposed` (`assessment_repository/forms.rs:15–120`,
  `assessment_engine.rs:351–420`).
- Deterministic grading covers Choice, TextVariants and Ordering as correct or
  not, one point each (`assessment_engine.rs:434–480`). Open items go through
  `assessment_generation::grade_open` with rubric criteria, quotes checked
  against the submitted text, and `Uncertain` when any score is null.
- Submission writes `learning_evidence_events` per outcome and dimension and
  raises follow-ups (`assessment_repository/submission.rs:200–300`).
- Outcomes are module-level only: `curriculum_repository.rs:157` inserts with
  `lesson_id NULL`. Lesson quizzes need lesson-level outcomes (section 5.3).
- Generation is already window-sized, not clock-sized: `Batch::room` halves the
  batch until it fits (`assessment_generation.rs:196–240`), through
  `model_call::{structured, send, fits, source_room}`. No timeouts exist there
  and none are added.
- Course delete cascades through every assessment table since
  `20261016100000_learning_delete_cascades.sql`, which edits `ON DELETE` text
  in place with `PRAGMA writable_schema` because sqlx runs migrations in a
  transaction where `foreign_keys=OFF` is a no-op.
- Packs (v5) already export the whole assessment table set
  (`portability/pack_repository/evidence_tables.rs:43–54`) and, separately, the
  quick-check `attempts.json` and lesson questions
  (`pack/sections.rs:9`, `pack_repository/export.rs:144`).
- The course eval scores lesson questions with L5 (one defensible key,
  plausible distractors) and reads `learning_answer_keys` directly
  (`lessons/live_course_eval_stages.rs:597`).

The engine is the right base: it has the properties the owner asked for (keys
hidden, immutable attempts, retakes that avoid repeats, honest grading status)
and lacks only authorship, pass/fail, richer types and a front door. Quick
checks have a front door and nothing else. So the quick-check tables and panels
go; the lesson prose step keeps authoring its six questions (they are how the
lesson's reasoning is checked) but they land in the item bank.

## 2. Product and UX

### 2.1 Where quizzes live

```
Studio › Program
┌──────────────────────────────────────────────────────────────────────────┐
│ Lessons   Practice   Quizzes   Recall   Notebook   Sources        More ⌄ │
└──────────────────────────────────────────────────────────────────────────┘
```

Three doors to the same thing:

1. **Lesson card, after the last block** – a "Lesson quiz" callout with its
   status in words and one button (Start quiz / Resume / Retake / Prepare
   questions). Finishing a lesson naturally lands on it.
2. **Module milestone strip** (the `01 Learn · 02 Apply · 03 Check · 04 Build`
   row on the Lessons tab) – `03 Check` opens the module test.
3. **Quizzes tab** – the dedicated space: course exam at the top, then each
   module with its test and its lessons' quizzes, every row with a verdict in
   words, best and latest scores, and attempts. The right column shows the
   outcome-evidence timeline and follow-ups that live in
   `AssessmentEvidencePanel` today.

Deep links: `StudyDestination.tab = 'quizzes'` with `formId` (resume) or
`lessonId`/`moduleId` (open that quiz). The command palette's `assessment` word
maps here. The coach's "Resume your assessment" step (`courseProgress.ts`)
keeps working because forms are unchanged.

### 2.2 Rules the UX follows

- **Nothing locks.** Every quiz, test and exam can be started in any order,
  before or after its lessons, with the coverage said in words ("Covers 2 of 5
  lessons in this module; lessons you prepare later add their questions").
- **State in words, never colour alone.** Verdicts are "Passed · 85 %",
  "Not passed yet · best 60 %", "Not taken", "Result pending · 2 written
  answers grading", "Paused · resume or start fresh". Icons accompany words.
- **Keys appear only after submission** (graded mode). Practice mode reveals
  per item after the learner answers it, and records practice evidence, not
  assessment evidence.
- **Leaving is safe.** Responses autosave per item (existing). Leaving mid-way
  leaves the form active; the callout and overview show Resume.
- **No app errors on the learner.** Grader failures read "We couldn't grade
  this answer yet" with Retry. A window that is too small reads in the user's
  words with the fix (`model_call::window_too_small`). An interrupted attempt
  is "Paused", never failed.
- **Keyboard first.** Roving tabindex on the question rail (as
  `AssessmentPanel` does today), radiogroups and checkbox groups, native
  `<select>` for matching, Move up / Move down buttons plus Alt+↑/↓ for
  ordering, labelled inputs for cloze, focus moved to the result heading on
  submit, `aria-live` for autosave and grading status, alert dialogs for
  submit and pause (existing pattern).

### 2.3 Defaults (and what is configurable)

| Setting | Lesson quiz | Module test | Course exam | Where |
|---|---|---|---|---|
| Pass mark | 70 % | 75 % | 75 % | Course quiz settings (three numbers, one card) |
| Form size | 7 items, plus 1 from the lesson before and 1 from a lesson further back | 3 per prepared lesson + 1 per module outcome | 1 per lesson + 1 per module | derived, shown, not edited |
| Formats | 3 single choice, 1 structure item (multi-select, matching or ordering), 1 short answer, 1 cloze, 1 explanation; the two earlier-lesson items are single choice from those lessons' banks. A lesson whose blueprint already asks the earlier mix (4 single choice, 1 true/false, 1 structure item, 1 short answer or cloze, 1 explanation) keeps it | 2 keyed + 1 short answer per lesson; explanation per module outcome | keyed per lesson; explanation per module | derived |
| Written answers | included | included | included | Course quiz settings: "Include written answers (graded by your model)" on/off; off removes explanation items and short-answer model fallback |
| Feedback | after submission | after submission | after submission | Practice mode gives per-item feedback |
| Expected minutes | derived: 1 min keyed, 2 min short answer/cloze, 6 min explanation | | | shown on the start screen |
| Bank size | 3 × form requirement per (outcome, format) | | | derived; "Prepare more questions" adds one more form's worth |

No per-item timers and no attempt limits.

Why this mix (2026-10-26): a mix of formats, with more items where the
learner produces the answer. Short-answer practice with feedback outlasted
multiple choice in Kang, McDermott & Roediger (2007) and McDaniel, Roediger &
McDermott (2007); choice items with competitive distractors also drive
retrieval (Little et al. 2012; Adesope et al. 2017), so the mix keeps three.
Two typed items replace a true/false item, a coin-flip guess, and one
single-choice item. The earlier-lesson items bring
back material the learner answered correctly, which the warm-up's due cards
(misses and rated checks) never return; retrieval spaced across sessions and
mixed with new material is what keeps it (Rawson & Dunlosky 2011; Rohrer,
Dedrick, Hartwig & Cheung 2020). They are drawn from the earlier lessons' own
banks, so nothing new is authored, and the quiz's instructions say why they
are there.

### 2.4 Screens

**Lesson-end quiz callout** (inside `LessonCard`, under the last block, above
"Mark lesson complete"):

```
──────────────────────────────────────────────────────────────────────
 ✓ LESSON QUIZ                                           9 questions · ~15 min
 Check what stuck from "Enzyme kinetics"
 Three choice questions, a matching task, a short answer, a fill-in,
 one written explanation, and two questions from earlier lessons.
 Keys are shown after you submit.
                                                      [ Start quiz → ]
 Best: not taken yet                      Practice first (ungraded) ›
──────────────────────────────────────────────────────────────────────
```

Variants of the status line: "Preparing questions… step 2 of 3 (writing short
answers)" with the lesson job line; "Questions not prepared yet · [Prepare]";
"Paused on question 5 · [Resume]"; "Passed · 88 % on 23 Oct · [Retake with
fresh questions]"; "Not passed yet · best 63 % · [Retake]".

**Quiz runner** (replaces the tab content; header band uses the Studio parchment
band `bg-[#eee8df] dark:bg-[#28251f]`, serif title, uppercase accent label):

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ← Quizzes                                                                │
│ LESSON QUIZ · ENZYME KINETICS                       5 of 8 answered      │
│ Check what stuck                                    Autosaved as you go  │
│ Keys are shown after you submit. You can pause and resume any time.      │
├──────────────────────────────────────────────────────────────────────────┤
│ (1)(2)(3)(4)(5)[6](7)(8)        ← rail: answered = ✓, current = filled   │
│                                                                          │
│ QUESTION 6 · MATCHING                                                    │
│ Match each inhibitor type with its effect on the Lineweaver–Burk plot.   │
│                                                                          │
│   Competitive        [ Choose… ▾ ]                                       │
│   Non-competitive    [ Same x-intercept, higher slope ▾ ]                │
│   Uncompetitive      [ Choose… ▾ ]                                       │
│                                                                          │
│ Based on: Lehninger ch. 6 (citations toggle respects the global switch)  │
├──────────────────────────────────────────────────────────────────────────┤
│ ‹ Previous                                   Next ›   [ Submit answers ] │
│ Answer all 8 questions before submitting · Pause                         │
└──────────────────────────────────────────────────────────────────────────┘
```

Per-type bodies: single choice and true/false are a radiogroup of pill rows
(existing style); multi-select is a checkbox group with "Choose all that
apply"; short answer is one input with "Numbers: 3.2 or 3.2 mM both work" when
the key is numeric; cloze renders the prompt with inline labelled inputs
("Blank 1 of 2"); ordering is a list with Move up / Move down buttons; the
explanation is a textarea with a visible rubric summary ("Graded on: accuracy,
reasoning, example") and a word count, no minimum.

Submit opens the existing alert dialog ("Your responses will be frozen and
graded. You can start a clearly labelled retake later."). Pause keeps the form
active.

**Result screen**:

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ← Quizzes                                                                │
│ RESULT · LESSON QUIZ · ENZYME KINETICS                                   │
│                                                                          │
│   ✓ Passed                                   Pass mark 70 %              │
│   85 %  ·  6.8 of 8 points                   Attempt 2 · 23 Oct 2026     │
│   Written answer: provisional (graded by Qwen3.5-9B). Keyed answers:     │
│   graded against the form. This is evidence from one attempt, not a      │
│   claim of mastery.                                                      │
│                                                                          │
│   [ Add 2 missed questions to Recall ]   [ Retake with fresh questions ] │
│   [ Mark lesson complete ]                                               │
├──────────────────────────────────────────────────────────────────────────┤
│ ✓ 1 · Correct · Which step is rate-limiting…        Source: Lehninger §6.3│
│ ✗ 3 · Not correct · You chose B; the key is D because …                  │
│       Why the others fail: A … · B … · C …              [Add to Recall]  │
│ ◐ 6 · Partly correct · 2 of 3 pairs matched                              │
│ ~ 8 · Provisional · 3 of 4 rubric points                                 │
│   ┌ Why this lost points ─────────────────────────────────────────────┐  │
│   │ You explained how the inhibitor binds, but gave no concrete       │  │
│   │ example, which the question asked for.                            │  │
│   │ Your answer: …the [¹inhibitor binds the ES complex] so…           │  │
│   │ ✓ Accuracy · met · 2/2 — rests on ¹ (your words)                  │  │
│   │ ✗ Example · missed · 0/2 — no example given. Full credit names    │  │
│   │   a real inhibitor and its effect, e.g. as in Lehninger §6.3.     │  │
│   │ Graded by Qwen3.5-9B · provisional                                │  │
│   └───────────────────────────────────────────────────────────────────┘  │
│       [ I think I was right ]   [ Add to Recall ]                        │
│ ⏳ (when a written answer is still grading) · Grading… · Stop            │
│ ⚠ (when the grader failed) · Couldn't grade yet · [Retry grading]        │
└──────────────────────────────────────────────────────────────────────────┘
```

Verdict words: Correct · Not correct · Partly correct (n of m) · Provisional ·
Uncertain · Needs review · Grading… · Couldn't grade yet · Overridden by you.
Each wrong item shows the key, the explanation, why each distractor fails
(single choice), and the source line (respecting the citation toggle).

Course and module tests show "Covers 4 of 5 lessons" in the header; a result
while written answers are pending shows "Result pending · keyed answers 71 %"
with no pass/fail verdict until grading completes.

**Quizzes overview** (the tab):

```
┌──────────────────────────────────────────────────────────────────────────┐
│ QUIZZES & TESTS                                   Settings · Prepare all │
│ Every lesson has a quiz; every module a test; the course an exam.        │
│ Nothing is locked. Pass marks: quiz 70 % · test 75 % · exam 75 %.        │
├──────────────────────────────────────────┬───────────────────────────────┤
│ COURSE EXAM                              │ OUTCOME EVIDENCE              │
│ Not taken · covers 7 of 12 lessons       │ Recall 14 signals · Explanation│
│ [ Prepare exam ]                         │ 3 · Application 1 · Transfer 0 │
│                                          │ …timeline (existing)…         │
│ MODULE 1 · Foundations                   │                               │
│  Module test   Passed · 80 % (best)      │ NEXT STEPS                    │
│                latest 72 % · 3 attempts  │ …follow-ups (existing)…       │
│   1. Cells     Passed · 88 %   [Retake]  │                               │
│   2. Enzymes   Not passed · best 63 %    │                               │
│                [Retake]                  │                               │
│   3. Membranes Not taken     [Start]     │                               │
│   4. Transport Questions not prepared    │                               │
│                [Prepare]                 │                               │
│ MODULE 2 · …                             │                               │
└──────────────────────────────────────────┴───────────────────────────────┘
```

Program header progress gains a second line, "Quizzes passed 5 / 12", beside
"Lessons complete".

### 2.5 The full flow

start (callout, strip or overview) → bank ready? (else prepare job with
progress; the start button waits) → form built (unseen first, options
shuffled) → answer with autosave, pause/resume → submit (alert dialog) → keyed
items graded at once; written answers graded as a stoppable job → result with
verdict in words → review every item with key, explanation, source → contest
or override a written grade → add misses to Recall → retake (fresh items, new
form, old attempt kept) → overview and lesson card show best and latest.

## 3. Question types

`LearningItemFormat` becomes: `single_choice` (renamed from `multiple_choice`),
`multi_select`, `true_false`, `short_answer`, `cloze`, `matching`, `ordering`,
`explanation`, `artifact` (kept for the practical path, not authored by quiz
blueprints). `LearningAssessmentKey` (internal, never over IPC) becomes a
tagged enum stored as `key_json`:

| Format | Key (internal) | Response | Grading path | Partial credit | Renderer |
|---|---|---|---|---|---|
| single_choice | `Choice(usize)` | `selected_indices: [i]` | deterministic | none | `ChoiceItem` (radiogroup) |
| true_false | `Choice(0\|1)`, options fixed `["True","False"]` | `selected_indices` | deterministic | none | `ChoiceItem` with two rows, statement as prompt |
| multi_select | `Choices(BTreeSet<usize>)`, 2–3 of 4–6 correct | `selected_indices` | deterministic | `max(0, hits − wrong picks) / |key|`, said as "2 of 3 right, 1 wrong pick" | `MultiSelectItem` (checkbox group) |
| short_answer | `Text { variants: Vec<String>, numeric: Option<{value, tolerance, unit_variants}> }` | `text` | deterministic normalisation (trim, case, whitespace, punctuation, diacritics, leading article, thousands separators; numeric parse with tolerance and unit variants; the tolerance is absolute, in the key's unit). A response that parses as a number is graded deterministically either way, so a wrong number is wrong with a code-written reason ("You gave 5; the answer is 7 ± 0.5") and no model call. An empty response scores 0 with the reason "No answer given" and no model call. On no match and non-empty non-numeric text: model equivalence judge → `Provisional` correct/incorrect with a reason quoting the answer | none (correct or not) | `ShortAnswerItem` |
| cloze | `Cloze(Vec<Text>)`, one per `{{n}}` blank | `blank_responses: Vec<String>` | deterministic per blank, same normalisation; no model fallback | blanks right / blanks | `ClozeItem` |
| matching | `Matching(Vec<usize>)` right index per left, options carry `left` and `right` lists; `right` may hold one extra distractor | `matches: Vec<Option<usize>>` | deterministic | pairs right / pairs | `MatchingItem` (selects) |
| ordering | `Ordering(Vec<String>)` | `ordered_values` | deterministic | correctly ordered pairs / all pairs (Kendall), said as "7 of 10 pairs in order" | `OrderingItem` |
| explanation | `Rubric`, 2–4 criteria with observable levels | `text` | model, rubric, quotes, `Provisional`/`Uncertain`; `Pending` until graded | criterion points / rubric points | `ExplanationItem` |

Points per item come from the blueprint slot (`points`), default 1.0 for keyed
items and the rubric's total for explanations, so a written answer is worth
its criteria, not one point. Scores are `Σ earned / Σ possible`.

Why these eight: they cover recall, discrimination, structure and expression
in every subject (a history ordering, a language cloze, a maths numeric short
answer, a biology matching). Not in v1: image hotspot, oral response, code
tasks (engineering extension only), essay with multiple rubrics per item.

## 4. Grading, results and progress

### 4.1 Two-phase submit

`submit_learning_assessment_form` freezes responses and grades every keyed
item in the transaction (existing path, extended with the new keys and partial
credit). If any explanation items or short-answer near-misses remain, the
submission is written with `grade_status = 'pending'` for those items and a
`quiz_grading` job is enqueued on the learning job runner (the one lesson
preparation uses, `lessons/generation_jobs.rs`; stuck-job recovery from #132
applies). The job batches open items by `model_call::fits`, runs
`grade_open`, writes item results, recomputes the score and verdict, and
records evidence. The learner can Stop it; stopping leaves items `pending`
with "Grading stopped · Resume grading".

A deterministic-only form is graded instantly and the verdict shows at once.

### 4.2 Verdict

`passed = score ≥ passing_score` (rounded to four places), computed only when
no item is `pending`. `LearningAssessmentSubmissionDto.passed` becomes
`Option<bool>`. Best attempt = highest completed score among submitted forms
of the same scope (any blueprint revision); latest = most recent submitted.
The status word on cards comes from best; latest is shown beside it.

### 4.3 Per-item feedback

After submission the form DTO carries, per item, a `key_reveal` (correct
indices, first accepted variant, matches, order, blanks), the explanation,
distractor rationales for single choice, source version ids, points earned
and possible, grade status, criterion results with quotes, and any contest.
Keys never appear before submission; `get_form` adds `key_reveal` only when
`status = submitted` (and in practice mode, per answered item).

### 4.3a Why it was marked wrong

Owner's requirement (2026-10-23): "please include somewhere for WHY the
question was scored incorrectly by the LLM". Rule: **every answer that loses
points says why, and a model grade without a checkable reason is not a
grade.**

**What the model grader returns** for each item it grades (explanation, and
the short-answer equivalence fallback). The JSON schema puts the reasoning
fields before the points, so a small model reasons first and scores second.

- `summary`: one or two plain sentences on what the answer got right and what
  cost points ("You named competitive inhibition correctly but didn't say why
  Vmax is unchanged, which the question asked for.").
- For each criterion:
  - `verdict`: met, partly_met or missed.
  - `points` out of `max_points`.
  - `what_was_missing`: the specific idea, step or example that was absent or wrong. Required whenever points are lost.
  - `your_words`: verbatim spans of the learner's answer the judgement rests on. Empty when the problem is an omission.
  - `full_credit_includes`: what a full-credit answer to this criterion says, taken from the rubric's "strong" description and the item's key points. It names the missing points and is never a complete model answer the learner could copy into a retake of the same item.
  - `source_quote` and `source_version_id`: the lesson or source passage that establishes the expected point, when the item has one.
- The grader's identity: model, rubric version and time, shown in words ("Graded by Qwen3.5-9B · provisional").

**Checks before it is shown.** These are deterministic and follow the same discipline as the claim ledger.

- Each `your_words` span must be an exact substring of the learner's answer
  after whitespace normalisation. Each `source_quote` must be an exact
  substring of the cited source version. A quote that fails is dropped, and
  that criterion reads `uncertain`.
- Points must agree with the verdict: missed is 0, met is the maximum, and
  partly met is in between. A criterion that loses points needs a non-empty
  `what_was_missing`. A disagreement is repaired by trusting the verdict and
  recomputing points; it is never shown as is.
- An empty written answer is never sent to the model: it scores 0 with the
  code-written reason "No answer given".
- **No reason, no wrong.** An item that loses points with an empty summary, or
  with no surviving `what_was_missing`, is a grader failure (§4.1). It reads
  "Couldn't grade yet: the model didn't explain this grade" with Retry
  grading, and the pass/fail verdict waits for it. It is never shown as
  "Not correct" without a why.

**On the result screen** (§2.4 wireframe), each model-graded item that lost
points has a **Why this lost points** panel. The panel is open by default for
Not correct and Partly correct, and collapsed for full-credit Provisional
items. It contains:

- the summary;
- the learner's answer with each `your_words` span highlighted and numbered,
  each number tied to its criterion in text, so colour is never the only
  signal;
- the criterion list with verdict word, points, what was missing and what
  full credit includes;
- the source line, respecting the citation toggle;
- the grader identity.

"I think I was right" and "Add to Recall" sit inside the panel. Practice mode
shows the panel per item as soon as that item is graded.

**Keyed items have a why too**, so no wrong answer is left unexplained:

- **Single choice:** "You chose B; the key is D". The rationale for B is shown
  first, then the explanation and the others.
- **Multi-select, matching, ordering and cloze:** each wrong part with its
  reason ("Pair 2: *ATP* matches *energy currency*, not *enzyme*, because …").
- **Short answer that matched no accepted variant:** "'…' isn't an accepted
  answer; the model judged it not equivalent because …". This uses the same
  rationale shape and the same checks.

**In a contest** (§4.4), the regrade never sees the first rationale. The two
rationales show side by side ("First grade | Regrade"), and any criterion
whose verdict changed is called out in words. The regrade must also answer
the learner's note directly (`response_to_note`), so "upheld" always says
why the argument didn't change the grade.

**Kept.** Rationales are stored with the item result and in contests,
exported in packs, and scored in the eval (§7.2). The rationale replaces the
current `observation`/`artifact_quote` pair on criterion results, so there is
one shape, not two.

### 4.4 Contest ("I think I was right")

Available on `Provisional`, `Uncertain` and `NeedsReview` items and on
short-answer items graded by the fallback judge.

1. The learner writes a note (optional). `contest_learning_quiz_grade`
   records a `learning_assessment_grade_contests` row (`status = 'open'`) and
   enqueues a regrade: the same rubric, the learner's answer, and the note as
   *data* ("The learner argues: …"); a fresh sample, never the first verdict.
2. The regrade writes a second criterion result set. If its score is higher,
   the item's current score is revised (`status = 'revised'`); otherwise
   `upheld`. Both verdicts stay in the row and are shown ("First grade 2/4 ·
   regrade 2/4, upheld because …").
3. After `upheld`, the learner may **Override**: the item counts as full
   credit for the score, the item is labelled "Overridden by you", the
   submission shows "score 85 % (1 answer overridden by you)", and the
   evidence event for that outcome is written `uncertain`, never `observed`.
   The pre-override score is kept (`score_without_overrides`).

Contests on deterministic items are not offered; a wrong key is a question
quality problem and gets a "Report this question" link that files a
`learning_assessment_item_reports` row (shown in the eval, section 7), and
excludes the item from future forms once reported.

### 4.5 Misses into Recall

The result screen's "Add n missed questions to Recall" calls
`add_learning_quiz_misses_to_recall(formId, itemIds)`. It creates cards
through the existing recall save path (`plugin/recall.rs::save_learning_recall_card`,
builder to confirm the request shape in `dto/memory.rs`): single choice,
true/false, multi-select, matching, ordering and short answer → question/answer
cards (prompt → key + one-line explanation); cloze → cloze card; explanation →
Q/A with the rubric's key points as the answer. Origin: the candidate id, with
its source version ids. Cards are drafts the learner can edit, per §3.5 of the
studio design; near-duplicate detection is the existing one.

### 4.6 Evidence and follow-ups

Unchanged mechanics, extended mapping: keyed items at difficulty ≤ 3 record
`recall`; keyed items at difficulty ≥ 4 record `application`; short answer
records `recall`; explanation records each criterion's `dimension`. Practice
mode records `source_kind = 'practice'`. Overridden items record `uncertain`.
Follow-ups (`missed_outcome`, `uncertain_grade`, …) keep working and now point
at lesson-level outcomes, so "Work on a gap" lands on the right lesson.

### 4.7 Course progress

Progress stays lesson-completion based ("Reading the lesson does not count as
completion"). Passing a quiz does not auto-complete the lesson; the result
screen offers "Mark lesson complete". The program header adds "Quizzes passed
x / y" (lesson quizzes only). `courseProgress.nextCourseStep` gains one rule:
after a lesson's independent practice is submitted and its quiz is not passed,
suggest "Take the lesson quiz".

## 5. Generation

### 5.1 Blueprints from scope

`assessment/quiz_blueprints.rs::blueprint_for(scope, program, outcomes, settings)`
is pure and deterministic:

- **Lesson**: one requirement set on the lesson's outcome (section 5.3):
  `single_choice ×4 (d 2–3)`, `true_false ×1 (d 2)`, one of
  `multi_select | matching | ordering ×1 (d 3)` chosen by a stable hash of the
  lesson id, `short_answer | cloze ×1 (d 3)` likewise, `explanation ×1 (d 3–4)`
  when written answers are on.
- **Module**: per *prepared* lesson, `single_choice ×2 (d 3–4)` and
  `short_answer ×1 (d 3)`; per module outcome, `explanation ×1 (d 3–4)`.
- **Course**: per prepared lesson, `single_choice ×1 (d 3–4)`; per module,
  `explanation ×1 (d 4)` on its first outcome.

The blueprint's `passing_score`, `feedback_timing = after_submission`,
`expected_minutes` (derived) and `source_version_ids` (the union of the scoped
lessons' cited versions, else the program's) are filled from settings. When a
module or course blueprint's requirements change (a new lesson prepared), a
new revision is created (`predecessor_revision`), the old stays `accepted` for
its forms' history, and new forms use the latest. The lesson blueprint changes
only when the lesson is re-prepared.

`ensure_blueprint(scope)` is idempotent and runs at: lesson Ready (lesson
scope), first open of a module or course quiz, and settings change (new
revisions where the pass mark or written-answer switch differ).

### 5.2 Bank authoring

`assessment_generation::generate` already authors per requirement,
source-grounded, window-sized. Changes:

- A per-format schema and instruction block (`SYSTEM` grows a format table):
  true/false gets a statement with a stated truth value and a one-line reason;
  multi-select states 2–3 correct of 4–6 with each option's rationale;
  cloze supplies the sentence with `{{1}}…{{n}}` and per-blank variants;
  matching supplies 3–5 pairs and at most one distractor on the right; short
  answer supplies 3–12 variants and a numeric block when the answer is a
  number; explanation keeps `teaching::rubric_schema`.
- **Deterministic validation** (`assessment_engine::validate_candidate`,
  extended), rejecting and re-requesting items that fail:
  exactly one key index for single choice and it is in range; 2–3 keys for
  multi-select; options distinct after normalisation; no option is a verbatim
  substring of the stem and the stem does not contain the key text (leak
  check, 4-gram overlap); the key option is not the unique longest option by
  more than 60 % of the median option length (length bias); true/false
  statements contain no "always/never" absolutes unless the lesson states
  them; cloze blank count matches `{{n}}` tokens; matching has no duplicate
  rights; ordering values are distinct and ≥ 3; every item names a lesson
  block or a source quote found verbatim in the cited passage (existing quote
  check); content hash de-dup against the program bank (existing
  `UNIQUE(program_id, content_sha256)`) plus a near-duplicate check
  (normalised stem Jaccard ≥ 0.8 against existing items of the same outcome).
- **Blinded key review** moves from `lessons/answer_review.rs` to
  `assessment/key_review.rs`, generalised to all keyed formats: the model
  solves each item without the key and reports its own key; a disagreement
  marks the candidate `review_status = 'disputed'` and triggers one targeted
  repair (existing recovery loop); a second disagreement drops the item. Only
  `agreed` items enter forms.
- **Bank target** per requirement is `3 × count`; `top_up(scope)` adds
  `1 × count` for requirements whose unseen count is below `count`.

The quality bar at 9B: the schema grammar, the deterministic checks and the
blinded solve are what make 9B output trustworthy; a 9B that cannot produce a
valid item after the batch's repair loop yields a smaller bank, said honestly
("12 of 24 questions prepared; retakes may repeat sooner"), never a broken
item.

### 5.3 Lesson-level outcomes

At lesson Ready, `ensure_lesson_outcome(lesson)` inserts one
`learning_outcome_definitions` row with `lesson_id` set, `title` = lesson
title, `description` = objective, `ordinal = 100 + lesson ordinal` (module
outcomes keep `0..n`, satisfying `UNIQUE(program_id, module_id, ordinal)`).
The migration backfills one per already-Ready lesson. Follow-ups and evidence
now resolve to lessons directly.

### 5.4 Where it runs

- **Lesson preparation** gains a final stage, `quiz_bank`, after claim
  verification and before Ready is announced: it writes the six prose-step
  questions into the bank (`single_choice`, difficulty by former kind:
  practice 2, quiz 3, test 4; their blinded review already ran), then authors
  the rest of the lesson bank. Progress shows "Writing quiz questions (step 3
  of 3)". A failure here does not un-ready the lesson: the callout says
  "Questions not prepared yet · [Prepare]" and the stage is retried alone.
- **Module and course banks** are prepared on first open or from "Prepare
  all" in the overview, as `quiz_bank` jobs with the same progress line.
- All calls go through `model_call` (window-sized, no timeouts) and are
  stoppable through the job runner. Settings that turn written answers off
  skip rubric authoring.

### 5.5 Not prepared yet

Opening a lesson quiz for an `outline` lesson shows "Prepare the lesson
first; its quiz is written as part of preparation" with the Prepare button.
Module and course tests say in words which lessons they cover now and that
later lessons add questions (the existing coverage line, now computed from
blueprint requirements).

## 6. Architecture

### 6.1 Backend modules (`crates/lattice-desktop/src/features/learning/`)

| File | Change |
|---|---|
| `assessment/assessment_engine.rs` | New `LearningItemFormat` variants; `LearningAssessmentKey` → `Choice, Choices, Text{variants,numeric}, Cloze, Matching, Ordering, Rubric`; `LearningAssessmentResponse` gains `selected_indices`, `blank_responses`, `matches`; `grade_deterministic_responses` returns `score: f64, earned, possible, detail: String` per item (partial credit rules of section 3); option-order permutation applied in `build_assessment_form` (stored per form item) and inverted before grading; `validate_candidate` extended per 5.2. |
| `assessment/assessment_generation.rs` | Per-format prompts and schemas; validation loop; `grade_open` unchanged in shape, batched by `fits`; new `judge_short_answer_equivalence`. |
| `assessment/key_review.rs` (new, moved from `lessons/answer_review.rs`) | Blinded solve for every keyed format; the same recovery loop. `lessons/answer_review.rs` deleted; `generation/lesson.rs` calls the moved function. |
| `assessment/quiz_blueprints.rs` (new) | `blueprint_for(scope…)`, `ensure_blueprint`, revision diffing, derived minutes. |
| `assessment/quiz_bank_jobs.rs` (new) | Job kinds `quiz_bank` and `quiz_grading` on the learning job runner; progress steps; Stop. |
| `assessment/quiz_overview.rs` (new) | The overview query (best/latest per scope, bank counts, active forms, coverage). |
| `assessment/grade_contests.rs` (new) | Contest, regrade, override, item report. |
| `assessment/recall_bridge.rs` (new) | Missed items → recall cards. |
| `assessment/assessment_repository/{forms,submission,workspace,blueprints}.rs` | `start_form` takes a scope and mode, shuffles options, writes `option_order_json`; `submit` two-phase; key reveal on submitted forms; settings table access. |
| `dto/assessment.rs` | DTOs of 6.3; `CreateLearningAssessmentBlueprintRequestDto` deleted (no IPC blueprint authoring). |
| `plugin/assessment.rs` | Commands of 6.3; `create_learning_assessment_blueprint` removed. |
| `lessons/generation/lesson.rs` | `assemble` writes questions through the assessment repository instead of `learning_questions`; prose-step schema unchanged (six questions, `kind`). |
| `lessons/generation_jobs.rs` | `quiz_bank` stage after verification. |
| `planning/curriculum_repository.rs` | unchanged; lesson outcomes come from `quiz_blueprints::ensure_lesson_outcome` at Ready. |
| `dto.rs`, `service.rs`, `repository/progress.rs`, `repository.rs`, `plugin/program.rs` | Quick-check types, `submit`, attempt reads/writes and `submit_learning_attempt` deleted; `LearningLessonDto.questions` removed. |
| `module_test_tests.rs` | deleted; replaced by tests in `assessment/assessment_tests.rs`. |
| `portability/pack.rs`, `pack/sections.rs`, `pack_repository/{export,importers,validation,program_entry,evidence_tables}.rs` | v6: remove attempts and lesson questions; add new tables to the evidence aggregate list. |
| `lessons/live_course_eval_stages.rs`, `live_course_eval.rs` | read the bank, add the `quiz` stage (Track C). |

### 6.2 Migration sketch: `migrations/20261024120000_quizzes_and_tests.sql`

```sql
-- Quizzes and tests: the assessment engine becomes the only quiz system.
-- Format and grade-status CHECK lists are widened in place with the same
-- writable_schema procedure as 20261016100000 (sqlx wraps migrations in a
-- transaction, so a copy/drop/rename rebuild would cascade-delete children).
PRAGMA writable_schema = ON;
UPDATE sqlite_schema SET sql = replace(sql,
  '''multiple_choice'',''short_answer'',''explanation'',''ordering'',''artifact''',
  '''single_choice'',''multi_select'',''true_false'',''short_answer'',''cloze'',''matching'',''ordering'',''explanation'',''artifact''')
 WHERE type='table' AND name IN ('learning_assessment_blueprint_slots','learning_assessment_candidates','learning_assessment_form_items');
UPDATE sqlite_schema SET sql = replace(sql,
  '''deterministic'',''provisional'',''uncertain'',''needs_review''',
  '''deterministic'',''provisional'',''uncertain'',''needs_review'',''pending''')
 WHERE type='table' AND name IN ('learning_assessment_submissions','learning_assessment_item_results');
UPDATE sqlite_schema SET sql = replace(sql, 'expected_minutes INTEGER NOT NULL CHECK(expected_minutes BETWEEN 1 AND 480)', 'expected_minutes INTEGER NOT NULL CHECK(expected_minutes >= 1)')
 WHERE type='table' AND name IN ('learning_assessment_blueprints','learning_assessment_forms');
PRAGMA writable_schema = RESET;   -- then bump schema_version as 20261016100000 does
UPDATE learning_assessment_candidates SET format='single_choice' WHERE format='multiple_choice';
UPDATE learning_assessment_form_items SET format='single_choice' WHERE format='multiple_choice';
UPDATE learning_assessment_blueprint_slots SET format='single_choice' WHERE format='multiple_choice';

-- Which quiz a blueprint is: the lesson, module or course it covers.
ALTER TABLE learning_assessment_blueprints ADD COLUMN scope_kind TEXT CHECK(scope_kind IN ('lesson','module','course'));
ALTER TABLE learning_assessment_blueprints ADD COLUMN scope_id TEXT;
CREATE INDEX learning_assessment_blueprints_scope ON learning_assessment_blueprints(program_id, scope_kind, scope_id, status, revision DESC);
-- Existing checkpoints were module tests: scope them by their first slot's outcome.
UPDATE learning_assessment_blueprints SET scope_kind='module', scope_id=(
  SELECT o.module_id FROM learning_assessment_blueprint_slots s JOIN learning_outcome_definitions o ON o.id=s.outcome_id
  WHERE s.blueprint_id=learning_assessment_blueprints.id AND s.blueprint_revision=learning_assessment_blueprints.revision ORDER BY s.ordinal LIMIT 1)
 WHERE scope_kind IS NULL;

-- One tagged key per candidate; the three typed columns go.
ALTER TABLE learning_assessment_answer_keys ADD COLUMN key_json TEXT NOT NULL DEFAULT '{}';
UPDATE learning_assessment_answer_keys SET key_json = CASE
  WHEN selected_index IS NOT NULL THEN json_object('kind','choice','value',selected_index)
  WHEN ordered_values_json <> '[]' THEN json_object('kind','ordering','value',json(ordered_values_json))
  WHEN accepted_answers_json <> '[]' THEN json_object('kind','text','value',json_object('variants',json(accepted_answers_json),'numeric',NULL))
  ELSE json_object('kind','rubric') END;
ALTER TABLE learning_assessment_answer_keys DROP COLUMN selected_index;
ALTER TABLE learning_assessment_answer_keys DROP COLUMN accepted_answers_json;
ALTER TABLE learning_assessment_answer_keys DROP COLUMN ordered_values_json;
ALTER TABLE learning_assessment_answer_keys ADD COLUMN distractor_rationales_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE learning_assessment_candidates ADD COLUMN review_status TEXT NOT NULL DEFAULT 'agreed' CHECK(review_status IN ('agreed','disputed','dropped','reported'));
ALTER TABLE learning_assessment_candidates ADD COLUMN origin TEXT NOT NULL DEFAULT 'bank' CHECK(origin IN ('lesson','bank','migrated'));
ALTER TABLE learning_assessment_candidates ADD COLUMN lesson_id TEXT;        -- for lesson-scoped selection
ALTER TABLE learning_assessment_form_items ADD COLUMN option_order_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE learning_assessment_forms ADD COLUMN mode TEXT NOT NULL DEFAULT 'graded' CHECK(mode IN ('graded','practice'));
ALTER TABLE learning_assessment_forms ADD COLUMN scope_kind TEXT; ALTER TABLE learning_assessment_forms ADD COLUMN scope_id TEXT;
ALTER TABLE learning_assessment_submissions ADD COLUMN score_without_overrides REAL;
ALTER TABLE learning_assessment_submissions ADD COLUMN pending_items INTEGER NOT NULL DEFAULT 0;
-- passed becomes nullable while items are pending: SQLite cannot drop NOT NULL, so passed stays
-- NOT NULL with -1 = pending, read as Option in Rust (document in the repository).
ALTER TABLE learning_assessment_item_results ADD COLUMN points_earned REAL NOT NULL DEFAULT 0;
ALTER TABLE learning_assessment_item_results ADD COLUMN points_possible REAL NOT NULL DEFAULT 1;
ALTER TABLE learning_assessment_item_results ADD COLUMN detail TEXT NOT NULL DEFAULT '';
ALTER TABLE learning_assessment_item_results ADD COLUMN learner_override INTEGER NOT NULL DEFAULT 0 CHECK(learner_override IN (0,1));
-- §4.3a: the validated grade rationale (summary, per-criterion verdict/missing/your_words/full_credit/source,
-- grader identity); '{}' for keyed items, whose why is key_reveal + detail. Contest rows keep both rationales
-- inside first_results_json / regrade_results_json, plus response_to_note.
ALTER TABLE learning_assessment_item_results ADD COLUMN rationale_json TEXT NOT NULL DEFAULT '{}';

CREATE TABLE learning_assessment_grade_contests (
  id TEXT PRIMARY KEY, program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
  form_id TEXT NOT NULL, item_ordinal INTEGER NOT NULL, note TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('open','revised','upheld','overridden','failed')),
  first_results_json TEXT NOT NULL, regrade_results_json TEXT, regrade_model TEXT, regrade_reason TEXT,
  created_at INTEGER NOT NULL, decided_at INTEGER,
  FOREIGN KEY(form_id,item_ordinal) REFERENCES learning_assessment_form_items(form_id,ordinal) ON DELETE CASCADE
);
CREATE TABLE learning_assessment_item_reports (
  id TEXT PRIMARY KEY, program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
  candidate_id TEXT NOT NULL REFERENCES learning_assessment_candidates(id) ON DELETE CASCADE,
  form_id TEXT REFERENCES learning_assessment_forms(id) ON DELETE SET NULL, note TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE learning_quiz_settings (
  program_id TEXT PRIMARY KEY REFERENCES learning_programs(id) ON DELETE CASCADE,
  quiz_pass REAL NOT NULL DEFAULT 0.7, test_pass REAL NOT NULL DEFAULT 0.75, exam_pass REAL NOT NULL DEFAULT 0.75,
  written_answers INTEGER NOT NULL DEFAULT 1 CHECK(written_answers IN (0,1)), updated_at INTEGER NOT NULL
);
-- Operation kinds for idempotency grow: contest, override, report, add_to_recall, prepare_bank.
UPDATE sqlite_schema SET sql = replace(sql, '''create_blueprint'',''create_form'',''save_response'',''interrupt'',''submit'',''decide_follow_up''',
  '''create_blueprint'',''create_form'',''save_response'',''interrupt'',''submit'',''decide_follow_up'',''contest'',''override'',''report'',''add_to_recall'',''prepare_bank''')
 WHERE type='table' AND name='learning_assessment_operations';   -- inside the same writable_schema block above

-- Lesson-level outcomes for every prepared lesson.
INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at)
SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),
       l.program_id, l.module_id, l.id, l.title, l.objective, 100 + l.ordinal, strftime('%s','now')*1000
FROM learning_lessons l WHERE l.preparation='ready'
  AND NOT EXISTS (SELECT 1 FROM learning_outcome_definitions o WHERE o.lesson_id=l.id);

-- Carry prepared lessons' reviewed questions into the bank, then drop quick checks.
INSERT INTO learning_assessment_candidates(id,program_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,source_version_ids_json,authored_by,author_model,content_sha256,created_at,review_status,origin,lesson_id)
SELECT q.id, l.program_id, o.id, 'single_choice', CASE q.kind WHEN 'practice' THEN 2 WHEN 'quiz' THEN 3 ELSE 4 END,
       q.prompt, q.options_json, NULL, '[]', q.source_ids_json, 'model', NULL, 'migrated:'||q.id, strftime('%s','now')*1000, 'agreed', 'migrated', l.id
FROM learning_questions q JOIN learning_lessons l ON l.id=q.lesson_id JOIN learning_outcome_definitions o ON o.lesson_id=l.id;
INSERT INTO learning_assessment_candidate_outcomes(candidate_id,outcome_id) SELECT c.id,c.outcome_id FROM learning_assessment_candidates c WHERE c.origin='migrated';
INSERT INTO learning_assessment_answer_keys(candidate_id,explanation,key_json)
SELECT k.question_id, k.explanation, json_object('kind','choice','value',k.correct_index) FROM learning_answer_keys k JOIN learning_assessment_candidates c ON c.id=k.question_id;
DROP TABLE learning_attempts; DROP TABLE learning_answer_keys; DROP TABLE learning_questions;
```

Notes for the builder: the `writable_schema` edits must be one block with
the schema-cookie bump exactly as `20261016100000` does; `DROP COLUMN` needs
SQLite ≥ 3.35 (the bundled sqlx-sqlite has it; verify the pinned version and
fall back to leaving the columns unread if not); `content_sha256` for migrated
rows is a uniqueness token, not a hash, so the Rust de-dup never collides with
it. Test the migration on a copy of the owner's app data (section 8.4).

### 6.3 IPC and DTOs (specta; `src/lib/bindings.ts` regenerated)

New enums and DTOs in `dto/assessment.rs`:

```rust
pub enum LearningQuizScopeKind { Lesson, Module, Course }
pub struct LearningQuizScopeDto { pub kind: LearningQuizScopeKind, pub id: Option<String> }  // None for course
pub enum LearningQuizMode { Graded, Practice }
pub enum LearningQuizBankStatus { NotPrepared, Preparing, Ready, Failed }
pub enum LearningQuizVerdict { NotTaken, Pending, Passed, NotPassed }
pub struct LearningQuizAttemptSummaryDto { form_id, score: f64, passed: Option<bool>, grade_status, submitted_at: i64,
    retake_of_form_id: Option<String>, overridden_items: i64, pending_items: i64 }
pub struct LearningQuizCoverageDto { covered_lessons: i64, total_lessons: i64 }
pub struct LearningQuizStatusDto { scope, title, bank_status, bank_message: String /* in words, empty when fine */,
    bank_size: i64, fresh_items: i64, form_size: i64, expected_minutes: i64, passing_score: f64,
    coverage: Option<LearningQuizCoverageDto>, active_form_id: Option<String>, verdict: LearningQuizVerdict,
    best: Option<LearningQuizAttemptSummaryDto>, latest: Option<LearningQuizAttemptSummaryDto>, attempts: i64,
    preparing_job_id: Option<String> }
pub struct LearningLessonQuizDto { lesson_id, quiz: LearningQuizStatusDto }
pub struct LearningModuleQuizzesDto { module_id, test: LearningQuizStatusDto, lessons: Vec<LearningLessonQuizDto> }
pub struct LearningQuizSettingsDto { quiz_pass: f64, test_pass: f64, exam_pass: f64, written_answers: bool }
pub struct LearningQuizOverviewDto { program_id, settings, exam: LearningQuizStatusDto, modules: Vec<LearningModuleQuizzesDto>,
    quizzes_passed: i64, quizzes_total: i64, outcomes, evidence, follow_ups }   // last three as in the workspace DTO today
pub struct LearningKeyRevealDto { correct_indices: Vec<usize>, accepted_answer: Option<String>, ordered_values: Vec<String>,
    matches: Vec<usize>, blanks: Vec<String>, explanation: String, distractor_rationales: Vec<String> }
pub struct LearningGradeContestDto { id, status, note, first_score: f64, regrade_score: Option<f64>, regrade_reason: Option<String>,
    first_rationale: LearningGradeRationaleDto, regrade_rationale: Option<LearningGradeRationaleDto>, response_to_note: Option<String>,
    created_at, decided_at }
// §4.3a. Replaces LearningAssessmentCriterionResultDto (observation/artifact_quote).
pub enum LearningCriterionVerdict { Met, PartlyMet, Missed, Uncertain }
pub struct LearningCriterionRationaleDto { criterion_id, title: String, verdict: LearningCriterionVerdict, points: Option<f64>,
    max_points: f64, what_was_missing: String, your_words: Vec<String> /* verified substrings of the answer */,
    full_credit_includes: String, source_quote: Option<String>, source_version_id: Option<String> }
pub struct LearningGradeRationaleDto { summary: String, criteria: Vec<LearningCriterionRationaleDto>,
    grader_model: String, rubric_version: String, graded_at: i64 }
```

`LearningAssessmentFormItemDto` gains `selected_indices: Vec<usize>`,
`blank_responses: Vec<String>`, `matches: Vec<Option<usize>>`, `left_items`,
`right_items`, `blank_count`, `points: f64`, `key_reveal: Option<…>` (post-submit
only); `selected_index` is removed. `LearningAssessmentItemResultDto` gains
`points_earned`, `points_possible`, `detail`, `learner_override`,
`contest: Option<LearningGradeContestDto>`. `LearningAssessmentSubmissionDto.passed`
becomes `Option<bool>`, plus `score_without_overrides`, `pending_items`,
`grading_job_id`. `LearningAssessmentFormDto` gains `scope`, `mode`.
`LearningAssessmentGradeStatus` gains `Pending`.

Commands (`plugin/assessment.rs`), all idempotent by `operation_id` where they
write:

| Command | Request → Response |
|---|---|
| `get_learning_quiz_overview` | `program_id` → `LearningQuizOverviewDto` |
| `update_learning_quiz_settings` | `{operation_id, program_id, settings}` → overview |
| `prepare_learning_quiz_bank` | `{operation_id, program_id, scope, top_up: bool}` → `LearningJobReceiptDto` (existing job receipt shape) |
| `start_learning_quiz` | `{operation_id, form_id, program_id, scope, mode, retake_of_form_id}` → `LearningAssessmentFormDto` (resolves the latest accepted blueprint for the scope; errors in words when the bank is not ready) |
| `get_learning_assessment_form`, `save_learning_assessment_response`, `interrupt_learning_assessment_form` | unchanged |
| `submit_learning_assessment_form` | unchanged request → form with instant keyed grades and `pending_items` |
| `resume_learning_quiz_grading` | `{operation_id, program_id, form_id}` → form (re-enqueues the grading job after Stop or failure) |
| `contest_learning_quiz_grade` | `{operation_id, program_id, form_id, item_id, note}` → form |
| `override_learning_quiz_grade` | `{operation_id, program_id, form_id, item_id}` → form (only after `upheld`) |
| `report_learning_quiz_item` | `{operation_id, program_id, form_id, item_id, note}` → form |
| `add_learning_quiz_misses_to_recall` | `{operation_id, program_id, form_id, item_ids}` → `{cards_created: i64, deck_id}` |
| `accept_learning_follow_up`, `dismiss_learning_follow_up` | unchanged |
| removed | `get_learning_assessment_workspace` (folded into the overview), `create_learning_assessment_blueprint`, `submit_learning_attempt` |

Job progress arrives on the existing learning job channel with kinds
`quiz_bank` and `quiz_grading`; the frontend's `LessonPreparationLine` and the
quiz callout read it.

### 6.4 Frontend (`src/features/learning/quizzes/`, new; `assessment/` deleted)

```
quizzes/
  useQuizzes.ts            queries: overview; mutations: settings, prepare, start, submit, resume grading, contest, override, report, misses→recall
  quizKeys.ts              query keys (added to api/learningQueryKeys.ts and learningInvalidation.ts)
  QuizzesOverview.tsx      the tab: header, settings card, exam row, modules with tests and lesson rows; right column EvidenceTimeline + FollowUps
  QuizStatusRow.tsx        one scope's verdict in words, best/latest, buttons (Start / Resume / Retake / Prepare / Practice)
  QuizVerdict.tsx          icon + words for LearningQuizVerdict and item grade words
  QuizRunner.tsx           header band, rail, item body, nav, submit/pause dialogs, autosave status; reuses the save/interrupt/submit hooks
  QuizResult.tsx           verdict card, actions, per-item review with key reveal, contest/override/report, grading status + retry
  GradeRationale.tsx       "Why this lost points" (§4.3a): summary, answer with numbered your_words highlights, criteria, source, grader; side-by-side in contests
  LessonQuizCallout.tsx    rendered by LessonCard after the last block
  ModuleTestCallout.tsx    rendered in the milestone strip (03 Check)
  items/
    itemRegistry.ts        format → { Editor, Review } components; the only switch on format
    ChoiceItem.tsx  MultiSelectItem.tsx  TrueFalseItem.tsx  ShortAnswerItem.tsx  ClozeItem.tsx  MatchingItem.tsx  OrderingItem.tsx  ExplanationItem.tsx
    itemResponse.ts        response fingerprint + "answered?" per format (replaces hasResponse/responseFingerprint in the old panel)
  evidence/
    EvidenceTimeline.tsx   moved from AssessmentEvidencePanel
    FollowUps.tsx          moved from AssessmentEvidencePanel
```

`ProgramWorkspace.tsx`: primary tabs become Lessons · Practice · Quizzes ·
Recall · Notebook · Sources; the Practice group keeps Written practice and
Code & simulations only; `quick-checks` and `assess-evidence` leave
`StudyTab` (`src/lib/studyActivity.ts`), replaced by `quizzes`;
`requestedFormId` routes into `QuizRunner`; the quick-check props
(`formFor`, `setAnswer`, `onSubmitAttempt`, `submitPending`, `submitError`)
are removed from `ProgramWorkspace`, `LearningStudioPage.tsx:221,260` and
`useLearningStudio.ts:143`. `LessonCard.tsx` renders `LessonQuizCallout`
(reads the overview query; no new lesson DTO fields).
`api/client.ts` and `VaultAPI` gain the commands of 6.3 and lose
`submitLearningAttempt`, `createLearningAssessmentBlueprint`,
`getLearningAssessmentWorkspace`.

Visual language, taken from the existing components: `font-serif` headings,
`text-[10px] font-semibold uppercase tracking-[.15em] text-accent` labels,
`rounded-2xl border border-border bg-surface shadow-sm` cards, parchment
header bands, `rounded-full bg-accent text-accent-fg` primary pills,
`min-h-11` targets, `TiptapViewer` for prompts and explanations (so KaTeX
maths renders in maths quizzes), `useCitationDisplayStore` for source lines.
Correct/incorrect use `emerald`/`rose` tints *with* the words and icons.

### 6.5 Packs, cascades, settings

- Pack v6 (`LEARNING_PACK_VERSION = 6`): remove `ATTEMPTS_ENTRY` and the
  lesson `questions` arrays from `program_entry.rs`/`export.rs`/`importers.rs`;
  add `learning_assessment_grade_contests`, `learning_assessment_item_reports`,
  `learning_quiz_settings` to `evidence_tables.rs` (aggregate rows, program
  filter), after the tables they reference. Keys travel as today (private
  section). Importing a pack therefore restores quizzes, banks, attempts and
  verdicts. v5 packs are refused with the existing version message.
- Cascades: every new table references `learning_programs` or a form item
  with `ON DELETE CASCADE`; the migration adds nothing with RESTRICT. The
  existing `pragma_foreign_key_list` guard query in `20261016100000` is
  repeated for the new tables at the end of the new migration.
- Settings are per course (`learning_quiz_settings`), created lazily with
  defaults; no global setting.

### 6.6 Deleted

Backend: `learning_questions`, `learning_answer_keys`, `learning_attempts`
(tables); `LearningAssessmentKind`, `LearningQuestionDto`, `LearningAnswerDto`,
`LearningQuestionResultDto`, `LearningAttemptDto`,
`LearningModuleTestCoverageDto`, `SubmitLearningAttemptRequestDto`,
`LearningLessonDto.questions`, `LearningProgramDto.attempts`;
`service::submit`; attempt code in `repository/progress.rs` and
`repository/memory.rs`; `plugin/program.rs::submit_learning_attempt`;
`module_test_tests.rs`; `lessons/answer_review.rs` (moved);
`CreateLearningAssessmentBlueprintRequestDto`,
`plugin/assessment.rs::create_learning_assessment_blueprint`,
`get_learning_assessment_workspace`; `20261017130000`'s columns go with the
table.

Frontend: `src/features/learning/assessment/` entirely (`AssessmentPanel.tsx`,
`AssessmentEvidencePanel.tsx` after moving its two sections,
`ModuleCheckpoint.tsx`, `moduleTestCoverage.ts`, `useLearningAssessment.ts`,
their tests); the `quick-checks`/`assess-evidence` tabs and props; the
"Model-authored answer key" attempt list.

Docs: `docs/user/*` mentions of quick checks are rewritten to Quizzes (Track B
owns the user docs touch).

## 7. Evaluation plan

Extend `evals/courses/` and `scripts/course_eval.py`; no new harness.

### 7.1 Item quality: a `quiz` stage per lesson

After a lesson reaches Ready in the eval run, the harness runs its `quiz_bank`
stage and records the bank with keys (eval only). Each item becomes
`<request>/<lesson>/quiz/<item>` with:

Mechanical checks (code decides, into `metrics.json`): schema and
`validate_candidate` pass rate, blinded key-review agreement rate (first
pass, after repair, dropped), leak check hits (the correct key or an accepted variant appearing verbatim in
the stem; other options in the stem are fine, as comparison questions name
them), length-bias hits, duplicate and
near-duplicate rate, format mix vs the blueprint, bank size reached vs target,
model calls and minutes per bank, share of items with a verbatim source quote
or lesson-block anchor.

Judged criteria (`lattice-course-judge/2`, one call each, with the lesson text
and the item). Q1 and Q2 are judged *without* the key (Q2 solves blind);
Q3–Q5 need it and see it:

- **Q1 Answerable from the lesson.** "A learner who read only this lesson can
  answer the item; it needs no outside fact the lesson does not teach."
- **Q2 Exactly one correct key.** The judge solves blind and returns its key;
  code compares with the authored key. Pass when they agree *and* the judge
  states no other option is defensible. (Replaces L5's key half.)
- **Q3 Plausible distractors.** "Each wrong option is one a learner who missed
  the point could pick; none is a joke, a category error, or trivially
  excluded by grammar or length."
- **Q4 No leak.** "The stem, the other options, and the lesson's own phrasing
  of the item do not give the answer away."
- **Q5 Keyed fairly.** Short answer and cloze: "The accepted variants cover
  the reasonable ways a correct learner would phrase this; the numeric
  tolerance is neither trivial nor forgiving of a wrong answer." Explanation:
  "Each rubric criterion names an observable thing in the answer at
  incomplete, adequate and strong levels, without stating the answer."

L5 is retired in favour of Q2–Q4 run on the lesson's six prose-step items (they
are in the bank). The slot `<request>/<lesson>/quiz` passes when every
mechanical check passes and ≥ 90 % of items pass all judged criteria (an item
bank, unlike a lesson, tolerates dropping a few). `compare` and the
`can't tell from noise` verdict apply unchanged.

### 7.2 Grading quality: a frozen answer set

`evals/courses/grading/quiz-grading-v1.jsonl`: for a fixed bank (the committed
eval lessons, three subjects, one non-software), a set of learner answers per
item type written by hand and by a model playing learner personas (correct,
near-miss, partial, wrong, off-topic, empty, contesting), ~120 rows. Each row
has a human grade (Josh labels blind through `calibration-sample`).

- Deterministic rows (seven types) are unit tests in
  `assessment/assessment_tests.rs`: normalisation, numeric tolerance,
  partial-credit arithmetic, option-order inversion, exactness of the detail
  strings.
- Model-graded rows (explanation, short-answer fallback) run through
  `grade_open` / the equivalence judge with the recipe's model; the scorer
  writes rows in the chat eval's shape and `calibration-score` gives
  false-pass and false-fail rates and the PPI-corrected agreement with exact
  intervals, as the course judge is calibrated today (design §5.2). Headline:
  agreement with human on pass/fail per item, and mean absolute point error.
  An item counts as a pass only at full credit; partly credited items are
  scored by point error. The label file adds `human_points` beside the
  pass/fail label (its schema is otherwise the calibration tool's).
- Rationale quality (§4.3a), checked mechanically on every model-graded row:
  - the share of `your_words` and `source_quote` spans that are verbatim;
  - points that agree with the verdict;
  - lost points that have a `what_was_missing`;
  - `full_credit_includes` that doesn't reproduce the reference answer.

  On the calibration slice, the human also labels the reason as one of: *right*, *verdict right but reason wrong*, or *reason fabricated*. Headline: the share of marked-down answers whose reason a human accepts. A grader with correct verdicts and unacceptable reasons fails this eval.
  Predeclared bar (set by the owner on 2026-10-24: a B or better): any reason
  labelled *fabricated* fails the grader, and the accepted share must be at
  least 83 %, reported with its exact 95 % interval.
- Contest rows: a regrade must not change its verdict on an unchanged answer
  more than 10 % of the time across three seeds (stability), reported
  separately.

### 7.3 Where the owner's rule shows

Results per model (9B local, 27B remote) go under
`evals/courses/results/<model>/…` with the run entry in
`docs/development/course-generation-history.md`. A bank that reaches target on
27B but not on 9B is a finding, not a failure; the summary quotes both.

## 8. Build plan

Contract first: Track A publishes the DTOs, enums and command signatures of
6.3 as the first commit (stubs returning `InvalidState("not built")`),
regenerates `src/lib/bindings.ts`, and Tracks B and C start from it. Three
tracks, strict ownership, at most three agents at once, `cargo check` only
(the coordinator runs gates once at the end; vitest with `--maxWorkers=2`,
never alongside lint; `rustfmt` per file, not `cargo fmt`).

### Track A: engine, storage, generation, jobs (backend)

Owns `crates/lattice-desktop/migrations/20261024120000_*`, everything under
`features/learning/assessment/`, `dto/assessment.rs`, `dto.rs`,
`plugin/assessment.rs`, `plugin/program.rs`, `service.rs`, `repository/*`,
`lessons/generation/lesson.rs` (assemble only), `lessons/generation_jobs.rs`
(the new stage), `lessons/answer_review.rs` (delete after move),
`portability/*`, `module_test_tests.rs` (delete).

Order: migration + first-start report on a copy of the owner's data → DTO
contract commit → engine keys, scoring, validation → blueprints and lesson
outcomes → generation per format + key review → bank job and lesson stage →
two-phase submit and grading job → contests, overrides, reports → recall
bridge → overview → packs v6 → deletions.

### Track B: the learner's screens (frontend)

Owns `src/features/learning/quizzes/**`, `src/features/learning/assessment/**`
(delete), `LessonCard.tsx` (callout mount only), `ProgramWorkspace.tsx`,
`LearningStudioPage.tsx`, `useLearningStudio.ts`, `courseProgress.ts`,
`api/client.ts`, `api/learningQueryKeys.ts`, `api/learningInvalidation.ts`,
`src/lib/studyActivity.ts`, `docs/user/*`.

Order: item registry and the eight editors + reviews with tests (keyboard
paths asserted) → runner against the stubbed form DTO → result screen →
overview, status rows, settings → callouts and navigation → deletions → user
docs. Screenshot the four key screens with the rig in `../ui-rig` and attach
them to the PR.

### Track C: evaluation

Owns `evals/courses/**`, `scripts/course_eval.py`,
`lessons/live_course_eval*.rs`, `docs/development/course-generation-testing.md`,
`docs/design/2026-10-20-course-eval.md` (appendix for `lattice-course-judge/2`).
Needs Track A's bank API (`prepare_learning_quiz_bank` and the repository
read) by its second week; until then builds the scorer, rubrics and the
grading set against fixtures.

### 8.4 Acceptance bar

- The migration runs on a copy of the owner's app data; the report states the
  counts: programs, Ready lessons, lesson outcomes created, questions carried
  into banks, checkpoint blueprints scoped, quick-check attempts dropped, and
  that first start blocks nothing.
- Live on the 9B recipe: prepare one lesson (the `quiz_bank` stage runs and
  reports its step); open its quiz from the lesson card; answer all eight
  types across two lessons; pause and resume; submit; see keyed results at
  once and the written answer graded by the job; fail below the mark, retake,
  get unseen items with shuffled options, pass; contest a written grade and
  see both verdicts; override after an upheld regrade and see it labelled;
  add misses to Recall and find the cards; take a module test that says its
  coverage; prepare and take the course exam; export the pack, delete the
  course, import, and find attempts and verdicts intact.
- A written answer that loses points shows "Why this lost points":
  - the summary;
  - at least one highlighted span of the learner's own words;
  - what full credit includes;
  - the grader.

  A grade whose rationale fails validation reads "Couldn't grade yet", never "Not correct". A contested item shows both rationales and the regrade's reply to the note.
- Stop a grading job mid-way: the result reads "Grading stopped · Resume
  grading"; nothing is marked failed.
- Keyboard-only run of a whole quiz; VoiceOver reads every verdict in words.
- Eval: a `quiz` stage run on the 9B recipe committed under results, with the
  judge-vs-human calibration slice labelled by the owner before the grading
  numbers are quoted.
- Gates green once at the end: cargo clippy, lib tests, layer checker, vitest,
  lint.

## 9. Owner decisions (2026-10-23)

1. **Pass marks:** 70 / 75 / 75 as defaults, editable per course. These were
   presented and not objected to.
2. **Passing a lesson quiz** offers "Mark lesson complete". It never completes
   the lesson by itself.
3. **Override after an upheld regrade:** allowed. It is recorded and labelled
   ("includes your override") and never becomes `observed` evidence.
4. **Lesson banks** are authored by the `quiz_bank` stage during lesson
   preparation, so the quiz is ready when the lesson is.
5. **Quick-check attempt history** is dropped by the migration, under the
   no-data-migration rule; prepared lessons' questions are kept. Stated to the
   owner, who did not object.
6. **The course exam** ships in v1.
7. **Added requirement:** every answer that loses points says why, and model
   grades carry a checkable rationale (§4.3a).
