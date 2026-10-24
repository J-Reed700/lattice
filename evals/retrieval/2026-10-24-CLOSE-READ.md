# Close read: end-to-end eval and wording A/B (2026-10-24)

Local Qwen3.5-9B, 16,384-token window, one slot. Design: `docs/design/2026-10-23-answer-coverage.md` §10 (the
close read) and `docs/design/2026-10-24-close-read-precision.md` (the wording fix measured here).

## 1. End-to-end eval of the close read as first built

`fix/reading-misses` at 7503dcc8: readings with no verified copy withdrawn, notes matched to their passage, the
close read with the instruction "Here again are the passages you marked with a hyphen. Read each on its own and
write its line in the same form." Run with `scripts/shipped_path_eval.py` on the same recipes and questions as
the 2026-10-23 answer-coverage runs.

| Set | Main after #174 | 7503dcc8 |
|---|---|---|
| 8 long-document items, three runs | 14/24 | **23/24** |
| 6 short single-fact items, one run | 2/6 | **5/6** |

Median time to an answer: 153 s short, 203 to 231 s long. The machine was shared with a C++ build for part of
the runs, so the times are an upper bound.

The two misses:
- long8-r2 `dp-long-chronicle`: the answer dropped the dates the notes held. That is the write step, not the
  reading.
- short6 `dp-section-after-clause`: the reading marked every passage with a hyphen and was withdrawn, and the
  answer took clause 24.111 from the wrong agreement (`2026-10-24-EVAL-JOURNEY.md`, "The last two misses").

The answers also showed a precision problem. On short questions the close read copied whole passages that said
nothing on the question (`dp-paraphrase-clock`: 1,385 and 1,214 output tokens). The writer then repeated one
sentence and ended with "(The answer was cut short…)", and some answers carried 15 to 17 citations. The cause is
the instruction: it gives no criterion, only "the same form", and the 9B took the form from the first passage in
the request. The design document has the full diagnosis.

## 2. The A/B

Offline, with `scripts/replay_close_read_ab.py`, on the saved prompts of two sets of runs: the 25 readings of the
first reading A/B (the 2026-10-23 runs) and the 26 readings of the end-to-end runs above. 51 readings, 15 of them
missing at least one fact before the close read.

### Arms

Stage 1 changes the close read, which decides what facts reach the notes:

| Arm | Instruction | Showings | Room for passages |
|---|---|---|---|
| C0R0 | as first built: "…write its line in the same form", before the passages | repeated while a pass takes a line | window less prompt, notes and the output reservation |
| C1R1 | restates the reading's criterion, after the passages | once | sized from what a reply can copy, no reservation |
| C2R1 | the same sentences, before the passages | once | as C1R1 |
| C1R0 | as C1R1 | once | as C0R0 |

C1R0 runs only when C1R1 loses a fact, to tell the wording apart from the room change.

Stage 2 changes the writing, on the notes of the stage-1 pick, two seeds per reading:

| Arm | Write instruction | Coverage continuation |
|---|---|---|
| W0K0 | as built | a passage counts as covered only when the answer cites it |
| W1K1 | adds "a sentence that several of your notes quote, write once, citing one of them" | a passage also counts as covered when the answer states its note's sentences |

### Stage 1 results

| Arm | Facts noted | Non-fact lines accepted | of them whole passages | Requests | Prompt tokens processed | Output tokens |
|---|---|---|---|---|---|---|
| C0R0 | 281/281 | 66 | 13 | 435 | 490,300 | 15,315 |
| C1R1 | 280/281 | 38 | 0 | 125 | 235,437 | 8,225 |
| C2R1 | 280/281 | 50 | 0 | 125 | 234,740 | 8,432 |
| **C1R0** | **281/281** | **31** | **0** | 494 | 264,502 | 8,859 |

On the 5 short readings, C0R0 accepted 16 non-fact lines, 13 of them whole passages; C1R0 accepted 4, none whole.

C1R1 and C2R1 both lost the same fact (`long8-r2|dp-long-household-log`, passage 7). C1R0, the same wording with
the old room, kept it, so the room change cost the fact and the wording did not. C2R1 accepted more non-fact
lines than C1R1 with the same sentences, so the placement after the passages matters as well as the wording.

Seconds are not compared. C1R0 ran on its own two hours after the other arms, with another build on the
machine, and took 4,457 s against C0R0's 2,699 s while processing about half the prompt tokens and writing
about half the output. Token counts are the cost measure here.

### Stage 2 results

102 writes per arm, on C1R0's notes.

| Arm | Passed | Abstained | Repeated a sentence | Continuations run | Continuations cut off by length | Citations |
|---|---|---|---|---|---|---|
| W0K0 | 101 | 0 | 2 | 3 | 0 | 728 |
| W1K1 | 101 | 0 | 2 | 7 | 0 | 726 |

Two W1K1 writes took 916 s and 305 s. Both answers were of normal length (1,963 and 691 characters) and ended on
their own, so these were stalls on the machine, not long generations.

### Decision

- Stage 1: C1R0 is the only arm that keeps every fact C0R0 copies, and it accepts the fewest non-fact lines.
  It ships.
- Stage 2: the design's rule ships W1K1 when it ties, and it tied. It measured no gain: the same passes, the same
  repeated sentences and the same citations, with four more continuations, each a model call. W0K0 is kept,
  so the write instruction and the coverage continuation are unchanged.
- The room change (R1) is not built.

## 3. Running it

One llama-server with the 9B, nothing else compiling or serving. The inputs are the `.calls.jsonl` request logs of
the runs being replayed (the committed `.calls.jsonl.gz` files, unzipped) and their question files.

```
python3 -I -u scripts/replay_close_read_ab.py --stage 1 --arms C0R0,C1R1,C2R1 \
  --out OUT --llama http://127.0.0.1:8089 \
  --rubric evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-24-reading-ab-rubric.json \
  --rubric evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-23-answer-coverage-short6.questions.json \
  --rubric evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-23-answer-coverage-long8.questions.json \
  --pass1 evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-24-reading-misses-ab-pass1.jsonl \
  --a3 evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-24-reading-misses-ab-pass3-a3.jsonl \
  --old long8-r1.jsonl.calls.jsonl --old long8-r2.jsonl.calls.jsonl --old long8-r3.jsonl.calls.jsonl \
  --old short6-r1.jsonl.calls.jsonl \
  --e2e short6-r1.jsonl.calls.jsonl --e2e long8-r1.jsonl.calls.jsonl \
  --e2e long8-r2.jsonl.calls.jsonl --e2e long8-r3.jsonl.calls.jsonl
```

The `--old` files are the 2026-10-23 answer-coverage request logs and the `--e2e` files the 2026-10-24
reading-misses request logs, each set unzipped into its own directory under the run's short name
(`2026-10-23-answer-coverage-long8-r1.jsonl.calls.jsonl.gz` becomes `long8-r1.jsonl.calls.jsonl`). The script
names a run by the file name up to its first dot, and the `--pass1` and `--a3` records use the short names. Then run
the same arguments with `--stage 1 --arms C1R0` if `--decision` reports that C1R1 lost a fact, `--stage 2` for
the writes, and `--summary` for the tables above. `--dry` builds every request without calling the server.

## 4. Files

In `results/chat/qwen3.5-9b-local/`:

- `2026-10-24-reading-misses-{short6-r1,long8-r1,long8-r2,long8-r3}.*`: the end-to-end runs (answers, request
  logs, scores, run records).
- `2026-10-24-close-read-ab-stage1.jsonl`, `-stage2.jsonl`: one record per reading and arm, and per write.
- `2026-10-24-close-read-ab-summary.txt`: the full tables, including per-subset rows and per-reading facts.
- `2026-10-24-close-read-ab-decision.json`: the decision rule's output.
- `2026-10-24-reading-ab-rubric.json`: the fact passages of the first reading A/B's questions.
