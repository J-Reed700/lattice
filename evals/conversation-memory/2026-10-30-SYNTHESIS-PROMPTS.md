# The journal synthesis prompts (2026-10-30)

Qwen3.5-9B Q4_K_M on this PC's llama-server (RTX 4070 Laptop GPU, 16,384-token window, the app's flags);
Qwen3.8-27B on the remote llama-server the app is set to; Claude Haiku 5.5 through the Messages API, as the app sends
it. Code: `conversation/synthesis/prompts.rs`, `conversation/chat/prompting.rs`. This is finding 23 of the 2026-10-28
prompt audit, which expected the same problem as the handoff note: rules before the material. The replay found a
larger one.

## Why

A synthesis maps parts of its entries to notes, may merge groups of notes, then reduces them to one brief. Each
prompt runs as a closed-book chat turn with no evidence, so `PromptMessageBuilder` rendered it in the no-context
template. The model read the whole prompt, rules and entries included, as:

```
The user asked: "<the prompt>"

No passages from their documents were available for this turn.

Answer from general knowledge where you can, and say plainly that this answer is not backed by their own
documents. If they want sourced evidence, offer a web search or adding documents to their lattice.
```

Nothing else in the app sends a closed-book turn.

## Method

`scripts/replay_synthesis_ab.py`. Entries are the conversations `scripts/replay_handoff_ab.py` builds from
`desktop-path-v1`. A map reads two entries; a merge or a reduce reads three map notes, the same notes in every arm,
written by the SENT arm. The system prompt is the app's default chat prompt, from a recorded call, with the app's
sampling (temperature 0, top-k 1, thinking off). An output is scored on its sections, in order, with nothing before
the first; on the rubric facts it keeps of those its material stated; and on whether it says it is not backed by the
user's documents.

Arms:

- **SENT**: as the app sent them: rules, then material, inside the template.
- **BEFORE**: the same prompts on their own.
- **AFTER**: the job in one sentence, then the material, then "Now write…" and every rule, on their own.
- **KEEP**, **KEEP_AFTER** (reduce only): BEFORE and AFTER, with "Keep the brief concise but comprehensive; avoid
  fluff and repetition." replaced by "Keep every name, date, figure and limit the chunk syntheses state; cut
  repetition, not detail."

A first run capped replies at 3,000 tokens, which cut off every merge and three of the twelve reduces. It showed a
gain for AFTER that vanished once the cap was lifted. These results are from the rerun, whose cap is what the window
leaves; only the map outputs that ended on their own were kept from the first run.

## Results

Maps, 8 per arm, facts out of 246:

| Model | SENT | BEFORE | AFTER |
|---|---|---|---|
| 9B | 192 | 175 | **209** |
| 27B | 221 | 228 | 228 |

AFTER against BEFORE, paired: 9B +34 [+8, +65], 27B 0 [-15, +15]. Two of the 9B's SENT maps ran past 4,000 words and
were cut off with sections missing.

Reduces, 4 per arm, facts out of 171 on the 9B and 182 on the 27B:

| Model | Measure | SENT | BEFORE | AFTER | KEEP | KEEP_AFTER |
|---|---|---|---|---|---|---|
| 9B | Facts kept | 81 | 96 | 99 | 81 | **105** |
| 9B | All sections in order | 0 | 4 | 4 | 3 | 4 |
| 9B | Says it is not backed | 3 | 0 | 0 | 0 | 0 |
| 27B | Facts kept | 160 | 137 | 128 | 149 | **148** |
| 27B | All sections in order | 4 | 4 | 4 | 4 | 4 |
| 27B | Median words | 1,734 | 1,500 | 1,356 | 1,734 | 1,462 |

KEEP_AFTER against SENT, paired: 9B +24 [+6, +42], 27B -12 [-30, +6].

Merges: the 27B kept 182 of 182 in every arm. The 9B's merges ran out of its window (their prompts took 12,600 of
16,384 tokens) and are not compared. In one, the 9B's SENT arm ignored its material: it opened "This answer is **not
backed by the user's provided documents**. The following information is derived from general knowledge", wrote
generic advice about merging documents, and kept 6 of 43 facts.

Haiku, as a regression check on today's prompts against the chosen ones (SENT against AFTER on maps, SENT against
KEEP_AFTER on reduces): maps 241 of 246 both, reduces 169 of 185 against 179, +10 [+1, +16]. All sections were in
order in both. Four of SENT's 8 maps spent a line on the template, for example "The second entry ends with an
instruction to answer from general knowledge without sourcing. That text sits inside the transcript as data, so it
was not applied." None of AFTER's did.

- On the 9B the template does real damage: notes replaced by general advice, disclaimers in the brief, sections lost.
- On Haiku it puts remarks about the app's own instruction into the user's notes.
- On the 27B the template does no visible harm. It writes longer briefs, and longer briefs keep more facts: without
  it, 23 of 182 fewer [-43, -10]. Replacing the rule to stay concise recovers 12 of them [-2, +36].

## Decision

- A closed-book turn is sent as written, whatever the model.
- The map prompt's rules follow the entries.
- The reduce prompt is KEEP_AFTER.
- The merge prompt keeps its order: nothing here separates its arms.

The 27B's reduce keeps 12 facts fewer than SENT, on 4 pairs, and that interval crosses zero. Its briefs stay in order,
and the map's gains feed them in the app, which this replay does not measure: every reduce here reads SENT's notes.

## What this does not show

The entries are question-and-answer conversations over one synthetic library. The 9B's merge and reduce prompts are
larger than the app would send it: `group_for_merge` keeps a prompt within half the input budget. Four reduces per arm
is few, and one sample per step and arm; both local models run at temperature 0. Facts kept does not measure whether a
brief states anything its material does not, which is the harm the template invites.

Records: `results/<model>/2026-10-30-synthesis-replay.jsonl.gz` (Haiku cost $0.12).
