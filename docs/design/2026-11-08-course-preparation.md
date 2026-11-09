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

### 1. Intent → admission: one path

`start_lesson_jobs(program, scope, expected_revision)` with `scope ∈ {Lesson, Module, Course}`. It generalises `start_module_jobs` (one write transaction, course order, stable per-lesson operation ids, the same `admit_job` dedupe).

- **Accept** composes two intents in the UI: accept, then prepare the course with the revision accept returned. The commands stay single-purpose.
- **"Prepare every lesson"** appears in the course header whenever an active course has unqueued, unfinished outline lessons, for example after a cancel or a failure.

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
- **Single-flight.** A per-program async lock plus a DB row keyed by outline revision. The first lesson job that needs the plan writes it; the others wait and read it, and so do restarts. Cost: one call per course, added once to lesson 1's time. A later option is to write it speculatively while the learner reads the passed outline.

Research mapping: Skeleton-of-Thought (Ning et al., 2023) expands a shared skeleton in parallel. DOC (Yang et al., ACL 2023) keeps long-form generation coherent through a detailed outline that every part conditions on. Here the course plan is the detailed outline and the lessons are its parallel expansions.

### 3. Shared course state: narrow critical sections, then no per-course exclusivity

The job runtime's own principle (`JobKindConfig::new`): "contention for a shared backend such as the model is that backend's scheduler's decision, not the job runtime's." Per-course exclusivity exists only because lesson jobs share course state. Shrink that to what is actually shared:

- **Course setup** goes in one per-program critical section, held only while setup runs: `seed_accepted`, `refresh_sources` (before-use refresh uses `expected_revision`, so concurrent refreshes conflict), `sweep_admission`, and bringing the course index current. Already safe and left as is: `seed_accepted` (BEGIN IMMEDIATE + re-check), admission sweeps (idempotent), trust writes (INSERT OR REPLACE), and per-job checkpoints.
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
- **Pace from measured history only.** After two lessons of this course finished in this run: "About one lesson every N min so far", the median interval between consecutive ready times.
- **Waiting state.** A running lesson whose current call is queued at the scheduler shows "Waiting for the model", not a frozen stage. Comes with piece 3's flip.
- **Errors.** A failed "Prepare all/every lesson" shows its error. Today `prepareError` passes only `prepare.error`.
- **Keep awake** (Josh's call, separate PR). While lesson or quiz jobs run, hold a system-required power request (Windows) or an idle-sleep-disabling activity (macOS). The display may sleep. Released when the queue is idle.

## Invariants (each one is a test)

1. Preparing a course admits one job per unfinished outline lesson, in course order. Repeating the request admits nothing new.
2. A lesson's writer context is identical whether or not earlier lessons are Ready.
3. A course plan is written once per (program, outline revision), however many lesson jobs ask concurrently.
4. Two lesson jobs of one course can set up, research, save sources and publish concurrently: no duplicate source, no revision conflict, no failed refresh.
5. Scheduler order is: Interactive > unranked > ranked by rank. A higher-rank request never takes the last free non-interactive slot while a lower rank is alive at that backend. A rank scope's drop re-dispatches waiters.
6. Every model call made inside a ranked job carries that job's rank.

## Rejected

- **Per-course lane count.** It duplicates capacity knowledge outside the scheduler, needs a per-backend number, and doesn't order calls, so lesson 1 is slowed by lesson 2's check burst.
- **Draft gate.** Lesson N+1 waits for N's first draft and reads its draft recap. That couples jobs: a cross-job wait, a failure chain if N fails, and a recap that repairs may change. It also only overlaps verification.
- **Drop recaps, keep objectives.** Free, but its quality is unknown. It is the baseline arm of the plan's A/B.
- **A continuity pass after parallel writing.** It edits published-to-be text after the fact and re-triggers verification of every edited part.

## Build order

| PR | Piece | Merge |
|---|---|---|
| A | Intent → admission + course progress UI + error surfacing (pieces 1, 5 minus waiting/keep-awake) | any time |
| B | Ranked admission + head reservation (piece 4) | any time |
| C | Course plan (piece 2) | after a Haiku course-eval A/B: plan vs recaps vs objectives-only; adjacent lessons read by hand |
| D | Shared course state safety (piece 3 minus the flip) | any time; no behaviour change |
| E | Keep awake | Josh's call |
| F | Flip: lessons of a course side by side + "Waiting for the model" | after B, C, D |

## Calls for Josh

- **Accept prepares the whole course.** Cost is about $0.6–1 per lesson on Haiku. The alternative, prepare ahead of the learner by a horizon, bounds cost to lessons actually taken.
- **A fresh install's Learning model is the first downloaded local model** (`default_learning_model`, download saga). Parallel lessons only help on a cloud model. For a tester, set Haiku as the Learning model.
- **Keep the PC awake while lessons prepare.**
- **Show each lesson's planned ideas on its card before it is prepared.** It's free once the plan exists.
