# The notes sentence on Haiku's whole run (2026-11-03)

Claude Haiku 5.5 through the Messages API, all fifty desktop-path questions through `shipped_path_eval`, with the
reranker on this PC. Code: `chat/tool_loop/reading.rs`.

**The notes sentence did not reach the leaks.** Haiku's working shows on 5 turns, as it did on 2026-10-31, and all five
are first writes, where the sentence is not. The language sentence holds, and false corrections fell from 3 to 1.

## Why

Reading all 67 Haiku answers on 2026-10-31 ([`2026-10-31-FOLLOWUP-SEARCH.md`](2026-10-31-FOLLOWUP-SEARCH.md)) found its
working showing on 5 turns ("the other passage in my notes"), false corrections on 3 of 17 follow-ups, and an English
question answered in German alone. #263 then added two sentences
([`2026-10-31-ANSWER-WORDING.md`](2026-10-31-ANSWER-WORDING.md)). "Write in the language of my question, translating any
passage in another" went on both writing instructions, and "Do not mention your notes or correct them" on
`CONTINUE_INSTRUCTION`. The replay could not show the second one helping, because no replayed reply leaked. This run asks
the same question of the whole path.

## Method

`shipped_path_eval` built from d04bec8e: fa702bb6 with #290's change. Main has since added #283 to #289. Of those, only
#288 touches the chat path, in the verifier, after the answer is written. The recipe is the 2026-10-31 one:
`claude-haiku-5-5`, the reranker on, `maxTokens` 128,000. All 67 answers were read by hand.

| Run | Mechanical | Wrong by reading | Working shows | False corrections | German alone | Median s | p95 s |
|---|---:|---:|---:|---:|---:|---:|---:|
| 2026-10-31 | 48 of 50 | 0 | 5 turns | 3 of 17 follow-ups | 1 | 12.5 | 19.8 |
| **2026-11-03** | **47 of 50** | **1 turn** | **5 turns** | **1 of 17** | **0** | 15.4 | 26.4 |

## What reading found

- **The working still shows, on 5 turns, all in the first write of their turn.** "The other passage in my notes covers a
  different case" (`dp-paraphrase-sick-child`), "The earlier notes also quoted [35]" (`dp-buried-handbook-ticket`),
  "Correction to my earlier notes: passage [1] is from the Brackleton Facilities framework agreement"
  (`dp-section-after-clause`), "From the other passages in my notes:" (`dp-mt-calderwick` turn 2), and "My earlier notes
  marked this passage as irrelevant" (`dp-mt-runbook-ticket`). Two of the five conversations leaked on 2026-10-31 too.
  None of these turns had a continuation round, and `CONTINUE_INSTRUCTION` is the only prompt that says not to mention
  the notes. Each leak is the writer commenting on what the reading step noted, mostly to say a noted passage does not
  bear on the question, or bears after all.
- **The language sentence holds.** `dp-mt-plant-room-key` answers in English and quotes the German passage.
- **One false correction, down from 3.** `dp-mt-shift-and-return` turn 2 withdraws turn 1's right answer ("The
  excerpts don't support that"). That turn's passages are about another project, and its reading wrote no notes. Turn 3
  then withdraws the withdrawal. The same conversation did this on 2026-10-31, so it is not about citation markers.
- **One wrong answer.** `dp-mt-merridew-reversal` turn 2 asks who owned the drift report action. Its answer restates
  turn 1. The top-ranked passage [1] is the Actions table that answers it. The reading marked it a hyphen, as it marked
  every passage but the one carried over from turn 1, so it read the passages for the previous question. This turn was
  right on 2026-10-31, and one run cannot say whether this is new.
- **Two mechanical failures are right answers.** `dp-transcript-rollback` gives v5.7.3 and names v7.2.8 as another
  system's target, and `dp-mt-encryption-versions` lists 73 days as superseded, as on 2026-10-31. Both trip a rubric
  that forbids the phrase.
- **Smaller habits.** At least five answers discuss passages that do not bear on the question, which the writing
  instruction forbids. `dp-long-hall-transcript` ends in a footer with raw document IDs. `dp-long-canal-log` says "in
  order" and lists days 39, 50, 28, 62, 17 and 6. The wrong citation number of 2026-10-31 (`dp-abstain-named-clause`) is
  right here.
- **The times are not comparable.** The build pool's test build ran from 02:13 to 02:50, beside this run (02:14 to
  02:44).

## What this does not show

- **One run each.** One or two questions either way is noise.
- **Haiku only.** It is a baseline check; the app runs on the local Qwen models.
- **The spend is an estimate.** About $0.50, what the same set cost on 2026-10-31. The record holds 647 Messages API calls
  and no token counts.

## Next

- Say "Do not mention your notes or correct them" in `WRITE_INSTRUCTION` and `SEARCH_OR_WRITE_INSTRUCTION` too, and
  measure it by replaying the five leaking turns' recorded requests.
- In the reading instruction, name the question being read for, instead of "my question", which a follow-up can read
  as an earlier one. Replay `dp-mt-merridew-reversal` turn 2 to measure it.

## Records

- `results/chat/claude-haiku-5-5/2026-11-03-shipped-path-notes.jsonl.gz`: answers and scores, as the scorer writes them.
- `results/chat/claude-haiku-5-5/2026-11-03-shipped-path-notes-turns.jsonl.gz`: every turn's question, answer, the
  reading step's notes and the steps the turn took, which the scores file keeps only for each conversation's last turn.
