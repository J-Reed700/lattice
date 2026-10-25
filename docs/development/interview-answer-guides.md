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
the claimed point. Hypothetical
deductions and behavioral evidence standards can be labeled reasoning; uncited
external facts must be unverified. Conditional design proposals can be reasoning
when justified against the question's requirements with explicit assumptions;
they must not be presented as source-mandated implementations. Each sourced
point must be supported in full by its citations, not merely share their topic.
Missing sources and unverified expectations
must be disclosed and cannot justify deductions. Behavioral guides must not
invent a candidate's experience.

The author and reviewer receive the actual question and round without the
plan's preliminary probes, rationale or evaluation hints. Behavioral reference
answers describe what a strong real example would demonstrate; direct invented
first-person histories trigger author correction before review. Credit levels
must describe distinct observable performance, rather than boolean placeholders.

The review runs separately from authorship, using the exact question, captured
evidence, source provenance, and context surrounding each quotation. It checks
correctness, answerability, claim support, and acceptance of valid alternatives.
A rejected standard can be revised and reviewed twice, for at most three
independently reviewed candidates. Repair instructions require
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
