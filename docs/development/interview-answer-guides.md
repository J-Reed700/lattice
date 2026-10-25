# Interview answer guides

New interview activities establish their evaluation standards before the first
learner response. Generation proceeds through company research, the question
plan, research of the questions' underlying concepts, the app-built public brief/rubric,
then a private answer standard and a fresh review call for each question.

Public criteria describe observable skills for the selected rounds. Private
evaluation signals never enter a public-material authoring call, so answer
recipes cannot be copied from those signals into the visible rubric.

Question research uses the existing typed source finder (`official_docs`,
`standard`, `paper`, `package`, and other supported needs), page capture, and
course source library. Search snippets never become evidence. Research is
bounded to six needs and three candidate pages per need. Search fields pass the
existing private-text guard; exact question wording and private learner notes
are not intended as search queries. Captures reuse saved source versions and
retain their provenance.

Each standard contains a reference approach, key points, acceptable alternatives,
common mistakes, full/partial-credit criteria, and evidence gaps. Sourced key
points require exact quotes from an owned immutable source version. The author
selects IDs from a token-bounded bank of captured sentence spans; code expands
those IDs into the original source, version and exact words. Copied quote objects
and IDs absent from that call are rejected. Long spans use the shared Unicode-safe
text splitter, and legitimate facts need not quote unrelated instructions in the
same paragraph. Semantic review still checks whether the chosen evidence supports
the claimed point. The author schema couples evidence labels with citation counts:
sourced points require 1–3 IDs; reasoning and unverified points require none.
Providers that ignore the schema still encounter the same validation. Parsing
never changes an evidence label to make an invalid point pass. The availability
of a reference alone does not require citing it for an unrelated claim.
Hypothetical deductions and behavioral evidence standards can be labeled reasoning; uncited
external facts must be unverified. Conditional design proposals can be reasoning
when justified against the question's requirements with explicit assumptions;
they must not be presented as source-mandated implementations. Each sourced
point must be supported in full by its citations, not merely share their topic.
Missing sources and unverified expectations
must be disclosed and cannot justify deductions. Behavioral guides must not
invent a candidate's experience.

The author and reviewer receive the actual question and round without the
plan's preliminary probes, rationale or evaluation hints. Source selection uses
the actual question; employer metadata and preliminary evaluation hints do not
become extra requirements for the answer. Authoring uses deterministic sampling.
Behavioral guides receive the question without employer or technical research:
those sources inform the question plan, but cannot prescribe a learner's past
job, architecture or achievements. Behavioral reference
answers describe what a strong real example would demonstrate; direct invented
first-person histories trigger author correction before review. Credit levels
must describe distinct observable performance, rather than boolean placeholders.

The review runs separately from authorship, using the exact question, captured
evidence, source provenance, and context surrounding each quotation. It checks
correctness, answerability, claim support, and acceptance of valid alternatives.
The review explicitly treats the guide as assessment criteria written before a
learner answers, so a behavioral outline is not mistaken for a personal story.
The reviewer first solves the question using the captured evidence bank, before
any proposed answer or grading criteria exist. Each separate comparison call
receives that same independent outline and the complete proposed standard. The
outline is a fallible comparison aid, never source evidence or an authoritative
answer key. The comparison reasons through suspected defects in prose before
formatting its findings. All review calls use deterministic sampling. Findings must
demonstrate an incorrect outcome, a claim/evidence mismatch, or incorrect scoring
of a valid answer; optional elaboration is not a rejection reason. Each finding
includes a concrete demonstration as well as its correction rationale. A missing
demonstration is malformed, and neither the outline nor a reviewer's preference
can stand in for evidence of a defect.
The reviewer receives a list of exact authored fields, each with its path.
Application-verified point provenance and captured quotations are supplied
separately, rather than as answer text to be graded. The schema restricts field
paths to that same authored-field list.
Each finding identifies a JSON pointer into the proposed standard and quotes
exact author-written prose at that path. Source passages, citation metadata and
invented quotations cannot be attributed to the author. Malformed findings get
the normal bounded correction attempt and then fail closed. An empty validated
finding list is the only acceptance signal; there is no independent boolean
that can contradict or erase outstanding issues. Positive rationale is not an
issue. Quotations are grouped by source, overlapping context windows are merged,
and excerpt boundaries are marked. Context never clips an exact quotation or
leaves a partial neighboring word that could be mistaken for quoted evidence.
A rejected standard can be revised and reviewed twice, for at most three
independently reviewed candidates. Repeating an already rejected candidate stops
early: unchanged input to the deterministic reviewer does not justify repeating
the same work or erasing its finding. Repair instructions require
updating the affected reference answer, key points, mistakes and scoring criteria
together; an evidence-gap disclaimer cannot preserve an unsupported requirement.
The reviewer assesses the exact question's requirements without demanding a
source mandate for each stated outcome. It rejects unsupported facts and
implementation mandates without adding unrelated requirements from broad sources.
Rejection after the third candidate prevents
the activity becoming ready. This is a separate model call, not necessarily a
different model, and does not establish that every model judgment is correct.

`interview/answer-guides.json` is stored as a hidden, read-only practical file.
It is excluded from public activity DTOs and portable practical files. Its cited
source versions are included in the activity's frozen context. Starting an
attempt freezes the guide with the question plan, so later activity changes
cannot change that attempt's standard. No schema migration or public IPC type
change is needed. Existing/imported activities without a guide retain their
previous context and disclose the missing precomputed standard at debrief;
regenerate the activity to obtain a new guide.

Live interviewer requests contain the question plan and evaluation signals,
never the reference answers. The closing coach receives the frozen guides and
their validated source quotes. It evaluates the learner's words, accepts valid
alternatives, discloses gaps, and presents a concise reference approach. Feedback
uses short app-owned question, criterion and evidence labels. The coach selects
saved evidence IDs, and the app expands them into canonical source/version/quote
citations. Unknown IDs and evidence belonging to an unevaluated question are
rejected; each evaluated sourced question needs its own supporting evidence.
The closing prompt omits raw internal IDs and preliminary plan hints.

Closing interview scores are structured observations, not free-form grades.
Every rubric criterion appears exactly once. Observed criteria require finite,
in-range provisional points and exact quotes from complete learner turns;
interviewer text, reference answers and paraphrases cannot serve as learner
evidence. Insufficient-evidence criteria require null points and no quotes, and
the app supplies their neutral explanation. The app renders the scorecard,
reference approach, frozen source limitations and next practice step into the
existing public turn. Generic simulations keep their existing output contract.
A request to select a question can finish as insufficient evidence without a
fabricated citation. Citation
disclosures let the learner read the exact saved source and its origin without
refetching the live page; independent Demonstrate attempts open those readers
after submission.

Regression coverage lives in `practice/interview_answers_tests.rs`, the existing
interview generation tests, `practice/interview_feedback_tests.rs`, and
`SimulationPanel.test.tsx`. It covers invalid
and foreign citations, evidence gaps, rejected-review repair, private storage,
attempt freezing, source capture rather than snippets, separation of live and
closing prompts, and reading the exact cited source.

`practice/interview_answer_live_tests.rs` adds opt-in local-model controls for
valid audio claims, Python control flow, and an unfair idempotency scoring rule.
It exercises the production independent solution and review calls, and records
synthetic prompts and model answers under `LATTICE_LEARNING_EVAL_LOG_DIR`.
These focused controls supplement, rather than replace, the full live authoring
matrix and retained Rust compiler counterexample.
