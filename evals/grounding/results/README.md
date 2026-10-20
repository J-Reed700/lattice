# Grounding eval results

Run history for the evals in [`../README.md`](../README.md). One row per run;
the per-finding JSONL files beside this one are named in the row when they
are committed.

## Page recall (`live_page_recall_eval`)

Two-pass claim pages on a stored run (course evidence index §7.1–7.2). The
three orders of §5.2 decide `REACHES` (D2) and the page numbers decide
`B_DEPTH` (D5); record the decision a run supports in its notes.

| date | database | program | order | retrieval | embedding identity | pace (prompt tok/s, output tok/s, answer tok) | settled by A | reached in B per page | never reached | tokens to support (median / p90 / total) | modelled minutes | run-out list (passages, tokens: median / p90 / max) | cited off subject | commit | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 2026-10-20 | owner's app DB copy (2026-10-19) | e26a7889 (.NET course, 150 sources) | relevance | lexical | — | 850, 11.3, 130 | 19 | p1: 29, p2: 14, p3: 7, p4: 4, p5: 4, p7: 2, p8: 1, p9: 1, p11: 1, p13: 1 | 7 | 16244 / 44193 / 1414634 | 70.2 | 80, 114306 / 252, 527093 / 313, 645590 | 3 | c2422f04 | `2026-10-20-page-recall-all-lexical.jsonl`; D2: relevance ≈ lesson-sources (7 vs 9 never reached), so relevance kept; D5 pending the hybrid run |
| 2026-10-20 | owner's app DB copy (2026-10-19) | e26a7889 (.NET course, 150 sources) | lesson-sources | lexical | — | 850, 11.3, 130 | 19 | p1: 29, p2: 9, p3: 4, p4: 5, p5: 4, p6: 6, p7: 3, p8: 1, p10: 1 | 9 | 13871 / 48032 / 1369047 | 69.8 | 79, 121365 / 254, 528370 / 315, 647417 | 3 | c2422f04 | `2026-10-20-page-recall-all-lexical.jsonl`;  |
| 2026-10-20 | owner's app DB copy (2026-10-19) | e26a7889 (.NET course, 150 sources) | tie-break | lexical | — | 850, 11.3, 130 | 19 | p1: 29, p2: 14, p3: 7, p4: 4, p5: 4, p7: 2, p8: 1, p9: 1, p11: 1, p13: 1 | 7 | 16244 / 44193 / 1414634 | 70.2 | 80, 114306 / 252, 527093 / 313, 645590 | 3 | c2422f04 | `2026-10-20-page-recall-all-lexical.jsonl`;  |
| 2026-10-20 | owner's app DB copy (2026-10-20) | e26a7889 (.NET course, 235 sources) | relevance | lexical | — | 850, 11.3, 130 | 19 | p1: 27, p2: 14, p3: 10, p4: 2, p5: 1, p6: 5, p7: 1, p13: 2, p16: 1 | 8 | 10033 / 45525 / 1524713 | 72.7 | 141, 211680 / 319, 611429 / 533, 1138060 | 2 | 6125b3e4 | `2026-10-20-page-recall-all-lexical-235.jsonl`; the course has grown to 235 sources since the run above |
| 2026-10-20 | owner's app DB copy (2026-10-20) | e26a7889 (.NET course, 235 sources) | relevance | hybrid | sha256:9a4938b7… (Qwen3-Embedding-0.6B, the app's own course index, copied) | 850, 11.3, 130 | 19 | p1: 32, p2: 10, p3: 6, p4: 5, p5: 3, p6: 4, p7: 2, p9: 1, p16: 1 | 7 | 9719 / 44339 / 1452868 | 69.8 | 76, 107175 / 303, 588576 / 461, 1043532 | 2 | 6125b3e4 | `2026-10-20-page-recall-all-hybrid-235.jsonl`; after course ranking dropped the 4-per-document spread, which had buried one long source and left 12 never reached; D5 = break-even (list end costs up to 27 min for one claim) |
| 2026-10-20 | owner's app DB copy (2026-10-20) | e26a7889 (.NET course, 235 sources) | lesson-sources | hybrid | sha256:9a4938b7… (Qwen3-Embedding-0.6B, the app's own course index, copied) | 850, 11.3, 130 | 19 | p1: 34, p2: 8, p3: 4, p4: 5, p5: 3, p6: 5, p7: 3, p8: 1, p9: 1, p12: 1, p16: 1 | 5 | 9578 / 45085 / 1583630 | 77.3 | 79, 121139 / 314, 594891 / 467, 1043867 | 2 | 6125b3e4 | `2026-10-20-page-recall-all-hybrid-235.jsonl`; relevance kept (D2): fewer minutes, two more never reached |
