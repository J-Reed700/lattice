# Course generation: live run history

Every live course-generation run is kept under `artifacts/course-e2e-<date>/` (gitignored): the 2026-10-14 runs in `course-e2e-2026-10-14/`, the acceptance runs of 2026-10-15 in `course-e2e-2026-10-15/` (their binaries are in `course-e2e-2026-10-14/bin/`). Each run used a fresh throwaway SQLite database, but the real Qwen llama-server and real web search. Runs were started with `run-all.sh <binary> <label>` or `run-requests.sh <binary> <label> <requests-dir>` in that folder, which call the ignored test `live_topic_course_passes_review_and_prepares_its_first_lesson` with `LATTICE_LLAMACPP_SETTINGS`, `LATTICE_COURSE_REQUEST` and `LATTICE_COURSE_ARTIFACTS`. Those scripts are now `scripts/live-course-e2e.sh` in the repo, with the requests in `evals/courses/requests/`. For how to run them, see [course-generation-testing.md](course-generation-testing.md).

All times are local (UTC-5); dates are 2026-10-14 unless stated. "Request" is the .NET interview request (`requests/1-dotnet-interview.json`) unless stated. Postgres, music theory and system design requests are now in `requests-later/`.

## Summary

| run | date / time | binary / commit | request | outline | lesson | minutes | outcome |
|---|---|---|---|---|---|---|---|
| baseline-1 | ~13:48-14:36 | cargo test on old pipeline (branch at 3410ab6b; exact commit not recorded) | .NET | NeedsRepair, 5 findings, 6 references | not reached | 48 | Failed: reproduced the stall |
| wp1-1 | 15:05-15:37 | `bin/wp1-1bc508bd` (1bc508bd) | .NET | Passed, 0 findings, 31 references, 0 citations | Failed: claim coverage stopped | 32 | Failed at lesson |
| lesson-fix-1 | 16:31-18:39 | cargo test in `../lattice-lesson` (learning/lesson-coverage, before 2885fd55 was committed) | retry of the wp1-1 lesson | reused | Ready: 70/70 claims supported | 128 | Passed |
| wp3-outline-2 | 18:16-18:26 | `bin/wp3-b21fcbaf` (b21fcbaf) | .NET | Unchecked: linker answer invalid, 18 references | not reached | 10 | Failed at outline |
| final-1 | 19:01-19:29 | `bin/final-4aa1d3fb` (4aa1d3fb) | .NET | Passed, 35 references, 63/70 linked | Failed: 2 unmatched quotations | 27 | Failed at lesson |
| final-2 | 19:29-19:46 | same | Postgres performance | NeedsRepair, 1 finding, 17 references | not reached | 16 | Failed at outline |
| final-3 | 19:46-19:58 | same | music theory production | Passed, 9 references, 18/18 linked | Failed: 4 unmatched quotations | 12 | Failed at lesson |
| final-4 | 19:58-20:10 | same | system design fundamentals | Passed, 28 references, 49/49 linked | stopped mid-writing | not recorded | Stopped, no result |
| final2-1 | 20:12-22:24 | `bin/final2-37c6ef57` (37c6ef57) | .NET | Passed, 36 references, 60/64 linked | Failed: context overflow in repair | 132 | Failed at lesson |
| final2-2, -3, -4 | 22:24 | same | Postgres, music, system design | not run | not run | 0 | Never ran |
| accept1 | 10-15 02:49-02:50 | `bin/accept-f90f440c` (f90f440c) | .NET | Failed at 81 s: server 503 while gathering references | not reached | 1 | Failed: model server outage |
| accept2 | 10-15 02:52-06:14 | `bin/accept-f90f440c` (f90f440c) | .NET | Passed, 0 findings, 46 references, 39/64 linked | Stopped by the lead in evidence deepening (page 8, 22 claims) | 202 | Stopped, superseded |
| accept3 | 10-15 from 06:14 | `bin/accept-09d5a552` (09d5a552) | .NET | running | running | running | Running |

## Runs

### baseline-1
- Code: the pipeline before the rebuild (draft with no sources, research after drafting, mandatory citations). The binary was not snapshotted; the log shows a fresh `cargo test` build.
- Outline: NeedsRepair after 2,910 s. 5 findings, 6 references. Note: "The model returned no usable correction."
- Lesson: not reached. The test asserts the outline passed before preparing a lesson.
- Cause: the failure the rebuild set out to remove (design doc section 1): mandatory citations became findings, and repair forced quotes from a few generic pages.
- Next: the references-first rebuild, commit 1bc508bd (design in 598a848b). Review and repair protocol followed in e7300d13.

### wp1-1
- Outline: Passed in 816 s, 0 findings, 31 references (Microsoft Learn, PostgreSQL docs, OWASP and similar). Zero citations survived: "0 citations verified, 7 removed because the passage did not match or address the item". 125 recorded model calls.
- Lesson: the job failed after about 18 min with "Claim coverage stopped making progress". The claim inventory judge compared a claim against source support that did not match. Claim count not recorded.
- Cause (outline): the author cited about 7 of 60 items and the coverage judge rejected the rest (design doc section 13). Cause (lesson): the fidelity judge and correction loop in the lesson coverage step stalled.
- Next: b21fcbaf (evidence linking, design section 13) for the outline; 2885fd55 for the lesson.

### lesson-fix-1
- What it did: retried the failed lesson job of wp1-1 (job `ffa5f37f`, lesson "What's New in .NET 10 & ASP.NET Core 10"), not the final2-1 lesson. The new job `7905f3ac` has `retryOfJobId` set to the old job. It used a copy of the wp1-1 database, saved as `lesson-fix-1/lesson.sqlite`.
- How: the ignored test `live_small_lesson_resumes_to_ready` (`crates/lattice-desktop/src/features/learning/lessons/live_small_lesson.rs`). It reads `LATTICE_LLAMACPP_SETTINGS` and `LATTICE_LESSON_CALLS` (the artifact directory holding `lesson.sqlite` and `job-id.txt`). It enqueues the retry, starts the production worker and waits. The exact command line was not recorded. The log shows the build reused (1.06 s) and the saved draft reused ("Reusing the saved lesson draft; continuing verification").
- Code: worktree `../lattice-lesson`, branch `learning/lesson-coverage`. The fix, 2885fd55, was committed at 18:52, after the run ended at 18:39; the binary was not snapshotted.
- Result: test passed. `LIVE PUBLICATION CONFIRMED`, preparation `ready`, 339 completed model calls in the directory, test time 7,657 s (128 min). Saved verification report: policy `lesson-evidence-v17`, 70 claims all `supported`, 0 issues, 56 sources.
- This log shows 213 calls, 3.99M input and 69k output tokens, peak request 95,396 input tokens.
- Next: 2885fd55 (adopt what the coverage audit finds missing; correction loop and progress guard deleted), merged as 4aa1d3fb.

### wp3-outline-2
- Outline only (the log stops after the outline). Review ended in 624 s with status Unchecked: "The evidence linking returned an invalid decision." 18 references, 0 findings, 8 model calls. Why the label ends in -2 is not recorded.
- Cause: the linker model returned an answer outside its schema (a candidate from another item). The step aborted the outline.
- Next: d3e1d54c, "Never stop an outline because a linking or review answer strays from its schema". Merged with the lesson fix as 4aa1d3fb.

### final-1-dotnet-interview
- Outline: Passed in 1,055 s. 0 findings, 35 references, 63 of 70 items linked.
- Lesson: failed after about 10 min. "The revised lesson contains 2 source quotations that could not be matched to the saved references. No material was published."
- Cause: the revised lesson kept quotation marks around spans that the references did not contain.
- Next: a4098b73, "Cite the real passage instead of discarding a lesson over a misquoted span". Also 5fd3ecb2 (quote only prose when linking).

### final-2-postgres-performance
- Outline: NeedsRepair after 1,014 s. 17 references, 1 finding left after the allowed repair attempts: the Module 3 summary implied an index fixes statistics problems. The test stopped; no lesson was tried.
- Cause: a Plan wording finding that repair could not settle blocked the whole course.
- Next: 37c6ef57, "Let plan suggestions that repair could not settle stop blocking a course". Factual and References findings still block.

### final-3-music-theory-production
- Outline: Passed in 263 s. 9 references, 18 of 18 items linked, 0 findings.
- Lesson: failed after about 8 min with "4 source quotations that could not be matched".
- Cause and next: as final-1 (a4098b73).

### final-4-system-design-fundamentals
- Outline: Passed in 708 s. 28 references, 49 of 49 items linked, 0 findings.
- Lesson: the log ends at 20:10 while writing "The Four-Step Methodology" (prose step 1 of 2). No END line in `final-summary.txt` and no `result.json`. Why it stopped is not recorded. The final2 batch began two minutes later with a new binary.

### final2-1-dotnet-interview
- Outline: Passed in 878 s. 0 findings, 36 references, 60 of 64 items linked.
- Lesson 1, "DDD & CQRS in .NET 10": failed after 132 min total (about 117 min for the lesson job). 16 sections, 155 claims; 99 unresolved after two verification passes (design doc section 15). Repair then ran per section ("Correcting affected section N of 15"). Sections 1 and 2 sent 61k and 104k input tokens; section 3 overflowed the 131k context: "The requested learning material does not fit in this model's context window."
- Tokens in the log: 173 calls, 3.42M input, 101k output; largest completed request 104,149 input.
- Causes (design doc section 15, measured from checkpoints and recorded calls):
  - The writer saw about 4,800 characters of evidence. `author_sources` ran one query for 12 passages; `bounded_sources` cut each source to 2,400 characters under `SOURCE_EVIDENCE_CEILING` (7,000 tokens).
  - No DDD source was among the 36 references. Research ran only after writing.
  - Repair sent all evidence of all failed claims of a section in one request.
  - The checker was right: the unresolved claims were statistics, intensifiers and explanations the sources do not state.
- Next: 4f4a2ef6, "Gather a lesson's evidence by topic before writing it" (design section 15): plan topics, retrieve per topic, research before writing, an evidence pack bounded only by the model's window (the design's 48,000-token ceiling was not built), repair split by token budget. Design section 16 (label unsupported but uncontradicted claims as general knowledge) followed; it landed in 569a76f7.
- Josh then cut the acceptance bar to this one request and moved the other three to `requests-later/`. The first runs of that code are accept2 and accept3 (2026-10-15).

### final2-2, final2-3, final2-4
- Never ran. Each log is a 521-byte `Error: Io ... NotFound` from the test, 0 minutes, at 22:24. `run-all.sh` expands `requests/*.json` once at the start; the three request files were moved to `requests-later/` at 21:10, so they were gone when the loop reached them. This explanation is inferred from the file times and the script; it was not logged.

### accept1
- Run with `scripts/live-course-e2e.sh`, binary `accept-f90f440c`, the acceptance request only.
- Outline: planned the reference searches (one model call), then failed at 81 s in `GatheringReferences`: "llama.cpp returned HTTP 503 Service Unavailable from /v1/chat/completions". The server was reloading. The test exited 101 after one recorded call.
- Cause: outline generation failed fast on any model error; only lesson jobs deferred transient errors.
- Next: 04021807 (`patient_model.rs`): in the app, outline generation and Repair wait out transient errors, retrying after 30 s growing to 5 min. The live test calls the service directly and does not use that wrapper, so a 503 still fails a test run.

### accept2
- Binary `accept-f90f440c`, started 02:52, the acceptance request only.
- Outline: Passed at 1,313 s (about 22 min). 10 subtopics and 30 searches gathered 46 references. Linking packed all 64 items into one 66k-token call (call 13: 65,998 input tokens) and linked 39 of 64 (`removed_unchosen` 34). One full review of 8 modules, 0 findings.
- Lesson 1, "Recall Drill: DDD, CQRS, and SOLID Under Pressure":
  - Evidence: 5 topics planned; 3 uncovered against the saved references (90 passages, depth 17). Research before writing saved 9 references, after which 0 were uncovered (149 passages).
  - Pack: 149 passages from 38 sources in a room of 80,615 tokens. The draft call took 69,399 input tokens (answer step 73,954).
  - Claims: 15 sections extracted, 95 claims. The first page settled 53; 42 stayed unsupported.
  - Deepening (pages 2 to 7) settled 7, 4, 4, 1, 0 and 4 claims (42, 35, 31, 27, 26, 26 claims checked per round). Each round took 10 to 24 minutes, and the batch calls carried up to 106,310 input tokens.
  - Names the lesson invented for its examples (`IOrderRepository` and similar) appeared in claims; no page can contain them, so those claims could not be supported, and the names kept walks going.
- Stopped by the lead at 06:14 on page 8 with 22 claims, after 3h20m and 201 model calls, superseded by `accept-09d5a552`. No `result.json`.
- Next: 09d5a552: extraction leaves invented example names out of claims; deepening runs another round only while the previous round settled a claim; the subject test ignores terms that appear in no passage or in every passage; linking returns to batches of 8 items.

### accept3
- Binary `accept-09d5a552`, started 06:14, the acceptance request only. **Running.** Result to be added.

## What the runs taught

- Citations are not the author's job. The old pipeline made citations mandatory and produced findings the pipeline itself invented (baseline-1). References are now gathered before drafting (1bc508bd) and code, not the model, links passages to outline items (b21fcbaf).
- A model answer outside its schema must never abort a stage. wp3-outline-2 stopped on one bad linking answer. Fixed in d3e1d54c.
- The claim-coverage loop could stall on a judge that looked at source support instead of the claim. wp1-1 failed on it; lesson-fix-1 then published the same lesson, 70 of 70 claims supported. Fixed in 2885fd55.
- Quotes in a revised lesson must be the saved passage, not the model's copy. final-1 and final-3 lost whole lessons over 2 and 4 unmatched spans. Fixed in a4098b73 (and prose-only linker quotes in 5fd3ecb2).
- One unsettled advisory finding must not block a course. final-2 stopped on a plan wording finding. Fixed in 37c6ef57.
- A writer given thin evidence invents specifics, and repair that sends everything overflows the context. final2-1 failed on both after 132 minutes. Fixed in 4f4a2ef6 (evidence by topic before writing; repair split by token budget).
- A single live run is slow and a long batch breaks on state changes. final2-1 took 132 minutes, and final2-2 to -4 failed because their request files moved mid-batch. Hence one acceptance request, and `requests-later/` for the rest.
- A lesson retry can reuse the saved draft and saved claim checks, so a fix to the later stages can be tested without rerunning the outline (lesson-fix-1, `live_small_lesson_resumes_to_ready`).
- A model server can disappear for a minute while it reloads. A run that fails on the first 503 measures the server, not the pipeline (accept1). Fixed for the app in 04021807; the live test still fails fast.
- Paging through evidence must stop when it stops paying. accept2 spent three hours deepening rounds that settled 0 to 7 claims each, and invented example names made some claims unsupportable by any page. Fixed in 09d5a552.
