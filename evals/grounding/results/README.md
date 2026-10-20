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
