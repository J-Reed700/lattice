# Code review drills: pattern recognition for code you didn't write

Date: 2026-11-04. Status: design, signed off 2026-11-04; nothing built.

## 0. The skill, and why the app doesn't train it yet

Interviewers increasingly hand over code and ask: what's wrong with it, what would you fix first, how would you
refactor it, how would you test it. The skill is reading unfamiliar code fast, recognising the pattern behind a
symptom (a check-then-act race, an N+1 query, a swallowed error, a leaky abstraction), ranking what matters, and
saying it crisply. It's perceptual expertise as much as knowledge: experts read code in chunks and "beacons" and match
them to plans they've seen before (Soloway & Ehrlich, 1984), and that kind of recognition is trained by many short,
varied, immediately corrected trials (perceptual learning, Kellman & Garrigan, 2009), interleaved so the learner must
discriminate between patterns, not repeat one (Rohrer & Taylor, 2007; Kornell & Bjork, 2008), and spaced.

What exists:
- **Code review is a label only.** A practical activity of kind `CodeReview` is a prompt label on the generic practical
  generator: a brief, files and a rubric. It has no submit or grade path at all (Run is disabled, nothing grades it).
- **No answer key anywhere.** No model of known defects with line spans, no line-level findings, no pattern taxonomy,
  no spacing of what the learner misses.
- **Model-written keys are unreliable.** A test fixture
  (`tests/fixtures/interview_reference_counterexample.json`) keeps an interview guide that claims a data race in Rust
  code `rustc` rejects.
- **The runtime can already run hidden tests.**
  - It runs JavaScript (QuickJS) and Python (Wasmtime/WASI) built in, with no container.
  - With Docker or Podman it adds Rust, C#, React/TypeScript and Python 3.13 presets.
  - It binds hidden `check` files read-only against a learner's files; results are exit codes, not per-test.
- **The mock interview** has a "debugging and code" round, but its code is markdown inside the question: no file,
  editor, line numbers or voice.
- **Recall** has a code-prediction card format with no review UI of its own.
- **The Tutor** renders code blocks (no line numbers) but has no code exercise tool.

## 1. What the learner does

**A drill.** A realistic snippet (20–150 lines; a function, a class, a handler, a migration, a config) in a language
they choose or the course's. They review it the way they'd review a PR: select lines, leave a comment (what's wrong,
why it matters, what to do), optionally set a severity, optionally write the fix or refactor in an editor beside it.
Submit. They see every known issue: found, half-found (right lines, wrong reason), or missed, with the consequence, the
fix as a diff, and a two-sentence "how to say it in an interview". Each of their comments is matched, credited as a
valid find the key didn't plant, or explained as a false positive. Then a side-by-side of the clean code and the code
they reviewed, the contrasting-cases step (Schwartz & Bransford, 1998; Alfieri et al., 2013).

**Three modes, one drill format:**

| Mode | Shape | Trains |
|---|---|---|
| Practice | untimed; a hint costs credit | finding and explaining |
| Interview | timed (default 10 min, the learner's choice); answered by voice or text to an interviewer who probes the critical issue they missed ("what happens if two requests hit this at once?"), then a debrief | performing under time, prioritising, talking |
| Rapid | 20–60 s micro-snippets, one issue each: name the pattern and the line; dozens a session | fast recognition (perceptual learning) |

**Patterns come back.** Every issue in the key carries a pattern from a catalogue (§3). A pattern the learner misses
or half-finds becomes due again on the FSRS schedule recall already uses; the next drills are built around due
patterns, interleaved with one or two mastered ones so the learner has to tell them apart. A mastery view shows each
pattern family by language: seen, found rate, when it's next due.

**Anywhere they already study.** In the course Tutor ("give me a code review on what we covered"), as a round of the
mock interview, and from the practice area. Same drill bank, same grading, same schedule.

## 2. The answer key is exact, because the defects are planted as edits and each is proven

A drill is only as good as its key: a model asked for "buggy code" plants bugs that aren't bugs, leaves bugs it didn't
plant, and misplaces line numbers, and then the learner is marked wrong for being right. So a drill is built the way
mutation testing builds mutants (DeMillo, Lipton & Sayward, 1978; Jia & Harman, 2011), as a durable, checkpointed job:

```
spec ──► clean code (+ tests where the language runs) ──► clean review ──► mutations as exact edits
  ──► each mutation proven ──► blind review of the result ──► drill (code, key, evidence) in the bank
```

1. **Spec.** Language, domain (the course's topic, or the learner's choice), size, and target patterns (from the
   scheduler: due patterns, interleaved with mastered ones, §4).
2. **Clean code.** The model writes idiomatic, realistic code for the domain, and, in a language the lab runtime can
   run, a test suite. The suite must pass (run in the existing runtime).
3. **Clean review.** An independent reviewer (no key, its own prompt) reviews the clean code. Real issues it finds are
   repaired (a text edit, re-run tests) or, if the domain needs them, become known issues of the drill. The clean code
   is the baseline every later step compares against.
4. **Mutations as exact edits.** For each target pattern the model proposes one edit: the exact lines before and
   after, the pattern, the severity, the consequence, the fix (the reverse edit) and the interview explanation. Code
   applies it; an edit whose "before" text doesn't match exactly, or that overlaps another edit's span, is refused
   and asked again once (the repair confinement the lessons already use). The key's line spans are computed from the
   applied edits, never stated by the model.
5. **Each mutation proven.**
   - *Behavioural defect, runnable language:* a test that fails on the mutated code and passes on the clean code: the
     mutant is "killed", which proves the defect is observable. The model writes the killing test; the runtime runs
     it against both versions.
   - *Anything else (a smell, a design problem, a security flaw with no test, any language without a runtime):* an
     independent judge is shown the clean and the mutated code and must confirm the edit introduces exactly that
     problem and that the clean code doesn't have it. Reasons first, then the verdict (a deliberate judgment).
   - A mutation that isn't proven is dropped, not kept on trust.
6. **Blind review.** A fresh reviewer reviews the final code with no key. Each issue it reports that the key doesn't
   hold is judged against the clean code: a real one joins the key as "also valid" (so a learner who finds it is
   credited), a non-issue is discarded. This bounds the key's incompleteness instead of pretending it's complete.
7. **Stored** with its evidence: models, test runs, the judges' reasons. A drill whose key ends empty, or whose code no
   longer matches its spans, isn't stored.

No compiler is required for a drill: reading is the skill. Where a runtime exists, it strengthens the key with killed
mutants: Python and JavaScript always, through the built-in runtime; Rust, C#, React/TypeScript too when Docker or
Podman is installed. Each killing test runs alone against each version, so the runtime's exit code is the per-test
result. Elsewhere the judge does. Any language the models write well works.

**A bank, so starting is instant.** Building and proving a drill takes a few minutes. A background job keeps a small
bank of ready drills per language and pattern family the learner uses, and refills it on the job runtime. Rapid-mode
snippets are cheap, single-mutation drills built in batches.

## 3. The pattern catalogue

One catalogue, data not prose, each pattern with an id, a family, a one-line definition, the languages it applies to,
and a source:

| Family | Examples |
|---|---|
| Correctness | off-by-one, boundary and empty input, null/None/undefined, wrong operator, mutated argument, stale state |
| Errors | swallowed exception, error converted to success, lost context, retry without backoff or idempotency |
| Concurrency | check-then-act race, unsynchronised shared state, lock ordering, missing await, blocking in async |
| Resources | leaked handle or connection, unbounded growth, missing timeout |
| Security | injection (SQL, shell, path), missing authorisation check, secret in code, unsafe deserialisation, SSRF (CWE Top 25) |
| Performance | N+1 queries, quadratic loop, repeated work in a loop, needless allocation |
| Design | long function, feature envy, primitive obsession, leaky abstraction, temporal coupling (Fowler, *Refactoring*, 2nd ed.) |
| Readability | misleading name, magic number, dead code, duplicated logic |
| Testing | untested branch, assertion-free test, test coupled to implementation |

Modern code review mostly finds maintainability problems, not functional defects (Bacchelli & Bird, 2013), and
interviewers ask about both, so drills mix families by default.

## 4. Grading and scheduling

**Matching a comment to an issue** is a cascade, cheapest first, as in chat verification:
1. *Span overlap*, by code: the comment's selected lines against each issue's span (a small tolerance, since a learner
   may comment on the line that exposes the bug rather than the line that causes it) gives candidate pairs.
2. *Same issue, not just the same lines*: a judge decides, per candidate pair, whether the comment names this issue's
   cause: found, half-found (right place, wrong or missing reason), or not.
3. *Unmatched comments*: a judge decides whether each names a real problem (against the code, the clean version and
   the blind review): valid extra or false positive, with the reason shown.

**The fix**, when the learner writes one: in a runnable language the clean code's tests and the killing tests run
against it (an objective grade, the fix must make the bug's test pass without breaking others); elsewhere a judge
compares it with the key's fix.

**The score** is reported as its parts, never one opaque number: severity-weighted recall, precision, whether the most
severe issue came first (prioritisation is what interviewers listen for), fix quality, and time in interview mode.

**The schedule.** Each pattern is a schedulable item per learner and language, on recall's FSRS (Ye et al., 2022):
found first and fast → Easy; found → Good; half-found → Hard; missed → Again. The next drill's spec targets due
patterns first, adds one or two mastered ones for discrimination, and rotates families so a session interleaves.

## 5. In the Tutor and the mock interview

**Tutor tools** (the Tutor's existing tool pattern):
- `code_review_drill` (language, focus, size; all optional): takes a ready drill from the bank (or builds one) and
  shows it as a code block with line numbers, with an attempt id.
- `grade_code_review` (attempt id): grades the learner's own last message for that attempt. The findings are
  extracted from the learner's words, not from the Tutor's paraphrase, and a drill the Tutor didn't start can't be
  graded: the same guardrail `record_recall` has against rating cards nobody asked.
- `due_patterns`: what the schedule says is due, so the Tutor can offer it.

**Mock interview:** a `code_review` round kind. The interviewer presents a drill as "a teammate's PR", the learner
answers by voice or text, the interviewer probes the most severe issue they haven't named, and the debrief is the
drill's key.

## 6. What is measured before it ships

| Question | How | Bar |
|---|---|---|
| Are planted issues real? | 30 drills across 4 languages, every key issue read by hand | ≥ 95% real |
| Is the key complete enough? | blind review's extra-issue rate on the same 30, plus a second blind reviewer | the second finds ≤ 1 real unlisted issue per drill |
| Does grading agree with a careful human? | 40 hand-labelled learner reviews (correct, half-right, right line wrong reason, valid extras, false positives) | ≥ 0.9 agreement on found / half / missed |
| Are rapid snippets unambiguous? | 50 snippets, two blind labellers name the pattern | ≥ 90% agree with the key |

On the 27B and on Haiku, every result naming its model, answers read by hand.

## 7. Build order

Each step test-first, built by an agent and reviewed line by line:

1. The catalogue, the drill / key / attempt types, span matching and scoring (pure, core).
2. The drill job: clean code, clean review, mutations as edits, proof (runtime and judge), blind review, the bank.
3. Grading: the matching cascade, fix grading in the runtime.
4. The review view: line selection and comments, results with the contrast diff and "how to say it".
5. The pattern schedule on FSRS and target selection; the mastery view.
6. Tutor tools.
7. The interview round and the timed mode (voice is already there).
8. Rapid mode.

Measured (§6) after steps 2–3, before any UI is built on them.

What each step builds on:

| Need | Existing piece |
|---|---|
| Running a killing test or a learner's fix against hidden tests | the lab runtime's `check` files, bound read-only (`runtime/`, `prepare_run`) |
| Validated model answers, asked again once with the reason | `complete_checked` |
| A learner's words checked verbatim | `ClosingEvidence`'s exact-quote check |
| Durable generation with idempotency | the job runtime and the practical `Operation` logs |
| Numbered, selectable, decorated code | the Explorer's `CodeViewer` (gutter selection, `Decoration.line`), plus comment widgets |
| The pattern schedule | recall's `CardSchedule::review` and `record_review`: one card per pattern and language with a deterministic uuid-v5 id, as `interview_deck.rs` makes its cards |
| Tutor tools | `CourseTurn`'s tool pattern (`definitions`, `dispatch`, `owns_tool`) and `rate_card`'s guard that a card was asked in the chat |
| The interview round | `learning_simulation_*` turns and `SimulationPanel`, plus the chat's `VoiceButton` |

## 8. Decisions for sign-off

1. The three modes (practice, interview, rapid), with the interview mode answering by voice or text.
2. Drills built by planting proven defects into clean code, with a blind review, rather than asking a model for buggy
   code.
3. Any language for reading; tests where the runtime can run them; no new compilers.
4. Patterns, not snippets, are what the schedule spaces.
5. The Tutor and the mock interview use the same drills, grading and schedule.
6. Later, not now: drills cut from the learner's own repositories (Explorer folders), mutated the same way.
