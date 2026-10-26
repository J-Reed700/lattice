# Course generation pipeline (as built)

This is the reference for how Lattice builds a course from a topic request: the outline, then each lesson, and how both are checked and repaired. It describes the code at `09d5a552` on `learning/outline-evidence` (2026-10-15). Where the code and the design records disagree, the code wins and the difference is listed in [section 9](#9-where-the-code-differs-from-the-design-records).

- Testing, live runs and checker measurement: [course-generation-testing.md](course-generation-testing.md).
- Every live run and what it taught: [course-generation-history.md](course-generation-history.md).
- Design records (history of decisions, not current behaviour): [2026-10-14-outline-evidence-rebuild.md](../design/2026-10-14-outline-evidence-rebuild.md) (§12 and §14 are binding amendments), [2026-10-04-learning-content-verification.md](../design/2026-10-04-learning-content-verification.md).
- Release evidence for the wider Learning Studio: [learning-studio-verification.md](learning-studio-verification.md).

Paths are under `crates/lattice-desktop/src/features/learning/` unless they start with `src/` (frontend), `crates/` or `migrations/`. Code is named by path and function, not line number.

## 1. Principles

1. **Evidence comes before writing.** The outline is drafted after references are gathered. A lesson is written after its topics are planned, retrieved, judged for coverage and researched.
2. **Code retrieves; the model plans, chooses and judges.** The model never chooses stored identifiers. Every quote attached to an outline item is the saved page's own text, so the exact-match check cannot fail. A lesson quote must match its excerpt exactly or it is re-anchored to the real passage.
3. **No arbitrary caps.** Josh's rule, in his words: "I don't want any arbitrary caps in this code... like this shit that is just chipping off the first 2400 characters". A fixed limit may never discard content. Sizes are derived from the model's context window. What does not fit is split across calls or paged. See [section 6](#6-budgets).
4. **Every judgement has a receipt.** Model decisions are saved under keys that include a policy string, the model name and the exact inputs. A resumed or repeated run reuses them and makes no model call. Bumping a policy string invalidates exactly the receipts that carry it ([section 7](#7-policy-strings-and-what-a-bump-invalidates)).
5. **Optional steps never stop the run.** An answer that strays from its schema in evidence linking, outline review, the lesson evidence plan or the coverage judgement is logged and treated as "no decision". Publication is the opposite: it fails closed.
6. **Loops end by construction.** The outline loop has per-finding attempts and per-module rewrite budgets. The lesson repair loop stops when repairs stop reducing its problems ([Repair progress stop](#repair-progress-stop)). Evidence deepening stops when a round settles no claim.
7. **Transient outages are waited out, everything else fails fast.** See [Failure policy](#failure-policy).

## 2. The stages

```
Learner request: goal, prior knowledge, minutes per session, course depth
 │
 │ service::generate_with_references_and_progress
 ├─ O1 Reference plan ........ planning/outline_references::plan ............ 1 call
 ├─ O2 Gather references ..... planning/outline_references::gather .......... per subtopic: searches, 1 select call, captures
 ├─ O3 Draft outline ......... lessons/generation/outline::draft_outline .... reason-first (2 calls)
 │     strip unmatched author quotes (code), decode, save program + sources + draft
 │
 │ planning/outline_draft::finish
 ├─ O4 Evidence linking ...... planning/outline_linking::link ............... 1 call per batch of 8 items
 ├─ O5 Review ................ planning/outline_review::review .............. reason-first (2 calls) per pass
 ├─ O6 Repair ................ planning/outline_repair::repair .............. research + judge per factual finding; 1 call per module rewrite
 │     O5-O6 repeat until Passed, or all open findings are exhausted, or nothing changed
 │
 │ service::accept (learner)  -> program Active
 │ lesson job: lessons/generation_jobs::LessonGenerationWorker (1-3 lessons per job)
 │
 │ lessons/generation/lesson::prepare_lesson_with_references
 ├─ L1 Evidence plan ......... lessons/generation/evidence::plan ............ 1 call
 ├─ L2 Retrieval per topic ... evidence::retrieve ............................ code
 ├─ L3 Coverage judgement .... evidence::judge ............................... 1 call per group that fits
 ├─ L4 Research before writing content_verification/research::research_topics  plan, search, select, capture; rounds while coverage grows
 ├─ L5 Evidence pack ......... evidence::pack, evidence::assemble ............ code, round-robin across topics
 ├─ L6 Write lesson .......... structured_call::complete_checked ............ reason-first (2 calls)
 ├─ L7 Teaching review ....... teaching::review_lesson ....................... quotes (code), answer keys, section review, <= 1 repair, final review
 │
 │ content_verification::verify_and_repair_with_references (repair_loop.rs)
 ├─ L8 Claim extraction ...... content_verification/inventory::extract ..... 1 call per 4 sections
 ├─ L9 Coverage audit ........ content_verification/coverage ............... mapping + fidelity; missing assertions adopted as claims
 ├─ L10 Examples ............. content_verification/execution::observe ..... runs Python and JavaScript examples
 ├─ L11 Verification ......... content_verification/evidence_checks::check . first evidence page per claim, deeper pages for unsupported claims; strict judge in batches of 8, then challenge
 ├─ L12 Research after writing research::expand .............................. for unsupported and contradicted claims
 ├─ L13 Section repair ....... content_verification/repair::candidate ...... per section, split by token budget; then L7 again
 │     L8-L13 repeat until the report has no issues
 │
 └─ L14 Readiness gate ....... service::validate_prepared, report validation, publish
```

All model calls go through `lessons/structured_call.rs` (`complete_json`, `complete_json_with_progress`, `complete_reasoned_json`) and `model_call::send`. A **reason-first** call (`CallKind::Authoring`, `model_call::REASON_FIRST_AUTHORING = true`) makes two requests: a prose draft, then a schema-constrained answer that continues the same conversation. If the window cannot hold the draft beside the answer, it answers in one step. A `CallKind::Classifier` call is one step (`REASON_FIRST_CLASSIFIERS = false`). Inside a lesson job the prose draft is checkpointed under `reason-first-draft-v1:<digest of model, system, prompt>`, so an interrupted job resumes at the answer step.

A call that turns its answer into data goes through `complete_checked`, with its parse and validation as an `accept` closure. When `accept` refuses the answer itself (`InvalidInput`, `InvalidData`, `ValidationFailed` or `Json`: it did not parse, or broke a rule the caller checks), the answer step is asked for once more. The request repeats the first request's messages, adds the refused answer as the assistant turn, then a user turn naming the reason. A two-step call keeps its draft. A second refusal fails with `AppError::ModelAnswerUnusable` ("The model's answer could not be used: <reason>"). Transport, window, configuration and cancellation errors are never asked again. `complete_json`, `complete_json_with_progress` and `complete_reasoned_json` accept any text, for the callers that tolerate an unusable answer on purpose (evidence linking, the evidence plan and check) or correct it piece by piece (teaching review, answer-key check, claim inventory, coverage mapping).

## 3. Course outline

### O1. Reference plan

`planning/outline_references::plan`. Runs only when a web service is available.

- **Input:** goal, prior knowledge, minutes per session, depth bounds (`LearningCourseDepth::bounds`), and the catalog of existing sources (`ReferenceCollection::catalog`).
- **Schema:** `{"subtopics":[{"title":1..120 chars,"queries":[3..240 chars]}]}`, `subtopics` 1..`max_modules`. The number of queries per subtopic is the model's choice (no `maxItems` since `f90f440c`). Output reservation `400 + 220 x max_modules` tokens.
- **Guard:** a query that repeats eight or more consecutive words of the goal or prior knowledge (`copies_private_text`, `COPIED_RUN = 8`) is dropped and logged. The learner's goal text never leaves the machine.
- **Errors:** "The reference plan was malformed; the draft was not started." (shape violations). "The reference plan contained no usable searches; the draft was not started." (no queries left after the guard, unless existing sources made new ones unnecessary).
- **Kept:** `OutlineDraft.reference_plan`. Its subtopic titles become the drafting retrieval queries.

### O2. Gather references

`planning/outline_references::gather` (wrapper of `gather_for` with `GatherPurpose::Course`).

For each subtopic, in order:

1. Run each planned search through `references/web_research::search_need`: 12 results per page (`PAGE_RESULTS`). A planned need's `site:` operators are its scope, lifted into `site` by `PlannedNeed::searches` (one search per site named); the source finder searches the site where it lives now and keeps results to it (toolbelt design §6.7). It asks for further pages while a page brings results not seen before. A failure on the first page is logged and the query skipped; on a later page it ends that search.
2. Merge results by URL, dropping URLs already saved.
3. One `content_verification/research::select_references` call per subtopic chooses, in preference order, every result that is relevant and credible for the gap (`decisions[].useForGap`; no count limit since `569a76f7`). Model calls stay sequential: the server is single-slot.
4. Capture the chosen pages with `web_research::capture`, at most four fetches at once (`FETCH_CONCURRENCY`). A page is read wherever its own redirects lead; a capture that is truncated or empty is skipped (`Ok(None)`).
5. Deduplicate by resolved URL and by `hash_text` of the text. Mirrors and redirects are not two sources.

Nothing is persisted here. The captures are saved with the program in `LearningRepository::create_outline_draft`, in the same transaction. A cancel during gathering leaves nothing behind. There is no cap on the number of sources; the bound is the plan.

**Progress:** stage `GatheringReferences` ("Gathering references"); detail "Subtopic n of m: searching for ...", "choosing among k results", "saving k selected pages".

### O3. Draft the outline

`lessons/generation/outline::draft_outline`, called from `service::generate_with_references_and_progress`.

- **Evidence in the prompt:** `planning/outline_evidence::select` runs each subtopic title (or the goal, when no plan exists) against all saved text with `ReferenceCollection::retrieve`, 8 passages per query per page (`PAGE`), and adds them round-robin by rank. It pages deeper, doubling the depth, while a page still adds passages and the rankings are not exhausted. Budget: a quarter of the model's window. Source indexes refer to the complete collection. (Before `569a76f7` it took 4 per query and filled the rest with 600-character source prefixes.)
- **Prompt:** goal, prior knowledge, minutes per session, `sources` (the selected passages), `sourceCatalog`, and design requirements (backward design from a capstone, measurable outcomes, milestones, lesson sizing).
- **Citation instructions:** `OUTLINE_REFERENCE_INSTRUCTIONS` (cite only where a passage directly addresses the item; otherwise `sourceIndex: null`, `quote: ""`). With no sources the topic-only instructions apply and every citation must be null.
- **Schema:** `outline_schema(depth, sources)`; `sourceIndex` is nullable.
- **Call:** reason-first, subject "the course outline", brief `OUTLINE_DRAFT_BRIEF`. Output reservation `max_modules x max_lessons x 240 + 1600` when a depth is set, else `OUTLINE_OUTPUT_RESERVE` (4,000).
- **After the call (code):** `outline_citations::restore_source_quotes` fixes typography only; `outline_linking::strip_unmatched` removes any author quote that does not match its source exactly; `decode_outline` validates structure (a quote without an index is fabrication and an error); `service::validate_outline`. The program is saved as `Draft` with the draft state and the gathered sources.
- **Errors:** "The course goal and its source catalog do not fit in <Learning model>'s <N>-token context window. Raise your Learning model's context length in Settings → Models, or choose a Learning model with a larger window" (does not fit; a configuration error), "No learning sources could be acquired." (user sources requested but none acquired), parse failures ("The model returned malformed or incomplete learning material. Try again.").

Author citations are provisional. They only survive if evidence linking chooses them.

### O4. Evidence linking

`planning/outline_linking::link`. Runs once at the start of `outline_draft::finish` over all items, and again on one module after each rewrite (`outline_repair`).

- **Items:** every module summary, outcome and lesson objective.
- **Batches:** `BATCH_SIZE = 8` items per chooser call, so one unusable answer costs one batch (`09d5a552`). A request that still does not fit is halved. Between `569a76f7` and `f90f440c` items were packed into as few requests as the window held; in accept2 that made one 66k-token call for 64 items.
- **Candidates (code, `candidates_for`):** `ReferenceCollection::retrieve_within(item text + module title [+ lesson title])` offers every retrieved passage, best first, while their citable spans fit the item's share of its batch call: the room `model_call::source_room` leaves for a chooser call with output `200 + 120 x 8`, divided by 8. Each candidate is cut to a citable span by `outline_linking/spans::citable_span`: whole sentences of prose (not headings, tables, link lists or code), at most `MAX_QUOTE_CHARS` (1,200) and at least five words. An author citation that matched exactly joins the candidates.
- **Call:** `CallKind::Classifier`, `LINK_SYSTEM`. Schema `{"links":[{"id":enum(item paths),"candidate":enum(p1..pn, "none"),"reason":1..300}]}` with `minItems = maxItems = batch size`. Output `200 + 120 x items`.
- **Applying (code):** the chosen passage becomes the citation with a positive receipt. "none" records a negative receipt keyed on the declined candidate set, so only new references reopen the question. An author citation that was not chosen is removed. An unreadable answer leaves its batch unlinked without receipts.
- **Receipts:** `OutlineDraft.citation_receipts`, key = policy `outline-evidence-link-v2`, model, item kind, item text, source content hash, quote.
- **Result:** `LinkLog { linked, items, removed_unmatched, removed_unchosen }`, shown in the review note as "n of m outline items linked to references". Linking is never a finding and never blocks acceptance.
- **Progress:** stage `CheckingCitations` ("Linking references").
- **Error:** "An evidence linking call exceeds this model's context window." (one item alone does not fit).

### O5. Review

`planning/outline_review::review`, scoped by `planning/outline_review_scope::ReviewScope`.

- **Scope:** a full pass the first time. Later passes are incremental: modules that changed or hold open findings, plus every module's instructional fields as `courseSequence` for cross-course effects. The receipt (`ReviewReceipt`, version `outline-incremental-review-v2`) hashes each module and a context of reviewer version, model and learner request. Sources are not part of it (design §14.1): research for one finding never forces a full pass.
- **Skip rule:** when the receipt matches and no finding is open, no review call is made (`outline_draft::finish`).
- **Payload:** goal, prior knowledge, minutes per session, the candidate or the modules in scope (citations stripped by `strip_citations`), `moduleNumbering`, `sources` (passages selected by `outline_evidence::select` for the lessons in scope), `openFindings` (`id, path, kind, claim, reason, attempts`), and `verifiedAssertions` (claims a strict judge already supported, text only).
- **Schema:** `{"verdicts":[{"id":enum(open ids),"status":"resolved|unresolved","reason"}], "newFindings":[{"path":enum(allowed paths),"kind":"factual|plan","claim","reason"}]}`; verdicts `minItems = maxItems = open count`; `newFindings` max 24. Allowed paths are the module and item paths inside the scope. The empty (course-wide) path is never allowed (§14.2).
- **Call:** reason-first, `REVIEW_SYSTEM`, brief `REVIEW_DRAFT_BRIEF`, output `6000 + 200 x open findings`.
- **Applying (code, `outline_review::apply`):** resolved findings are removed; unresolved ones get the new reason and their `claim` refreshed from the current text. A new finding at an outcome or lesson objective with kind `factual` becomes `plan` (`factual_path`). New findings are deduplicated against open ones by path and normalized claim. A factual finding that names a `verifiedAssertions` item is dropped. Unknown or repeated verdicts are ignored; a finding without a verdict stays open for the next pass.

**Finding kinds** (`LearningOutlineIssueKind`):

| Kind | Meaning | Raised by |
|---|---|---|
| `factual` | An assertion about the world stated as true (version, behaviour, number, date, API fact), in a module summary or module | Reviewer |
| `plan` | Sequencing, prerequisites, duplication, scope, session fit, project progression, contradictory requirements | Reviewer |
| `references` | "No references could be gathered..." (course-wide) | Code, when web was available and no source was saved |

Each finding (`LearningOutlineIssueDto`) carries a stable `id`, JSON-pointer `path`, `claim`, `message`, `attempts`, `exhausted`, `quote`, `source_id` and `searched`.

### O6. Repair

`planning/outline_repair::repair`, called by `outline_draft::finish` for actionable findings.

| Kind | Attempt 1 | Attempt 2 |
|---|---|---|
| `factual` | Targeted research (`outline_repair::research`): `web_research::plan_queries` with the claim and reason as the gap and `alreadySearched` from `OutlineDraft.research_log`, then `outline_references::gather_for` (`GatherPurpose::Finding`), saved with the draft. Then judge (`resolve_factual`) with `content_verification::judge_for_publication` on pages of evidence: the first page fills the checker's window for this one claim (`claim_share(llm, claim, 1)`); an unsupported verdict walks on to the next page of passages not yet shown while that page mentions the claim's subject (`mentions_subject_of`). **Supported**: the located span is attached as the item's citation, receipts recorded, finding closed by code and never sent back to the reviewer (§12.1). **Contradicted**: module rewrite with `CORRECT` and the contradicting passage shown in the finding. **Unsupported** (or no passage): rewrite with `NARROW`. | Rewrite with `REMOVE` ("Remove the assertion or restate it as a learning activity rather than a fact."). No research. |
| `plan` | Rewrite with `PLAN_FIX` ("do not add or change citations"). | Rewrite with `PLAN_SMALLEST`. |
| `references` | Not repaired by rewrites. `repair_saved` re-gathers when the draft has no sources. | Stays open. |

- **Module rewrite** (`rewrite_module`): one `complete_json_with_progress` call (one step), `REPAIR_SYSTEM`, the module's repair schema, output 6,000. Afterwards `preserve_identities` keeps module and lesson IDs and counts, `preserve_valid_citations` restores receipted citations on unchanged items, `strip_new_citations` removes any citation the rewrite added, and evidence linking runs on that module. Repair never adds a citation of its own.
- **Factual claim receipts:** `OutlineDraft.claim_receipts`, key = policy `outline-claim-v1`, model, claim, passage hashes.
- **Budgets:** `ATTEMPT_LIMIT = 2` per finding per operation; `MODULE_REWRITE_BUDGET = 3` rewrites per module per operation (in memory). A finding is actionable while it has attempts left and either it is a factual finding on its first attempt (research needs no rewrite) or its module still has rewrite budget. Otherwise it is marked `exhausted`.
- **Termination** (`outline_draft::finish`): stop when no finding is open (`Passed`); when every open finding is exhausted (`NeedsRepair`, or `Passed` with an advisory note if only exhausted `plan` findings remain); or when a repair pass changed nothing and added no evidence ("The model returned no usable correction..."). Passes are bounded by `1 + 3 x modules`.
- **Acceptance:** `LearningOutlineIssueDto::blocks_acceptance` is false only for an exhausted `plan` finding (commit `37c6ef57`). Every other open finding blocks `service::accept`.
- **Progress:** `Reviewing`, `Researching` ("Researching an unresolved finding"; detail "Finding n of m: ..."), `Repairing`, `CheckingRepair`.

**What the user can do next.** `outline_draft::repair_saved` (the Repair button) resets `attempts` and `exhausted` for open findings, keeps the research log so the planner tries new queries, re-gathers if the draft has no sources, and runs `finish` again.

### Outline state, checkpoints and resume

- `OutlineDraft` (in `learning_outline_drafts.state_json`): `request`, `candidate`, `source_ids`, `review` (`LearningOutlineReviewDto`: status, note, issues, counts `references_gathered`, `citations_verified`, `citations_removed`, `repair_passes`), `completed_review`, `reference_plan`, `citation_receipts`, `claim_receipts`, `research_log`, `findings_raised`.
- `repo.checkpoint_outline` writes the draft and appends a row to `learning_outline_checkpoints` before review, after review and after each repair.
- Status values: `Unchecked`, `NeedsRepair`, `Passed`.
- A non-transient error inside `finish` is caught: the committed draft is reloaded, its status set to `Unchecked`, and the note becomes "Review or repair stopped: {reason} Completed work is saved. Use Repair to continue." Receipts make the next run cheap.
- No overall deadline applies to outline generation.

### Failure policy

- **Outline generation and Repair wait out transient model errors** (`patient_model.rs`, since `04021807`). `plugin/program.rs` loads the model through `patiently` and wraps it in `PatientModel`. A 503 while the server loads, a rate limit, a dropped connection or a timeout (`JobFailure::is_transient`, the same errors lesson jobs defer) is retried call by call after a backoff that starts at 30 seconds and grows to 5 minutes (`Backoff::default()`). The wait is shown as progress and ends when the learner cancels. Completed calls and searches stand.
- **Search and page-fetch failures** are logged and skipped (O2, research).
- **Everything else fails fast:** a schema or parse failure in a gating call, a request that does not fit the window, a malformed reference plan, cancellation. During drafting the outline is not saved; inside `finish` the error is caught as described above.
- **Lesson jobs** defer transient errors with the same backoff and fail on others ([The job](#the-job)).
- **The live test does not wait.** `live_course_e2e` calls `service::generate_with_references_and_progress` directly with its recording model, without `PatientModel`, so a server outage during the outline still fails the run (accept1).

### Outline progress stages

`planning/outline_progress::OutlineStage`, labels in `src/features/learning/curriculum/OutlineGenerationProgress.tsx`:

| Stage | Label |
|---|---|
| `ReadingSources` | Reading your references |
| `LoadingModel` | Getting the model ready |
| `GatheringReferences` | Gathering references |
| `Drafting` | Writing your course outline |
| `CheckingCitations` | Linking references |
| `Reviewing` | Reviewing the teaching plan |
| `Researching` | Researching an unresolved finding |
| `Repairing` | Revising the course outline |
| `CheckingRepair` | Checking the revised outline |
| `Saving`, `Completed`, `Cancelled`, `Failed` | Saving your outline / Your draft is saved / Generation cancelled / Generation stopped |

## 4. Lesson preparation

### The job

After `service::accept` the program is `Active`. A lesson job (`LearningGenerationJobKind::LessonPreparation`, `requested.lessonIds`, one to three upcoming lessons) runs in the native job runtime (`crates/lattice-core/src/shared/runtime/jobs`) through `lessons/generation_jobs::LessonGenerationWorker::prepare`:

1. Checks: the program is active, its content has not changed since the job started, and it has active, untruncated sources. Errors: "Add reference material in Sources before preparing a lesson...", "A reference is incomplete or was saved as an old excerpt...", "Course content changed before generation resumed...".
2. Loads the `ReferenceCollection` (library-ranked where a library copy exists, keyword-ranked otherwise) and the model.
3. For each lesson: reuse the `prepared-v1` checkpoint if its report is still reusable; otherwise run `prepare_lesson_with_references` inside `lesson_drafts::run` (checkpoint context) and `content_verification/research::run` (research context with the web service).
4. After all lessons, recheck that every staged report is still reusable against the current sources (research for a later lesson can add sources); redo invalidated lessons; then publish.

**Failure policy** (`planning/generation_recovery::lesson_failure`): a transient error (network, rate limit, model service unavailable) defers the job with backoff growing to five minutes and never exhausts a budget. Any other error fails the job with code `generation_failed`. The runtime requeues running jobs after a restart (`RecoveryPolicy::Requeue`) and runs one job per course at a time.

**Retry:** `LearningCurriculumRepository::retry_job` creates a new job (`retry_of_job_id`) and copies the old job's `job_checkpoints` and `staged_result_json` (the drafts). Completed work is reused; nothing is approved by the copy.

### L1. Evidence plan

`lessons/generation/evidence::plan`.

- **Input:** lesson title and objective, module (title, outcomes, lesson sequence), and the reference catalog.
- **Schema:** `{"topics":[{"topic":>=3 chars,"queries":[>=3 chars, min 1]}]}`, at least one topic. No maximum is set.
- **Call:** `complete_json`, `PLAN_SYSTEM` (public terminology only, no learner or course details), output 2,000.
- **Fallback:** an unusable answer becomes one topic built from title and objective. It never fails the lesson.
- **Checkpoint:** `lesson-topic-evidence:plan:<digest of model, system, lesson + module>`. The catalog is not part of the key, so references saved by research do not reopen the plan.
- **Progress:** phase `References`, "Finding evidence for {lesson}".

### L2. Retrieval per topic

`evidence::retrieve` runs every query of a topic against all saved references with `ReferenceCollection::retrieve(query, depth)`, merges by rank and stores each passage once (`intern`: a passage overlapping one already held for the same source reuses it). The depth is the evidence room shared among every search, in passages of `PASSAGE_CHARS` (1,800 characters) measured with the model's tokenizer, scaled up when the sources hold less text than one passage (`evidence::depth`). `retrieve` has no count ceiling (since `569a76f7`).

### L3. Coverage judgement

`evidence::judge`.

- Lists each topic with its candidate passage IDs and each passage once.
- **Schema:** `{"topics":[{"id":enum(t1..),"covering":[enum(p1..)]}]}`, `minItems = maxItems = topics in the group`. Output `64 + 24 x topics + 8 x pairs`.
- **Call:** `CallKind::Classifier`, `COVERAGE_SYSTEM` ("A passage covers a topic when it states what a lesson must teach about that topic... Shared vocabulary, a passing mention, an adjacent topic... does not qualify").
- Groups are sized to fit and halved while they do not. A single pair that cannot fit is an error: "A lesson evidence check exceeds this model's context window."
- Unknown IDs are ignored. A topic missing from the answer counts as uncovered. An unreadable answer leaves its topics uncovered.
- **Checkpoint:** `lesson-topic-evidence:coverage:<digest of model, system, prompt>`.
- **Progress:** "Checking which of n passages cover the lesson's topics".

### L4. Research before writing

`content_verification/research::research_topics`, called from `evidence::gather` with the uncovered topics.

- Gaps are `{"topic", "missingEvidence": "No saved reference covers this lesson topic"}`.
- The shared `research` function plans the public queries the gaps need (`web_research::plan_queries`, with `alreadySearched`; no count limit since `569a76f7`), searches, calls `select_references` per query, captures every selected page, deduplicates against saved sources by URL and text, and saves each page to the program's source library (`LearningSourceLibraryRepository::add`). It then reloads the collection. (Before `569a76f7` it planned at most three queries and saved at most two pages per query.)
- Completed queries are checkpointed under `research-v1:<digest of topic, model, lesson evidence policy>`, so a resumed job does not repeat them.
- `evidence::gather` re-retrieves the uncovered topics against the expanded collection and judges only passages not judged before. It repeats research rounds while a round covers at least one more topic.
- Without web research, or when planning or selection gives no usable answer, the topics stay uncovered. This step never stops the lesson.
- **Progress:** phase `Research`, "Finding references for lesson topics no saved reference covers".
- **Error:** "No saved passages match this lesson. Add relevant references in Sources, or narrow the lesson objective." when no topic has any passage at all.

### L5. Evidence pack

`evidence::pack` fills the writer's evidence room round-robin across topics: each topic offers its covering passages first, then its other passages, best first, so every topic is represented before any topic gets another passage. `evidence::assemble` turns the packed passages into the writer's `sources`: one entry per source whose excerpt is its packed passages in document order, plus `evidenceByTopic` (each topic with the `sourceIndex` values whose passages cover it; an empty list means no evidence). `prepare_lesson_with_references` then builds the prompt and drops the last packed passage until the measured prompt fits.

The authoring context (prompt, sources, reference hashes) is saved (`lesson_drafts::authoring`). It is reused while the scope hash matches (policy `lesson-authoring`, prompt scope including every requirement, revision, model) and every original source is unchanged, so references added by research do not cause a rewrite.

### L6. Write the lesson

- **System:** `lesson_system(true)`: "Teach one rigorous, accessible lesson..." plus `grounding_instructions(true)`.
- **Grounding rules** (`lessons/generation.rs::grounding_instructions`): every block, outcome, summary and question cites a `sourceIndex` with a verbatim quote; state a fact only when a supplied excerpt states it or it follows necessarily; add no statistics, typical figures, intensifiers (purely, entirely, always, never, guarantees) or causal explanations the excerpts do not give; present illustrations as explicit hypotheticals ("Suppose...", "In this example...") that say nothing about real products, versions or behaviour beyond the excerpts; for a topic without evidence, say what the learner should find out and why, asserting no facts.
- **Prompt** (`lesson_prompt`): program goal, prior knowledge, course sequence (whole objectives, and the recap of every earlier Ready lesson; the measured window decides what fits), current module, lesson, `sources`, `evidenceByTopic`, `referenceCatalog`, and requirements (learning science lessons design §4): two or three core ideas, each explained, worked and checked, with at least one contrast; then guided practice, independent practice, reflection (explaining from memory) and recap, 10 to 16 blocks; the check format (case, one `### Check your thinking` line, answer and misconceptions); minimum body lengths; rubrics for practice blocks; runnable Python or JavaScript examples in worked examples, contrasts and checks; and exactly two questions each of practice, quiz and test, with misconception distractors.
- **Schema:** `lesson_schema(sources)`; `sourceIndex` is required (non-null) when sources exist.
- **Call:** reason-first, brief `LESSON_DRAFT_BRIEF` + `CITATION_DRAFT_BRIEF`, output `LESSON_OUTPUT_RESERVE` (10,000).
- **Checkpoint:** the raw draft is stored in the job's staged drafts (`lesson_drafts::save`, in `jobs.staged_result_json`) under an input hash of policy `lesson-draft`, prompt, schema, revision, model and sources. A resumed job reuses it ("Reusing the saved lesson draft; continuing verification").
- **Progress:** phase `Writing`, "Writing {lesson}".

### L7. Teaching review

`teaching::review_lesson` then `review_material`. Runs after writing and after every section repair.

1. **Quotes (code, `teaching/quotes.rs`):** restore typography; on the second attempt, re-anchor a quote the author could not match to the nearest real passage (`reanchor_lesson_quotes`: the span sharing at least 60% of the quote's terms, else the span sharing the most terms with the item). Citations are provenance; claims are checked separately.
2. **Answer keys:** `answer_review::check` solves each question blind (reason-first) and compares with the key.
3. **Section review:** `teaching_review::review` (policy `section-review-v3`). The reviewer is asked for "at most six concise, actionable defects"; more are kept if returned. Findings are bound to app-owned passages by `review_evidence` (policy `review-finding-protocol-v2`): each finding retrieves passages with `ReferenceCollection::retrieve_within` up to its equal share of the binding call's room (output 6,000, reserved twice); the answer lists at most 8 evidence IDs per finding.
4. **One repair** of listed issues (one `complete_json_with_progress` call, the lesson schema), then a final review.
- **Receipts:** `teaching-sources:<digest>` and the teaching receipt (policy `lesson-teaching-review`). An unchanged candidate with unchanged sources is not reviewed again.
- **Error:** "The teaching review still found an issue: ... No material was published. Try again."
- **Progress:** phase `Review`.

### L8. Claim extraction

`content_verification/inventory::extract` (and `extract_remaining`).

- **Input** (`inventory::inputs`): each section as app-owned passages with IDs. For assessment items, wrong options are passed as `distractors` and never extracted as facts.
- **Call:** `complete_json`, sections in batches of four, up to two attempts per batch (the second includes the response errors), output 12,000.
- **Schema:** one `unit-N` slot per requested section: `{"claims":[{"passageId":enum(that section's passage ids),"statement"}],"nonFactualReason"}`. The app supplies the quote from the passage ID.
- **Rules (system text):** leave out names the lesson invented for its examples (such as `IOrderRepository`) and extract the real-world fact in general terms without them (`09d5a552`; in accept2 such names made claims no page could support); extract every independently checkable factual assertion, preserving scope, conditions, defaults and quantifiers; course organization, chosen exercise constraints, deliverables and preferences are nonfactual with a reason; record assertions even when false; do not extract quiz-label claims (the answer-key check owns those).
- **Checkpoint:** `InventoryCheckpoint` (policy `INVENTORY_POLICY`, model, context size, content hash, inventory, coverage, coverage policy), saved per candidate (`lesson_drafts::record_claim_inventory`). Section receipts (`section_checkpoints.rs`) keep extraction for byte-identical sections across repairs.
- **Progress:** phase `Inventory`.

### L9. Coverage audit

`content_verification/coverage.rs` (policy `passage-coverage-v6`). A mapping review accounts for every passage under its exact ID (`coverage/mapping.rs`). A fidelity judge then lists, per passage, the factual assertions the inventory fails to represent (`missingAssertions`). The application adopts those as claims for the evidence check (commit `2885fd55`); it does not argue with the judge. A malformed answer gets one correction and is never read as a gap. Phase `Coverage`.

### L10. Examples

`content_verification/execution::observe` runs complete Python and JavaScript examples in the built-in runtimes. A failed example is an issue ("Example {id} did not complete: ..."). An unavailable runtime stops publication: "An example runtime is unavailable or cancelled. No lesson was published; retry when the runtime is available." Passed observations become evidence for their section's claims. Phase `Examples`.

### L11. Verification

`content_verification/evidence_checks::check`, judge in `content_verification::judge_batch_provisionally` and `challenge_batch_for_publication`.

- **First page per claim** (`content_verification::evidence_for`): passed execution observations for its section, then `ReferenceCollection::verification_page`: ranked passages from the whole collection, each widened to its surrounding source text (1,700 characters each side), best first, while they fit the claim's share (`within_share`). The share (`claim_share`) is an equal part of the evidence room a strict batch call leaves after its instructions, claims and answers, for as many claims as share the call (up to 8). A claim checked alone gets the whole room. Contradicting passages are not filtered. `evidence_selection.rs` keeps passages already compared stable as research adds sources.
- **Judge:** `ClaimChecker` (`crates/lattice-core`, `application/services/claim_verification`) with `CheckPolicy::Strict`, deterministic sampling, 512 output tokens per claim. Claims go in batches of `BATCH_SIZE = 8`; a batch that does not fit is halved; a claim a batch answer leaves unusable is checked alone.
- **Deepening** (`evidence_checks::deepen`, since `f90f440c`): a claim the first page leaves `Unsupported` is checked again against its next page, passages it has not been shown, batched with the other claims still looking. A walk ends at the first `Supported` or `Contradicted` verdict, or when the next page has no passage that mentions the claim's subject. Another round runs only while the previous round settled at least one claim (`09d5a552`). A deciding page's finding replaces the first page's.
- **Subject test** (`ReferenceCollection::mentions_subject_of`): a passage mentions the claim's subject when it contains one of the claim's distinctive content terms, those no more common across the collection's passages than the median of the claim's own terms. Terms in no passage (such as an invented example name) and terms in every passage are left out before the median is taken (`09d5a552`). Vector neighbours alone never continue a walk.
- **Verdicts** (`ClaimVerdict`): `Supported` (with a supporting quote that must appear in the evidence), `Unsupported`, `Contradicted`, `Unverified` (the checker could not finish).
- **Challenge** (`content_verification/challenge.rs`): every provisional `Supported` is challenged. A separate call plans research questions about omitted conditions, exceptions or conflicting evidence (as many as it finds; no count limit since `f90f440c`). Any question turns the verdict into `Unsupported` with the questions as the reason, so research follows. A failed challenge leaves the claim unverified.
- **Receipts:** per claim and page `claim-v1:<digest of POLICY, model, context size, comparison>` (`ClaimReceipt`: verdict, reason, supporting quote, interpretation policy); provisional batch results under `PROVISIONAL_BATCH_POLICY`. The first page's key still names a decision reached on a deeper page, so a later pass on the same evidence reuses it.
- **Report** (`LessonVerificationReport`): policy, checker model, source bindings (id + sha256), coverage, coverage audit, findings, executions, issues, retrieval mode. Every non-`Supported` finding becomes an issue "Section n: {statement} — {reason}", except an unsupported claim the general-knowledge gate keeps (see [section 5](#5-labelling-general-knowledge)).
- **Progress:** phase `Evidence`, "Checking claims"; while deepening, "Checking N unsupported claims against their next page of evidence (page P)".

### L12. Research after writing

`content_verification/research::expand` takes the `Unsupported` and `Contradicted` findings as gaps (`{"claim","missingEvidence": reason}`) and runs the same `research` function as L4. New sources are bound to the claims that asked for them (`evidence_selection::record_research_scope`, policy `claim-research-scope-v1`), so a broadly relevant page does not reopen unrelated approvals. The report is rechecked (`check_evidence`) against the expanded collection. Rounds repeat while the gap count falls and no claim is contradicted. Phase `Research`, "Finding authoritative references for unresolved claims".

### L13. Section repair

`content_verification/repair_loop::verify_and_repair_with_references` and `content_verification/repair::candidate`.

- If any finding is `Unverified`, the job stops: "The evidence checker could not complete n claim checks. No lesson was published. Retry when the checker is available; durable jobs retain the lesson draft."
- Otherwise the issues, the failed claims with their evidence IDs and an evidence table (each passage once), execution results and `repairHistory` (claim variants already rejected) form the repair context (`repair_context`). If the same issue set repeats, `repeatedDefects` asks the repairer to remove the disputed assertion.
- `repair_sections` repairs each affected section in turn. `claim_groups` splits a section's failed claims into consecutive groups whose request fits: room = window minus the section context minus twice the output reservation. A claim's price is its own text plus the passages the group does not list yet. A claim whose own evidence does not fit keeps its passages in recorded order while they fit (logged). A claim that cannot carry a single passage is an error: "A failed claim's evidence passage does not fit in this model's context window." Each group's repair runs on the section as the previous group left it.
- The repairer answers with text edits (`lessons/text_edits.rs`); an ambiguous edit falls back to one validated-section call. Each patch is checkpointed as a `PendingRepair` with the work that remains, so a stopped job resumes mid-repair.
- After all sections, the teaching review (L7) runs on the repaired lesson, then the loop returns to L8.
- **Progress:** phase `Repair`, "Correcting defects found by evidence checks" or "Removing a repeatedly unsupported claim"; per section "Correcting affected section n of m (part p of q)".

### L14. Readiness gate and outcome

- `prepare_lesson_with_references` decodes the final candidate and enforces the shape: 10 to 16 blocks with the required kinds and minimum lengths, unique titles, six questions (two per kind), four distinct options, valid source IDs from this program, and binds the report to the lesson (`report.bind`).
- `service::validate_prepared` rechecks the shape and source references against the program's current sources.
- The report must validate at publication (`report.rs::validate`): its policy equals the current `POLICY`, it has no issues, and it matches the lesson's content hash. `report.reusable` additionally requires the same checker model and unchanged source hashes.
- The prepared lesson is checkpointed as `prepared-v1`, then all staged lessons are published in one transaction with their reports (phase `Publishing`, "Publishing verified lesson material").

| Outcome | Meaning |
|---|---|
| Job `Completed`, lesson `Ready` | Every claim passed and the report is stored |
| Job `Pending` (deferred) | A transient model or network error; it retries automatically |
| Job `Failed` (`generation_failed`) | A non-transient error: context overflow, teaching review still failing, checker unverified, malformed material. Drafts and checkpoints are kept; retry reuses them |
| Job `Cancelled` / `Interrupted` | User cancel or shutdown; requeued or retried from checkpoints |
| Lesson `Outline` | Not prepared yet (the only other `LearningPreparation` value) |

### Lesson progress phases

`planning/curriculum::LearningGenerationPhase`: `References`, `Writing`, `Review`, `Inventory`, `Coverage`, `Examples`, `Evidence`, `Research`, `Repair`, `Publishing`. Stage text is set with `lesson_progress::stage`; examples are quoted under each step above.

## 5. Labelling: general knowledge

**Status: built in `569a76f7`; unit-tested. Live runs accept2 and accept3 include it; accept2 was stopped before the gate was reached.**

- **The gate** (`content_verification/general_knowledge.rs`, used by `check_evidence`, `report.rs` and `repair_loop.rs`). After the research rounds, a claim that is still `Unsupported` is kept as general knowledge: it is not a lesson issue, is never repaired, stays out of `repairHistory`, and publication accepts it. Its verdict is `general_knowledge`.
- **Exceptions that stay issues** and go to repair, which removes or softens the detail:
  - an absolute, matched as a whole word: always, never, must, all, every, only, guarantee / guarantees / guaranteed, purely, entirely;
  - a token containing a digit (a number, version, date or percentage) that none of the claim's evidence passages contains, or the word "percent" when no passage says "percent" or "%".
- **Unchanged:** `Contradicted` claims and failed examples are issues and are repaired. `Unverified` still stops the job.
- **Lesson-internal statements.** The extraction (`inventory.rs`), fidelity audit (`coverage.rs`) and revision (`inventory/revisions.rs`) prompts classify statements about the lesson's own example code, scenario or exercise answers as nonfactual with the reason "lesson-internal". Any real-world fact such a statement relies on is still extracted as a claim. Examples still run; quiz keys still go through the answer-key check.
- **What the learner sees** (`learner_view.rs`, `lesson_evidence.rs`, `LessonEvidencePanel.tsx`):
  - Each such claim carries a neutral badge: "General knowledge · not checked against a source".
  - Lesson summary: "N claims checked against saved evidence · M general knowledge, not checked against a source".
  - Section summary: "Evidence · N checked claims · M general knowledge · P passages".
  - The general-knowledge part appears only when M > 0. The DTO carries `generalKnowledgeCount`.
- **Outlines are unchanged.**
- **Policies** at `09d5a552`: `lesson-evidence-v19`, inventory `lesson-evidence-v7`, `passage-coverage-v8`, `pending-repair-v2` (see [section 7](#7-policy-strings-and-what-a-bump-invalidates)).

### Repair progress stop

`repair_loop.rs` keeps the best candidate seen: the one with the fewest issues after verification.

- A round that does not beat the best sends the next repair with `repeatedDefects = true` (remove the disputed assertions whole), even when the issue fingerprint differs.
- If that escalated round also fails to beat the best, the best candidate is saved as the draft and the job fails: "Repairs stopped reducing the lesson's problems: N remain (first 3 listed). The best draft is saved; retry to continue."
- A round that beats the best clears the escalation.
- The state lives in `PendingRepair.progress`, so it survives a resume.

Why: a sample of 40 claims the failed final2-1 lesson left unsupported, judged against world knowledge, was mostly correct but unsourced, plus 13 statements about the lesson's own code that no outside source can confirm. On the 85 labelled .NET pairs the shipped judge wrongly blocked none of 34 supported claims, so these were gaps in evidence, not checker errors. See [course-generation-testing.md](course-generation-testing.md#qwen-factual-accuracy-sample).

## 6. Budgets

**Principle.** "I don't want any arbitrary caps in this code... like this shit that is just chipping off the first 2400 characters" (Josh). No fixed limit may discard content that a model call needs. Sizes derive from the model's context window. What does not fit is split across calls or paged. Fixed numbers are allowed only for the shape of model output, for termination, for batch sizes that split work without dropping any, and for display. `crates/lattice-desktop/src/features/learning/content_caps_tests.rs` holds the regression tests ("Content reaching a model is bounded by its window, never by a fixed count or a character prefix").

**How the window is divided.** `model_call::fits(llm, request)` asks the grounded-generation service whether the system text, prompt and output reservation fit the model's window (`LLMPort::max_context_tokens`, counted with `count_tokens`). `model_call::source_room(llm, request, ceiling)` returns the room left for evidence once the prompt skeleton and the output reservation are paid for; every pipeline caller passes `usize::MAX`, so the window alone bounds it. A request that does not fit is never sent; `model_call::send` maps it to the caller's error text.

**Paged retrieval.** `ReferenceCollection::retrieve(query, limit)` has no count ceiling. `retrieve_within(query, fits)` and `verification_page(query, shown, fits)` return passages best first while the caller's `fits` accepts them: they ask for 16 (`FIRST_PAGE`) and double the depth while everything found still fits. `outline_evidence::select` pages 8 per query (`PAGE`), doubling. Web search pages 12 results at a time (`PAGE_RESULTS`) while new in-scope results appear.

| Call | Evidence or input size | Output reservation |
|---|---|---|
| O1 reference plan | catalog: whole headings, as many as a sixteenth of the window holds | 400 + 220 x max modules |
| O2 selection | every merged result of the subtopic's searches | per result |
| O3 outline draft | passages within a quarter of the window, paged 8 per query | max modules x max lessons x 240 + 1600, or 4,000 |
| O4 linking | 8 items per call, halved while it does not fit; each item's candidates fill an equal share of the call's room | 200 + 120 x items |
| O5 review | passages for lessons in scope within a quarter of the window | 6,000 + 200 x open findings |
| O6 module rewrite | passages for the module within a quarter of the window | 6,000 |
| O6 factual judge | first page fills the checker's window for one claim; deeper pages while they mention the claim's subject | 512 |
| L1 evidence plan | lesson, module, catalog | 2,000 |
| L2 retrieval depth | room / (searches x tokens of a 1,800-character passage), at least 1 | |
| L3 coverage | groups sized to fit, halved on overflow | 64 + 24 x topics + 8 x pairs |
| L5/L6 writer | `evidence_room`: window minus prompt skeleton minus (3 x 10,000 + briefs + 512) for the two-step call; if that leaves nothing, minus 2 x 10,000. The pack is measured and trimmed until the prompt fits | 10,000 |
| L7 review evidence | per finding, an equal share of the binding call's room | 6,000, reserved twice |
| L8 extraction | 4 sections per call | 12,000 |
| L11 verification | first page: the claim's equal share of a batch call (up to 8 claims); deeper pages the same, for unsupported claims | 512 per claim |
| L11 challenge | claim and its passages | 1,200 |
| L13 repair | room = window minus section context minus 2 x output; claims grouped to fit | 2 x 10,000 reserved |

**What remains fixed** at `09d5a552`:

- **Output shape** (schema bounds on what the model writes): lesson 10 to 16 blocks, exactly 6 questions, 4 options each; outline review at most 24 new findings per pass; at most 8 evidence IDs per review finding (`review_evidence`); project milestones 2 to 5 deliverables and criteria; practice and practical tasks at most 5 citations; reference-plan subtopics at most `max_modules` (from the requested depth); text lengths such as `MAX_BODY` 5,000 (checked on decode), `MAX_QUOTE_CHARS` 1,200, titles 120 to 180 characters.
- **The 80,000-character response check** on an assessment response (`assessment_generation.rs`) and a practical-activity response (`practical_generation.rs`, `MAX_RESPONSE_CHARS`). A longer answer is rejected, not cut.
- **The teaching reviewer's wording** "at most six concise, actionable defects" (`teaching.rs`). More findings are kept if the model returns them.
- **Termination:** `ATTEMPT_LIMIT` 2 and `MODULE_REWRITE_BUDGET` 3 for outline repair; the lesson [repair progress stop](#repair-progress-stop); the deepening stop.
- **Batch sizes that drop nothing:** 8 items per linking call, 8 claims per check batch, 4 sections per extraction call.
- **Display:** stored source previews of 2,400 characters (`references/sources.rs`, `source_library.rs`) and 180-character titles. Retrieval and verification read the full capture.

The [caps audit](../design/2026-10-14-content-caps-audit.md) records the 36 fixed limits found on 2026-10-14. Its learning rows were removed in `569a76f7` and `f90f440c`. Its chat and web rows are on branch `fix/no-content-caps`. The Explorer folder-chat bounds (audit row 22) are outside this pipeline.

## 7. Policy strings and what a bump invalidates

Values at `09d5a552`.

| String | Where | A bump invalidates |
|---|---|---|
| `outline-evidence-link-v2` | `planning/outline_linking.rs` `POLICY` | All link receipts; every item is linked again |
| `outline-incremental-review-v2` | `planning/outline_review_scope.rs` `REVIEW_VERSION` | The review receipt; the next pass is a full review |
| `outline-claim-v1` | `planning/outline_repair.rs` `CLAIM_POLICY` | Factual-claim receipts from outline repair |
| `reason-first-draft-v1` | `lessons/structured_call.rs` `DRAFT_CHECKPOINT_POLICY` | Saved prose drafts of two-step calls in lesson jobs |
| `lesson-topic-evidence` | `lessons/generation/evidence.rs` `POLICY` | Lesson evidence plans and coverage judgements |
| `lesson-authoring` | `lessons/generation/lesson.rs` `AUTHORING_POLICY` | The reusable authoring context; evidence is gathered again and the lesson rewritten |
| `lesson-draft` | `lessons/generation/lesson.rs` (draft input hash) | The saved lesson draft |
| `lesson-teaching-review`, `section-review-v3`, `review-finding-protocol-v2` | `lessons/teaching.rs`, `teaching_review.rs`, `review_evidence.rs` | Teaching review receipts |
| `lesson-evidence-v7` (`INVENTORY_POLICY`) | `lessons/content_verification/mod.rs` | Claim inventory checkpoints |
| `passage-coverage-v8` | `lessons/content_verification/coverage.rs` `POLICY` | The coverage audit (the inventory is kept) |
| `lesson-evidence-v19` (`POLICY`) | `lessons/content_verification/mod.rs` | Claim receipts (`claim-v1:`), the research query log (`research-v1:`), pending repairs, and every report: `prepared-v1` checkpoints stop being reusable and must be verified again |
| `pending-repair-v2` | `lessons/content_verification/repair_loop.rs` (`repairPolicy` in the pending-repair fingerprint) | A saved mid-repair state; the repair starts again from the diagnosis |
| `repair-issue-set-v1` | `lessons/content_verification/repair_loop.rs` (`repair_issue_fingerprint`) | The fingerprint that detects a repeated issue set |
| `claim-research-scope-v1` | `lessons/content_verification/evidence_selection.rs` | Bindings of research sources to the claims that requested them |
| `STRICT_INTERPRETATION_POLICY` | `crates/lattice-core` claim verification | Non-supported claim receipts and provisional batch results |
| `prepared-v1` | `lessons/generation_jobs.rs` checkpoint key | The verified-lesson checkpoint |

Change the string whenever the prompt, schema or meaning of the stored answer changes. Never reuse an old receipt under a new protocol.

## 8. Model calls

| Stage | module::function | Input | Output / schema | Checkpoint key |
|---|---|---|---|---|
| O1 | `outline_references::plan` | goal, prior knowledge, depth, catalog | `{subtopics:[{title, queries}]}` | `OutlineDraft.reference_plan` (after save) |
| O2 | `research::select_references` (from `outline_references::gather_for`) | subtopic, queries, gap, merged results | `{decisions:[{id, useForGap, reason}]}` | none (sources saved with the draft) |
| O3 | `generation::draft_outline` (reason-first) | request, passages, catalog, requirements | `outline_schema` (nullable citations) | none; the draft is saved as the program |
| O4 | `outline_linking::link` | up to 8 items with candidate IDs, passages | `{links:[{id, candidate|none, reason}]}` | `OutlineDraft.citation_receipts` (`outline-evidence-link-v2`) |
| O5 | `outline_review::review` (reason-first) | candidate or scoped modules, passages, open findings, verified assertions | `{verdicts, newFindings}` | `OutlineDraft.completed_review` |
| O6 | `web_research::plan_queries` (from `outline_repair::research`) | claim, reason, catalog, already searched | `{queries}` | `OutlineDraft.research_log` |
| O6 | `research::select_references` | finding, results | decisions | saved sources |
| O6 | `content_verification::judge_for_publication` (from `outline_repair::resolve_factual`), per evidence page | claim, page of passages | verdict, reason, quote; then challenge | `OutlineDraft.claim_receipts` (`outline-claim-v1`) |
| O6 | `outline_repair::rewrite_module` | module, findings with instructions, passages, course sequence | `{module}` | `learning_outline_checkpoints` row |
| L1 | `evidence::plan` | lesson, module, catalog | `{topics:[{topic, queries}]}` | `lesson-topic-evidence:plan:<digest>` |
| L3 | `evidence::judge` | topics with candidate IDs, passages | `{topics:[{id, covering}]}` | `lesson-topic-evidence:coverage:<digest>` |
| L4, L12 | `web_research::plan_queries` (from `research::research`) | topics or claims as gaps, catalog, already searched | `{queries}` | `research-v1:<digest>` (completed queries) |
| L4, L12 | `research::select_references` | gaps, query, results | decisions | sources saved to the library |
| L6 | `structured_call::complete_checked` (from `prepare_lesson_with_references`) | lesson prompt with sources and `evidenceByTopic` | `lesson_schema` | prose step: `reason-first-draft-v1:<digest>`; answer: staged draft (`lesson-draft` input hash) |
| L7 | `answer_review::check` (reason-first) | questions without keys | correct indices and reasons | teaching receipt |
| L7 | `teaching_review::review` (reason-first), `review_evidence` | sections, questions, authoring context; passages per finding | section checks, findings with evidence IDs | teaching receipt, `teaching-sources:<digest>` |
| L7 | `teaching::review_material` repair (one step) | candidate, issues, quote checks | `lesson_schema` | staged draft |
| L8 | `inventory::extract` | section passages | `{units:{unit-N:{claims, nonFactualReason}}}` | `InventoryCheckpoint` (`lesson-evidence-v7`) |
| L9 | `coverage` mapping and fidelity | passages, inventory | coverage units, `missingAssertions` | inventory checkpoint (`passage-coverage-v8`), section receipts |
| L11 | `judge_batch_provisionally` (first page, then each deepening round) | up to 8 claims, each with its page | verdicts | provisional batch key, `claim-v1:<digest>` per page |
| L11 | `challenge::guard_batch` | provisionally supported claims | `{queries}` | `claim-v1:<digest>` |
| L13 | `repair::candidate` (`repair_sections`) | section, failed claims, evidence table, history | text edits, or one validated section | `PendingRepair` in staged drafts (`pending-repair-v2`) |

## 9. Where the code differs from the design records

| Design record says | The code does |
|---|---|
| §3.2: up to three queries per subtopic; §3.3: `select_references` approves at most four results; research plans up to three searches | No count limits since `569a76f7` and `f90f440c`: the planner writes the searches a subtopic or gap needs, and selection takes every relevant, credible result |
| §4.3: a new `planning/outline_citation_check.rs` judges author citations for coverage | Replaced by evidence linking (§13): `planning/outline_linking.rs`. No citation-check file exists |
| §5.1, §6.3, §11: unresolved findings block acceptance; no dismiss | An exhausted `plan` finding no longer blocks acceptance (`blocks_acceptance`, commit `37c6ef57`); it is shown as advisory |
| §6.1: factual repair judges against `retrieve_for_verification(claim, 8)` | The judge pages through ranked evidence (first page fills the window for one claim; deeper pages while they mention the claim's subject) |
| §13.1: linking offers the top 4 passages per item | Every passage the item's share of its batch call holds |
| §15.1: the evidence plan returns 3 to 8 topics with 1 to 3 queries each | The schema sets only minimums (at least one topic, at least one query); there is no maximum |
| §15.3: one research round before writing | Rounds repeat while a round covers at least one more topic |
| §15.4: the writer's evidence ceiling rises from 7,000 to 48,000 tokens | There is no ceiling: `evidence_room` uses `usize::MAX` and is bounded by the window alone |
| §15.6: at most three passages per failed claim in repair | No per-claim count; claims are grouped by token budget, and only a claim whose own evidence exceeds the room loses passages |
| §16.1: an unsupported claim with a specific detail "the evidence does not give" stays an issue | Implemented as a token containing a digit that no evidence passage contains, or "percent" with no "percent" or "%" in the evidence |
| `repair_loop.rs` doc comment: "Continue while defects decrease; preserve a draft when repairs stop helping" | Enforced since `569a76f7` (see [Repair progress stop](#repair-progress-stop)); before that no code enforced it |
| `2026-10-04-learning-content-verification.md` "First implementation slice": one evidence-driven repair, research only after checks | Superseded: repair repeats, and research runs before writing (§15) as well as after |

## 10. Known gaps

1. **The acceptance bar is not met yet.** accept2 passed the outline and was stopped during lesson 1's evidence deepening after 3h20m; accept3 (`09d5a552`) is running. See [course-generation-history.md](course-generation-history.md).
2. **§16 labelling is unproven live.** Tuning risk: the absolute-word list ("all", "only") is broad, so ordinary phrasing can still send a claim to repair.
3. **Deepening can be slow.** In accept2 each deepening round over 22 to 42 claims took 10 to 24 minutes, its batch calls carried up to 106k input tokens, and the rounds settled 7, 4, 4, 1, 0 and 4 claims. `09d5a552` stops a round sequence once a round settles nothing and drops invented example names from claims; whether that is enough is what accept3 measures.
4. **The live test does not wait out outages.** Only the app's outline path uses `PatientModel`; a 503 during the live test's outline stage still fails the run.
5. **Repair can drop evidence for one claim** when that claim's recorded passages alone exceed the window. It logs a warning and keeps the passages that fit.
6. **A non-transient outline error is not resumed automatically.** The draft waits for the learner to press Repair.
7. **The second checker question is closed for now.** Measurement found no second judge that helps (see the testing doc). The weak point is generation grounding, not the checker.
