# The tutor on a real model (2026-11-02)

Qwen3.8-27B (`/models/Qwen3.8-27B-Q5_K_S.gguf`, as the server names it), thinking off, on the remote llama-server the
app is set to. Course search on Qwen3-Embedding-0.6B, hybrid. The rows are in `results/qwen3.8-27b-remote/`. Every
reply below was read by hand.

**The tutor works end to end on the code at #272, and four things in it were wrong.** It ran, cited, quizzed, moved
cards in FSRS and saved to the Notebook on the first try. It also gave answers away, lost reviews, made up card ids
and rated cards it had not asked. The fixes are replayed turn by turn below. **They have not been run end to end:**
builds were stopped before they could be, so the run after the fixes is still to come and nothing here says the fixes
compile.

## What the first runs found

One session, then nine sessions and 29 turns (`2026-11-02-first-session`, `2026-11-02-before-fixes`). Tools ran,
citations resolved, a note reached the Notebook, a card was saved on request, and every review read back from the
database had moved the card's schedule. `ChatRuntime::turn_source` gave the chat its course, and every row says
`index_mode: hybrid`, so the course was searched by meaning and not by words alone.

| Finding | Evidence | What it did to the learner |
|---|---|---|
| The reading round overrode the tutor | The round after the notes says "start straight with the answer, in the passages' own words". `due-first` turn 2: the learner answers a card and the reply restates the passage. `pick-a-passage` turn 2: "Okay, got it." gets the answer to the card still open. In `due-first` and `quiz-due`, 2 of the 4 cards the learner answered were never rated. The reading also took about 9 of a median 20 seconds. | Answers given away, reviews lost |
| The model made up card ids | A card is asked on one turn and answered on the next, when the `due_reviews` result is gone. 6 of the 7 ratings first tried an id like `holding-pen-fifth-attempt` or `c1`, then called `due_reviews`, then rated: three rounds for one rating. | Slow, and right only because the tool refuses |
| Cards rated that were never asked | 3 of the 7 ratings: `lesson-fences` turn 3, `explain-simply` turn 2 and `teach-retries` turn 2 each rated a card on the strength of something else the learner said. | A false review moves FSRS as a real one does |
| First requests deferred | 5 of the 7 sessions whose first message asked for something opened with the cards instead: "Let's open with a quick review before we dive in." to "Give me the two main differences between tw drain and tw quiesce." | The learner's question goes unanswered |
| Session talk flagged | 60 of the run's 152 verdicts were sentences that cite nothing, most of which only run the session: "Card logged — it'll come back in two days." reads "could not be grounded in your sources". 0.48 of claims were supported, against 0.70 for the chat on this model (`2026-11-01-shipped-path-wording`: 187 of 266). | A warning on most replies, so it stops meaning anything |

Two worries from before the run did not come true. "Tell me more about the second one." was answered from what was
already there, with no extra search. And the heuristic for how long since the tutor last asked something hardly
matters: 29 of 33 replies closed on a question. Of the other four, two asked and then closed on "Take your time",
which the heuristic misses, one asked for a summary with no question mark, and one really did not ask.

## The citation check stays on, and sorts what it checks

`verifies_cited_only: true` would have cleared all of that and the real findings with it. Of the 75 uncited sentences
flagged across the first two runs, a reader found 28 that say something about the subject, some of them not in the
course at all: "When it lapses, the broker assumes the worker is gone" is the tutor's own inference. Word overlap
could not separate them: the safest rule tried that lost no claim dropped 21 of the 47 sentences of talk, measured
with a Python approximation of the app's tokens.

So the sentences that cite nothing go to the judge once a turn, and only those it says state something about the
subject stay claims. A sentence that does both stays. If the judge's reply cannot be read, they all stay.

`2026-11-02-session-talk-replay` is that prompt on the 27B, per turn as the app would send it, each turn's sentences
with one invented false claim mixed in, in a tutor's voice ("Good, and note that fence numbers are issued by the
store, not the broker."):

| Sentences | Kept as claims |
|---|---|
| False claims mixed in | 28 of 28 |
| About the subject, by hand | 27 of 28: the one lost is a heading, "The full shape of a failing job's life:" |
| Session talk, by hand | 7 of 47 |

That was run from a scratch script with the same prompt, same seed and same sentences as
`scripts/replay_session_talk.py`. The check costs one more call a turn on the verifier model. The app is set to
Claude Haiku as its verifier, and that has not been tried.

Later the same day `scripts/replay_session_talk.py` itself ran on the 27B, on both runs' rows, twice:

| Prompt | False claims kept | About the subject kept | Session talk kept | Replies it could not read |
|---|---|---|---|---|
| As above, with the reply read only when it is one JSON array (`2026-11-02-session-talk-strict-parse`) | 28 of 28 | 27 of 28 | 7 of 47 | 0 of 28 |
| Asking instead for the numbers of the sentences that only run the session (`2026-11-02-session-talk-naming-the-talk`) | 23 of 28 | 21 of 28 | 2 of 47 | 0 of 28 |

The first is what the app now does: it reads the judge's reply only when the whole reply is the array, so numbers in
a sentence of prose leave every sentence a claim. Before, it took the first bracketed list anywhere in the reply.
The second was tried because a judge that names what to drop fails safe when it misses a sentence. It measured
worse where it matters: it named five false claims as talk, all of them said in a tutor's voice ("Your broker
refuses enqueues at 80 percent full."), so the prompt that names the sentences to keep stays. Its wording was: "Some
only run the session: [the same list]. Others state something about the subject being taught; those must be checked.
Reply with a JSON array of the numbers of the sentences that only run the session, and nothing else. When a sentence
also states something about the subject, or you are not sure, leave it out."

## Reading off, and the ids in the facts

Replayed from the first run's own requests, before anything was built: the turn's message with the reading taken out,
and then with the due cards in the facts (`2026-11-02-rating-replay`, two samples an arm). One round only, so these
show what the model does next and not how the turn ends.

| Turn | As it ran | No reading | No reading, ids in the facts |
|---|---|---|---|
| `due-first` 2, right answer to a card | Restated the passage, no rating | Rated, invented id, 2 of 2 | Rated, real id, `good`, 2 of 2 |
| `quiz-due` 2, wrong answer to a card | Searched and taught, no rating | Rated, invented id, 2 of 2 | Rated, real id, `again`, 2 of 2 |
| `quiz-due` 4, card settled after follow-ups | "I'll mark it", no rating | Rated once, invented id | Moved on unrated, 2 of 2 |
| `pick-a-passage` 2, "Okay, got it." | Gave the card's answer away | Asked for the cards | Next card, 2 of 2 |
| `summary-note` 1, "save that to my notes" | Saved | Saved | Saved |

`quiz-due` 4 is why the template now says to rate "as soon as the learner answers a card you asked ... before you say
anything else": with that wording the replay rated it with the real id (`2026-11-02-wording-replay`, one sample). It
did not change one thing: the model still calls `due_reviews` on the first turn with the cards in front of it, a
cheap round. On "explain watermarks" it searched first with that wording. On "Teach me how retries work" and the
drain question it still reached for the cards, and what comes after that round has not been seen.

A rating now also has to be for a card some reply of the chat asked in its own words, so the three false reviews
above would be refused. That is untested on a model: it may refuse a card the tutor put in other words.

## What it does well

Naming a misconception, with the source's words against the learner's:

> The handbook is blunt about the first one: **"A lease is not a lock."** It "records who the broker *expects* to be
> working on a job, and nothing stops a slow worker from carrying on after its lease has expired" [1].

> The dedupe_key is **not** what protects the data, and it does **not** stop a worker from running a job twice —
> that's the opposite of how it works. ... It blocks a *second enqueue*, not a second execution

A question that needs the thing taught, not its words:

> **A poison job sits at the head of an ordered lane. With the default settings, roughly how long can it hold that
> lane before it reaches the pen — and what do runbooks usually do to shorten that?**

Marking the learner's own summary, after saving their words:

> The one thing your summary leaves out is the *cure* for that stall: most runbooks shrink `max_attempts` on ordered
> queues to 3 so a poison job reaches the pen sooner [1].

Saying what the course does not hold, both times it was asked, and marking its own guess as a guess:

> None of them mention how jobs are encrypted at rest or name a cipher.

> So I can't give you a supported answer to "which cipher does Tidewell use" from this material, and I won't guess one
> (AES-256-GCM would be my prior for a queue like this, but that's from general knowledge, not your course

## What it does badly

Things the fixes are meant to stop:

> That's card 2 settled — I'll mark it.

No call followed. And to "Okay, got it.", with a card still open:

> A job that fails its fifth attempt is not deleted — it moves to the holding pen, a separate store with no consumers,
> kept there for 14 days [4].

Things they do not touch:

- It lectures. After one right answer in the first session it searched, read three stretches of a source, saved two
  cards nobody had missed, ran out of its 10 rounds and wrote five sections and a diagram, on a template that says one
  stretch at a time.
- It reasons past the source. In `quiz-due` turn 3: "The failed job still has to finish its current rung of the
  backoff ladder before it steps aside". The check marked that contradicted, which is the check working.
- A slip the check missed, cited and judged supported: "keeps getting refused all the way down through 80, 70… until
  the disk is nearly empty at 60". A disk at 60 percent is not nearly empty.
- It can be terse where it should give feedback: "Recorded — that one comes back in 8 days." And it miscounts:
  "**Final card:**" with six left.
- Proving a negative is slow. `not-covered` took 8 and 9 rounds, 43 and 47 seconds, reading every source, because
  hybrid search always returns something.

## After the fixes

The fixes compile and pass their tests with every open branch merged, as CI runs them on Windows. Their six tests
failed with the fixes reverted, and the other five, which guard what the fixes must keep, passed both ways. The
pinned digest held on the first run.

`2026-11-02-after-fixes` is the same nine sessions on `/models/Qwen3.8-27B-Q5_K_S.gguf` (named by the server), built
from that merged tree:

| | Before the fixes | After |
|---|---|---|
| Sentences checked as claims | 152 | 123 |
| Supported | 0.48 | 0.58 |
| Ratings that moved the card's schedule, read back | 7 of 7 | 12 of 12 |
| Ratings refused | 6, all made-up ids | 5, all real ids of cards no reply had asked |
| A teaching stretch reached a question | 1 of 1 | 2 of 2 |
| Said the course does not cover it | 1 of 2 | 2 of 2 |

Read by hand, two things the numbers hide:

- **A card can be rated that the learner never answered.** In `pick-a-passage` the first reply answered the question
  first, then asked a card. The learner's next message was "Tell me more about the second one.", meaning the second
  difference, and the tutor rated the card `again` and told them "you missed both cards". The asked check passed,
  because the card had been asked; it cannot tell an answer from a change of subject. Replayed from the recorded
  request (`2026-11-02-rating-replay-moved-on`, 5 samples each): the template as shipped rated it in 2 of 5, and with
  "A message that asks something else or moves on is no answer: leave that card unrated and ask it again later."
  added, in 3 of 5. Answers to cards were rated in 5 of 5 under both. The sentence is not a fix, and the template
  stays as it is. What would be is open.
- **The learner's first request mostly still waits for the cards.** "Teach me how retries work in Tidewell.", "Teach
  me what happens to a job that keeps failing, in a few sentences.", "Teach me this lesson." and the lease
  misconception all opened with the due cards; "Give me the two main differences between tw drain and tw quiesce." was
  answered first. The design says a session opens with its cards, and the template's "unless the learner's message
  asks for something else" is the departure that waits on Josh's decision; on this model it mostly does not take.

## Still unproven

- A rating only for a card the learner answered (above), and the first request first on this model (above).
- Multiple-choice cards, a test date pulling reviews forward, and the note reaching a real vault: none was exercised.
- Any model but the 27B, Claude Haiku as the verifier included.
- Times. A played learner evicts the slot between turns, so the seconds here are not the app's.
- The Tutor tab itself: this drives the turn, not the window, and the diagrams were read, not rendered.
