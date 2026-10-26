# One learning method for every part of a course

Status: in progress (2026-10-26). Builds on
[2026-10-25-learning-science-lessons.md](2026-10-25-learning-science-lessons.md),
which rebuilt lessons on learning science and lists the evidence with its
limits (§2).

Lessons already follow the method: checks that commit an answer before the
reveal, contrasts with a near-miss, misconception-naming feedback, explaining
from memory before the recap, and FSRS review cards that open the next lesson.
The rest of a course did not: the outline, quizzes and assessments,
diagnostics, the practice workbench and tutor, study decks, labs, simulations
and interviews each wrote their own instructions. This change gives them one
text to follow, so a course teaches the same way from outline to interview.

## The shared text

`crates/lattice-desktop/src/features/learning/method.rs`:

- `LEARNING_METHOD`: how material is built. Retrieval first on a fresh case;
  feedback after every attempt; small steps tied to prior knowledge with
  concrete examples; worked example, then fading, skipping what the learner
  already knows; elaboration; contrast and interleaving of what is easily
  confused; spacing and several correct recalls across sessions; about four
  in five attempts succeeding; never rereading, highlighting, summaries to
  read or learning-style matching. Kept near 250 tokens so it fits beside a
  local model's task.
- `FEEDBACK_METHOD`: how an answer is graded or explained: what was right,
  the most important gap as a specific fix, the misconception, and one next
  step or fresh question. Never only a score.

Prompts import these constants rather than paraphrasing them, so changing the
method changes it everywhere.

## Evidence

The strongest findings are retrieval practice and spacing (both "high utility"
in Dunlosky et al. 2013, PSPI 14(1), doi:10.1177/1529100612453266).
Successive relearning (retrieval to a criterion, then spaced relearning;
Rawson & Dunlosky 2022, Current Directions 31(4)) combines them. Worked
examples help novices and fade with expertise (Kalyuga et al. 2003);
interleaving helps most for easily confused categories (Brunmair & Richter
2019); prequestions help when feedback follows (Richland, Kornell & Kao 2009);
feedback works through its information about the task and how to improve
(Wisniewski et al. 2020); about 80% success during practice (Rosenshine 2012).
Weinstein, Madan & Sumeracki (2018) summarise the six strategies for learners.
The 2026-10-25 document cites each with its DOI.

## Who changes what

| Area | Files | Owner |
|---|---|---|
| Shared text, course outline | `method.rs`, `lessons/generation/outline.rs`, `planning/*` | lattice-05 |
| Lessons and lesson recall | `generation/lesson.rs`, `lesson_rules.rs`, `teaching.rs`, `teaching_review.rs`, `generation/recall.rs`, `recall/schedule.rs` | lattice-b3 |
| Decks, study sessions, labs, simulations, interviews, Feynman, brain dump | `recall/study_generation.rs`, `practice/practical_generation.rs`, interview review and turns | lattice-c9 |
| Quizzes, assessments, grading, diagnostics, practice workbench and tutor, plan, study guide, notebook | `assessment/*`, `planning/diagnostic_generation.rs`, `practice/practice_generation.rs` | lattice-f7 |

## The outline

The outline prompt carries `LEARNING_METHOD` and one structural requirement:
later lessons retrieve and apply earlier modules' ideas in new cases, so they
return spaced across the course; easily confused ideas sit close together with
an objective that tells them apart; each module moves from worked examples to
independent application; and stated prior knowledge is applied rather than
taught again.

The outline reviewer is not made stricter in this change. Every new reviewer
finding costs a repair pass, and repair loops are what made lesson
preparation expensive on cloud models; a sequencing check can follow once
outlines generated under the method have been read.

## Cost and caching

Every prompt that carries the method grows by about 300 tokens. Authoring
requirements are part of the draft cache key, so a changed prompt discards
drafts cached under the old one; a course being prepared when this lands
regenerates the affected steps.

## Decks, study sessions, labs, simulations and interviews

**Study decks** (`recall/study_generation.rs`). Both deck prompts, from
documents and from a conversation, carry `LEARNING_METHOD` and a card rule,
`RETRIEVAL_CARDS`. A study session hides a card's options until the learner
has answered from memory, so the question alone must call up one answer. Each
card tests one idea, every wrong choice is a mistake a learner would plausibly
make, and the explanation names the misunderstanding behind the most tempting
one. Document decks prefer questions that use an idea (why or how it works,
a new concrete case, which of two easily confused ideas applies) over a
sentence's wording, and word a concept's topic label the same on every card,
so sessions interleave by concept. Conversation decks ask for a claim's
substance without quoting it.

**Study sessions.** The Studio session already retrieved first, relearned
misses within the session and interleaved topics
([interview-prep design](2026-10-26-interview-prep-and-study-sessions.md)), and
the interleaved panel uses the same session. The program Recall tab did not:
one click revealed the answer, and a miss waited until the next day. It now
asks for an answer first ("Check my answer", or "I don't know yet: show me"),
shows it beside the real answer, and brings a card rated Again back after
three other cards, up to four attempts, with the session's constants.

**Labs and simulations** (`practice/practical_generation.rs`). The activity
prompt carries `LEARNING_METHOD` and `PRACTICE_DESIGN`. The exercise is the
independent end of the lesson's worked-example fade, so it gives no worked
solution, though it may step from guided to independent. It sets the lesson's
ideas in a new, realistic case, makes the learner choose between easily
confused approaches, asks for a prediction before running where that applies,
and asks for a short justification of the approach and how it was checked;
the rubric grades that reasoning and verification, not only the artifact. A
simulated counterpart lets the learner's last decision have its realistic
consequences, so the scenario itself shows whether it worked.

**Interviews.** The plan orders each round's questions from foundational to
harder and, where the role has easily confused approaches, asks the learner
to choose between them and say when each applies. The interviewer asks why
when an answer states a choice without its reason. Every closing debrief
carries `FEEDBACK_METHOD` and `RETRY_STEP`: the next practice step retries
the one or two weakest answers from memory, now and again on a later day.
"Make drill deck" turns the questions and answer guides into a deck spaced
to the interview date.

**Left as they are.** The answer-guide and guide-review prompts already list
key points, valid alternatives and common mistakes, which is what the
feedback and the drill deck need. The brain dump already recalls before the
reveal and turns missed concepts into recall cards. The Feynman panel is
lattice-f7's.
