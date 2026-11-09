# Course preparation: one click, lessons side by side, lesson 1 never slower

Status: design, 2026-11-08. Builders: see "Build order".

## What a learner meets today

- After Accept nothing starts. The draft says "Accept to prepare lessons individually". Each module needs its own "Prepare all lessons" click (#415).
- A course prepares one lesson at a time. `lesson_job_config()` is `exclusive_per_subject` per program, and quiz jobs are too (`quiz_job_config()`).
- The last measured lesson (Haiku, three-course run, before #416/#420/#426) took 64 min end to end. About 22 min of that was the quiz, which #416 moved into its own job. The checks now run side by side (#420).
- A queue of 18 lessons is therefore hours of strictly serial work. Windows may idle-sleep through it: `UserActivity` only guards against App Nap on macOS and allows system sleep.

## The real constraint: lessons coordinate through each other's output

The writer's context is `{"curriculum": <all objectives>, "recentTeaching": <recaps of earlier lessons>}` (`lessons/generation/lesson.rs:88-98`). The prompt leans on those recaps:

- "Reuse the course vocabulary and build on the supplied earlier recaps."
- "Open the first explanation with a short question asking the learner to recall it."
- The last check mixes this lesson's ideas with "an earlier lesson's in recentTeaching".
- "Teach every action ... unless ... an earlier lesson (courseSequence, recentTeaching) already covers it."

`recentTeaching` holds recaps only for lessons whose `preparation == Ready`. Two consequences follow.

1. **Lessons form a chain.** Lesson N cannot start well until lesson N-1 is published. Running lessons side by side silently drops this context.
2. **A lesson's content already depends on timing (a latent bug).** Lesson N loses lesson N-1's recap when N-1:
   - failed;
   - was held for review;
   - was cancelled;
   - or the learner prepared N first.

   Nothing records that the context was missing.

## Design

Five pieces. Each owns one concern, and no concern lives in two places.

### 1. Intent → admission: start with module 1, stay one module ahead

One admission core, `admit_lessons`: one write transaction, course order, a stable operation id per lesson, the same `admit_job` dedupe. Three requests use it.

- **A lesson or a module** (`start_lesson_jobs`, #415): the learner's Prepare and "Prepare all lessons". A module request retries a stopped job of the same operation.
- **Start this course** (was "Accept program") composes two intents in the UI: accept, then prepare module 1 at the revision accept returned. The commands stay single-purpose.
- **One module ahead.** Opening a ready lesson of an active course calls `prepare_learning_ahead(program, lesson)`. In one BEGIN IMMEDIATE transaction it admits the untried outline lessons of that lesson's module and of the next module by ordinal.
  - *Untried* means no lesson_preparation job of any status at the current content revision. A failed or cancelled lesson waits for the learner's own Prepare, so this path never retries.
  - The operation id is `stable_job_operation_id(program, "ahead:{lesson}", revision)`, so reopening a lesson admits nothing new.

The next module prepares while the learner studies this one, and cost follows the lessons the learner actually reaches. That is about $0.6–1 per lesson on Haiku, never beyond the current module and the next. A course of many long modules is never prepared wholesale.

### 2. The course plan: lessons compile against a header, not each other's object code

Cross-lesson coordination moves from outputs (recaps) to an input made before any lesson is written.

- **What it is.** A course plan, one per outline revision. For each lesson it lists:
  - `ideas`: the two or three core ideas, named as topics, never as statements of fact;
  - `terms`: vocabulary it introduces;
  - `actions`: what the learner will perform that this lesson teaches;
  - `contrast`: the confusion it resolves.

  It is one structured call. Items are keyed by an enum of lesson ids, which scales to hundreds of lessons. Four properties per item stay well inside the grammar limit.
- **How the writer uses it.** The writer gets `coursePlan` instead of `recentTeaching`:
  - earlier lessons: taught already, so build on them, recall them, interleave them, and don't re-teach;
  - this lesson: teach exactly these;
  - later lessons: not yours, so bridge to the next one in the recap.
- **Determinism.** A lesson's inputs are a pure function of (outline revision, plan, its sources). Scheduling order can no longer change content. That fixes the latent bug and makes lessons independent.
- **The plan carries no claims.** Topics, terms and actions only. Truth stays where it is checked today, in the lesson's verification.
- **Storage.** One row per lesson, `learning_course_plan_lessons(program, outline revision, lesson)`. A revision is planned when every lesson of the outline has a row.
  - `ensure` reads the rows. Only when some are missing does it make one call. It saves every entry in one write transaction (`ON CONFLICT DO NOTHING`), then reads again.
  - It is idempotent and holds no lock: two overlapping calls both return what the first to commit saved.
  - An unreadable row is an error, never a missing one.
- **When it is made.** A `course_plan` job runs as soon as the outline is made or repaired, so the learner sees each lesson's topics with the outline. Starting the course keeps the plan, because accepting doesn't change the outline revision. A lesson job also ensures the plan before it writes, which covers courses made before this change and a plan whose job failed.
- **Single-flight.** Callers serialize on the per-course lock from piece 3. The composition root builds that lock once and gives it to both the lesson worker and the plan worker. Cost: one call per outline revision.
- **The learner can change the topics.** Until a lesson's writer starts, the learner can:
  - edit its entry by hand;
  - ask for a change: one Interactive call rewrites the selected lessons' entries against the whole plan;
  - undo, through a per-entry history stack.

  It uses the lesson review screen's patterns: cards, select many, undo. The writer reads the plan when it starts, so a queued lesson picks up an edit. A running writer locks its lesson's entry:
  - the edit's write transaction checks that no running job covers the lesson;
  - the job's claim commits before the job reads the plan;
  - so an edit and a writer start serialize.

  A written lesson's entry is fixed too ("This lesson is already written; its topics are fixed."). A lesson stopped mid-way and edited before its retry is written from the edited entry. Its checkpoints cannot carry the old plan through, for two reasons:
  - A draft is reused only when its scope hash matches, and that hash covers the whole plan.
  - A verified lesson is saved as `prepared-v2` = {plan digest, lesson}, in one write, and is reused only when the digest equals the stored plan's.

Research mapping: Skeleton-of-Thought (Ning et al., 2023) expands a shared skeleton in parallel. DOC (Yang et al., ACL 2023) keeps long-form generation coherent through a detailed outline that every part conditions on. Here the course plan is the detailed outline and the lessons are its parallel expansions.

### 3. Shared course state: narrow critical sections, then no per-course exclusivity

The job runtime's own principle (`JobKindConfig::new`): "contention for a shared backend such as the model is that backend's scheduler's decision, not the job runtime's." Per-course exclusivity exists only because lesson jobs share course state. Shrink that to what is actually shared:

- **Course setup** goes in one per-program critical section, held only while setup runs. The lock is a keyed value that the worker owns and the composition root builds. It is not a static, because the architecture check's global-state rule forbids that.
- The section covers: `seed_accepted`, `refresh_sources` (before-use refresh uses `expected_revision`, so concurrent refreshes conflict), `sweep_admission`, and bringing the course index current. Already safe and left as is: `seed_accepted` (BEGIN IMMEDIATE + re-check), admission sweeps (idempotent), trust writes (INSERT OR REPLACE), and per-job checkpoints.
- **Research saves** dedupe in the database inside the save transaction (same program, same page), not only against the in-memory `sources` list in `research/saving.rs`.
- **Publishing** two lessons of one course concurrently must not conflict. Since #415 a job's `content_revision` is the outline revision only; a test proves it.

Then `lesson_job_config()` drops `exclusive_per_subject`.

### 4. Order and capacity: one place, the backend's scheduler

The inference scheduler already admits by class (Interactive > Verification > Maintenance > Background), in arrival order within a class, and keeps one slot for interactive work. Two additions:

- **Rank.** Job kinds that opt in (`JobKindConfig::ranked()`) carry a rank: the lesson's position in course order. A quiz job takes its lesson's rank. While a ranked job's worker runs, the runtime scopes the rank in a task-local. The lesson pipeline already threads its context through task-locals and has no `tokio::spawn` in production code, so every call inherits it. `ScheduledLlm` reads the rank into `AdmissionRequest`. Queue order:
  1. Interactive, by arrival.
  2. Unranked non-interactive, by class then arrival (unchanged).
  3. Ranked, by rank, then class, then arrival.

  The backlog never delays the present, and the backlog is served in course order. Rank must come before class within the backlog. Otherwise lesson 3's verification burst (Verification) would overtake lesson 1's writer call (Background).
- **Head reservation.** While the lowest live rank seen at a backend is alive, one non-interactive slot is kept from higher ranks. The head and unranked work may use it.
  - Liveness: the scheduler keeps a `Weak` to the job's rank scope.
  - When the scope drops, the scheduler re-dispatches at once.
  - This mirrors the existing interactive reservation and `hold(key)`.

Consequences, with no lane count to tune:

| Backend | Slots | What happens |
|---|---|---|
| Cloud | 8 | 1 interactive, 1 head, 6 shared. Several lessons progress at once, in course order. |
| Local llama-server | 1 or 2 | Lower ranks never run while the head lives. Effectively today's one-at-a-time, because local is model-bound anyway. |
| Any | — | Lesson 1 always has a slot within one admission. Later lessons use only spare capacity. Its time does not get worse. |

### 5. Observation

- **Course progress line** in the header, derived from job rows: "4 of 18 lessons ready · 2 preparing · 11 waiting · 1 needs attention" (zero parts omitted).
- **Pace from measured history only.** "About one lesson every N min so far" is the median gap between consecutive finishes, counting only gaps where the later lesson was already queued when the earlier one finished. Idle time, such as a day between sessions or lessons prepared one at a time, never counts. The line is hidden until such a gap exists.
- **Waiting state.** A running lesson whose current call is queued at the scheduler shows "Waiting for the model", not a frozen stage. Comes with piece 3's flip.
- **Errors.** A failed lesson, module, start or ahead request shows its error. Today `prepareError` passes only `prepare.error`. The error shown is the one from the latest preparation action, and only on the course it was for.
- **Keep awake** (Josh's call, separate PR). While lesson or quiz jobs run, hold a system-required power request (Windows) or an idle-sleep-disabling activity (macOS). The display may sleep. Released when the queue is idle.

## Invariants (each one is a test)

1. Starting a course admits module 1's unfinished outline lessons, in course order. Opening a ready lesson admits the untried outline lessons of its module and the next, in course order, and never a failed or cancelled one. Repeating either admits nothing new.
2. A lesson's writer context is identical whether or not earlier lessons are Ready.
3. A course plan is written once per (program, outline revision). However many callers ask, every lesson's entry comes from one answer.
4. Two lesson jobs of one course can set up, research, save sources and publish concurrently: no duplicate source, no revision conflict, no failed refresh.
5. Scheduler order is: Interactive > unranked > ranked by rank. A higher-rank request never takes the last free non-interactive slot while a lower rank is alive at that backend. A rank scope's drop re-dispatches waiters.
6. Every model call made inside a ranked job carries that job's rank.
7. A lesson's topics change only while it is an outline lesson that no running job covers. A writer uses the entry as it stood when it started. A retry after an edit writes from the edited entry, never from a draft or verified lesson made from the old one.

## Rejected

- **Per-course lane count.** It duplicates capacity knowledge outside the scheduler, needs a per-backend number, and doesn't order calls, so lesson 1 is slowed by lesson 2's check burst.
- **Draft gate.** Lesson N+1 waits for N's first draft and reads its draft recap. That couples jobs: a cross-job wait, a failure chain if N fails, and a recap that repairs may change. It also only overlaps verification.
- **Drop recaps, keep objectives.** Free, but its quality is unknown. It is the baseline arm of the plan's A/B.
- **A continuity pass after parallel writing.** It edits published-to-be text after the fact and re-triggers verification of every edited part.

## Build order

| PR | Piece | Merge |
|---|---|---|
| A #441 (reds #439) | Start this course (module 1), one module ahead, course progress UI, error surfacing (pieces 1, 5 minus waiting/keep-awake) | any time |
| B #442 (reds #438) | Ranked admission + head reservation (piece 4) | any time |
| C #444 (reds #443) | Course plan (piece 2) | after a Haiku A/B in #450's two test apps: one outline, lessons 1–3 of module 1 prepared one after another in each arm, read by hand |
| D #440 (reds #437) | Shared course state safety (piece 3 minus the flip) | any time; no behaviour change |
| E #436 (reds #435) | Keep awake | any time (Josh: yes, if not intrusive; system-required only, the display may sleep) |
| G1 #446 (reds #445, follow-up reds #449) | Topics planned with the outline: the course_plan job, get/plan/edit/change/undo, per-entry history, the edit lock, the plan digest on a verified lesson | after C |
| G2 #447 (reds #448) | Topics under each lesson and the "Review topics" screen | after G1 |
| F | Flip: lessons of a course side by side, the plan worker and lesson worker share the course lock, "Waiting for the model" | after B, C, D, G |
| #450 | The A/B's two test apps (never merges) | never |

## Calls for Josh

- **A fresh install's Learning model is the first local model downloaded** (`default_learning_model`, download saga). Parallel lessons only help on a cloud model, so a tester should set Haiku as the Learning model.
- Decided:
  - "Start this course" prepares module 1, and opening a ready lesson keeps the course one module ahead. It never prepares the whole course: every module can be a lot of lessons.
  - "Accept program" becomes "Start this course".
  - keep the PC awake while lessons prepare;
  - show each lesson's planned topics before it is prepared;
  - let the learner change them in the review screen's style.
