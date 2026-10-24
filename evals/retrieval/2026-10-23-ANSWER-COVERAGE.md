# Answer coverage (#174) on the merged tree — 2026-10-23

Shipped-path eval, Qwen3.5-9B-Q4_K_M local (llama.cpp, 16,384-token window, one slot, thinking off), on main at 775f03ee plus #171, #173 and #174 (eval tree `e29e840b`). Questions come from `desktop-path-v1`. These are the items that failed on the shipped requests before #174: the 8 long-document items run three times, and 6 short single-fact items run once as a regression check.

"Before" is the shipped answer request replayed on two seeds (`scripts/replay_answer_round.py`, arm "all passages, tools offered"), from the run that #167 was measured on. "After" is the full pipeline end to end.

Result files are in `results/chat/qwen3.5-9b-local/2026-10-23-answer-coverage-{long8-r1,long8-r2,long8-r3,short6-r1}.*`.

## Long documents: 1/16 → 14/24

| Item | Before | r1 | r2 | r3 |
|---|---|---|---|---|
| household-log | 0/2 | pass | fail: 2025-03-19 | pass |
| hall-transcript | 0/2 | fail: 66.6 per cent | fail: 66.6 per cent | fail: 6,497.92 |
| chronicle | 0/2 | fail: 1850-11-18 | pass | pass |
| lab-notebook | 0/2 | pass | pass | pass |
| choir-minutes | 0/2 | pass | fail: 2027-06-19 | pass |
| conservation-record | 0/2 | fail: 13 weeks | pass | fail: 46.8 per cent, 13 weeks |
| family-history | 0/2 | fail: 73.8 per cent | pass | fail: 73.8 per cent |
| apiary-log | 1/2 | pass | pass | pass |

Every missing fact was in the prompt. Almost all of the misses come from the reading round, which wrote a hyphen for the passage that holds the fact; in r1 and r2 that was 6 of the 7 failures. Analysis of all 24 readings, covering 144 passages that bear on their question:

| What the reading did with a bearing passage | Count |
|---|---|
| Quoted it under its own label | 121 |
| Quoted it under [1], then wrote a hyphen for its real label | 7 |
| Hall r3: wrote `1. [1] -` and so on, a hyphen for every label | 6 |
| Wrote a hyphen and quoted the sentence nowhere | 10 |

The misses follow rank, not position within the passage. Among an item's bearing passages, those ranked 1–2 were never missed (0/46), ranks 3–5 were missed 5/69 times, and rank 6 was missed 5/23 times. Facts at the very end of a passage were quoted as often as facts at its start. The other answer-side miss is chronicle r1: the notes copied "1850-11-18", but the answer wrote "1850".

## Short questions: regressed

| Item | Before | After |
|---|---|---|
| paraphrase-clock | 2/2 | fail: abstained |
| buried-handbook-ticket | 2/2 | pass |
| buried-agreement-meeting | 2/2 | pass |
| section-after-clause | 1/2 | fail: abstained |
| opening-agreement | 2/2 | fail: abstained |
| transcript-rollback | 2/2 | fail: abstained |

There are two causes:

- **Readings that were all hyphens.** On opening-agreement, section-after-clause and transcript-rollback, the reading wrote a hyphen for every label. On two of them it then added its own "not available" paragraph. Those notes are replayed as the model's own message, and the answer followed them into abstaining, even though the fact sat in [1], [2] and [3] respectively. The reading parser takes an all-hyphen reading as complete, so the sweep never runs.
- **A hedge on literal mismatch.** On paraphrase-clock the reading quoted the right passage, but the answer opened with "The provided context contains no information regarding … not general machine disagreement", then gave the fact.

## Time

Answered in a median of 92 s, for both the long and the short items. The 2026-10-20 full desktop-path run had a median of 57 s over all 18 items, but the item mix differs.

## Next

The follow-up PR fixes the reading omissions and the short-question regression. Its design will be `docs/design/2026-10-23-answer-coverage.md` §10, and its offline replay A/B uses short6 as the regression gate.
