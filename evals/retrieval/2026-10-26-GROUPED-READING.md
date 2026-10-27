# Grouped reading: A/B against the single reading and its close read (2026-10-26)

Local Qwen3.5-9B Q4_K_M (sha256 `03b74727…`), 16,384-token window, one slot, llama.cpp build 11669
(`620d2225c`) on an RTX 4070 Laptop GPU, every layer offloaded. Code: `chat/tool_loop/reading/groups.rs`.

## Why

The reading asks for one line per numbered passage before the answer is written: the sentences that bear on
the question, copied, or a hyphen. As shipped it read every passage in one pass (16 to 36 of them here, median
32), deciding each passage thousands of tokens after the passage itself. The 9B marked passages that held the
facts with a hyphen, the later in the list the likelier, and on some questions marked every passage: no line
was a verified copy, and the reading was withdrawn. The close read then showed the marked passages again, one
request per group that fit: up to 34 requests a question in a held 32k window. In the 2026-10-24 close-read A/B
every fact the close read recovered stood first or second in the group it was shown in.

The grouped reading shows every passage that way from the start. The turn's message closes, in place of the
reading instruction, with a group of its passages shown again and the close read's measured criterion after
them; the model writes the group's lines. Nothing is read twice.

## Method

`scripts/replay_grouped_reading_ab.py`, on the saved reading prompts of eight runs: the 2026-10-23
answer-coverage runs (`long8-r1..r3`, `short6-r1`) and the 2026-10-24 reading-misses runs (the same sets).
60 readings whose question has at least one fact passage, 300 fact passages in all. Every arm runs live on one
server, on the same prompts, each timed on a warm prefix. Parsing, the copy check, attribution and the fact
scoring are `replay_close_read_ab.py`'s own, imported.

- **SHIPPED**: main's reading (`READING_INSTRUCTION`, sweeps while one accounts for a label, attribution), then
  the close read as built (`CLOSE_READ_INSTRUCTION`, one showing, room = window − prompt − notes −
  instruction − reservation).
- **G*k***: groups of *k* passages in label order, each shown again in place of the reading instruction; one
  sweep loop per group; attribution over every group's lines; no close read.

A fact is noted when the notes hold a verified copy from the passage that states it.

## Results

All 60 readings (G1 was stopped after 25 readings; its row is those 25, against SHIPPED's on the same 25):

| Arm | Facts noted | Readings missing a fact | Non-fact lines | of them whole passages | Requests | Prompt tokens processed | Output tokens | Seconds | Median s/reading |
|---|---|---|---|---|---|---|---|---|---|
| SHIPPED | 281/300 | 9 | 75 | 1 | 556 | 317,994 | 29,763 | 1,094 | 18.1 |
| G2 | 293/300 | 7 | 117 | 1 | 880 | 507,926 | 29,009 | 1,201 | 17.0 |
| **G4** | **294/300** | **4** | 112 | 5 | 468 | 420,026 | 28,122 | 1,065 | **14.8** |
| G6 | 281/300 | 10 | 106 | — | 325 | 539,037 | — | 1,041 | 16.6 |
| G8 | 278/300 | 7 | 115 | — | 260 | 474,576 | — | 1,052 | 16.4 |
| *25 readings:* SHIPPED | 138/145 | 2 | 29 | 0 | 224 | 140,225 | 14,316 | 543 | 20.1 |
| *25 readings:* G1 | 136/145 | 6 | 67 | 3 | 768 | 356,512 | 17,255 | 840 | 27.2 |

By set:

| Arm | Long (288 facts) | Short (12 facts) |
|---|---|---|
| SHIPPED | 276 (2 readings missing) | 5 (7 missing) |
| G2 | 283 (5) | 10 (2) |
| G4 | 285 (1) | 9 (3) |

- SHIPPED's misses are mostly whole readings: on `dp-long-hall-transcript` (two runs) the single reading marked
  all 16 passages with a hyphen, was withdrawn, and the close read never ran (0 of 6 facts). G4 noted all six
  on both. On five of the six short questions SHIPPED's reading missed the one fact passage.
- One passage per request (G1) is worse, not better: shown alone, the 9B marked fact passages with a hyphen it
  noted beside a few others. G2 sits between.
- Larger groups fall back: G6 and G8 note no more facts than SHIPPED, in fewer requests but no faster, each
  request writing more.
- G4 costs precision: 112 non-fact lines against 75 (every one a verified copy of its own passage, so no fact is
  invented, but the writer has more to leave out).
- G4's misses: `rm-long8-r1|dp-long-family-history` (3 facts), and one fact on each of three short questions.

Seconds are this machine's, the A/B sharing it with a Rust build for part of the run (both arms alike).

## Decision

Groups of four (`PASSAGES_PER_GROUP`): the most facts noted, the fewest readings missing one and the lowest
median time of every size tried (1, 2, 4, 6, 8).

## The answers

`scripts/replay_answer_ab.py` writes the answers on 30 of the same prompts (the 2026-10-23 runs: 24 long
readings, 6 short), every arm on the same server, writes with calls forbidden (a replay cannot run them),
seed 1, scored by the rubric's accepted forms in the answer:

| Arm | Facts stated | Answers with every fact | Long: facts | Short: answers right | Median s |
|---|---|---|---|---|---|
| DIRECT: the answer from the passages, no reading | 137/152 | 19/30 | 131/144 | 5/6 | 6.8 |
| NOTES: grouped reading, then the answer, then coverage | 149/152 | 28/30 | 143/144 | 5/6 | 25.7 |
| **NOTESV: as NOTES, only verified notes replayed** | **149/152** | **28/30** | **143/144** | 5/6 | 24.9 |
| CHECK: DIRECT's answer, then the grouped notes as a coverage check | 148/152 | 27/30 | 142/144 | 5/6 | 25.4 |

- The reading earns its time on the long questions: answered from the passages alone, 11 of 30 answers left
  out a fact, against 2 of 30 from the notes.
- On the six short questions it changes nothing (every arm 5/6, the same item missed by all) and costs 10 to
  25 s an answer.
- Replaying only the notes that copy their own passage loses no fact, so it is built: a line that copies no
  passage, or another passage's, is replayed as a hyphen.
- Answering first and checking after recovers most of DIRECT's misses but not all, and is no faster.

## Running it

One llama-server with the 9B (`-ngl 99 --ctx-size 16384 -np 1 --jinja`), the eight `.calls.jsonl.gz` logs of
`results/chat/qwen3.5-9b-local/` unzipped (`2026-10-23-answer-coverage-long8-r1.jsonl.calls.jsonl.gz` →
`cov-long8-r1.jsonl.calls.jsonl`, `2026-10-24-reading-misses-…` → `rm-…`):

```
python -X utf8 -I -u scripts/replay_grouped_reading_ab.py --out OUT --llama http://127.0.0.1:8089 \
  --rubric evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-24-reading-ab-rubric.json \
  --rubric evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-23-answer-coverage-short6.questions.json \
  --rubric evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-23-answer-coverage-long8.questions.json \
  --calls cov-long8-r1.jsonl.calls.jsonl … --calls rm-short6-r1.jsonl.calls.jsonl --arms SHIPPED,G2,G4
python -X utf8 -I scripts/replay_grouped_reading_ab.py --out OUT --summary
```

The run resumes per reading and arm. `results/chat/qwen3.5-9b-local/2026-10-26-grouped-reading-ab.jsonl.gz`
holds every record, each with its requests, timings and lines.
