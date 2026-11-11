# Practice sessions for any subject: kinds of work behind one session

Date: 2026-11-10. Status: architecture approved in outline 2026-11-10; being built. Builds on
`2026-11-08-coding-sessions-and-one-practice-area.md`, which stays the design of the Program kind.

**The ask.** Coding sessions read as a feature for courses about code. "I need every piece of these modules to be
agnostic to the source. We need to keep the coding training", and the same practice must work for any course: for
example a molecule shown with some of its parts missing, to be filled in.

**The calls made here:**
- **A session is not about code.** A session holds a piece of work, checks that decide whether it is right, a hint
  ladder, a counterpart, a clock and a debrief. All of that becomes one engine that imports nothing about programs.
- **An activity is a kind of work × a form × a setting.** The three axes vary independently.
  - *Kind* is what the piece is made of and how it is checked: Program today; Structure, Table and Sequence next.
  - *Form* is how the starting state is derived from proven material: Build, Repair, Complete, Arrange.
  - *Setting* is who watches and what is disclosed: Coached, Mock, Watched (unchanged).
- **A kind is a form of work, never a subject.** There is no biology kind and no chemistry rule anywhere. A molecule,
  a circuit, a food web and a state machine are all the Structure kind. Whatever is specific to a subject is data in
  the exercise, written by the model and verified against the course's sources.
- **Every exercise is proven before it is shown, by one template for every kind.** Program proves by running code.
  Kinds whose truth is the world prove by verifying each fact against the course's sources through the acceptance
  policy that lessons already use.
- **The app makes the gap, not the model.** The model writes a complete, correct piece as data. Code removes parts,
  shuffles them or swaps in a proven mistake. The answer key is right by construction.
- **Open along kinds, closed along forms and settings.** A new kind is a new folder and one registry entry. Forms
  and settings are small stored vocabularies; each keeps all of its rules in one file.
- **#422 lands only the seam.** Program moves behind the kind interfaces with no change in behaviour, and
  everything shared is named for practice sessions. Each further kind is a small PR of its own.

## 1. What exists today (#422 at `5adcdf3b3`)

`P` is `crates/lattice-learning/src/practice`.

| Piece | Where | Tied to code by |
|---|---|---|
| Session commands (start, stage, run, hint, turn, clock, setting, reveal, finish) | `P/coding/session.rs` | drafts are `Vec<LearningLabFile>`; runs go to the Python runtime |
| Counterpart policies (Coach, Interviewer, Watching interviewer) | `P/coding/policy.rs` over `P/simulation_policy.rs::TurnPolicy` | prompts say code, function, run |
| Hint ladder (Nudge, Idea, Plan, Line) | `P/coding/hints.rs` | checks parse and run fenced blocks; Line is a line of `solution.py` |
| Counterpart replies | `P/coding/counterpart.rs` | refuses a block that passes every case |
| Debrief and scorecard | `P/coding/debrief.rs`, core `scorecard.rs` | four criteria, one named "Code"; evidence kinds `lines`, `run` |
| Ratings and spacing | `P/coding/patterns.rs` | material `problem:<language>`, targets are algorithm patterns |
| What the counterpart sees | `P/coding/observation.rs` | files, line diffs, signature, complexity |
| Proof | `P/coding/proof.rs` | runs the reference and the misreadings in the harness |
| Problem source, harness, examples | `P/coding/source.rs`, `harness/`, `examples.rs` | Program by nature; they stay Program |
| Fit | `P/practice_fit.rs` | one stored bit, `teaches_code` |
| Storage | `learning_coding_sessions`, `_events`, `_examples`, `_debriefs` | names; `task`, `stage` and `scope` vocabularies |

The session already depends on three small interfaces (`CodingAids`, `CodingRuns`, `Debriefer`). The seam grows from
those. Nothing of #422's migration is on main, so names and stored vocabularies change in place, with no data
migration.

## 2. The model

### 2.1 Vocabulary

- **Exercise.** The proven material: a public brief, a hidden reference piece, hidden misreadings (wrong pieces, each
  with a description), cases, a proof record, and the starting piece for its form. Today's `Problem`.
- **Piece.** One state of the work: the learner's draft, the reference, a misreading. The engine stores it, hands it
  over and never reads inside it.
- **Part.** An addressable unit of a piece: a line, a node, a link, a cell, a position. Named by a `PartRef`.
- **Case.** One check on a piece. The engine knows its id, title, visibility and status; the kind owns the rest.
- **Check.** One evaluation of a draft against cases. What Program calls a run.

### 2.2 Kinds

| Kind | A piece is | Right means | Truth comes from | First forms |
|---|---|---|---|---|
| Program | files of code | every case passes when run | execution | Build, Repair (today) |
| Structure | labelled nodes and labelled links | the same labelled graph as the reference, whatever the layout | the sources | Complete, Repair |
| Table | rows and columns of cells | each asked cell equals the reference under its column's comparison | the sources | Complete, Repair |
| Sequence | items in an order | the reference order, allowing the ties the exercise declares | the sources | Arrange, Complete |
| Figure (later) | an image from the sources with label regions | each label equals the one read from the image | the image itself | Complete |

A kind's checker is code. Work that needs a judge to decide right from wrong (an explanation, a review) is not a
kind: Explain and the drills keep their own proven ways of grading. The reason is that the proof, the answer key and
hints that cannot give the answer away all rest on a checker that needs no judgement.

The test of the model is that new subjects need no code. A circuit, a cell cycle, a legal procedure and a grammar
tree are each one of the rows above, with their nouns and labels carried by the exercise.

### 2.3 Forms

| Form | The start is derived by code as | Sound when | Learning method |
|---|---|---|---|
| Complete | the reference with the asked parts emptied | restoring them passes; leaving any one empty fails a case | completion problems and faded worked examples (van Merriënboer; Renkl and Atkinson) |
| Arrange | the reference's parts in a seeded shuffle | the shuffle fails a case | ordering tasks, as in Parsons problems (Parsons and Haden) |
| Repair | the proven misreading that fails the most cases | it fails a case (true by its proof) | erroneous examples (Große and Renkl) |
| Build | the kind's empty piece or a starter | the starter fails a case | generation and retrieval practice (Roediger and Karpicke) |

The order of the table is the order of growing demand on the learner, which is the order a target should be
practised in as it stabilises (expertise reversal, Kalyuga). §9 step 12 uses it; nothing before that step does.

A kind supports a form when it has the role that form needs (§3.2): `Gaps` for Complete, `Order` for Arrange. There
is no separate list of supported forms that could drift from the code.

### 2.4 Stages

One stored vocabulary: `understand, examples, plan, make, check, review, reproduce, locate, fix, verify`. Today's
`code` and `test` become `make` and `check`; the Program kind still labels them "Code" and "Test".

| Form | Stages |
|---|---|
| Build | Understand → Examples (only when the kind takes learner cases) → Plan → Make → Check → Review |
| Repair | Reproduce → Locate → Fix → Verify |
| Complete, Arrange | Understand → Make → Check → Review |

### 2.5 Settings and disclosure

Unchanged: `Setting::discloses(Hidden::{FailingCase, UncaughtMisreading, WorkedSolution}, submitted)`. One table
decides for every kind what a learner may see. A learner's piece that equals a proven misreading is an "uncaught
misreading" in every kind, and Coached shows its description.

## 3. Architecture

### 3.1 Layers and the one dependency rule

```
lattice-core   domain/learning/practice/work.rs        values only: ids, enums, Piece, PartRef, Case, Exercise
lattice-learning  practice/session/                    the engine: service, policy, hints, counterpart, debrief,
                                                       observation, proof, build, spacing, records, dto, kind.rs
                  practice/kinds/program/              everything that knows about code
                  practice/kinds/structure/ …          one folder per kind
                  practice_repository/, practical_repository/sessions/    the only SQL
lattice-desktop   features/learning/plugin/practice_sessions.rs   commands; builds the registry (composition root)
src/features/learning/practice/session/                the screen
src/features/learning/practice/kinds/<kind>/           one editor and one results view per kind
```

**The rule:** nothing under `practice/session/` names anything under `practice/kinds/`, `drills::runner` or
`lab_runtime`. Kinds depend on the engine's roles; the engine depends on no kind. The architecture check enforces
it (§9 step 6), and the engine's own tests run on a fake kind whose pieces are plain strings, with no code runner in
the test at all.

### 3.2 Roles

A kind is a record of small roles, not one large interface. Each consumer takes only the role it uses, and kinds
share role implementations (§5.1).

```rust
pub struct Kind {
    pub id: KindId,
    pub spacing: Spacing,                         // Technique | Item (§4.6)
    pub wording: Arc<dyn Wording>,                // the words prompts and labels use for this work
    pub checker: Arc<dyn Checker>,
    pub parts: Arc<dyn Parts>,
    pub diff: Arc<dyn Diff>,
    pub showing: Arc<dyn Showing>,
    pub screen: Arc<dyn Screen>,                  // what the learner's screen draws of the work
    pub hinting: Arc<dyn Hinting>,
    pub reply_guard: Arc<dyn ReplyGuard>,
    pub rubric: Arc<dyn Rubric>,
    pub account: Arc<dyn Account>,
    pub budget: Arc<dyn Budget>,
    pub author: Arc<dyn Author>,
    pub truth: Arc<dyn Truth>,
    pub gaps: Option<Arc<dyn Gaps>>,              // enables Complete, and revealing one part
    pub order: Option<Arc<dyn Order>>,            // enables Arrange
    pub learner_cases: Option<Arc<dyn LearnerCases>>,   // enables the Examples stage
}
pub struct Kinds(BTreeMap<KindId, Kind>);         // built once, in the composition root
```

| Role | Answers | Used by |
|---|---|---|
| `Checker` | `check(exercise, piece, cases) -> CheckReport { results, marks }` | check command, proof, hint and reply guards |
| `Parts` | which parts a piece has, whether a `PartRef` exists, its display name | debrief evidence, hints, marks |
| `Diff` | the changes between two drafts, in the kind's own units | draft observation |
| `Showing` | the brief, a piece and a result as text for a model; never the reference | observation, counterpart, hints, debrief |
| `Wording` | the nouns, verbs and stage labels for this work, per exercise | every prompt template and label |
| `Hinting` | the hint for a rung, already checked | hint command |
| `ReplyGuard` | why a counterpart reply may not be shown, if so | counterpart |
| `Rubric` | `criteria(form)`, in the order the scorecard lists them, and `statement(form)`: what the debrief states beside the scores, as keys with the sentence that asks for each (None when the kind states nothing) | debrief |
| `Account` | the session's work as the debrief's model reads and cites it: the exercise with its reference, the form, each check given the one before it, a piece no check holds, how parts and checks are cited, and the fields an evidence item has beyond its kind, ref and text | debrief |
| `Budget` | the seconds a session gets when the learner chose none, per brief; None leaves it untimed | start, change of setting |
| `Author` | a draft exercise for a lesson, or "this lesson has nothing of this kind to make" with a reason | build job |
| `Truth` | whether the reference is right, and whether the task has one right answer | proof |
| `Gaps` | a piece with parts emptied; a piece with one part restored | Complete, the Point rung |
| `Order` | a piece with its parts in a seeded shuffle | Arrange |
| `LearnerCases` | how a case the learner wrote is shown and accounted for, and `case(exercise, learner_case) -> Case`: the case a check runs, with the comparison the exercise's own examples use | Examples stage, check, debrief |

`CheckReport.marks` is the kind's own diagnosis: which parts are wrong or empty. The verdict never depends on it.

**Pieces are opaque values, not generics.** `Piece` wraps a JSON value; each kind parses it into its own type at its
boundary and returns an invalid-state error when it does not parse. The engine does three things with a piece:
stores it, hands it to a role, and compares two hashes. A type parameter would add compile-time safety to code that
never looks inside the value, and would then need a second erasure at IPC and at storage.

### 3.3 Words: one set of rules, any work

Session rules (one question per turn, when the Watching interviewer may stay silent, what each rung may contain)
live once, in the engine's templates. Every word that names the work comes from the kind's `Wording`, and every
block that shows the work comes from its `Showing`. A rule that only makes sense for one kind (a fenced block that
parses as Python is refused on Plan) lives in that kind. No rule is written twice.

Some words come from the exercise, not the kind: a Structure exercise carries its own nouns ("atom", "bond"). That
is how one kind serves any subject without a subject list.

## 4. The engine

### 4.1 Session service

`PracticeSessions { store, kinds, aids, checks, debriefer, clock }` with `start, move_stage, check, ask_hint,
send_turn, respond, observe_draft, set_setting, reveal_solution, finish` and `happened`. It is today's
`CodingSessions` with every call into code replaced by a call to a role of the session's kind. It is written against
five ports, so its tests need no database, model or runner:

| Port | Answers |
|---|---|
| `SessionStore` | the session as the engine reads it, and every write as one transaction: create, stage move, draft, a recorded check, the pending cue (offer, take, restore), reply, turn, hint, setting, reveal, finish |
| `Checks` | one check of a piece against cases, stored; the same check id answers with the check it stored |
| `Aids` | the model calls of a session: the hint for a rung and the counterpart's checked reply, and the model's name |
| `Debriefer` | the debrief of a finished session |
| `Clock` | the time |

A command reads what it needs, makes its model call and its check outside any write, and then writes. A command
that did something the counterpart may answer reports it with `happened(event)`; the setting's policy decides
whether that becomes a cue.

The engine's `check` runs the scopes the engine knows, `shown` and `all`. A kind's own commands stay with the kind's
adapter: Program keeps three, which go through `LearnerCases` and the Program checker: check an example, remove an
example, run on my own input. The input run stores its check itself and ends with
`happened(Checked { scope: Own })`, as every engine check does, so the same policy answers it. Checks sit behind a
port, not inside the engine, until step 9: the run machinery (a stored row while it runs, a cancel registry,
recovery at startup) is the app's, and a second kind has not yet shown which part of it is general.

A check of any kind is stored the way a run is stored today: one row per check with its scope (`shown`, `all`) and
one row per case with the kind's detail as JSON. No new table.

A clock default comes from the kind's `Budget`: Program keeps 20, 35 and 45 minutes by difficulty. A kind with no
default is untimed unless the learner sets a time. Coached never has a budget; leaving Coached gives an untimed
session the kind's default, and a session that has one keeps it.

### 4.2 What the counterpart sees

`Observation { setting, form, stages reached, transcript, brief, piece, changes, last check, sections }`. The first
four are the engine's. `brief`, `piece`, each change and the last check are text the kind's `Showing` and `Diff`
produce; `sections` are the kind's extras (Program: the learner's examples and their coverage, the signature, the
target complexity, the technique names). The render is one template with the blocks in a fixed order. The existing
rule holds for every kind: the observation never holds the reference, a misreading or an internal id.

### 4.3 Counterpart policies

`Coach`, `Interviewer` and `WatchingInterviewer` stay `TurnPolicy<Observation>`. Events and cues are renamed for
what they are (`RunFinished` → `Checked`, `RunReview` → `CheckReview`); the cue table and its ranks are unchanged.
For kinds whose truth is the world, the counterpart's best move is to ask the learner why a part belongs where they
put it (self-explanation, Chi). That needs no knowledge of the answer, so those kinds do not hand the counterpart
the reference values of empty parts.

### 4.4 Hints

Rungs: `Nudge, Idea, Plan, Point`. Point replaces Line: it points at parts, by `PartRef`. The order, the rating a
rung costs and the settings that allow hints are the engine's. How a rung is produced is the kind's:

- **Program (unchanged).** The model writes the hint with the reference in view. Every fenced block is run; a block
  that passes every case, a line of the reference word for word, or a technique name outside a Coached Idea is
  refused, and the hint is asked for once more with the reason.
- **Kinds checked by comparison: made by construction, with no model prose.**
  - Nudge: the wrong and empty parts, highlighted (the checker's marks).
  - Idea: the section of the lesson that teaches the first such part (the citation its fact was verified with).
  - Plan: that part's choices narrowed to the reference value and the values proven misreadings give it.
  - Point: that one part restored from the reference, locked and recorded.

  A rung the exercise cannot produce for a part (no misreading touches it) yields the next rung, and the hint
  records the rung it actually was.

For those kinds a plain check answers only "right" or "n of m parts are right". Which parts are wrong is the Nudge
rung, so that help is always on the ladder and always counted.

**What cannot be proven.** A model that knows the subject can name a missing label from its own knowledge. No
structural check catches that in prose, and a wording filter on replies would be the brittle kind of fix this
codebase does not take. The counterpart is instructed to speak about method and never content, and an eval measures
it (§8). Help that names content comes only from the ladder.

### 4.5 Debrief

The prompt, the evidence checks and the scorecard are the engine's. Criteria come from the kind's `Rubric` for the
form, and so do the statements the debrief asks for beside them. Program keeps its four criteria and states an
approach and a complexity. The text the model reads, and the evidence fields it fills, come from the kind's `Account`:
the keys the exercise, the form and the checks sit under, how a part or a check is cited, and the extra fields of an
evidence item, all required. Evidence kinds become `quote, part, check, learner_case, hint`; a cited part must
exist in the draft (`Parts`), a cited check must be one the session made.

### 4.6 Ratings and spacing

One rating rule for every kind, from `Outcome { all_passed, highest_rung, solution_seen_before_passing }`, as today.
What is scheduled differs by what is being learned, so the kind declares it:

- `Spacing::Technique` (Program). The targets are the exercise's technique patterns. When one is due, a new problem
  is made for it: the aim is transfer.
- `Spacing::Item` (Structure, Table, Sequence). The target is the exercise itself. When it is due, the same proven
  exercise comes back: the aim is recall of that content.

Both use the existing FSRS scheduler and both appear in `practice_due`.

### 4.7 Fit

`learning_practice_fits` holds one row per course and kind instead of `teaches_code`. One model call per outline
revision answers, for every registered kind at once, whether the course asks the learner to produce that kind of
work; the description of each kind comes from the kind. A kind may also be wanted outright by the practice profile
(a profile language wants Program). Per lesson, the answer costs nothing extra: the kind's `Author` either returns a
draft or says the lesson has nothing of this kind to make, and that answer is stored so it is not asked again.

## 5. Proof and the build job

### 5.1 One template

```
build(kind, form, lesson):
  draft      = kind.author.draft(lesson, form)              # or: nothing to make → stored, stop
  proven     = prove(kind, form, draft)                     # Unproven(reason) → the author is asked again with it
  save(exercise)

prove(kind, form, draft):
  1. truth       kind.truth.establish(draft)                # the reference is right
  2. misreadings each one, checked: fails a case, and does not error on every case
  3. enough      the form and the kind have the misreadings they need, each need with its reason
  4. start       the form derives the starting piece by code (§2.3)
  5. sound       the form's soundness rule holds (§2.3)
  6. one answer  kind.truth.unambiguous(exercise)
```

Two implementations of `Truth` cover every kind:

- **`Executed` (Program).** Today's proof, unchanged: the reference passes every case twice, scale cases take their
  digest and time from the reference's own run, and the reference stays inside the scale budget. Ambiguity is
  handled by the statement's clarifications and the misreadings, as today.
- **`Sourced` (every kind whose truth is the world).**
  - *Establish.* The kind turns the reference into claims: one for the whole piece as rendered, and one for each
    asked part, described by its place ("the part joined to …"). The claims go through the claim ledger and the
    acceptance policy, against the lesson's sources, exactly as a lesson's own statements do. No second policy.
    Each asked part keeps the citation it was verified with; the Idea rung shows it.
  - *One answer.* A solver that has the brief, the lesson and the starting piece, but not the reference, attempts
    the exercise. If its piece passes, the task is doable as stated. If it differs, the differing parts are
    verified the same way: supported by the sources means the exercise has two right answers and is unproven;
    unsupported means the solver was wrong and the exercise discriminates.

Misreadings are needed for a reason, and the reason sets the number: Program needs two, because the examples'
coverage line counts them (today's rule). A Repair exercise of any kind needs one, its start. Complete and Arrange
need none; misreadings there only feed the Plan rung and the common-mistake note.

Every check the proof runs is timed. A kind whose check can be slow (graph comparison) is held to the rule Program's
scale cases already follow: the proof's own checks must finish inside the same share of the time limit, and a check
that does not finish in a session is an error, never a wrong answer.

### 5.2 The build job

`learning.practice_build` replaces `learning.coding_build`: the same steps and re-ask rule, with the kind in the
payload. Program's source, its pattern targets and its harness are the Program kind's `Author`.

## 6. Structure, the first kind beyond code

```rust
pub struct Structure { pub nodes: Vec<Node>, pub links: Vec<Link>, pub directed: bool }
pub struct Node { pub id: String, pub label: Option<String> }                      // None is an empty slot
pub struct Link { pub id: String, pub from: String, pub to: String, pub label: Option<String> }
pub struct StructureSpec {
    pub title: String, pub subject: String,           // what the structure depicts
    pub node_noun: String, pub link_noun: String,     // the exercise's own words
    pub asks: Vec<PartRef>,                           // the parts the lesson's outcome asks the learner to know
    pub extra_labels: Vec<String>,                    // further labels for the tray
}
```

- **Authoring.** The model is given the verified lesson and writes the whole structure as data, plus the misreadings
  as alternative labellings. It never writes a picture, coordinates or a gap.
- **Right means.** The learner's piece and the reference are the same labelled graph: an isomorphism that keeps node
  labels, link labels and direction. Layout is never compared, so two interchangeable positions filled the other way
  round are right. The comparison is a backtracking search pruned by label and degree; it is general graph code with
  no rule of any subject in it (no valence, no polarity).
- **Marks.** When the piece is wrong, the slots that differ from the reference slot by slot. Marks only place the
  highlight; the verdict is the isomorphism.
- **Entry.** The learner fills a slot from a tray: every label of the piece, the misreadings' labels and the extra
  labels. Typed entry would need a rule for when two spellings are the same answer, and a checker that rejects a
  true answer is a defect; typed entry waits for verified accepted spellings.
- **Drawing.** Positions are computed by code from the topology alone, never from labels and never by the model, and
  travel in the public view. The screen draws nodes, links, empty slots and marks as SVG, operable by keyboard.
- **Forms.** Complete (the asked parts emptied) and Repair (a misreading's labels in place). Build needs a
  free-drawing editor and comes later.

The molecule of the ask is this kind with nothing added: the lesson states the molecule, the model writes its atoms
and bonds as nodes and links, each asked atom is verified against the sources, the app empties those slots and
draws the rest, and filling them in is graded by comparison.

Table and Sequence reuse `Sourced`, the by-construction ladder and the comparison checker's frame; each adds only
its shape, its parts and its editor. A table's asked cells make natural claims ("for this row, this column is this
value"), and a sequence's claims are the adjacent pairs and the declared ties.

## 7. Names and storage

Anything shared is named for practice sessions. Only what is truly about code keeps "coding".

| Today | Becomes |
|---|---|
| `learning_coding_sessions` (`task`) | `learning_exercise_sessions` (`kind`, `form`) |
| `task`: `problem`, `bug_fix` | `form`: `build`, `repair`, `complete`, `arrange` |
| `stage`: `code`, `test` | `make`, `check` (the other names stay) |
| `learning_coding_events`, kind `run` | `learning_exercise_session_events`, kind `check` |
| `learning_coding_debriefs` | `learning_exercise_debriefs` |
| `learning_practical_runs.coding_session_id`, scope `examples` | `exercise_session_id`, scope `shown` (`all` and Program's `input` stay) |
| `learning_practical_activities.kind = 'coding_problem'` | `'exercise'` |
| `learning_practice_fits.teaches_code` | one row per course and kind: `kind`, `fits` |
| hint rung `line` | `point` |
| `learning_coding_examples` | stays: learner-written cases are Program's |
| material `problem:<language>` | stays: Program's technique targets |
| commands `*_coding_session`, `move_coding_stage`, `ask_coding_hint`, `send_coding_turn`, `set_coding_clock`, `set_coding_setting`, `reveal_coding_solution`, `start_coding_build`, `observe_coding_draft`, `run_coding_session` | `*_practice_session`, `move_practice_stage`, `ask_practice_hint`, `send_practice_turn`, `set_practice_clock`, `set_practice_setting`, `reveal_practice_solution`, `start_practice_build`, `observe_practice_draft`, `check_practice_piece` |
| `check_coding_example`, `remove_coding_example` | stay; `run_coding_input` is new for Program's run on the learner's own input |
| types `CodingStage`, `CodingSetting`, `CodingTask`, `CodingEvent`, `CodingCue`, `CodeHintRung`, `Problem*` in the engine | `Stage`, `Setting`, `Form`, `SessionEvent`, `Cue`, `Rung`, `Exercise`; `Problem*` stays inside the Program kind |
| `CodingSessions` | stays: it is Program's adapter over the engine's `PracticeSessions` |
| wire field `task` | `form` |
| query key `learning-coding-sessions` | `learning-exercise-sessions` |

The tables and the run column take `exercise_` because `learning_practice_sessions` and
`learning_practical_runs.practice_session_id` already belong to the workbench.

The migration `20261109100000_learning_coding_sessions.sql` is edited in place and renamed for what it creates. It
has never been on main.

## 8. How each step is proven

**The seam changes nothing.** Four proofs, written before the move:

1. *Golden requests.* For a fixed fixture, the system text, user text and schema of every model call a session and a
   build can make (each rung in each setting, each cue, the debrief, the source, the fit) are captured from the
   code as it is. After the move they are byte for byte the same.
2. *Existing tests.* The session, policy, hint, counterpart, debrief, pattern, proof and source tests pass with
   names and imports changed and no assertion changed. An assertion that would have to change is reported, not
   edited.
3. *Stored shapes.* Every stored JSON value (events, cues, the exercise file) round-trips, and the migration test
   lists the new names.
4. *The fake kind.* The engine's own tests run every command, every cue, the ladder order, the disclosure table and
   the proof template on a kind whose pieces are strings.

**A kind is offered only after its eval.** On Haiku, on hand-checked lessons from at least three unrelated subjects
(runs need the owner's go-ahead, as every eval does):

- every proven exercise's reference is true;
- no exercise marks a true answer wrong;
- no lesson that teaches nothing of the kind is offered an exercise of it, and every "nothing to make" is right;
- no counterpart reply states the value of an empty part.

## 9. Build plan

Tests come first in every step. Nothing is built locally; each step is pushed to its PR and proven on CI.

**Inside #422** (the seam; Program is the only kind; behaviour unchanged):

0. Merge main, and move #422's data access onto main's rules.
1. The vocabulary in core: `work.rs` (§2, §3.2 values), with the stage lists and the disclosure table.
2. The roles, `Kind`, `Kinds` and the fake kind (§3.2).
3. The golden requests of §8, captured from today's code.
4. The move, in dependency order. Built so far:
   - 4.1 Program's roles as adapters over today's functions, and the session observation on the kind's roles.
   - 4.2 The counterpart policies, worded by the kind.
   - 4.3 The counterpart's reply and the hint ladder, with Program's reply guard and hints.
   - 4.4 The debrief, with `Rubric` and `Account`.
   - 4.5 The session service and its records behind the kind seam: the four ports and `Clock`, `Budget`,
     `LearnerCases::case`, the pure functions (`observe`, `cases_for`, `outcome`, `submit_counts`), Program's
     store, checks and adapter, and the removal of the coding observation, policies, hint entry point and
     counterpart reply that lost their last caller. The stored cue stays `CodingCue` until step 5.
   - 4.6 The proof template and the build job behind the kind seam: `build(kind, form, lesson)` and
     `prove(kind, form, draft)` in the engine, the `Author` role, the kind's declared need for misreadings, the
     practice build job over Program's author and store, and the removal of the coding problem source and its
     assembly. The stored job kind stays `learning.coding_build` until step 5. The `sound` step has no rule
     before step 8 and `Sourced` truth, with the storage of "nothing to make", waits for step 9.
   - 4.7 Ratings into the engine: one rule over the engine's `Outcome` and `Rung` in `session/rating.rs`, computed
     by the session service when it finishes a session and handed to the store in `Finish`, which no longer
     carries the outcome. Program's store records it; its technique spacing stays in `coding/patterns.rs`.
     Nothing branches on `Kind.spacing` until item spacing in step 9.
   Nothing is left to move in step 4.
5. The renames of §7: migration, DTOs, commands, bindings. Built so far:
   - 5a Storage. The migration is edited in place and renamed `20261109100000_learning_exercise_sessions.sql`.
     The tables are `learning_exercise_sessions` (`kind`, `form`), `learning_exercise_session_events` and
     `learning_exercise_debriefs`; `learning_practical_runs.coding_session_id` is `exercise_session_id`. The prefix is
     `exercise_` and not `practice_` because `learning_practice_sessions` and
     `learning_practical_runs.practice_session_id` already belong to the older workbench. Stored values: stage
     `make` and `check`, rung `point`, form `build`/`repair`/`complete`/`arrange`, run scope `shown`, event kind
     `check`, activity kind `exercise`, job kind `learning.practice_build`. Program's duplicate enums
     (`CodingStage`, `CodingSetting`, `CodingTask`, `CodeHintRung`, `CodingCue` and their helpers) are gone: the
     session, the store and the DTOs hold the engine's `Stage`, `Setting`, `Form`, `Rung` and `Cue`, and their serde
     names are the stored values. The adapters between them are removed; `scope_of`/`coding_scope_of` stay because
     Program's run scope has `Input`, `check_status_of` because the run status is the workbench's, and
     `program_form` replaces `task_of` as the one place that refuses Complete and Arrange. The fits table and
     `run_coding_input` are not part of this step.
   - 5b Commands, DTOs, bindings and the screen's API. The commands are `start_practice_session`,
     `get_practice_session`, `list_practice_sessions`, `finish_practice_session`, `respond_practice_session`,
     `move_practice_stage`, `ask_practice_hint`, `send_practice_turn`, `set_practice_clock`,
     `set_practice_setting`, `reveal_practice_solution`, `start_practice_build` and `check_practice_piece` (was
     `run_coding_session`); `observe_coding_draft` became `observe_practice_draft`; `check_coding_example` and
     `remove_coding_example` keep their names. The request and result DTOs follow their commands; the session, summary, debrief, check and hint views
     are `PracticeSessionDto`, `PracticeSessionSummaryDto`, `PracticeDebriefDto`, `PracticeCheckDto` and
     `PracticeHintDto`. The wire types `CodingStage`, `CodingSetting`, `CodingTask` and `CodeHintRung` are the
     engine's `Stage`, `Setting`, `Form` and `Rung`. The wire field `task` is `form`, and so are Program's `ProblemRequest.task` and the
     session records' `task`. `CodingSessions` stays: `PracticeSessions` is the engine's service and the Program type is its adapter.
     `run_coding_input`, the new command for Program's run on the learner's own input, is not part of this step.
     The screen's API follows (calls, types, one query key); the components are step 6.
6. The screen: `session/` and the kind registry, with Program's editor and results views under `kinds/program/`;
   the dependency rule in the architecture check; the fake-kind tests. In two parts, because a screen that names
   no kind needs a wire that names none:
   - 6a The wire. Today the session view is Program's (`problem`, `examples`, `runs`, `draft` as files), it carries
     no kind, and the requests that carry the learner's work (`check_practice_piece`, `ask_practice_hint`,
     `send_practice_turn`, `observe_practice_draft`, `finish_practice_session`) take `files`. The view becomes the
     engine's, built by one function from the engine's `Session` for every kind: the session's own fields, its
     `kind`, the label of each stage (from `Wording`; the screen's other words are the kind's own copy), a summary of
     each check, the hints, the turns, how many misreadings the learner's own cases catch where the kind has such
     cases, and `work`: what the kind's own screen code draws, as one JSON value from a new role, `Screen`. The
     debrief is split the same way: scorecard, counts, misreadings and technique ratings from the engine, the rest
     in the kind's `work`. `Showing` stays text for a model. The engine decides what a learner may see now
     (`Seen`, from the setting and what the session has reached) and hands it to `Screen`, which leaves out what
     it withholds; a kind never reads the setting. What `Screen` needs beyond the engine's record sits in the
     record's kind-owned values (`CheckReport.detail`, a learner case's `found`), so the role is a pure function
     of `Session`. The requests carry `piece` as a JSON value, which the engine already stores and hands on
     without looking inside (§3.2). The engine's check takes the scopes it knows, `shown` and `all`; Program's
     run on the learner's own input is its own command, `run_coding_input`, beside its two example commands
     (§4.1). Program's `work` types stay typed in the bindings; Program's screen code reads `work` as them at
     its own boundary, as the kind does on the server. A stage's instruction for the learner is display text and
     stays with the kind's screen code: `Wording::stage_instruction` is what the counterpart is told.
   - 6b The screen. `session/` owns every session call and everything a session is whatever the work: loading, the clock,
     the counterpart's turns and cues, stage moves, hints, the checks' status, finishing, the debrief frame. It
     takes a kind's view from a registry (`kinds/index.ts`, the one place that names every kind): the brief, the
     editor that yields the piece, the check controls, the results, the learner's own cases where the kind has
     them, and the kind's part of the debrief. `session/` imports nothing under `kinds/`, and no kind imports
     another; the architecture check holds both. The screen's tests run on a fake kind whose piece is a string.
   A kind's own commands are called from its view and hand the session back to `session/`. Rejected: a kind's
   view making the session's calls with its own request types. It keeps the wire Program's and
   moves the same change to the second kind.
7. Fit per kind.

**After #422, one small PR each:**

8. Start soundness for every kind (§2.3), and the form on an exercise's proof record.
9. Structure: shape, comparison, marks, gaps, layout; `Sourced`; the second reading; the by-construction ladder;
   item spacing; the editor. Complete and Repair.
10. The Structure eval of §8, then its card.
11. Table, then Sequence.
12. Later, each with its own note: the form offered following the last rating (§2.3); typed entry with verified
    spellings; free-drawing Build; Figure, which needs label regions found in a source image.

## 10. Out of scope

- Explain, the drills, the mock interview and role-play keep their own grading. They can share the session screen
  later; nothing here depends on it.
- No change to what Program asks, checks or discloses.
- No subject lists, no subject rules, no per-subject prompts.

## 11. Decisions for the owner

1. **Cards.** Recommended: one card per kind that fits the course, named in plain words by the kind ("Code",
   "Diagrams", "Tables", "Order"); a course shows only the cards whose kind fits.
2. **Wrong submits in the rating.** Today a learner who passes after failed submits is rated as one who passed
   first time. Recommended: one rule for every kind that reads the number of wrong submits, landing with step 9.
   It changes Program's ratings, so it is not part of the seam.
3. **Bounds carried over unexplained.** `MAX_PROOF_REASKS = 2` moves into the build template as it is. It needs a
   stated reason or removal, with the other bounds already listed for review.
