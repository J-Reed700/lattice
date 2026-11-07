# Claims by class: replay of a real lesson's saved claims, 2026-11-01

Design: `docs/design/2026-11-01-lesson-claim-classes.md`. Script: `claim-classes/replay_classes.py`. Model: the served
`/models/Qwen3.8-27B-Q5_K_S.gguf`, read from `/v1/models` and written on every row. Inputs: the 105 claims of one
lesson of a real course (`inventory-section-v1`), the passage each was last judged against (`claim-evidence-v2`
spans sliced from `learning_source_versions.full_text`, 104 of 105 verified by sha256), five claims Josh cited with
their saved evidence, and 17 planted statements. The lesson's own sentences stay out of the repo; the planted rows
are in `claim-classes/results/2026-11-01-planted.jsonl`. Every row was read by hand before the counts below.

## What was asked

Can a lesson's statements be sorted by what it takes to check them, and does each class's check pass what is true and
fail what is invented, on the local 27B? Today's pipeline asks every statement "which passage states this?"; 64% of
this lesson's statements are explanations, scenario or rubric text, or common knowledge, which no passage states.

## Results

| Stage | Rows | Right by hand | Notes |
|---|---|---|---|
| classify, rule layer | 46 of 122 decided | 46 | every planted specific caught (digit, quote, attribution cue); 10 lighter statements sent to Sourced (cost only) |
| classify, model layer | 76 | 90/122 exact overall | lightened 11 of 16 cue-less technical findings, one of them false: the model never declares Sourced on its own |
| strict judge, walk's saved page | 36 Sourced | 9 supported, 25 unsupported, 2 contradicted | the saved page is the last the walk reached: off-subject for most |
| strict judge, best windows of the lesson's own sources | 36 Sourced | 33 supported, 3 unsupported, 0 contradicted | same prompt, same claims; 2.8 s a call; planted figures still rejected |
| consistency judge (prompt of record) | 45 Reasoning + 3 planted | 41 consistent, 2 contradicted (1 real catch, 1 audit injection), 2 unfounded specific (off-subject pages) | planted contradiction, smuggled specific and true explanation all right |
| knowledge judge, closed-book, 3 samples | 8 Common + 8 planted | 16 of 16, unanimous | 2+2=4 and "Tesseract is an OCR engine that reads pixels into text" pass; 2+2=5, "OCR converts audio", "a bounding box is measured in seconds" fail |
| common-knowledge bridge, part-wise, on cited evidence | 5 Josh + 5 planted | 10 of 10 | "Tesseract … reads pixels into text" = stated + common; "… written by Microsoft in 2019 …" = contradicted parts |
| falsity veto, 3 samples, majority | 45 Reasoning + 11 lightened | 5 vetoed, all read as fair; 0 true statements removed | the "any sample" rule rejected two true explanations on a lone dissent and is not used |

Calls for this lesson under the design: 247 (classification 7, strict judge 36, consistency 45, knowledge 24,
veto 135), about 14 minutes of 27B time, counted before the first call; the app had made 739 checks on the same lesson
when the data was dumped and had not finished its first pass.

## What this does not show

Haiku and Sonnet were not run (no Anthropic spend without approval; estimated under $1 for the same 247 calls at the
2026-10-29 arms eval's rate). The windows for "the lesson's own sources" were chosen by word overlap, a floor for what a
writer's statement-level citation gives. The one-lesson set is a single course's lesson; the planted set is small.
