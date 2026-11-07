# Extraction batches that come back empty (2026-11-06)

Qwen3.8-27B (Q5_K_S) on the remote llama-server, Qwen3.5-9B (Q4_K_M) on a local llama-server with the app's flags, and Claude
Haiku 5.5 through the Messages API. Code: `conversation/memory/prompts.rs`. Replay: `scripts/replay_memory_extract_ab.py`.

## What it is

The 27B's active-evidence recall stood at 81 of 81 on 2026-09-26. It was 69 of 81 on 2026-10-21 and 65 of 81 on 2026-11-05,
with a mandatory-item shortfall of 13.

The cause is in the extraction batches. A window that holds the user's rule comes back valid and with `add: []`, so the
rule never reaches memory and every later gate has nothing to hold. Decoding is greedy, so a window's answer depends on
nothing but its text, and that includes its random message ids: the same conversation can return items or nothing.

## What it is not

The replay sends each family's first window, rendered as `render_extractor_prompt` renders it, under each arm, with the same
ids in every arm, so a difference is the arm's. On the 27B, 12 windows, against the prompt as #43 left it:

| Arm | Empty | Required quotes |
|---|---:|---:|
| The prompt as #43 left it | 8 | 4 of 15 |
| `repeat_penalty 1.1`, as before #43 (7 windows) | 2, against 2 | 6 of 9, against 6 of 9 |
| `reasoning_effort: none`, as before #43 | 8 | 4 of 15 |
| The label's `maxLength: 200`, as before #301 | 8 | 4 of 15 |
| The sentence "Return only JSON...", as before #43 | 0 | 15 of 15 |
| All that request's settings together | 0 | 15 of 15 |

So it is one sentence. #43 removed it with the other format demands in learning, memory and intent prompts, because "asking
for JSON in the prompt costs small models most of their structured-output accuracy; the grammar does not". That was never
measured on the 27B. `the_extractor_prompt_states_every_rule_the_design_requires` has asserted the sentence's absence since.

Putting it back is not the fix, because #43 was right about the 9B:

| Wording | 27B: empty | 27B: quotes | 9B: empty | 9B: quotes | Haiku: empty | Haiku: quotes |
|---|---:|---:|---:|---:|---:|---:|
| None, as #43 left it | 8 of 14 | 7 of 18 | 0 of 18 | 22 of 24 | 0 of 72 | 108 of 108 |
| "Return only JSON..." | 0 of 12 | 15 of 15 | 3 of 36 | 46 of 54 (none: 51) | 0 of 72 | 108 of 108 |
| "Return only JSON." | 1 of 14 | 17 of 18 | 4 of 18 | 16 of 24 | | |
| "Return JSON matching the requested schema." | 1 of 14 | 17 of 18 | 3 of 18 | 17 of 24 | | |
| "Return only JSON..." at the prompt's end | 5 of 14 | 13 of 18 | 0 of 18 | 21 of 24 | | |
| **What belongs in `add`, and when it is empty** | **0 of 14** | **18 of 18** | **0 of 18** | **22 of 24** | **0 of 72** | **108 of 108** |
| The same at the prompt's end | 0 of 14 | 18 of 18 | 0 of 18 | 22 of 24 | | |

Each 27B and 9B row compares the windows both it and the first row ran.

So a sentence about the format helps one model and costs the other, and a sentence about what `add` holds costs neither. Why
a format sentence moves a greedy decode this far is not established here.

## The fix

After "Never classify an action request as a decision.", the extractor's prompt now says:

> Every requirement, goal, decision, fact, preference or unresolved question the user states goes in add; add is empty only
> when no segment holds one.

It cost nothing on any of the three. On Haiku the same windows give 214 items against 198, because a rule stated in two
clauses becomes two items. In `quoted_adversarial_text` on all three models, what comes out is still only the user's own
rule: the pasted vendor instruction is never taken for one.

`memory-extractor/2026-11-06.16` records the change, because committed memory records which contract wrote it.

## The release baseline on the 27B

20 families, 3 repeats, the settings of 2026-09-26.

| Run | Recall | Shortfall | Review calls | Abstention |
|---|---:|---:|---:|---:|
| 2026-09-26 | 81 of 81 | 0 | 108 | 6 of 9 |
| 2026-10-21 | 69 of 81 | 7 | about 71 | 7 of 9 |
| 2026-11-05, main with the branches then open | 65 of 81 | 13 | 74 | 9 of 9 |
| "Return only JSON..." back, with the reviewer's fix | 81 of 81 | 0 | 95 | 8 of 9 |
| **What belongs in `add`, with the reviewer's fix** | **81 of 81** | **0** | **92** | **8 of 9** |

Every other gate holds in the last row, and the test's one failure is abstention. Both of the 8 of 9 are the grader's, and both answers read as right. In the old
sentence's run the refusal holds a curly apostrophe, and `contains_marker` compared it with a straight one. In this one the
answer is "Not set yet", and the family had no marker for it. Both are fixed here (`conversation-memory-eval/2026-11-06.2`).

All 117 distinct answers of the last run's bounded-memory arm were read by hand, and none is wrong. Required quotations hold
in 101 of 102, against 99 on 2026-09-26, and the one miss states the same rule in other words. Two of the three runs of
`multi_turn_tool_work` say they cannot rebuild the report without the notes, and give its constraints: an honest answer, since
the notes' contents are not in that turn's context. Review calls are below 2026-09-26's (92 against 108) with recall the same;
this does not establish why.

## The memory families on the 9B

13 families, 3 repeats, with the server settings of 2026-09-26, on the tree with this sentence and the reviewer's fix
(`31d92851`), 2026-11-06 21:32-23:21. Builds ran beside it, so its latencies are not comparable.

| Run | Recall | Shortfall | Review calls | Answers quoting what they must |
|---|---:|---:|---:|---:|
| 2026-09-26: 13 families, 1 repeat, harness 2026-09-23.1 | 17 of 20 | 6 | 20 | 20 of 21 |
| With this and the reviewer's fix: 13 families, 3 repeats | 53 of 60 | 7 | 247 | 56 of 63 |

Every pipeline gate held but one: `exact_identifiers`' mandatory-item floor, short 6 over its three runs, while its
recall was 12 of 12 and every answer quoted the identifiers. The 9B files the paths, versions and station codes as
`user_fact`, which the floor does not count: "Write output to /srv/Data/Réservoir/tide_2026Q1.parquet — capital D,
capital R, and the accent matters." comes out as a fact, not a requirement. It does the same without this sentence: in
the first-window replay above, its 9 items in that family are `user_fact` under both arms. The shortfall is the 9B's
labelling, not this change's.

- `topic_return`: recall 0 of 3, as on 2026-09-26 (0 of 1); all 6 answers quote the original turn.
- The 7 answers that miss their required quotation (`utility_failure` 3, `long_user_messages` 3,
  `storage_lifecycle` 1) are all right, read by hand: each states the rule in other words ("forbids overwriting" where
  the quotation is "never overwrite").
- The four families where this run fell short (`exact_identifiers`' floor; the recall of `many_active_constraints`,
  `mid_conversation_correction` and `topic_return`) run again without this sentence, same harness and model, to compare
  like with like: PENDING.

## How this was missed

The release baseline is what catches this, and it takes two hours, so #43 did not run it on the 27B. #154 did on 2026-10-21,
saw the fall and found the empty batches. The 09-26 traces were not kept, so it inferred a cause: #43 had also turned on key
order, so the passages no longer came last. It sorted the keys and asked for a live rerun, and none ran until 2026-11-05. That
run has the sorted keys, and it is lower still.

The replay here asks the same question in minutes. The extractor's prompt now has a test for this sentence, and a change to
that prompt should run `replay_memory_extract_ab.py` on the 27B and the 9B before it merges.

## Records

- `results/qwen3.8-27b-remote/2026-11-06-empty-extractions.jsonl.gz`, `results/qwen3.5-9b-local/2026-11-06-empty-extractions.jsonl.gz`,
  `results/claude-haiku-5-5/2026-11-06-empty-extractions.jsonl.gz`: every arm, a row a window, with its arm and model.
- `results/qwen3.5-9b-local/2026-11-06-release-baseline.md`: the 9B's 13 families with this sentence.
- `results/qwen3.8-27b-remote/2026-11-06-release-baseline.md`: the wording that shipped. `2026-11-06-release-baseline-sentence.md`: the old sentence back. `2026-11-05-release-baseline-before-the-wording.md`: before either.
