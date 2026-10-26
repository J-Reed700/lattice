# Interview prep and study sessions built on learning science

Status: implemented. Research: 2026-10-25; every new source below was
checked against Crossref by its DOI. Builds on
[Lessons built on learning science](2026-10-25-learning-science-lessons.md),
whose sources keep their keys here.

The owner had a first-round interview a day away and asked for the interview
and study modules to follow the best-supported ways people learn and retain.
Two gaps stood out. A study session showed a card, revealed the answer and
moved on: a miss came back a day later at the earliest, which for a test
tomorrow is after the test. And interview practice kept its answer guides
private for grading, so a learner never retrieved an answer, saw what a strong
one covers, and tried again.

## 1. Decisions at a glance

| Question | Decision |
|---|---|
| How a card is answered | From memory first: a typed (or spoken) attempt, then "how sure are you?" (Guessing, Not sure, I know this), which also reveals the answer. Interview answers get a soft timer: 2 minutes, 5 for system design. |
| How a miss is relearned | It comes back after three other cards, until recalled once, up to four attempts per session (successive relearning). |
| How sessions are ordered | Topics in turn (interleaved), each topic's cards by due time. |
| What confidence is used for | A confident miss stays on screen until the learner has read the correction (hypercorrection). The summary lists confident errors and compares "I know this" with results. |
| How long answers are checked | An answer written as a list is a checklist: the learner ticks each point they covered, and the share covered suggests the rating. |
| How spacing fits a test date | A deck may have a target. Before it, each gap is a share of the time left (Hard a quarter, Good a half, Easy three quarters), a miss returns after 10 minutes, and inside the last 90 minutes a recalled card waits until after the target. FSRS still updates memory on every review, and its own date wins when sooner. |
| What setting a target does | Every card that would next come up after the target becomes due now. |
| Where the target is stored | `study_deck_targets`, beside the deck: it is the learner's own date and never travels in a pack. |
| How interview practice feeds study | "Make drill deck" turns an interview's questions and answer guides into question-answer cards: the answer is the guide's key points, and the explanation holds the reference answer, acceptable alternatives, common mistakes, the signals the interviewer listens for, follow-ups and why the company asks. Citations keep the guide's sources. The deck is separate from the program, so packs still omit private interview material. |
| What the plan says | First pass now; the next session when cards come due; one before sleep; a ten-minute warm-up two hours before; no drilling in the last hour. |
| How the mock interview keeps to real pace | While the learner answers, it shows how long the current answer has run against about two minutes. |
| What is not claimed | No retention percentages and no readiness score. |

## 2. What the research supports

| Finding | Strength | What the session does | Limit |
|---|---|---|---|
| Retrieval beats restudy, and feedback after it roughly doubles the gain [R06, A17, RW14]. | Strong | Attempt first, answer second, every card. | Self-checking a long answer depends on honest ticking. |
| Retrieve to one correct recall in a session, then again in later sessions. One correct recall in each of three spaced sessions kept 68% after a week, against 26% for three in one session [RD11, RDS13, RVWD18]. | Strong | Misses come back in-session; the target schedule brings every card back across sessions. | Tested with facts and concepts; interview answers are longer. |
| The best gap between sessions is a share of the time until the test, about 20-40% for short horizons [C08]. | Strong for weeks; extrapolated for a day | Gaps of a quarter to three quarters of the time left. | No study measured a one-day horizon directly, so the shares are a heuristic within the ridgeline. |
| Mixing similar kinds of questions helps learners tell them apart [BR19]. | Moderate | Topics in turn: behavioral, technical and design questions mixed. | Helps less for dissimilar material. |
| Errors made with confidence are corrected best once corrected [BM01]. | Moderate | Confidence before the answer; confident misses held and listed. | The effect needs the learner to read the correction. |
| Sleep between practice sessions makes relearning faster and longer lasting [M16, RB13]. | Moderate | A session before sleep and a warm-up the morning of. | Effects vary by person. |
| Retrieval practice protected memory under acute stress, where restudy did not [SFT16]. Practice in the form of the test transfers best [MBF77]. | Moderate (one study for stress) | Answer aloud, timed, without notes, as in the room. | The stress study used word and image lists. |

## 3. The session

```
for each card, topics in turn:
  question                     (topic and timer for interview answers)
  your answer                  typed, or said aloud
  how sure are you?            Guessing / Not sure / I know this → reveals
  answer                       checklist of key points, or the answer
  full guide, citations
  rate                         Again / Hard / Good / Easy (suggested from the checklist)
    Again and "I know this"    hold: "You were sure, and it was wrong" → Continue
    Again                      the card comes back after three others
summary
  first try · relearned · still shaky
  confident errors, calibration, missed topics
  next due time, and the plan if the deck has a target
```

Number keys work throughout: 1-3 for confidence, then 1-4 to rate. They are
ignored while typing in the answer box.

## 4. Storage and commands

- Migration `20261026090000_study_deck_targets.sql` adds
  `study_deck_targets(deck_id, target_at)`, deleted with its deck.
- `set_study_deck_target` sets or clears a target (it must be ahead of now and
  within five years) and returns the deck.
- `create_interview_study_deck` makes a deck from a ready interview activity,
  optionally with a target. An interview made before answer guides existed
  drills what the interviewer listens for instead.
- `StudyDeckDto` and `StudyDeckSummaryDto` carry `targetAt`.

## 5. The same practice in decks, reviews, labs and interviews

Each section keeps its own job; these changes make each one do that job the
way the research above supports, without a shared instruction block.

- **Deck prompts** (`recall/study_generation.rs`) write cards for retrieval.
  The session hides the options until the learner answers, so the question
  alone calls up one answer. Each card tests one idea, its wrong choices are
  plausible mistakes, and its explanation names the misunderstanding behind
  the most tempting one. Document decks prefer questions that use an idea
  (why, how, a new case, which of two confusable ideas applies) over a
  sentence's wording, and word a concept's topic the same on every card, so
  sessions interleave by concept.
- **The program Recall tab** asks for an answer before the reveal ("Check my
  answer", or "I don't know yet: show me") and brings a card rated Again back
  after three other cards, up to four attempts, with the session's constants.
  **Card history** asks for an answer before revealing a due card.
- **Labs and simulations** set the lesson's ideas in a new, realistic case,
  ask for a prediction before running where that applies and a short reason
  for the approach and how it was checked, and grade that reasoning. A
  simulated counterpart lets the learner's last decision play out. A code
  lab's hidden checks say what they expected and what they saw, without
  showing an implementation.
- **Interviews** stay interviews. Each round runs from foundational to harder;
  the interviewer asks why when an answer states a choice without its reason;
  and the closing debrief's next step is a retry of the one or two weakest
  answers from memory, now and on a later day.

## 6. Later

- Keep confidence with each review, so a confident error is put first in the
  next session.
- Model feedback on a typed answer against the guide, for learners whose
  self-checks are unreliable.
- Spoken answers, transcribed, so "answer aloud" is checked rather than
  trusted.

## 7. Sources

New sources; the rest are listed in the lessons design.

- **[RW14]** Rowland (2014). The effect of testing versus restudy on retention: a meta-analytic review of the testing effect. *Psychological Bulletin* 140(6). doi:10.1037/a0037559
- **[RD11]** Rawson & Dunlosky (2011). Optimizing schedules of retrieval practice for durable and efficient learning: how much is enough? *JEP: General* 140(3). doi:10.1037/a0023956
- **[RDS13]** Rawson, Dunlosky & Sciartelli (2013). The power of successive relearning: improving performance on course exams and long-term retention. *Educational Psychology Review* 25(4). doi:10.1007/s10648-013-9240-4
- **[RVWD18]** Rawson, Vaughn, Walsh & Dunlosky (2018). Investigating and explaining the effects of successive relearning on long-term retention. *JEP: Applied* 24(1). doi:10.1037/xap0000146
- **[M16]** Mazza, Gerbier, Gustin, Kasikci, Koenig, Toppino & Magnin (2016). Relearn faster and retain longer: along with practice, sleep makes perfect. *Psychological Science* 27(10). doi:10.1177/0956797616659930
- **[SFT16]** Smith, Floerke & Thomas (2016). Retrieval practice protects memory against acute stress. *Science* 354(6315). doi:10.1126/science.aah5067
- **[MBF77]** Morris, Bransford & Franks (1977). Levels of processing versus transfer appropriate processing. *Journal of Verbal Learning and Verbal Behavior* 16(5). doi:10.1016/S0022-5371(77)80016-9
