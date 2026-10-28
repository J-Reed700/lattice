# Evidence cut to fit the window: would reading it in its own groups help? (2026-10-27)

Local Qwen3.5-9B Q4_K_M, 16,384-token window, one slot, llama.cpp build 11669 on an RTX 4070 Laptop GPU, the
app's cache flags. Code under test: none; this asks whether a change is worth making.

## Why

When a turn's request will not fit the window, the context assembler gives up the turn's evidence one item at
a time, lowest-ranked first (`evict_until_fits`), and the turn drops the sources of what was cut
(`TurnMessage::keep_sources`). The model is told that lower-ranked evidence was left out; what that evidence
said is lost. With the reading in groups, a cut passage could still be read in a group of its own, shown only
in that group's request, and reach the writer as a note.

## Method

`scripts/replay_cut_evidence_ab.py`, on the 60 saved readings of the grouped-reading A/B (2026-10-26). Each
prompt keeps its first half of passages, as `keep_sources` keeps the first items; the cut half is read in groups
of four after the cut message, each shown in full with its document heading under an instruction that says they
did not fit above. Notes are judged as that A/B's: a verified copy from a passage that states a fact. The
comparison is that A/B's G4 arm on the uncut prompt, where the same passages sat in the window.

## Results

| | Fact passages noted | Non-fact lines | Requests | Seconds |
|---|---|---|---|---|
| Cut half, read in groups of its own | 24/25 | 65 | 240 | 843 (median 11.8 a reading) |
| The same passages in the window (G4, uncut) | 25/25 | 61 | | |

- Read outside the window, the cut passages are noted about as well as inside it. The one miss:
  `rm-long8-r3|dp-long-family-history` [21].
- Facts sit near the top of the ranking. Of the 300 fact passages in these 60 readings, the lowest-ranked half
  holds 25 (in 6 readings), the lowest third 10 (5 readings), and the lowest quarter 1.

## Decision

Not built. The cut is an overflow guard: retrieval is sized to the evidence budget the turn's allocation gives
it, so the assembler cuts only the last few items when the plan overruns. At that size the cut passages hold
almost none of the facts (one in 300 at a quarter), and reading them would cost every such turn a request per
four passages, with their sources carried through the reading, the citations and resumed research turns.
Worth revisiting if the turn log's "Cut the turn's lowest-ranked evidence" line shows large cuts in use.

Records: `results/chat/qwen3.5-9b-local/2026-10-27-cut-evidence.jsonl.gz`.
