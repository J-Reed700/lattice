# How much raw history a long chat should send (2026-10-31)

Qwen3.8-27B on the remote llama-server the app is set to, 196,608-token window. Code:
`context_assembler/budget.rs`. The record is
`results/qwen3.8-27b-remote/2026-10-31-history-share.jsonl.gz`.

**35% stays.** Of 18 answers, 15 were right at 15%, 16 at 25%, 17 at 35% and 17 at 50%. Going to 50% sends about
35% more tokens on every turn for no gain. The gap between 25% and 35% is one answer, inside the usual noise, so
this says 35% is sound, not that it is the best number.

## Why

The budget gives recent history 35% of the room left after the fixed costs. The design doc calls that an initial
policy and nothing had measured it. `desktop-path-v1` cannot: its conversations run at most three turns, so
they never reach the limit. The literature gives no ratio to copy. LongMemEval (arXiv 2410.10813) found models
answer worse from a full ~115,000-token history than from the relevant part alone, so more is not free.

## Method

`scripts/replay_history_share.py`. Three synthetic conversations of about 100,000 tokens, built from
`synthetic-library-v3`: the user asks to be walked through a document and the assistant gives it. Each is cut at
15%, 25%, 35% and 50% of the 152,651 tokens left after the fixed costs. The older part goes through the app's own
summary prompt, and the recent part goes whole, rendered as the app renders a turn, with the app's system prompt.

Six questions per cut, 72 answers:

| Kind | What it asks | Where the answer is |
|---|---|---|
| `early` | A number the user stated 3% of the way in | Summary or recall at every share |
| `update` | A code the user replaced at 70% | Raw from 25%, otherwise summary |
| `detail-old` | A detail inside what the assistant said at 20% | Never raw: summary or recall only |
| `detail-middle` | A detail inside what the assistant said at 60% | Raw at 35% and 50%, not below |
| `absent` | Something never stated | Nowhere |
| `library` | A question on a passage supplied this turn | The passage |

`detail-middle` is the direct test of the share: it is the kind whose answer moves out of raw history between 35%
and 25%. Both detail kinds ask for something only one document says.

I read all 72 answers. `passed` is the script's check and `by_hand` my grade, with a `note` on each one that needed
explaining.

## Results

Correct by hand, of 3 each:

| Kind | 15% | 25% | **35%** | 50% |
|---|---:|---:|---:|---:|
| `early` | 3 | 3 | **3** | 3 |
| `update` | 3 | 3 | **3** | 3 |
| `detail-old` | 1 | 2 | **3** | 2 |
| `detail-middle` | 2 | 2 | **3** | 3 |
| `absent` | 3 | 3 | **3** | 3 |
| `library` | 3 | 3 | **2** | 3 |
| **Of 18** | **15** | **16** | **17** | **17** |
| The script's check | 13 | 16 | 18 | 17 |

The two differ in three places. Two refusals at 15% used a curly apostrophe the check did not know, and those
answers were right; that is fixed. The check passed one wrong answer at 35%: it quoted "every 48 calendar days" and
then said to expect a reply "within roughly two weeks".

Cost per turn:

| | 15% | 25% | 35% | 50% |
|---|---:|---:|---:|---:|
| Median prompt, tokens | 31,087 | 47,116 | 64,910 | 87,603 |
| Median answer, seconds | 8.5 | 10.9 | 10.2 | 8.4 |
| First answer on a new prompt, seconds | 30 | 48 | 70 | 103 |

The first answer builds the prompt cache, and that wait returns whenever the older part of the prompt changes. After
it, size hardly shows on the 27B. On a cloud model the size is paid for on every turn unless it is cached.

## What this shows

- **What the user said survives at every share.** Numbers, replaced values and refusals: 36 of 36. The summary
  kept them even at 15%, where about 80% of the conversation was folded.
- **More raw history did not hurt the 27B.** With about 76,000 tokens of it, the answer in a supplied passage was
  still found. The one slip was arithmetic, at 35%, with the passage in front of it. This is three distinct questions,
  so it is no answer to LongMemEval, but nothing here repeats its drop.
- **What the assistant said earlier is where the share shows.** The two detail kinds score 3, 4, 6 and 5 of 6.
- **That gap is thinner than it looks.** In `detail-middle`, the direct test, both misses are one question, "My kid
  was sick", whose words share nothing with the passage's "dependant's illness". The replay's recall is word
  overlap, which cannot make that link. `detail-old` is never in raw history, so its spread comes from what the
  summary and recall happened to carry, not from the share.

## What this does not show

- **Three conversations are few.** ±1 or 2 answers between runs is noise here, which covers every gap in the totals.
- **This is a replay.** It leaves out the app's ledger of what the user stated and its richer recall, and it writes
  one summary in a single pass where the app rolls one forward. All three should make small shares look worse here
  than in the app, so 25% could be as good as 35%.
- **One model, one conversation shape.** These assistant turns are pasted documents, far denser in detail than real
  replies, so 15% is punished more than a normal chat would punish it.

## Also seen

- **One reply was the summary repeated back**: `detail-middle` at 25%, opening with the summary's own bracketed label
  and stopping mid-line. The app sends a summary as an assistant message too, so this can happen there. It is one
  answer of 72, in a conversation of pasted documents, so no change was made.
- **Long pasted histories teach the model to paste.** One answer was a whole document, seven opened with a heading
  as a document does, and one added a diagram nobody asked for, at 25% and above. That comes from this conversation's
  shape.
- **Summaries did not fill their room.** They ran 5,500 to 12,400 characters against about 69,000 available, and
  one at 50% lost a fact that those at smaller shares kept, so a summary's length is set by the model and not by this
  budget.

## Decision

Keep recent history at 35% and do not raise it. Any later change should come from the shipped path, with the app's
recall and ledger, on real conversations.
