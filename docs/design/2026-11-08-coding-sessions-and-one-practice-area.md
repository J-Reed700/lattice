# Coding sessions with a coach or an interviewer, and one Practice area

Date: 2026-11-08. Status: signed off 2026-11-08; being built.

**The ask.** Interview practice "like easy [online-judge] questions guided while someone watches me code in python". Then: "Design something magnificent, utilize existing infrastructure, follow SOLID … super intuitive for users to understand". Then an audit of the Practice UI for code smells and screens that make it hard to know what to do. Then: "unify redundant experiences as well".

**The calls made here:**
- **One coding session for solo practice and for interviews.** A session holds a problem, an editor, cases you can run and a counterpart who sees your code and your runs. Three settings add pressure step by step: Coached, Mock and Watched.
- **Problems are proven before they are shown.** A hidden reference solution must pass every case, and each listed common mistake must fail at least one. This is how drills already prove their planted defects.
- **Practice becomes one area.** It has five starting points, one Start button each, and one vocabulary:
  - code review practicals fold into drills;
  - code lab, project and debugging fold into Code;
  - interviews and the other role-plays share one engine.
- **A course states once what it is preparing for, and every practice generator reads it.** This answers the earlier question of where to tell Lattice to cater a drill or a problem to an interview format.

## 0. What exists today

Checked against the code at `1b53a9fa2`:

- **Code lab.**
  - The model writes a brief and starter files (`practice/practical_generation.rs:287-467`).
  - A second call writes one hidden `checks.py`, without ever seeing a solution (`practical_evaluator.rs:94-140`).
  - Python runs in the built-in CPython 3.14 WASI runtime.
  - A run is stored as a single check row, "Contained evaluator" (`practical_repository/runs.rs:219`). Which test failed exists only in stdout.
  - There is no hint, tutor or debrief. Explore, Practice and Demonstrate are stored for practicals but never enforced.
- **Interview.**
  - It has research, a question plan, reviewed answer guides, a text interviewer that asks one question at a time, and a scorecard with checked quotes (`interview_generation.rs`, `interview_feedback.rs`).
  - It refuses an execution environment (`plugin/practical.rs:197-204`).
  - The interviewer's context holds no files and no runs (`SimulationActivityContext`, `practical_generation.rs:63-69`).
- **Written practice** has the only hint ladder (`LearningPracticeHintLevel`, four rungs), the only record of help used, and the only enforcement of the three modes (`practice_repository/assistance.rs`).
- **Drills.**
  - Each planted defect is proven by a test that kills it, run through `CodeRunner` (`drills/runner.rs:46-67`).
  - A judge grades the review.
  - FSRS schedules each pattern and each lesson (`drills/schedules.rs`, `learning_drill_pattern_schedules`).
- **Voice.** The mic (`VoiceButton`) is mounted only in the app's chat composer (`chat/components/composer/Composer.tsx:260`). No practice screen has it.
- **The Practice tab.**
  - It has three sub-tabs: Written practice, Code & simulations, and Review drills.
  - Toggles are nested inside them, and there are four different start buttons.
  - Some words change meaning from one screen to the next (§6.3).

So for "someone watches me code", a code lab gives you a problem and a pass or fail, and an interview gives you an interviewer who cannot see code. Nothing joins the two.

## 1. Decisions at a glance

| Decision | Why |
|---|---|
| **One coding session.** A problem, an editor, runnable cases, and a counterpart that sees the draft and the runs. | Solving and explaining is the same job under different pressure, so it gets one engine and one screen. |
| **Three settings.** Coached: a coach, no clock, every hint. Mock: an interviewer, a soft clock, hints are noted. Watched: the interviewer reacts to the code as you write it, and you think aloud. | Being watched more than halved performance in a randomized study [BSBP20]. Practising while self-aware or anxious protected performance under later pressure [BC01, OP09]. Guidance should fade as skill grows [RA03, K03]. |
| **Problems are proven before they are shown.** A hidden reference passes every case; each listed misreading fails at least one. | A wrong expected output misleads both the learner and the interviewer. Drills already apply this principle with killing tests. |
| **Stages are on screen.** Understand, Examples, Plan, Code, Test, Review. A bug fix uses Reproduce, Locate, Fix, Verify. | Explicit stages plus self-monitoring help novices [LKJ16], and subgoal labels help too [MGC12, MMG15]. |
| **Examples come before code.** They are checked against the reference and against the misreadings. | This follows Examplar [WK19] and writing tests first [D19]. |
| **A hint ladder: nudge, idea, plan, point at the line.** Every hint says why. Any code in a hint is run, and the hint is refused if that code passes every case. | This addresses the assistance dilemma [KA07], gives help without handing over the solution [K24], adds explanations to hints [MWP19, MLWP19], and guards against hint abuse [A03]. |
| **A debrief on four criteria** (problem solving, code, testing, communication). Every piece of evidence is checked by the app: quotes, lines and runs. | Feedback should be specific, about the task and the process, and come after the attempt [S08, HT07]. |
| **Problem patterns come back on FSRS schedules,** interleaved by family. A problem never names its pattern. | In an interview, choosing the method is the skill [RT07, BR19]. Spacing helps retention [C08, FSRS, RD22], and deliberate practice targets weak spots [EKT93]. |
| **"Preparing for":** course-level details that every practice generator reads, plus an optional interview date that paces reviews. | This answers the question of where to put a specific prompt. Decks already pace reviews to a target date (`recall/schedule.rs`). |
| **One Practice area.** Five cards, one Start each, options written as a sentence, one vocabulary. | The audit's P1 findings 3 and 4. |
| **Shared pieces instead of copies.** One thread with the mic, one clock, one criteria list, one turn-policy seam, one scorecard type. | See §6. |

## 2. Research

DOIs checked against Crossref on 2026-11-08. Keys already used in `2026-10-25-learning-science-lessons.md` keep their meaning there.

| Key | Finding | What it does here | Limit |
|---|---|---|---|
| BSBP20 | In a randomized study (n=48), solving a problem while watched more than halved performance compared with solving it in private. Stress and cognitive load were higher. | Watched is a separate setting that you reach after Coached and Mock. | One study, one task, students. |
| BC01 | Novice golfers were trained to a high level. Those trained with a camera on them did not choke under later pressure; those trained alone or with a distracting task did. | Rehearse in the watched condition before the real interview. | A motor skill, 54 novices. |
| OP09 | Experts who trained under induced anxiety kept their performance under pressure, while controls declined. | Same as BC01. | Perceptual-motor experts. |
| FEB11 | Across 94 studies, plain think-aloud did not change performance (r = −.03). People asked to explain their thinking performed better than silent controls. Verbal reports took longer. | Watched asks you to explain your reasoning, which interviews require anyway. The clock budget allows for the extra time. | Lab tasks, not interviews. |
| LKJ16 | Teaching explicit problem-solving stages, with a self-monitoring prompt, made novices more productive and independent, and raised their self-efficacy. | The stage rail, and the one-line instruction for each stage. | One camp, short programs. |
| MGC12, MMG15 | Subgoal labels in worked examples improved problem solving and transfer. | The stage names are the labels, and the coach names each step. | Labels in examples, not in live sessions. |
| WK19 | Students wrote input–output examples before coding and got feedback from a correct implementation and buggy ones. Effects were positive or neutral, never negative. | The Examples stage checks your examples against the reference and the misreadings. | Course context, larger assignments. |
| D19 | Writing test cases before programming as a scaffold. | Supports the Examples stage. | Its results are not confirmed here, so this support is weaker. |
| KA07 | The assistance dilemma: too little help wastes time, and too much help stops learning. | A ladder of graded help. | A framework, not a single result. |
| A03 | Learners often avoid help, or click straight through to the final hint. | Hints open one rung at a time, and each one is recorded and shown in the debrief. | Tutoring systems. |
| K24 | CodeAid gave pseudo-code, line-by-line explanations and highlighted wrong lines, and never gave direct code. Students found it useful. | Rung 3 is pseudo-code, rung 4 points at lines, and no hint contains solution code. | One course; mostly students' perceptions. |
| MWP19, MLWP19 | Hints with a text explanation improved immediate performance (the post-test gain was only on similar tasks), and students rated them as more useful. | Every hint carries its reason. | Small samples, block-based programming. |
| SK21 | A meta-analysis: solving problems before instruction gave better conceptual knowledge and transfer than instruction first. | The worked solution unlocks only after a submit, and the debrief shows the strong answer afterwards. | The effect depends on how faithfully the design is implemented. |
| RA03, K03 | Support should fade on the way from examples to independent problems. Guidance that helps novices can hurt experts. | The settings go Coached, then Mock, then Watched. The coach also opens stages less often as a pattern's stability grows. | — |
| S08, HT07 | Feedback works when it is specific, about the task and the process, and comes after the attempt. | The debrief's evidence and its next step. | — |
| RT07, BR19 | Interleaving problem types improves the choice of method. The effect is strongest when the categories are similar. | Patterns interleave by family, and problems never name their pattern. | Mostly maths tasks. |
| C08, FSRS, RD22 | Spacing, an FSRS scheduler, and successive relearning. | Pattern schedules, paced to the interview date. | — |
| EKT93 | Deliberate practice: targeted, effortful work on weaknesses, with feedback. | Due patterns come first, and the debrief gives a next step. | The expertise literature is debated. |
| EMR17 | Parsons problems: learners finished faster, with equivalent learning. | Later: an "Arrange the lines" rung in Coached. | Not in the first build. |

## 3. The learner's path

A learner is preparing for a Python interview of easy problems, solved while someone watches.

1. **Practice page.** The "Preparing for" line shows the course goal plus what the learner added: "Easy problems in Python, live, someone watching and guiding. Interview on 20 November."
2. **Start.** Mock interview carries a "Fits what you're preparing for" badge. They start with Code, whose sentence reads "An **easy** **Python** problem, **coached**." They press Start.
3. **Building.** Three named steps show behind a progress bar. The time is measured in §9.
4. **Understand.** The coach asks what the function returns and what could be unclear. They ask "Can the list be empty?", and the coach answers from the problem's clarifications.
5. **Examples.** They add three rows, and each is checked. The panel says "Your examples catch 2 of 3 common mistakes". In Coached it names the one missed: "an input where the same number appears twice". They add a fourth.
6. **Plan.** They type the plan, or say it. The coach asks what a lookup costs.
7. **Code.** They write the function. Run examples (Ctrl+Enter) shows each case's expected and actual result.
8. **Test.** Submit runs every case: 7 of 8 pass. In Coached the failing case shows its input, and they fix it.
9. **Review.** The coach asks for the complexity. They answer and press Finish.
10. **Debrief.** Four criteria, each with evidence, and the strong answer. "Hash map lookup comes back in 3 days." A button, "Try it under interview conditions", opens Mock interview's options at Live coding, in the setting this course last used there (Mock the first time).
11. **A week later, in Watched.** The interviewer stays quiet while they type. After a run it asks "Why a set here?", and later asks them to talk through the edge cases. They use the mic, and the replies are read aloud, as in any voice chat.

## 4. The coding session

### 4.1 Tasks

| Task | Built from | Checked by | Stages |
|---|---|---|---|
| Problem | `ProblemSource`: a statement, a signature, cases, a hidden reference and misreadings | the app's case harness, case by case (§7.4) | Understand, Examples, Plan, Code, Test, Review |
| Fix a bug | a proven Problem whose starter code is one of its misreadings | the same harness | Reproduce, Locate, Fix, Verify |
| Project | today's code lab generator: a brief, starter files, hidden checks | the model's `checks.py`, test by test when it uses the check helper | Understand, Plan, Build, Test, Review |

- **Fix a bug** replaces the Debugging kind for new work.
- **Project** replaces the Code lab and Project kinds. A runtime is optional; without one, the debrief reads the code and the conversation, so there is still a way to finish and get feedback.

### 4.2 Settings

| | Coached | Mock | Watched |
|---|---|---|---|
| Counterpart | A coach who teaches | An interviewer, in role | An interviewer who watches the code change |
| Clock | Elapsed time only | The session budget (default from difficulty, changeable). It is soft and never ends the session. | As Mock |
| Stages | The coach opens each stage. "Next" moves on, and any stage can be skipped. | If you skip Examples or Plan, the interviewer asks for them, as interviewers do | As Mock |
| Hints | All four rungs, any time | All four rungs, on request. Each is noted in the debrief. | You ask the interviewer ("I'm stuck"). It answers in role from the same ladder, and the hint is noted. |
| A failing hidden case shows | Its input, the expected output and your output | Its category only ("an empty input") | As Mock |
| Worked solution | After the first submit | In the debrief | In the debrief |
| It speaks | On every message, every stage and every run, and after three minutes with no edit, run or message (a starting value, tuned in §9) | On your messages, after a submit, and when you skip a stage | As Mock, plus at checkpoints: after a run, when you pause typing with changes it hasn't seen, and after something you say. It may choose to stay silent. |
| Help level recorded | Full help | Hints only | Hints only |

You can change the setting during a session. The change is recorded, as `ModeChanged` is today, and the debrief names every setting the session used.

### 4.3 Examples before code

- **Adding examples.** The learner adds rows of arguments and an expected result, typed as Python literals in one small field per parameter, taken from the signature.
- **Checking each row.** The row's arguments are run on the reference inside the sandbox; nothing runs on the learner's code at this point.
  - A row is "Right" when the reference agrees.
  - Otherwise, Coached shows "The problem expects …". Mock and Watched say "That's not what the problem expects. Check the statement", as an interviewer would.
- **Coverage.** Each Right example also runs on every misreading; a row the reference disagrees with counts for nothing until it's fixed. An example catches a misreading when the two results differ. The panel shows "Your examples catch 2 of 3 common mistakes". Coached describes one that is still uncaught; Mock and Watched give only the count.
- **Reuse.** Your examples join the statement's examples in Run examples.

### 4.4 Running

- **Run examples (Ctrl+Enter).** Runs the statement's examples and yours against your code. For each case you see the expected result, your result, what you printed, and the time taken.
- **Try an input.** Runs your code on any arguments. Coached also shows the reference's answer.
- **Submit (Ctrl+Shift+Enter).** Runs every case.
  - Failures are listed first.
  - A one-line summary is announced to screen readers: "7 of 8 passed. Failing: an input with duplicates".
- **Per-case results.** These are possible because the app writes `checks.py` itself (§7.4). The model supplies case data, not test code.
- **No cold start.** The first run is already warm, because the build's proof runs have compiled the interpreter.

### 4.5 Hints

| Rung | Gives | Never gives |
|---|---|---|
| 1 Nudge | A question about what to look at | Code |
| 2 Idea | The idea or structure that fits, and why | Code |
| 3 Plan | Steps in pseudo-code, each with its reason | Python |
| 4 Point at the line | The lines in your code that are wrong, and why, highlighted in the editor | The fixed line |

- **One rung at a time.** The next rung appears only after the current one has been read [A03].
- **Every hint carries its reason** [MWP19].
- **Checked before it is shown:**
  - rungs 1 and 2 contain no code;
  - rung 3 has no runnable Python;
  - rung 4's lines exist in the current draft;
  - any code in a hint is run as the solution, and the hint is refused if that code passes every case [K24];
  - no line of the reference appears verbatim. Lines that are also in the starter, or that hold only keywords and punctuation, don't count.
- **Who sees what.** The hint writer sees the reference, the draft, the last run and the examples. The live counterpart does not see the reference, just as today's interviewer does not see the answer guides.
- **Recorded:** the rung, the stage, the time and the run. Hints appear in the debrief and feed the pattern rating (§4.9).
- **Rungs are per surface.** Coding has its own four rungs in `CodeHintRung`. Written practice keeps its own. Both sit behind one `AssistancePolicy` (§7.1).

### 4.6 The counterpart

**What it sees** (`WorkspaceObservation`):
- the public problem: statement, signature, constraints and examples;
- its clarifications, target complexity and patterns;
- the stage and the clock;
- the draft with line numbers, and what changed since it last looked;
- the last run, case by case;
- the examples and their verdicts;
- the hints given so far, and the transcript.

It never sees the reference or the misreadings.

**When it speaks** is decided by code (the policy, §7.1), following the table in §4.2, not by the model. At Watched checkpoints the model may also choose silence (`speak: false`).

**Checked in every reply** (re-asked through `complete_checked`, as today):
- An interviewer asks at most one question per turn. Question-and-answer interviews keep the existing rule of exactly one.
- An answer to a clarifying question comes from the problem's clarifications, or says "Your call: state your assumption".
- Any code in a reply is run, and the reply is refused if that code passes every case.
- Line references point at lines that exist.
- No internal ids appear (an existing check).

**The editor never waits for the counterpart.** Replies land in the thread while the learner keeps typing. Meanwhile, the thread says what the counterpart is doing ("Reading your code…").

### 4.7 Voice

- The thread's message box carries the same optional mic as every other message box: `VoiceButton` with `useVoiceConversation`. Its `send` and `fill` callbacks already let a host plug in its own turn (`voice/hooks/useVoiceConversation.ts:29-35`).
- What you say becomes your turn, and replies are read aloud, as in any voice conversation.
- Not clicking the mic changes nothing.
- Watched suggests it ("Talk through your thinking: type or use the mic") but never requires it.

### 4.8 The debrief

**Criteria.** Each is scored 0–4, the same scale as the interview rubric (`interview_generation.rs:280`):

| Criterion | What earns points |
|---|---|
| Problem solving | Understood and clarified the problem, chose a sound approach, and reached the target complexity or explained the trade-off. |
| Code | Correct on the cases, clear, idiomatic. |
| Testing | Examples before code, edge cases, ran the code, read the failures and fixed them. |
| Communication | Explained the plan and the decisions, answered probes, and used hints well. |

**Each row** has:
- points, or "not enough evidence". Silence never scores zero, which is today's `feedback_boundary` rule (`practical_generation.rs:620`);
- evidence that the app checks: a quote that is an exact substring of a learner turn, lines that exist in a run's files, a run, an example or a hint;
- one next step.

**Shown beside the scores, not judged by the model:** time per stage, runs, hints by rung, and the setting.

**The strong answer:** the approach and its complexity, with the reference code folded away.

**Pattern ratings** are computed by rule (§4.9), not by the model. Scores are stored as rows (§7.3), so the course's evidence is a number, not text.

### 4.9 Coming back

- **A catalogue of problem patterns** in `lattice-core`, beside the drill catalogue: about twenty patterns in six families (lookup, scanning, ordering, structures, search, building). Each pattern has a recognition cue for the debrief, such as "pairs that sum to a target → remember what you've seen in a dict".
- **A rating for each pattern in the problem, by rule.** This mirrors the drills' `rating(&IssueOutcome)`:

| Outcome | Rating |
|---|---|
| Every case passes, with no hint and no worked solution | Easy |
| Every case passes, with at most a nudge | Good |
| Every case passes after an idea or plan hint | Hard |
| Not every case passes, a line hint was used, or the solution was seen before passing | Again |

- **Schedule.** `CardSchedule::review(rating, now, target)`, where the target is the interview date from "Preparing for", if one is set. Schedules are stored in `learning_drill_pattern_schedules` under the material `problem:<language>`, through the `Table` helpers in `drills/schedules.rs`. The drills use `code:<language>` and `passage`, so the two namespaces don't collide.
- **The next problem.** `targets()` ranks patterns: due ones first, then ones never practised, then the rest, interleaved by family. It picks the patterns the next build must use.
- **Pattern names stay hidden.** The problem never names its pattern; the build checks the statement against the catalogue names. Coached's Idea rung may name it. Due, Continue and the building screen say "problem patterns" or the lesson instead; the debrief is the first screen to name one.

## 5. Preparing for

- **On the page.** The top of Practice reads "Preparing for:", then the course goal and the learner's details, then the interview date if one is set, with an Edit button.
- **Fields:** details (free text), language, interview date, company and role. The last two are defaults for Mock interview.
- **Storage:** one row per course, in the new table `learning_practice_profiles`.
- **Who reads it:**
  - the problem source: difficulty, style and language;
  - Mock interview setup: its defaults;
  - drills: the default language, plus the details in the lesson brief. That change must pass the drills' own fit bars before it ships;
  - written practice task generation.

  This is the learner's own context, not a method text pasted into every prompt.
- **The interview date** paces the pattern schedules and the interview deck, through the existing `set_study_deck_target` command and the schedule's `target_at` (`recall/schedule.rs:52-57`).

## 6. One Practice area

### 6.1 What overlaps

Each row was checked against the code. The UI audit and the backend map are the sources.

| Overlap | Verdict | Resolution |
|---|---|---|
| Code review practical vs Review drills | **Same job**: find what is wrong in code you didn't write. The practical has no comment tool, no answer key and no grade. | Code review leaves the new-activity choices. "Spot the mistakes" is the drill. |
| Debugging practical vs the new Fix a bug | **Same job.** Fix a bug adds proof, per-case results and stages. | Fix a bug replaces Debugging for new work. |
| Debugging vs drills | **Different jobs**: fixing against checks vs finding by reading. | Both stay. Fix a bug can reuse the drill's `CodeContrast` view for "what was wrong". |
| Code lab vs Project | **Same job.** The composer already treats them alike; the only difference is whether a runtime is required (`ActivityComposer.tsx:136-137`). | Both become the Project task. |
| Writing revision kind vs written practice | **Same job**, but the practical version has no submit or feedback. | Moves to Explain: "Revise a draft" starts the attempt prefilled with the text. |
| Interview vs Conversation vs Incident vs System design simulations | **One engine**: the same tables, repository, commands and turn generator, with forked prompts. | One `TurnPolicy` seam (§7.1). Mock interview and Role-play are presets on it. |
| Interview deck vs study sessions | **Already one implementation**, entered from the interview's brief. Its date field starts at a guess (tomorrow at 9:00), and the deck opens in Studio, outside the course (`InterviewDrill.tsx:10-30`). | Opens in a drawer in the course. The date comes from Preparing for, or is asked for. |
| Workbench tutor, simulation log, Tutor tab | **Different contracts** (graded help vs free chat), but three separate UIs. | One `PracticeThread` for every practice conversation. The Tutor tab keeps the app chat. |
| `AnswerTimer` vs `InterviewClock` | Same | One `ElapsedClock`. |
| `RubricPanel`, interview scorecard markdown, drill outcome badges | **Same job**: "what you did, against criteria". | One `CriteriaList`. |
| Three "insufficient evidence" encodings: interview `insufficient_evidence`, practice grade `Uncertain`, simulation evidence `uncertain` with a NULL score | Same | One `Scorecard` type. Coding uses it first; the interview debrief and written grade move to it later (§11). |
| Written practice's hint ladder vs no help in practicals | **Same principle**, applied to different material | One `AssistancePolicy`, with rungs per surface. |
| Four start buttons in four places | Same | The Practice page (§6.2). |
| `MisconceptionCard` vs Feynman's missed concept | **Same mutation**: both save a recall card draft (`PracticeWorkbenchPanel.tsx:87-95`, `ProgramWorkspace.tsx:404-411`) | One `SaveAsReviewCard`. |
| Feynman vs written practice | **Different jobs** | Feynman becomes an Explain option, "Teach it to a beginner". It is otherwise unchanged. |

### 6.2 The Practice page

**Top to bottom:**
- **Preparing for** (§5).
- **Continue**: the open session, attempt or drill, and where it stopped.
- **Due**: patterns and lessons that are due, from drills and from coding, with one "Start the most overdue" button.
- **Five cards**, each with a one-line promise and one Start button:

| Card | Promise | Starts | Replaces |
|---|---|---|---|
| Explain | Answer in your own words, get feedback, revise. | Written practice, with "Teach it to a beginner" as an option | The Written practice tab and its Feynman toggle; the Writing revision kind |
| Spot the mistakes | Review code or an explanation with planted mistakes, then see what you missed. | A drill | The Review drills tab; the Code review kind |
| Code | Solve a problem, fix a bug or build a project, and run it. | A coding session, in Coached | The Code lab, Project and Debugging kinds |
| Mock interview | Rehearse with an interviewer: live coding, debugging, technical questions, system design or behavioral. | A coding session in Mock or Watched (for Live coding and Debugging; Coached stays on the Code card), or today's interview | The Interview kind |
| Role-play | Talk through a scenario: an incident, a design review, a hard conversation. | A role-play | The Conversation, Incident and System design kinds |

- **Options read as a sentence.** Each card's defaults read as one line, and each choice in it is a menu. Examples: "An **easy** **Python** problem, **coached**." and "A **live coding** round, **mock**, **20 min**." Changes are remembered per course.
- **History**: one list of past sessions, attempts and drills, with filters.
- **The badge.** Mock interview shows "Fits what you're preparing for" when the profile has an interview date or mentions an interview. It is a badge, not a reorder: cards keep their positions.

### 6.3 One vocabulary

| Today | From now on |
|---|---|
| "Practice" as the tab, a mode, a drill mode and a composer mode | Practice is only the area. |
| Explore / Practice / Demonstrate; Open aid / Guided aid / Independent; aid contract; aids | **Help**: Full help / Hints only / No help |
| Interview (as a drill mode) | Timed |
| Drill deck (the interview deck) | Review deck |
| Activity, practical, lab, exercise, brief, simulation | **Session** (coding, role-play), **attempt** (written), **drill** (Spot the mistakes) |
| Execution environment, runtime profile | Runs on: Built-in Python, or a container |
| Contained evaluator | Checks |
| Retry same operation; Send simulation turn | Try again; Send |
| Revision N | Not shown |
| Weighted recall, Precision, False positives, Valid extras | Found, Missed, Not an issue, Also valid |

### 6.4 Shared pieces (frontend)

| Piece | Replaces | Used by |
|---|---|---|
| `PracticeThread`: a transcript and a message box with the mic, Ctrl+Enter shown, focus kept, and a busy line. The mic is the existing `VoiceButton`: the textarea stays, and `VOICE_STATUS` ("Listening…") takes the shortcut hint's place beside it. No other mic indicator on the screen. | The `SimulationPanel` log; the workbench tutor list | Coding, role-play, the written-practice tutor |
| `ElapsedClock`, with a soft limit | `AnswerTimer`, `InterviewClock` | Coding, interviews, drills |
| `CriteriaList`: points or "not enough evidence", evidence and a next step, with an icon and text for each state | `RubricPanel`, the scorecard markdown, the drill outcome badges | Debriefs, written results, drill reveal |
| `CaseResults`: failures first, with a summary in a live region | `RunOutput`'s single row | Coding sessions, projects |
| `FileTabs` | The tab lists in `ArtifactEditor`, the drills' `CodePane` and `CodeContrast` | All three |
| `StatusLine`: busy and error states, in `aria-live` | About twelve copied strips | Everything |
| `SaveAsReviewCard` | `MisconceptionCard`, Feynman's missed concept | Written practice, debriefs |
| `unwrap` and `newId` beside the client in `src/lib/api.ts`; error text through the existing `getErrorMessage` (`src/lib/errorUtils.ts:21`) | 18 copies of `unwrap`, 13 id helpers, 10 error-text helpers | Everything |
| The existing `useDraft` (`src/shared/drafts/drafts.ts:65`) for unsent text | Feynman's own `localStorage` effects | Feynman, every practice message box |
| `useAutosave`, with a conflict state | Three autosave implementations, each with its own conflict check (`PracticeWorkbenchPanel.tsx`, `useLearningLabDraft.ts`, `useQuizAutosave.ts`) | Written practice, code, quizzes |

### 6.5 Shared paths (backend)

- **`TurnPolicy`.** The four prompt branches in `generate_simulation_turn` (`practical_generation.rs:610-617`) become two policies, InterviewQuestions and Scenario. Coach, Interviewer and WatchingInterviewer join them.
- **`Scorecard`.** The coding debrief uses it first.
- **`CodeRunner`** (from drills) runs the problem proofs, the example checks and the hint checks. Learner runs stay on `practical_runs` (registry, cancel, recovery) and gain per-case rows.
- **Pattern schedules** use the drills' table and helpers, with a new material namespace.
- **The problem build** is a job (progress, cancel, recovery), like a drill build. Practical generation today is a plain awaited call with no progress.

## 7. Architecture

### 7.1 Seams

```rust
/// Where a problem comes from. The model writes one today; a pasted problem is the next source.
#[async_trait]
pub trait ProblemSource: Send + Sync {
    async fn problem(&self, request: &ProblemRequest, steps: &dyn BuildSteps, cancel: CancellationToken) -> Result<Problem>;
}

/// Cases in one language: the evaluator a run adds beside the learner's files, and each case's result.
pub trait CaseHarness: Send + Sync {
    fn evaluator(&self, problem: &ProblemSpec, cases: &[Case], nonce: &str) -> Vec<LearningLabFile>;
    fn results(&self, stdout: &str, nonce: &str) -> Vec<CaseResult>;
}

/// The counterpart: when it speaks, what it is told, and what a reply must satisfy. `S` is what it may read:
/// a simulation's activity and transcript, or a coding session's `WorkspaceObservation`, never an answer key.
pub trait TurnPolicy<S>: Send + Sync {
    fn cue(&self, event: &SessionEvent, state: &S) -> Option<Cue>;
    fn system(&self, cue: &Cue, state: &S) -> String;
    fn check(&self, content: &str, cue: &Cue, state: &S) -> Result<()>;
}

/// What help a surface allows in a help level. Each surface keeps its own rungs and its own ledger.
pub trait AssistancePolicy: Send + Sync {
    fn allows(&self, help: Help, level: HelpLevel) -> bool;
}

/// A scorecard whose every piece of evidence the app has checked.
#[async_trait]
pub trait Debriefer: Send + Sync {
    async fn debrief(&self, evidence: &SessionEvidence) -> Result<Scorecard>;
}
```

The session holds its `Debriefer` beside its policy rather than reaching it through the policy, so a policy only decides when and how the counterpart speaks. A policy's `check` holds only its own rule on the text (an interviewer asks one question); the checks every reply gets, and the ones that run code, stay in the turn generator.

`BuildSteps` is new and has one method. The build job implements it over `JobContext::progress` (`lattice-core/src/shared/runtime/jobs/runtime.rs:148`), and tests record the steps. `LearningLabFile` is the lab's existing file type (`runtime/lab_runtime.rs:133`).

A setting is an enum, not a trait. `Setting::discloses(hidden, submitted)` answers what the learner may see: a failing hidden case's input, an uncaught misreading, the worked solution (Coached shows it after the first submit). The DTO mapper applies it, so one stored session can be shown in any setting.

How the SOLID principles land:

| Principle | Here |
|---|---|
| **Single responsibility** | Each type has one job: a source writes problems, a proof proves them, a harness runs cases and reads results, a policy decides when and how the counterpart speaks, a debriefer scores, and the rating rule rates. |
| **Open/closed** | A new problem source, language, setting or counterpart is a new implementation. The session code does not change. |
| **Liskov substitution** | Every `TurnPolicy` keeps the same contract: a reply passes `check` or is asked for again. Today's interview behaviour becomes the InterviewQuestions policy, unchanged, and its current tests prove that. |
| **Interface segregation** | The counterpart gets an observation, not the session's tables. The hint writer gets the reference; the counterpart does not. |
| **Dependency inversion** | The session depends on these traits, and the desktop plugin wires up the implementations. |

### 7.2 Modules

**Backend: `crates/lattice-learning/src/practice/coding/`**

| File | Holds |
|---|---|
| `problem.rs` | `Problem`, `Case`, `Compare` (exact, unordered, float within a tolerance the case states), `Misreading`, `Clarification`, `Signature` |
| `source.rs` | `ProblemSource`, and `GeneratedProblems` with its prompt, schema and checks |
| `proof.rs` | The proof: the reference passes every case; each misreading fails at least one, and the case that kills it is recorded; examples agree with the reference; reference timings for scale cases |
| `harness/` | `CaseHarness`, `python.rs`, the `checks.py` template |
| `session.rs` | Start, stages, examples, run scopes, the clock, finish |
| `observation.rs` | `WorkspaceObservation` |
| `policy.rs` | Coach, Interviewer, WatchingInterviewer |
| `hints.rs` | `CodeHintRung`, the hint writer and its checks |
| `debrief.rs` | The coding `Debriefer` and its evidence checks |
| `patterns.rs` | The rating rule; schedule reads and writes through the drills' helpers |
| `jobs.rs` | The build job and its steps |

**Elsewhere:**
- **Core** (`crates/lattice-core/src/domain/learning/practice/`): `problem_patterns.rs` (the catalogue) and `scorecard.rs` (`Scorecard`, `Evidence`).
- **Simulation refactor**: `practice/simulation_policy.rs`, holding `TurnPolicy` and the two existing policies, taken out of `practical_generation.rs:522-688`.
- **Desktop**: `plugin/coding.rs` (the commands), registered in `plugin.rs`.
- **Frontend** (`src/features/learning/practice/`):
  - `home/`: `PracticeHome`, `StartSentence`, `PreparingFor`, `DueStrip`, `History`;
  - `coding/`: `CodingSession`, `StageRail`, `ProblemPane`, `ExamplesTable`, `RunBar`, `CaseResults`, `HintLadder`, `Debrief`;
  - `shared/`: the pieces in §6.4.

### 7.3 Data

One migration:

- **New kind.** Rebuild `learning_practical_activities` with `coding_problem` in its kind CHECK. The CHECK can't be altered in place, so the table is rebuilt, as earlier kind changes were.
- **`learning_coding_sessions`**, one row per simulation session:
  - `id` (primary key, references `learning_simulation_sessions` on delete cascade);
  - `program_id`, `activity_id`;
  - `task` (problem, bug_fix, project);
  - `setting` (coached, mock, watched);
  - `stage`;
  - `budget_seconds` (NULL in Coached);
  - `active_ms`, `opened_at`, `finished_at`, `revision`.
- **`learning_coding_events`**: (`session_id`, `ordinal`) is the primary key.
  - `kind` is one of stage, run, example, hint, setting, opened, closed or draft_seen.
  - Each row also holds `payload_json` and `at`.
  - These give time per stage, and the timeline that the counterpart and the debrief read.
- **`learning_coding_examples`**: `session_id`, `ordinal`, `args_json`, `expected_json`, `verdict` (right, wrong, invalid) and `catches_json`.
- **`learning_practical_runs`** gains:
  - `coding_session_id`, NULL, on delete set null;
  - `scope`, NULL, one of examples, all or input.
- **`learning_practical_run_checks`** gains `detail_json` (NULL): the category, arguments, expected and actual output, what was printed, and any error. The table already allows many rows per run; today only ordinal 0 is written.
- **`learning_scorecards`**:
  - `session_id`, `criterion_id`, `ordinal`;
  - `status` (observed or insufficient_evidence);
  - `points` (NULL when there isn't enough evidence) and `max_points`;
  - `evidence_json`, `feedback`, `next_step`.
- **`learning_practice_profiles`**: `program_id` is the primary key and references `learning_programs` on delete cascade. It also holds `details`, `language`, `interview_at`, `company`, `role` and `revision`.
- **The hidden problem** lives in the activity's check-role file `problem/problem.json`, the way interview answer guides live in `interview/answer-guides.json`.
- **Pattern schedules** stay in `learning_drill_pattern_schedules`, under material `problem:<language>`.

**Clock.** Time counts only while the session is open on screen. Hiding or closing the window pauses it; this is recorded as opened and closed events. Coming back hours later never shows a mock interview as three hours long.

### 7.4 The Python harness

- **Files.** For each run, the app writes `checks.py` (the built-in Python's entrypoint and evaluator path, `runtime/embedded_runtime.rs:52-56,73`) and `cases.json`. Both sit read-only beside the learner's `solution.py`.
- **For each case**, the harness:
  - deep-copies the arguments and calls the function;
  - captures what the function prints;
  - compares with the case's `Compare`;
  - catches exceptions;
  - times the call;
  - prints one line, `<nonce>{json}`, as soon as the case ends.
- **Exit status.** It exits non-zero if any case fails, so today's run status stays correct.
- **Parsing.** The app parses those lines into one check row per case.
  - If a run times out, the finished cases keep their results and the rest show "not run".
  - Scale cases run last, and the proof keeps their reference time well inside the run limit.
- **Projects.** The model's `checks.py` can import the same helper (`lattice_check.case(name)`) to report each test. A `checks.py` without the helper reports one row, as today.
- **Example checks** use `CodeRunner` on the reference and the misreadings, not the learner's code. The literals are evaluated with `ast.literal_eval` inside the sandbox.

### 7.5 Commands

Operation ids and replay work as in the practical commands.

| Command | Does |
|---|---|
| `start_coding_build(program_id, lesson_id?, task, setting, language, difficulty, focus?)` → job | Builds and proves a problem, then saves the activity |
| `start_coding_session(activity_id, setting)` → session | Creates the simulation and coding rows; the policy gives the first turn |
| `get_coding_session(session_id)` → `CodingSessionDto` | Everything the screen shows, redacted by setting |
| `move_coding_stage(session_id, stage, expected_revision)` | Records the stage, and may cue a turn |
| `check_coding_example(session_id, args, expected)` → `ExampleDto` | Returns the verdict and the coverage |
| `run_coding_session(session_id, scope, files, input?)` → run | Runs through `practical_runs` with that scope's evaluator |
| `ask_coding_hint(session_id, expected_revision)` → turn | Returns the next rung, checked |
| `send_coding_turn(session_id, content, spoken, expected_revision)` → turns | Adds the learner's turn; the policy decides whether a reply follows |
| `observe_coding_draft(session_id, files)` | A Watched checkpoint after a pause in typing; may produce a turn |
| `set_coding_clock(session_id, open)` | Active-time accounting |
| `finish_coding_session(session_id)` → debrief | Writes the scorecard, the pattern ratings and the evidence events |
| `get_learning_practice_profile`, `save_learning_practice_profile` | Read and save Preparing for |
| `practice_due(program_id)` | Patterns and lessons due, across drills and coding |

### 7.6 What changes in existing code

- **`generate_simulation_turn`** (`practical_generation.rs:522-688`) takes a `TurnPolicy`. Its behaviour does not change, and the existing tests prove that.
- **`finish_run`** (`practical_repository/runs.rs:180-255`) writes one row per case when the harness reports cases, and one row otherwise.
- **`plugin/practical.rs:197-204`** stays as it is for Interview. `coding_problem` takes a runtime.
- **`ProgramWorkspace.tsx`**: the Practice group becomes `PracticeHome`. The `practicalOpened`, `drillsOpened` and `writtenOpened` flags (`:82-88`) give way to one `usePracticeView`: home, or the open session, attempt or drill.
- **`ActivityComposer`** stops offering code_review, debugging, project and writing_revision. Activities of those kinds that already exist still open.

## 8. UI audit: what gets fixed and where

The audit read the code only; screens are inferred from markup. Every claim cited here was checked against the source.

| # | Finding | Fix | Lands in |
|---|---|---|---|
| P1-1 | Tutor, critique and hint requests only disable the buttons; nothing shows in the thread until the reply lands (`PracticeWorkbenchPanel.tsx:361-362`) | `StatusLine` in the thread, using `aria-live` | Shared pieces |
| P1-2 | Practical kinds that can't run have no way to finish or get feedback (`practiceSessionId: null`, `PracticalWorkbenchPanel.tsx:225`; the Run button is disabled with no reason, `ArtifactEditor.tsx:53,108`) | Those kinds leave the new choices. Projects get the coach and the debrief. Old activities get "Get feedback". | Code › Project |
| P1-3 | One word, many meanings | §6.3 | Practice page |
| P1-4 | Four or five layers of choices before any work | One Start per card, with options as a sentence | Practice page |
| P2-5 | Jargon | §6.3, and plain labels | All |
| P2-6 | 10–11 px body text, and walls of panels | A 12 px floor for anything you read. Asides fold. | All |
| P2-7 | Criteria hidden inside a closed `<details>` | `CriteriaList`, open by default, beside the work | Shared pieces |
| P2-8 | Disabled controls with no reason given | Each one states its reason beneath it | All |
| P2-9 | The textarea loses focus after every turn; Ctrl+Enter is not shown | `readOnly` plus `aria-busy`, refocus when the reply lands, and the shortcut shown | `PracticeThread` |
| P2-10 | Run results not announced; the panel is always dark | `CaseResults`: a summary with `role="status"`, failures first, theme tokens | Coding |
| P2-11 | No streaming in practice chats | The thread shows what the counterpart is doing, and the editor never waits. A checked reply can't be shown before its check passes, so streaming comes later (§11). | `PracticeThread` |
| P2-12 | The review deck opens in Studio, outside the course, and its date starts at a guess, tomorrow at 9:00 (`InterviewDrill.tsx:10-30`) | A drawer in the course; the date comes from Preparing for, or is asked for | Practice page |
| P2-13 | An open drill is buried under the start form | The Continue card | Practice page |
| P2-14 | No task statement in a drill review | One line above the panes: "Find the 4 issues…" | Drills |
| P3-15 | States shown by colour alone | An icon and text for every state | All |
| P3-16 | Two `main` landmarks; heading order | One `main`, and headings in order | Practice page |
| P3-17 | `aria-label` on a plain div (`PracticeWorkbenchPanel.tsx:329`); ⌘F shown on Windows (`LearningCodeEditor.tsx:239`); the run shortcut undocumented | Fix all three | Workbench, editor |
| P3-18 | Small screens: the shelf pushes the editor down; `calc(100dvh-8rem)` | The session stacks into tabs; heights come from the layout | Coding, drills |
| P3-19 | The back button "← Review drills" is shown inside a review | "Back to drill setup" | Drills |
| P3-20 | Radio cards with no description wired up | A fieldset and `aria-describedby` | Start sentence |
| P3-21 | Loading and empty states differ everywhere | Shared loading and empty states | Shared pieces |

**Code smells:**
- **Split the oversized components:**
  - `ProgramWorkspace.tsx`, 510 lines → `PracticeHome` plus `usePracticeView`;
  - `PracticeWorkbenchPanel.tsx`, whose line 356 alone is 2,768 characters → `usePracticeSession` plus its parts;
  - Feynman's analysis → `feynmanAnalysis.ts`.
- **Remove duplication:**
  - 18 copies of `unwrap`, 13 id helpers and 10 error-text helpers → §6.4;
  - two lists of interview round labels (`ActivityBrief.tsx:8`, `ActivityComposer.tsx:161-162`) → one;
  - the retry pattern written out in five panels (`PracticeWorkbenchPanel`, `SimulationPanel`, `RecallEvolutionPanel`, `SourceMaintenancePanel`, `SourcesPanel`) → one hook.
- **Effects:**
  - replace state derived by effects (`DrillChoose.tsx:122-133`, `PracticalWorkbenchPanel.tsx:69-71`, `SimulationPanel.tsx:47-50`) with computed values;
  - Feynman's save effect runs on mount and writes the empty initial state before the saved draft loads (`FeynmanTechniquePanel.tsx:309-335`). It moves to `useDraft`.
- **Magic strings and numbers:**
  - typed busy states instead of comparing labels (`busyLabel === 'Saving response'`);
  - conflict checks match a regex over the message (`PracticeWorkbenchPanel.tsx:50-53`, `useQuizAutosave.ts:16`, `StartingPointPanel.tsx:92`). `ErrorCode` has no conflict code (`lattice-core/src/shared/ipc/mod.rs:152`): add one, and match `details.code`;
  - each `maxLength` literal either gets a nameable purpose or goes.
- **Theme:** tokens replace raw palette classes and hard-coded hex (`RunOutput.tsx:18` is `#211f1b` in both themes).
- **Misnamed and inert code:**
  - `SparklesIcon` draws a plus (`ActivityComposer.tsx:494-496`);
  - the new-attempt form filters its modes by the selected session, but none is selected there, so the filter never removes anything (`PracticeWorkbenchPanel.tsx:347`).

## 9. Measured before it ships

**Ground rules:**
- Test runs use claude-haiku-5-5, and every result names its model.
- Reading is by hand, in the dated write-up `evals/coding/2026-11-xx-CODING-SESSIONS.md`.
- One or two items' difference between runs is noise.

**1. Problem builds** (Python, Easy, 30 builds):
- At least 27 of 30 prove within the build's re-ask budget: the reference passes, and at least two misreadings are killed.
- p50 and p90 build times are recorded.
- Read by hand, the statement is clear and matches its cases in at least 28 of 30.
- "Easy" holds by my reading in at least 27 of 30.
- The pattern tags are right in at least 27 of 30.
- No statement names its pattern: 30 of 30, checked by code.

**2. Hints** (40 per rung, over 10 problems with seeded wrong drafts):
- No hint's code passes the cases. This is checked by code, and a single failure blocks the release.
- At least 36 of 40 per rung are on-rung by hand.
- Rung 4 points at the wrong line in at least 36 of 40.

**3. Counterpart** (20 replayed sessions per setting, built from scripted learner turns, code states and runs):
- Everything it says about the code or a run is true in at least 19 of 20.
- Answers to clarifying questions agree with the problem in at least 19 of 20.
- The interviewer's question rule holds in every reply, checked by code.
- For Watched, the interjection rate is recorded, and at least 16 of 20 interjections read as useful and realistic.

**4. Debrief** (20 sessions, from strong to weak):
- All evidence is valid, checked by code.
- At least 85% of rows' points are within one point of my grading.
- The pattern-rating rule is covered by unit tests.

**5. Latency** (on Haiku):
- p50 and p90 for a counterpart turn and for a build;
- p50 for Run examples once warm;
- a UI test proves the editor stays usable while a turn is pending.

**6. Walkthrough** in a test build, on a copy of the data:
- the §3 path in Coached, then Mock, then Watched with the mic, with screenshots in the light and the dark theme (every new screen uses only the theme tokens in `src/index.css`, so both themes come from one set of classes);
- the whole path done by keyboard alone;
- no serious axe violations on the new screens.

**7. No regressions:**
- The existing suites stay green on CI.
- After the `TurnPolicy` refactor, the interview eval is replayed, and its validator decisions must not change.

## 10. Build order

All builds run on one integrated branch with one CI run, and fixes are batched. Each step's failing tests are seen red on CI before its fix goes in.

**Wave 1** (independent; everything starts together):

1. **Harness and per-case results** (backend). Tests:
   - a three-case run reports three rows;
   - a timeout keeps the finished cases;
   - a project `checks.py` without the helper reports one row.
2. **Problem model, catalogue, generated source, proof and build job** (backend). Tests:
   - a reference that fails a case is asked for again;
   - a misreading that passes everything is dropped;
   - a statement that names its pattern is refused.
3. **`TurnPolicy` extracted with the two existing policies** (backend). The existing simulation and interview tests are the proof; no behaviour changes.
4. **Shared frontend pieces** (§6.4), with tests. Workbench, simulation and drills move to them.
5. **The rest of the Practice area:**
   - the Practice page and start sentences;
   - Preparing for, with its table and commands;
   - the new vocabulary and History;
   - the composer's kind changes.

**Wave 2** (starts once wave 1's types are on the branch):

6. **Coding-session backend:** tables, commands, the clock, examples, run scopes, observation, the coding policies, hints and their checks.
7. **Debrief and scoring:** the debrief and scorecard, pattern ratings and schedules, `practice_due`, evidence events.
8. **Coding screens:** the session and debrief screens, the mic in `PracticeThread`, and the Watched checkpoints.

**Then:**
- run the §9 bars on Haiku;
- do the walkthrough in a test build;
- write it up.

## 11. Later

- Paste a problem you were given: a second `ProblemSource`.
- JavaScript (the runtime already has it), and container languages, for problems.
- An "Arrange the lines" rung in Coached [EMR17].
- A full interview loop that includes a live coding round.
- Streaming checked turns.
- The interview debrief and the written grade on the shared `Scorecard`.
- Practical runs on the jobs framework.

## Sources

New here (DOIs checked against Crossref, 2026-11-08):

- **[A03]** Aleven, Stahl, Schworm, Fischer & Wallace (2003). Help seeking and help design in interactive learning environments. *Review of Educational Research* 73(3). doi:10.3102/00346543073003277
- **[BC01]** Beilock & Carr (2001). On the fragility of skilled performance: what governs choking under pressure? *Journal of Experimental Psychology: General* 130(4). doi:10.1037/0096-3445.130.4.701
- **[BSBP20]** Behroozi, Shirolkar, Barik & Parnin (2020). Does stress impact technical interview performance? *ESEC/FSE '20*. doi:10.1145/3368089.3409712
- **[D19]** Denny, Prather, Becker, Mooney, Homer, Albrecht & Powell (2019). A closer look at metacognitive scaffolding: solving test cases before programming. *Koli Calling '19*. doi:10.1145/3364510.3366170
- **[EKT93]** Ericsson, Krampe & Tesch-Römer (1993). The role of deliberate practice in the acquisition of expert performance. *Psychological Review* 100(3). doi:10.1037/0033-295X.100.3.363
- **[EMR17]** Ericson, Margulieux & Rick (2017). Solving Parsons problems versus fixing and writing code. *Koli Calling '17*. doi:10.1145/3141880.3141895
- **[FEB11]** Fox, Ericsson & Best (2011). Do procedures for verbal reporting of thinking have to be reactive? A meta-analysis and recommendations for best reporting methods. *Psychological Bulletin* 137(2). doi:10.1037/a0021663
- **[K24]** Kazemitabaar, Ye, Wang, Henley, Denny, Craig & Grossman (2024). CodeAid: evaluating a classroom deployment of an LLM-based programming assistant that balances student and educator needs. *CHI '24*. doi:10.1145/3613904.3642773
- **[KA07]** Koedinger & Aleven (2007). Exploring the assistance dilemma in experiments with cognitive tutors. *Educational Psychology Review* 19. doi:10.1007/s10648-007-9049-0
- **[LKJ16]** Loksa, Ko, Jernigan, Oleson, Mendez & Burnett (2016). Programming, problem solving, and self-awareness: effects of explicit guidance. *CHI '16*. doi:10.1145/2858036.2858252
- **[MGC12]** Margulieux, Guzdial & Catrambone (2012). Subgoal-labeled instructional material improves performance and transfer in learning to develop mobile applications. *ICER '12*. doi:10.1145/2361276.2361291
- **[MLWP19]** Marwan, Lytle, Williams & Price (2019). The impact of adding textual explanations to next-step hints in a novice programming environment. *ITiCSE '19*. doi:10.1145/3304221.3319759
- **[MMG15]** Morrison, Margulieux & Guzdial (2015). Subgoals, context, and worked examples in learning computing problem solving. *ICER '15*. doi:10.1145/2787622.2787733
- **[MWP19]** Marwan, Williams & Price (2019). An evaluation of the impact of automated programming hints on performance and learning. *ICER '19*. doi:10.1145/3291279.3339420
- **[OP09]** Oudejans & Pijpers (2009). Training with anxiety has a positive effect on expert perceptual–motor performance under pressure. *Quarterly Journal of Experimental Psychology* 62(8). doi:10.1080/17470210802557702
- **[RA03]** Renkl & Atkinson (2003). Structuring the transition from example study to problem solving in cognitive skill acquisition: a cognitive load perspective. *Educational Psychologist* 38(1). doi:10.1207/S15326985EP3801_3
- **[S08]** Shute (2008). Focus on formative feedback. *Review of Educational Research* 78(1). doi:10.3102/0034654307313795
- **[SK21]** Sinha & Kapur (2021). When problem solving followed by instruction works: evidence for productive failure. *Review of Educational Research* 91(5). doi:10.3102/00346543211019105
- **[WK19]** Wrenn & Krishnamurthi (2019). Executable examples for programming problem comprehension. *ICER '19*. doi:10.1145/3291279.3339416

Reused from `2026-10-25-learning-science-lessons.md`: [BR19], [C08], [FSRS], [HT07], [K03], [RD22], [RT07].
