# Course generation pipeline (as built)

This is the reference for how Lattice builds a course from a topic request: the outline, then each lesson, and how both are checked and repaired. It describes the code at `09d5a552` on `learning/outline-evidence` (2026-10-15). Where the code and the design records disagree, the code wins and the difference is listed in [section 9](#9-where-the-code-differs-from-the-design-records).

- Testing, live runs and checker measurement: [course-generation-testing.md](course-generation-testing.md).
- Every live run and what it taught: [course-generation-history.md](course-generation-history.md).
- Design records (history of decisions, not current behaviour): [2026-10-14-outline-evidence-rebuild.md](../design/2026-10-14-outline-evidence-rebuild.md) (§12 and §14 are binding amendments), [2026-10-04-learning-content-verification.md](../design/2026-10-04-learning-content-verification.md).
- Release evidence for the wider Learning Studio: [learning-studio-verification.md](learning-studio-verification.md).

Paths are under `crates/lattice-learning/src/` unless they start with `src/` (frontend), `crates/`, `plugin/` (the app's learning plugin, `crates/lattice-desktop/src/features/learning/plugin/`) or `migrations/`. Code is named by path and function, not line number.

## 1. Principles

1. **Evidence comes before writing.** The outline is drafted after references are gathered. A lesson is written after its topics are planned, retrieved, judged for coverage and researched.
2. **Code retrieves; the model plans, chooses and judges.** The model never chooses stored identifiers. Every quote attached to an outline item is the saved page's own text, so the exact-match check cannot fail. A lesson quote must match its excerpt exactly or it is re-anchored to the real passage.
3. **No arbitrary caps.** Josh's rule, in his words: "I don't want any arbitrary caps in this code... like this shit that is just chipping off the first 2400 characters". A fixed limit may never discard content. Sizes are derived from the model's context window. What does not fit is split across calls or paged. See [section 6](#6-budgets).
4. **Every judgement has a receipt.** Model decisions are saved under keys that include a policy string, the model name and the exact inputs. A resumed or repeated run reuses them and makes no model call. Bumping a policy string invalidates exactly the receipts that carry it ([section 7](#7-policy-strings-and-what-a-bump-invalidates)).
5. **Optional steps never stop the run.** An answer that strays from its schema in evidence linking, outline review, the lesson evidence plan or the coverage judgement is logged and treated as "no decision". Publication is the opposite: it fails closed.
6. **Loops end by construction.** The outline loop has per-finding attempts and per-module rewrite budgets. The lesson repair loop stops when repairs stop reducing its problems ([Repair progress stop](#repair-progress-stop)). A lesson's teaching review stops when every finding has been charged `REPAIRS_PER_FINDING` (2) repairs or is held for the user ([L7](#l7-teaching-review)). Evidence deepening stops when a round settles no claim.
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
 ├─ L7 Teaching review ....... teaching::review_lesson ....................... quotes (code), answer keys, reviewer, plan check; one repair of every open finding, <= 2 repairs each
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
- **Schema:** `{"verdicts":[{"id":enum(open ids),"status":"resolved|unresolved","reason"}], "newFindings":[{"path":enum(allowed paths),"kind":"factual|plan","claim","reason"}]}`; verdicts `minItems = maxItems = open count`; `newFindings` has no cap. Allowed paths are the module and item paths inside the scope. The empty (course-wide) path is never allowed (§14.2).
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
- **Grounding rules** (`lessons/generation.rs::grounding_instructions`): every block, outcome, summary and question cites a `sourceIndex` with a verbatim quote; state a fact only when a supplied excerpt states it or it follows necessarily; add no statistics, typical figures, intensifiers (purely, entirely, always, never, guarantees) or causal explanations the excerpts do not give; a worked example's, practice's or check's own names, paths and values are made up for its case and written plainly, with no remark that they are invented or hypothetical, and real product behaviour such a case shows must be one an excerpt states; for a topic without evidence, say what the learner should find out and why, asserting no facts.
- **Prompt** (`lesson_prompt`): program goal, prior knowledge, course sequence (whole objectives, and the course plan: every lesson's planned ideas, terms, actions and contrast, marked earlier, this or later; `lessons/course_plan.rs` makes the plan once per outline revision, so which lessons are published never changes it), current module, lesson, `sources`, `evidenceByTopic`, `referenceCatalog`, `lessonShape` (`lessons/shape.rs` `ShapePolicy::asked` for `Phase::Draft`, under the key `shape::WORDS`; two sentences whose numbers are read from the constants the schema uses): two or three core ideas, each explained, worked and checked, with at least one contrast, then one guided practice, independent practice, reflection (explaining from memory) and recap, 11 to 16 blocks; exactly two multiple-choice items of each question kind. The `requirements` (learning science lessons design §4) hold the rest: the check format (case, one `### Check your thinking` line, answer and misconceptions); minimum body lengths; rubrics for practice blocks; runnable Python or JavaScript examples in worked examples, contrasts and checks; four choices per question with misconception distractors, written alike in form (`alike_in_form!()` in `assessment/assessment_generation.rs`, a sentence the quiz bank writer's `SYSTEM` in `assessment_generation.rs` carries too). Every model that later reviews or repairs the lesson is given the writer's request again with `lessonShape` replaced by the words of the phase the lesson is in ([L7](#l7-teaching-review), [L13](#l13-statement-repair-and-cuts)): the draft's, or, once verification may cut the lesson (`Phase::Kept`), the floor's (at least one explanation; each check after a worked example shown since the previous check; the recap after the reflection when it has both; a question of every kind).
- **Schema:** `lesson_schema(sources)`; `sourceIndex` is required (non-null) when sources exist.
- **Call:** reason-first, brief `LESSON_DRAFT_BRIEF` + `CITATION_DRAFT_BRIEF`, output `LESSON_OUTPUT_RESERVE` (10,000).
- **Checkpoint:** the raw draft is stored in the job's staged drafts (`lesson_drafts::save`, in `jobs.staged_result_json`) under an input hash of policy `lesson-draft`, prompt, schema, revision, model and sources. A resumed job reuses it ("Reusing the saved lesson draft; continuing verification").
- **Progress:** phase `Writing`, "Writing {lesson}".

### L7. Teaching review

`teaching::review_lesson`, which runs `teaching/review_loop.rs::review_material` over a `LessonSubject` (`teaching/subject/lesson.rs`). Runs after writing and after every section repair. One loop reviews a lesson, a curriculum outline (`OutlineSubject`, from `review_and_repair_with_progress`) and other material such as a quiz batch (`MaterialSubject`, from `review_and_repair`). The entry point picks the subject; the schema never does (`ReviewSchema::Lesson` carries a lesson's shape and brings the lesson's own rules, `ReviewSchema::Other` does not). The loop owns the order of a pass; the `Subject` trait (`teaching/subject.rs`) holds what differs between subjects. The loop first puts `lessonShape` into the writer's request it was given, with the words of the phase the lesson is in (`LessonShape::state_in`), so the reviewer, the check of the reviewer's notes and the repair read the shape the lesson is held to now. The plan check (step 5) is given the request but does not read that key.

A pass has these steps; a lesson goes through as many passes as its repairs need.

1. **Quotes (code, `teaching/quotes.rs`):** restore typography; on every pass after the first, re-anchor a quote the author could not match to the nearest real passage (`reanchor_lesson_quotes`: the span sharing at least 60% of the quote's terms, else the span sharing the most terms with the item). Citations are provenance; claims are checked separately. A quotation that still matches no saved reference is a finding at the part that cites it (`quotation_findings`).
2. **Structure (code, `structural_findings`):** the schema, each section's body and practice rubric, source wording copied outside quotation marks (`copied_wording::findings`) and the lesson's shape rules for its phase (`ShapePolicy::rules`: the counts and order of sections and questions, and the rules of `generation/lesson_rules.rs`, the citation rule among them; they check that a question has four distinct, non-blank choices and a reason per choice, but not the right choice's length or its overlap with the prompt). While a structure finding that the machine can still repair is open (`holds_up_review`), the pass is held up: steps 3 to 6 do not run and the repair is asked for the whole lesson.
3. **Question review:** `question_review::check` (`assessment/question_review.rs`; key prefix `question-review-v5`) reviews each question that has no saved verdict as a quiz item (`item_review::review`), the settled ones standing as the questions the learner already has. Two calls are made for it: the blind solver (`key_review::solve_items`, reason-first, shown no key) whose answer is compared with the key (`key_review::compare`), and the key-aware judge (`ITEM_REVIEW_SYSTEM`). The judge is shown the key, the explanation, the reasons, the criteria, `referenceQuote` and the blind solution; it is told that `referenceQuote` is one passage of what the learner was taught, not all of it, that a mention of the lesson is not a mention of a source, and not to judge the form, length or wording of the options (the writers are told to write the choices alike in form; nothing reads for it afterwards). Its outcome is the lesson's title and its objective followed by "The lesson teaches: ..." and the ideas, terms and actions the course plan gives the lesson (`Outcome::of_lesson`). The solver sends its questions through one loop (`assessment/batched.rs::answers`): the questions go in as many requests as the window fits (a request that does not fit is halved), an answer cut off at the output reservation halves the room for the rest, an incomplete answer is asked for again once, then the call fails with `ModelAnswerUnusable`. Each finding has `Origin::AnswerKey`.
4. **Reviewer:** `teaching_review::review` (policy `section-review-v5`), asked what `Standing::round` says it still has to judge (the reviewer bullet below). Run together with step 5.
5. **Plan check:** `teaching/contract.rs::check` (policy `lesson-contract-v6`) holds the lesson against its course plan and against itself (the plan check bullet below).
6. **Findings:** what the reviewer reports newly and the answer-key findings are bound to app-owned passages by `review_evidence::check` (policy `review-finding-protocol-v6`): each finding retrieves passages with `ReferenceCollection::retrieve_within` up to its equal share of the binding call's room (output 6,000, reserved twice); the answer lists the evidence IDs per finding (any number, bounded by the bank), and an actionable decision names the part it changes in `partId` (a candidate ID or `lesson`), which is where a reviewer issue without a site is filed. The gate is also shown what the other checks recorded in the same pass (`recordedFindings`: the quotation, question-review, plan-check and structure findings, each with an id the application gives, its part and its defect) and the checks that read the candidate beside the reviewer (`otherChecks`: the question review, the plan check and the rules, each with what it decides), and each decision names in `leftTo` the recorded finding that describes the same defect, else the check that decides what the finding is about, else `none`; a reviewer finding that names a recorded one is not established, so a defect another check owns is repaired once, from that check's finding. A finding that names a check is judged on its evidence like any other: `leftTo` says only which check rules on it, and the gate hands the finding back with its own verdict (`Left::established`, set when the decision is actionable and grounded). A reviewer finding is dropped only by a ruling on it: the gate's evidence verdict, or the plan check ruling on it as a defect of a kind its four checks decide. The lesson pass admits the established finding of a left whose check is the question review or the rules, which never rule on it, and of a left to the plan check when the plan check's second ask says the report is about another kind (`outside`); it admits none the plan check rules on. With nothing recorded and no other check the request, the schema and the system text carry none of this. The pass's findings are the quotation findings, the answer-key findings, the reviewer's open findings, the plan check's findings and the structure findings, a finding with a blank note dropped.
7. **Decide** (`Passes::next`): every finding that can still be repaired is open, and all open findings go to one repair. When none is open the pass ends the review (`Decision::Publish`), unless structure held it up: then the content has not been reviewed, and the loop starts another pass, which is no longer held up (`Decision::Review`).
8. **Repair** (`teaching/repair.rs::ask`, through `Repairs::make`): one call for all open findings; its answer is settled finding by finding, and the loop starts the next pass only when the lesson changed.

- **Findings and their ids** (`lessons/review_findings.rs`): a `ReviewFinding` has an `Origin` (`Rule` with the rule and the one sentence or citation it is about, `Plan` with its check, `AnswerKey`, `Reviewer { number }`), a site (the lesson, a section or a question) and a note. Its `FindingId` is built from the origin and the site, never from the note (`rule/<rule>/<site>[/<sentence>]`, `plan/<check>/...`, `key/<site>`, `reviewer/<number>`), so a defect reworded between passes is the same finding and two defects at one place are two. `Passes::next` folds notes with one id into one finding.
- **Repairs per finding** (`teaching/passes.rs`: `Passes`, `Outcome`): a finding is charged a repair when that repair is settled (`charge`), with what became of its attempt: `Applied`, `NotApplied` (with the reasons), `Reverted` (undone by the check that keeps protected sentences as they were), `Unnamed` (no edit named it) or `Unusable` (the answer could not be used). A repair is not charged when its edit was left out for overlapping another finding's (`Deferred`) or when its edits could not be placed for the first time since the finding was last charged: a `NotApplied` is charged only when the last outcome before it that was not `Deferred` was also a `NotApplied`, so a deferral in between does not reset it. A finding that has been charged `REPAIRS_PER_FINDING` (2) repairs stops being repaired, as does one the user holds (`Held`, below) and one held during the review (`hold`): a finding about a sentence a live claim stands on after a claim repair or a user edit (`protected.rs`; not a finding that the lesson lacks teaching, `ReviewFinding::adds`, whose correction adds sentences beside the protected ones), the structure findings that need a rewrite of the whole lesson while there are such sentences, and a finding whose repair does not fit the window alone. Every charge records the sentences the finding is about at its count under the finding's kind of defect (`Origin::kind`: `rule/<rule>`, `plan/<check>`, `key`, `reviewer`; keyed by the sentence digest), and a repair's `worked_on` gives the sentences it wrote every kind's count of the words it was for and, under the finding's own kind, at least its charge, so a later finding about one of them starts at the most repairs of its own kind they have been through and a chain of findings about rewritten words still ends; a new reviewer finding about a sentence a stored reviewer finding is about starts spent and is not repaired; and a new reviewer finding with no passage starts at the highest count of a reviewer finding with no passage at its place, so a defect no repair fixes cannot start over under a new id. `last_attempt` turns the last outcome into plain words for the finding's next repair.
- **The repair** asks in one of two forms (`Edits`, `Rewrite`). `Edits` is asked when no structure held the pass up (`Found::edits`); `Rewrite`, the whole lesson in the lesson schema, when structure did, and always for an outline. `issuesToFix` has one entry per finding (`Issue::entry`): `id`, `part` (the path of a block or question; null when it is about the whole material, has no part, or is a planned item the lesson does not teach: `ReviewFinding::mends_anywhere`), `also` (a second part), `passage` (the words it is about), `note` and `lastAttempt` (null on a first attempt). The answer holds one entry for each distinct id sent (`answers`: the finding, its plan, and the edits, citations and withdrawn citations that make that change; `text_edits::schema_for_findings`, read by `teaching/answers.rs`, which refuses an answer that leaves an id out, names one twice or gives a blank plan), and an entry's changes may touch only that finding's part or its second part; those for a finding that `mends_anywhere` may touch any part (`reach`, `text_edits::apply_for`). A finding whose entry gives a plan and no change is charged as declined and told its plan, and an answer of such entries alone changes nothing without being asked again. The Edits system text tells the repair to add what a lesson does not teach after a sentence whose thought is complete. An edit outside its finding's part is left out without costing the other findings their edits. After a claim repair or a user edit, `protected::confine_review` puts back every other sentence and every protected one. A rewrite names no finding: a finding counts as applied when its part changed. The answer is kept with the job (`teaching-repair-answer:<digest of policy, model, window, system, prompt, schema>`, policy `REPAIR_POLICY` = `teaching-repair-v3`), the accepted raw answer or the refusal's reason, and a repeated repair reads it back instead of asking again.
- **A repair that cannot be made** (an unusable answer, a request over the window, a window too small): asked as edits for more than one finding, it is made for the first half of the findings and then the second half, against what the first half made (`Settled::Split`), and a split group is charged nothing. A single finding, or a rewrite (which is the whole lesson however many findings it is sent, so a half would neither fit where the whole did not nor be more usable), is charged as `Unusable`; one that does not fit the window is held without a charge.
- **The reviewer** (`lessons/teaching_review.rs`). Its prompt carries the writer's request as `authoringContext`, the candidate with each section's `bodyPassages` (ids the application gives, `section-N-passage-M`) and `sectionIndicesToReview`. When the round asks for verdicts it also carries `openFindings`, one entry per open finding to judge (`id`, `part`, `passageId` when a passage still holds its words, `note`): every open finding, wherever in the lesson it is. Its system is the subject's first-review system (`REVIEW_SYSTEM` for a lesson) on a first review and `RECHECK_SYSTEM` ("judge the correction") on a recheck; a first review with open findings to judge adds `OPEN_VERDICTS`, the verdict instruction `RECHECK_SYSTEM` is built from; then `SHARED_CHECKS` (inspect every block) or `CHANGED_CHECKS` (inspect only the listed sections), then `LESSON_REVIEW_EVIDENCE_SCOPE` (all in `teaching/subject.rs::review_instructions`). The answer schema has `openFindings` with exactly one `{id (enum of the ids asked), quote, verdict (present | resolved)}` per id asked, in that order so the words are written before the judgement, `issues` (strings; for a lesson, `{note, concern}` objects whose concern is one a section cannot hold: `promise`, `later_lesson` or `workload`) and `blockChecks` (one `{sectionId, passageId (a passage of that section), finding, hasDefect}` per listed section; for a lesson each also carries `concerns`, the `LessonConcern`s the section fails, and the code keeps a defect only for a concern that applies to the section's kind); output `REVIEW_ANSWER_TOKENS` (6,000). Only the first attempt of a call reasons in prose first (`REVIEW_DRAFT_BRIEF`, plus `OPEN_FINDINGS_BRIEF` when findings are asked: read the part each finding names as the candidate now has it, whether or not `sectionIndicesToReview` lists it, and quote the words that still show the defect). A `present` verdict is grounded when its quote stands in the current text of the finding's part (`review_findings::part_holds`, by `same_text::find`: the section's or question's own strings, anywhere in the candidate for a finding on the whole lesson); an ungrounded one is asked for again through `responseCorrection`, and still ungrounded when the correction ends it counts as `resolved` and is logged. An answer that fails validation is asked again once with the problems and the accepted checks and verdicts (`responseCorrection`: `openFindings` lists only the verdicts still wanted, and a section's reported defect takes its passage from the check that was accepted), and again while each correction validates more than the one before; otherwise the call fails with "The model could not return a complete teaching review after response correction: ... Nothing was published." For a lesson (`LengthRule::DropBlank`) a blank finding is dropped after the correction. The result is `Judged`: a `Verdict` per finding asked, and the defects reported (`Reported`): a `blockChecks` entry with `hasDefect` is placed at its section with the passage it chose, an `issues` entry has no place yet.
- **What a verdict stands for** (`teaching/standing.rs`: `Standing`, `Round`): the same text under the same inputs. The reviewer's state for a material is the digest of each part when it last saw it, `basis`, its open findings in the order raised, the digests of the sentences each is about (its passage's and its sentence's, in its own part, and what applied repairs wrote for it) and the next number. `basis` is the digest (`teaching_review::basis`) of what the reviewer is given besides the material: the review policy, the model and its window, the subject's first-review system and evidence scope, the authoring context with `lessonShape`, the correctness priority and the reference versions. `review_material` takes it once, before the first pass (`Subject::given`, `Standing::under`), so nothing a pass selects from the candidate enters it; under another basis no verdict stands. `Standing::round` returns nothing when the reviewer has seen exactly this material under this basis: its findings stand and it is not called. Otherwise the round is a first review (`Round::first`) when no verdict stands: the subject's first-review system and every section, and each open finding (stored ones at the start of a job, all of them after the basis changed) asked by its id with `OPEN_VERDICTS`. When verdicts stand, the round is a recheck: only the sections whose digest changed, a verdict asked for every open finding, whether or not its own part changed, since a change elsewhere can mend it. A finding is about every part it names, its site and a second part (`also`), which `review_evidence` gives a reviewer issue without a site from the first other candidate part its evidence cites. `judged` removes a finding the reviewer calls resolved; one it calls present keeps its note, summary, site and number for as long as it is open (a finding is its first report), and its passage becomes the verdict's quote. A report that continues a finding that is present, or sits at the passage of one, is dropped as a duplicate; any other is numbered, in report order, from `next_number`, so a report that continues a finding the reviewer resolved or gave no verdict is a new finding. `admit` keeps a new finding `review_evidence` established when one of its parts changed since the reviewer saw it (the whole lesson counting as changed whenever the lesson did); a finding not kept is logged. A stored finding keeps its number and a new finding is numbered past the highest number stored for the lesson (`Held::next_number`), so no two findings of a lesson share one.
- **The plan check** (`teaching/contract.rs::check`, `contract/confirm.rs::settle`): a reason-first `complete_checked` call (`SYSTEM`, `BRIEF`, output `ANSWER_TOKENS` 6,000). The prompt carries `priorKnowledge`, `thisLesson` (its title and the planned ideas, terms, actions and contrast not already shown taught), `earlierLessons` (title, actions, terms), `laterLessons` (title, ideas, terms, actions), `thisModule`, `courseOutline` and the lesson's `sections` and `questions`. The four checks: each planned item taught; a use before it is taught (one entry for each such thing, however many parts use it: its name `what`, every place that uses it before it is taught `uses`, and the words that first teach it after those places `taughtLater`); contradictions; words that point at something the learner never sees. The learner sees the outline and the earlier lessons and will see the later ones, so pointing at them, or saying what a later lesson teaches, is not unseen. Code keeps only what the lesson's own words bear out, and every negative is put to the model once more in a question of its own (`lesson-contract-confirm:`). Each contradiction the sweep lists, once both its quotes are located, is put the same way as one question (`CLASH_TASK`: can the two statements both be true; if they can, the case in which both hold, if not, what one says happens that the other says cannot), and only one the read upholds is a finding, whose note ends with the case it names. A thing used before it is taught is one finding, sited at the first place the learner is asked to use it: code locates every quote of `uses` and orders them by part and position, not by the model's order, and the confirming read goes use by use, stopping at the first that is not read as "not asked" (a use that stands is the finding; a use that is taught ends the thing with no finding). Its note names the other parts that use it ("this step is the one place that teaches it"); when `taughtLater` is located after the standing use and its words are not protected, the note tells the repair to move that teaching to the first use, and the finding's second part is the part that holds it. What needs teaching about a record is defined once (`own_notes!()`): writing in the learner's own notes needs nothing taught; a record kept anywhere else needs the way to make it taught. The plan check's `SYSTEM` and `BRIEF`, the confirming read and the lesson writer's rule (`generation/lesson.rs::lesson_prompt`) carry it. The note of a planned item the lesson does not teach says "It fits best in <part>." when the confirming read named a part. When the gate left reviewer findings to this check, it is asked a second time with them as `reported` (each with an id `report-N`; a pointer where to read again, not evidence), and the answer gains `reports`: for each id, `about`, whether the defect it describes is of a kind these four checks decide, whether or not it holds. The check rules on each report whose `about` is true by its rules, and its findings join the pass's; a report with `about` false, or with no entry (the first entry for an id counts, and a missing one is logged), is returned in `Ruled::outside` and stands on the gate's verdict. The first ask has no `reports` and its schema and brief are unchanged. Its findings have `Origin::Plan`.
- **Publishing with what is left.** A lesson publishes with the findings the repairs did not settle waiting for the user (`SetAside`, with the repairs each was charged); no teaching-review finding fails a lesson job. `teaching/waiting.rs::settle` stores them in `learning_lesson_teaching_findings` by finding id (`claim_reviews/teaching_findings.rs`; the `identity` column, migration `20261110000000_teaching_finding_identity.sql`), bound to the digest of their part (`unit_sha256`); a finding the review no longer raises was fixed and goes. While the part's text is unchanged, `waiting::held` gives the finding back to the next review as held: the machine does not repair it again, a "keep as written" stands for that finding alone, and a reviewer finding is read back with its number, note, summary and passage. A later job's first review judges each stored reviewer finding again by its number, since what its review was given is not recorded: present, it stays, is not repaired again and waits; resolved, it goes. Once the text changes, all of that lapses. A row stored by kind and place (`identity = 'place'`, from before finding ids) stands for every finding of its kind at that place until its text changes. `waiting::record` adds the findings of the prepared lesson (`generation/lesson.rs`: its rules under the kept shape, and citations that do not match their source) to those already waiting.
- **Receipts and checkpoints:** the teaching receipt (policy `lesson-teaching-review`, over `review-finding-protocol-v6`, `section-review-v5`, the model, system, prompt, schema and the hashes of the saved references) and `teaching-sources:<digest>`, which keeps the hashes of the references a completed review read. An unchanged candidate whose reviewed references are unchanged is not reviewed again; references added since do not reopen it. Inside a job the loop also keeps the reviewer's answer (`teaching-review-findings-v6:<digest of policy, model, window, system, prompt, candidate, sections>`), the repair's answer (`teaching-repair-answer:`) and the plan check's answers (`lesson-contract-answer:`, `lesson-contract-confirm:`). `Passes` (repairs charged, last outcomes, worked sentences) and `Standing` (the reviewer's state) are saved with every draft the loop saves (`lesson_drafts::save_with_passes`) and taken up again when a review cut short resumes on that draft.
- **Material that fails closed** (`OutlineSubject`, `MaterialSubject`; `teaching/subject/closed.rs`) runs the same pass, with the reviewer's `LengthRule::Fail`, one repair and no waiting: a finding in the first pass is repaired once, and a finding in the pass after it is an error. Material a repair returns unchanged leaves the reviewer's findings standing (`Standing::round` is empty), so it fails without another reviewer call. Material reviewed through `review_and_repair` has the receipt `lesson-teaching` over the model, system, prompt and schema; an outline has none. An outline selects the passages it is reviewed against on every pass (`outline_evidence::select`) and reports its stages to the outline's progress.
- **Error:** only material that fails closed raises: "The teaching review still found an issue: ... No material was published. Try again.", "The course quality review returned invalid findings. Try again." (a blank note), or "The revised <outline | lesson> contains N source quotation(s) that could not be matched to the saved references. No material was published."
- **Progress:** phase `Review`: "Checking assessment answer keys", "Reviewing every teaching section" (first review) or "Rechecking the revised lesson", and, for a repair, "Correcting the lesson from review findings".

### L8. Claim extraction

`content_verification/inventory::extract` (and `extract_remaining`).

- **Input** (`inventory::inputs`): each section's body and each answer's explanation as app-owned passages with IDs, and nothing else: rubric fields, reflection prompts, quiz options and the lesson's quotation of the course brief are not read (claim classes design §3.5, item 2). For assessment items, wrong options are passed as `distractors` and never extracted as facts.
- **Call:** `complete_json`, sections in batches of four, up to two attempts per batch (the second includes the response errors), output 12,000.
- **Schema:** one `unit-N` slot per requested section: `{"claims":[{"passageId":enum(that section's passage ids),"statement"}],"nonFactualReason"}`. The app supplies the quote from the passage ID.
- **Rules (system text):** leave out names the lesson invented for its examples and extract the real-world fact in general terms; extract every independently checkable factual assertion, preserving scope, conditions, defaults and quantifiers; course organization, chosen exercise constraints, deliverables and preferences are nonfactual with a reason; record assertions even when false; do not extract quiz-label claims (the answer-key check owns those).
- **Checkpoint:** `InventoryCheckpoint` (policy `INVENTORY_POLICY`, model, context size, content hash, inventory, coverage, coverage policy), saved per candidate (`lesson_drafts::record_claim_inventory`). Section receipts (`section_checkpoints.rs`) keep extraction for byte-identical sections across repairs.
- **Progress:** phase `Inventory`.

### L9. Coverage audit

`content_verification/coverage.rs` (policy `passage-coverage-v9`). A mapping review accounts for every passage under its exact ID (`coverage/mapping.rs`). A fidelity judge then lists, per passage, the factual assertions the inventory fails to represent (`missingAssertions`). Each one the rule layer does not make Internal joins the inventory in the first pass and is sorted and checked like any other statement (claim classes design §3.4); one the rule layer makes Internal stays on the coverage, recorded in the report. A malformed answer gets one correction and is never read as a gap. Phase `Coverage`.

### L10. Examples

`content_verification/execution::observe` runs complete Python and JavaScript examples in the built-in runtimes. A failed example is an issue ("Example {id} did not complete: ..."). An unavailable runtime stops publication: "An example runtime is unavailable or cancelled. No lesson was published; retry when the runtime is available." Passed observations become evidence for their section's claims. Phase `Examples`.

### L11. Verification

Design: [`2026-11-01-lesson-claim-classes.md`](../design/2026-11-01-lesson-claim-classes.md) (§3.1 to §3.3, §3.11). Code: `content_verification/classify.rs`, `class_checks.rs`, `evidence_checks/*`, `claim_ledger.rs`, `provenance_gate.rs`.

- **Sorting.** Every statement the inventory extracts is sorted into a class: *Sourced* (a sentence the writer cited, quoted wording, or what the model finds attributes something to a source, reports a figure, date or version as a fact, or states a named product's or system's behaviour), *Reasoning* (an explanation or the lesson's own reasoning, a calculation with the lesson's own figures included), *Common* (standard knowledge with no specific) or *Internal* (a remark about the lesson itself, or one that only sets the lesson's own figures; counted, never written to the ledger). A cited sentence is Sourced (`ClassifiedBy::Writer`). The rule layer (`classify.rs` `ruling`) decides only what a string settles (`ClassifiedBy::Rule`): a rubric or an option is Internal, quoted wording (`quoted_wording`: between double quotation marks, at least `MIN_QUOTE_CHARS` characters and `MIN_QUOTE_WORDS` words) is Sourced. Otherwise it is a floor under the model (`Ruling`, `Ruling::settle`): a figure beyond the givens of a part that states what is so (an explanation, a contrast, a recap, or an answer explanation whose prompt and options are the givens; a worked example's, practice's or check's figures are its own) is `Checked`, never left Internal, and the model's Internal for it is taken as Reasoning, the lightest class whose check has an edge to a source. The model sorts the rest and the checked statements (`Model`, one call per 20 statements, in the first pass only; its answer is saved per statement under `classes-v2:` and settled the same way when read back). The ledger keeps the class of the strongest authority, and a statement a repair wrote keeps its parent's class.
- **The lesson's own figures** (claim classes design §3.13) are the quantities the lesson's own example, scenario or exercise sets, and the results calculated from them; they are not specifics, and no source states them. Whether a figure is the lesson's own or is reported as a fact is a question of meaning, so no code decides it; one literal (`lattice_core::own_figures!`) carries the definition into the classifier, the consistency judge, the knowledge judge, the part-wise judge, the writer's citation instructions (`generation.rs` `citation_instructions`) and the teaching repair's (`text_edits/asking.rs`). The classifier sorts a statement that calculates with them as Reasoning and one that only sets them as Internal. The consistency judge never calls them an unfounded specific; the knowledge judge calls a calculation with them `true_common` when it is right and `false` when it is wrong.
- **Each class meets its own check** (`CheckPolicy`, `crates/lattice-core` `claim_verification`). Sourced: the part-wise strict judge (`Bridge`) against the passages its sentence's own citation names, in one call: each part is stated, common knowledge, the lesson's own (`own`: no passage states it and none needs to), a specific no passage gives, or contradicted, so a Sourced statement can pass with a common-knowledge or own part beside its passage and still fails on a specific no passage gives (a common part that names a specific counts as one; an own part is not held to that guard, because nothing in a string tells a quantity an example sets from one a source reports); a statement that cites nothing is read against the course's page (`page_one`, `evidence_selection`), never both. Reasoning: the consistency judge (`consistency-v2`) against the passages its section cites, then a three-sample falsity veto that the sources can outrank. Common: the knowledge judge (`knowledge-v2`), closed-book, three samples, unanimous; a lone dissent sends the statement to its source check.
- **Where a statement ends** (`end_state`): ResolvedByEvidence (Sourced), Consistent (Reasoning), `GeneralKnowledge` by the machine (Common), Removed by the machine, or Edited when a repair's successor answers for it. A job's own checks park nothing: no Queued, Open or InsufficientStanding at its end.
- **The provenance gate** labels a supported Sourced statement ("per <source>", "according to <source>", "a practice, per <source>") and never demotes it.
- **A checker that gives no answer** teaches nothing about a statement: it is asked once more, and the lesson stops (`checker_unreadable`) when it still cannot be read.
- **Receipts:** per statement and comparison `claim-v1:<digest of POLICY, model, context size, comparison>` (`ClaimReceipt`, with the bridge's parts); a decision is restored without asking again.
- **Report** (`LessonVerificationReport`): policy, checker model, source bindings (id + sha256), coverage, coverage audit, findings (each with its class, the gate's label and its parts), executions, issues, retrieval mode.
- **Progress:** phase `Evidence`, "Checking claims". Once the statements are sorted the panel shows the plan, "N statements: S to check against their sources, R to check for consistency, C common knowledge", and "N calls so far, at most T" (`call_plan.rs`), a ceiling that only comes down. The calls are the requests that sort, check or rewrite statements and a section's flow repair, each counted where it is made; extracting claims, the coverage audit and the teaching review are not.

### L12. Research after writing

`content_verification/research::expand`. Per Sourced statement whose cited passage failed, at most one round and in it at most two pages: the selector's first choice among the web searches for the statement's gap and, when it names an owner, one owner-site result. A page is fetched only after the admission gate (`references/admission.rs`, `admission-v1`) passes its search result's own facts, and the page read passes it again before it is saved; a page fetched counts against the total whether or not it is saved. The count is written to the progress before the first request ("Looking up N statements on the web, at most 2N pages"). Research is once per statement for its life. What research saved is checked at once, and the course's page is read once for a statement whose section cites a passage, after its round. Phase `Research`.

### L13. Statement repair and cuts

`content_verification/repair_loop::verify_and_repair_with_references`, `repair::candidate` and `repair/cut.rs`.

- A statement gets two machine attempts in its life, whatever evidence later checks bring (`MACHINE_ATTEMPTS`): a rewrite (a model call that edits only its sentence), then taking the sentence out, made in code and never asked of the model. A Common statement the knowledge judge calls false has no rewrite to make and is cut at once. A sentence several statements stand on is rewritten for all of them, and each is checked once through its successor.
- A sentence that is the whole of its field, or sits in a code block, cannot be taken out: its statement ends `Removed { by: Machine }` and its section goes out of the lesson. Each section that took a cut gets one flow repair; a section left with too little goes; a lesson that loses its first explanation, more than a third of its sections or every statement it had stops with `sources_do_not_support` (design §3.10).
- **A question whose finding the review could not settle** (`content_verification/sections.rs::settle`): after the claim cuts are sent out (`send_out`), each question that still has a teaching finding waiting for the user (`teaching/waiting.rs::question_texts`, from `waiting_question_hashes`: rows of `learning_lesson_teaching_findings` with site `question` and state `waiting`, matched by `site_sha256` of the question's own JSON) goes with the cut units (`unsettled_questions`, `Cuts::go_unsettled`; `Cuts.unsettled` is saved with the job's cuts), one at a time and only while the lesson shape's floor still holds without it. If the floor then does not hold, the unsettled questions are readmitted (`Cuts::readmit`) and the pass runs again before the lesson is stopped with `sources_do_not_support`. A question marked to go that would leave the lesson unpublishable keeps its place (`repair/cut.rs`), and its mark in `unsettled` is dropped.
- `repair_sections` repairs each affected section in turn, `claim_groups` splitting a section's failed statements into groups whose request fits. The repairer answers with text edits (`lessons/text_edits.rs`); each patch is checkpointed as a `PendingRepair` so a stopped job resumes mid-repair. The repair's `requirements` are the writer's request with `lessonShape` replaced by the words of the phase the lesson is held to (`repair_loop.rs`, `LessonShape::state_in`: the floor's, [L6](#l6-write-the-lesson)), so it is not asked to restore a count verification no longer holds the lesson to. The teaching review (L7) runs on the repaired lesson, then the loop returns to L8.
- **Progress:** phase `Repair`, "Correcting defects found by evidence checks" or "Removing a repeatedly unsupported claim".

### L14. Readiness gate and outcome

- `prepare_lesson_with_references` decodes the final candidate and holds it to the lesson's rules: 11 to 16 blocks with the required kinds (a lesson that lost sections to cuts is held to the thinness rule of §3.10 instead), unique titles, six questions (two per kind), four choices that differ, a reason for each. A broken rule is a finding that is repaired or waits for the user while the lesson publishes; it never fails the job. Two choices are the same only when a reader could not tell them apart: typography, spacing and a closing full stop do not count, letter case, accents and path punctuation do. A practice section whose criteria are refused is saved with none and is a finding. The report is bound to the lesson (`report.bind`).
- `service::validate_prepared` (through `validate_generated`) checks only what our own code builds: the lesson has blocks, saved criteria are well formed, every saved source reference is one of the program's sources (a part that names no source it was given is saved with none, and a finding waits for it), question ids are unique, and each question has exactly one key. It does not judge what the model wrote. A question that cannot be asked (a blank prompt, fewer than two choices, a repeated choice, a key out of range) is kept in its place by the quiz bank, marked dropped, and never drawn.
- The report must validate at publication (`report.rs::validate`): its policy equals the current `POLICY`, it has no issues, and it matches the lesson's content hash. `report.reusable` additionally requires the same checker model and unchanged source hashes.
- The prepared lesson is checkpointed as `prepared-v1`, then all staged lessons are published in one transaction with their reports (phase `Publishing`, "Publishing verified lesson material").

| Outcome | Meaning |
|---|---|
| Job `Completed`, lesson `Ready` | Every statement ended in a state its class allows and the report is stored |
| Job `Pending` (deferred) | A transient model or network error; it retries automatically |
| Lesson held, review `Stopped` | The checker's answers could not be read twice for a statement (`checker_unreadable`), or the sources do not support enough of the lesson (`sources_do_not_support`); the draft is kept and the reason is on the review |
| Lesson held, review `NeedsReview` | Only what the user's hands hold: a Verifier dispute, a reopening, or a claim the user's own change of a source's trust re-decided |
| Job `Failed` (`generation_failed`) | A non-transient error: context overflow, teaching review still failing, malformed material. Drafts and checkpoints are kept; retry reuses them |
| Job `Cancelled` / `Interrupted` | User cancel or shutdown; requeued or retried from checkpoints |
| Lesson `Outline` | Not prepared yet (the only other `LearningPreparation` value) |

### Lesson progress phases

`planning/curriculum::LearningGenerationPhase`: `References`, `Writing`, `Review`, `Inventory`, `Coverage`, `Examples`, `Evidence`, `Research`, `Repair`, `Publishing`. Stage text is set with `lesson_progress::stage`; examples are quoted under each step above.

## 5. Labelling: statement classes

**Status: built on `lesson/design` (claim classes design); not yet accepted live (§3.11's run is scored by `scripts/score_lesson_acceptance.py`).**

- **No regex gate.** The old "keep an unsupported claim as general knowledge unless it has an absolute or a number" gate and the user's acceptance of a proposal are gone (design §3.7). A statement is Common because the knowledge judge, closed-book, three samples, unanimously calls it standard knowledge; a Sourced statement is never lightened by the model.
- **Lesson-internal statements** (a remark about the lesson's own code, scenario or exercise answers) are sorted Internal: counted, never checked, never in the ledger. Any real-world fact such a statement relies on is still extracted. Examples still run; quiz keys still go through the answer-key check.
- **What the learner sees** (`learner_view.rs`, `lesson_evidence.rs`, `evidencePresentation.ts`, `LessonEvidencePanel.tsx`): each statement's class and its check. A Sourced statement reads "According to <source>", "A practice, per <source>", "Per <source>, not the owner's documentation" and the other labels of the source-provenance design, or "Stated by <source>" with none; what it says beyond its passage that is common knowledge is listed apart ("Common knowledge, not in the source"). A Reasoning statement reads "Consistent with <sources>"; a Common one "Common knowledge · not checked against a source". The summary counts the classes apart, and a statement the user confirmed is counted as theirs.
- **The lesson's own figures are not specifics** ([L11](#l11-verification), design §3.13). The rule layer is a floor under the model, not its judge (`classify.rs` `Ruling`): quoted wording is Sourced by rule, a rubric or an option Internal, and a figure beyond the givens of a part that states what is so only bars Internal. The part-wise judge has the kind `own`, which counts toward standing as a common part does (`PartKind::Own`, `BridgeOutcome::decision`); the learner's panel lists such parts apart ("The lesson's own figures, not in the source").
- **The citation rule reads quoted wording only** (`generation/lesson_rules.rs` `uncited_quotations`, `classify::quoted_wording`). In an explanation, a contrast, a recap and every question's explanation and reasons, a sentence that holds quoted wording stands only when the part's citations list that sentence with a quote that contains the wording; otherwise the note says to cite it when an excerpt holds the wording, or to remove the quotation marks. A figure is no citation finding: whether it is the lesson's own or a fact a source must state is the classifier's question, and an uncited Sourced statement is cited by its own check (`Recite`).
- **Policies:** `lesson-evidence-v22`, inventory `lesson-evidence-v8`, `passage-coverage-v9`, `pending-repair-v3`, `consistency-v2`, `knowledge-v2`, and the saved class key `classes-v2` (see [section 7](#7-policy-strings-and-what-a-bump-invalidates)).
- **Outlines are unchanged.**

### Repair progress stop

`repair_loop.rs` keeps the best candidate seen: the one with the fewest issues after verification.

- A round that does not beat the best sends the next repair with `repeatedDefects = true` (remove the disputed assertions whole), even when the issue fingerprint differs.
- If that escalated round also fails to beat the best, the best candidate is saved as the draft and the job fails: "Repairs stopped reducing the lesson's problems: N remain (first 3 listed). The best draft is saved; retry to continue."
- A round that beats the best clears the escalation.
- The state lives in `PendingRepair.progress`, so it survives a resume.

Why: a sample of 40 claims the failed final2-1 lesson left unsupported, judged against world knowledge, was mostly correct but unsourced, plus 13 statements about the lesson's own code that no outside source can confirm. On the 85 labelled .NET pairs the shipped judge wrongly blocked none of 34 supported claims, so these were gaps in evidence, not checker errors. See [course-generation-testing.md](course-generation-testing.md#qwen-factual-accuracy-sample).

## 6. Budgets

**Principle.** "I don't want any arbitrary caps in this code... like this shit that is just chipping off the first 2400 characters" (Josh). No fixed limit may discard content that a model call needs. Sizes derive from the model's context window. What does not fit is split across calls or paged. Fixed numbers are allowed only for the shape of model output, for termination, for batch sizes that split work without dropping any, and for display. `crates/lattice-learning/src/content_caps_tests.rs` holds the regression tests ("Content reaching a model is bounded by its window, never by a fixed count or a character prefix").

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
| L7 reviewer | the sections that changed with their passages, and the open findings | 6,000 (`REVIEW_ANSWER_TOKENS`) |
| L7 plan check | the whole lesson, the course plan | 6,000 (`ANSWER_TOKENS`) |
| L7 review evidence | per finding, an equal share of the binding call's room | 6,000, reserved twice |
| L7 repair | the lesson and every open finding, the authoring context fitted to the window; an edit repair that does not fit is halved | the caller's reservation (10,000 for a lesson) |
| L8 extraction | 4 sections per call | 12,000 |
| L11 sorting | 20 statements per call, in the first pass only | per statement |
| L11 Sourced check | the passages its sentence cites, one call per statement; the course's page for one that cites nothing | 512 per claim |
| L11 consistency, knowledge, veto | the passages its section cites; a Common statement alone, three samples | 512 per claim |
| L13 repair | room = window minus section context minus 2 x output; claims grouped to fit | 2 x 10,000 reserved |

**What remains fixed** at `09d5a552`:

- **Output shape** (schema bounds on what the model writes): lesson 11 to 16 blocks, exactly 6 questions, 4 options each; outline review has no cap on new findings; project milestones need at least one deliverable and criterion; practice and practical tasks cite one or more of the offered sources, none twice; reference-plan subtopics at most `max_modules` (from the requested depth); no length on text the model writes but non-blank, apart from the 25 characters and five words a quotation needs; `MAX_QUOTE_CHARS` 1,200 bounds only a span the code picks as a quotation.
- **The 80,000-character response check** on an assessment response (`assessment_generation.rs`) and a practical-activity response (`practical_generation.rs`, `MAX_RESPONSE_CHARS`). A longer answer is rejected, not cut.
- **The teaching review's repairs:** `REPAIRS_PER_FINDING` 2 for each finding of a lesson's teaching review ([L7](#l7-teaching-review)), which bounds the loop; the reviewer is not told how many defects to report and every one it reports is kept.
- **Termination:** `ATTEMPT_LIMIT` 2 and `MODULE_REWRITE_BUDGET` 3 for outline repair; the lesson [repair progress stop](#repair-progress-stop); a statement's two machine attempts and one research round.
- **Batch sizes that drop nothing:** 8 items per linking call, 8 claims per check batch, 4 sections per extraction call.
- **Display:** stored source previews of 2,400 characters (`references/sources.rs`, `source_library.rs`) and 180-character titles. Retrieval and verification read the full capture.

The [caps audit](../design/2026-10-14-content-caps-audit.md) records the 36 fixed limits found on 2026-10-14. Its learning rows were removed in `569a76f7` and `f90f440c`. Its chat and web rows are on branch `fix/no-content-caps`. The Explorer folder-chat bounds (audit row 22) are outside this pipeline.

## 7. Policy strings and what a bump invalidates

Values on `lesson/design` (claim classes design).

| String | Where | A bump invalidates |
|---|---|---|
| `outline-evidence-link-v2` | `planning/outline_linking.rs` `POLICY` | All link receipts; every item is linked again |
| `outline-incremental-review-v2` | `planning/outline_review_scope.rs` `REVIEW_VERSION` | The review receipt; the next pass is a full review |
| `outline-claim-v1` | `planning/outline_repair.rs` `CLAIM_POLICY` | Factual-claim receipts from outline repair |
| `reason-first-draft-v1` | `lessons/structured_call.rs` `DRAFT_CHECKPOINT_POLICY` | Saved prose drafts of two-step calls in lesson jobs |
| `lesson-topic-evidence` | `lessons/generation/evidence.rs` `POLICY` | Lesson evidence plans and coverage judgements |
| `lesson-authoring` | `lessons/generation/lesson.rs` `AUTHORING_POLICY` | The reusable authoring context; evidence is gathered again and the lesson rewritten |
| `lesson-draft` | `lessons/generation/lesson.rs` (draft input hash) | The saved lesson draft |
| `lesson-teaching-review`, `section-review-v5`, `review-finding-protocol-v6` | `lessons/teaching.rs`, `teaching_review.rs` `POLICY`, `review_evidence.rs` `POLICY` | Teaching review receipts and `teaching-sources:` checkpoints: the lesson is reviewed again |
| `question-review-v5` | `assessment/question_review.rs` `check` (checkpoint key prefix; the key also holds the model, its window, the outcome and the question) | The saved verdict of an unchanged question |
| `teaching-review-findings-v6` | `lessons/teaching_review.rs` (checkpoint key prefix; the key also holds `section-review-v5`) | The reviewer's saved answers in a job |
| `teaching-repair-v3` | `lessons/teaching/repair.rs` `REPAIR_POLICY` | The teaching repair's saved answers and refusals (`teaching-repair-answer:`) |
| `lesson-contract-v6` | `lessons/teaching/contract.rs` `POLICY` | The plan check's saved answers and confirming reads (`lesson-contract-answer:`, `lesson-contract-confirm:`) |
| `lesson-teaching` | `lessons/teaching.rs` `review_and_repair_under` | Teaching receipts of material reviewed through `review_and_repair` |
| `classes-v2` | `lessons/content_verification/class_checks.rs` `class_key` | The model's saved class for a statement |
| `lesson-evidence-v8` (`INVENTORY_POLICY`) | `lessons/content_verification/mod.rs` | Claim inventory checkpoints |
| `passage-coverage-v9` | `lessons/content_verification/coverage.rs` `POLICY` | The coverage audit (the inventory is kept) |
| `lesson-evidence-v22` (`POLICY`) | `lessons/content_verification/mod.rs` | Claim receipts (`claim-v1:`), the research query log (`research-v1:`), pending repairs, and every report: `prepared-v1` checkpoints stop being reusable and must be verified again |
| `pending-repair-v3` | `lessons/content_verification/repair_loop.rs` (`repairPolicy` in the pending-repair fingerprint) | A saved mid-repair state; the repair starts again from the diagnosis |
| `repair-issue-set-v1` | `lessons/content_verification/repair_loop.rs` (`repair_issue_fingerprint`) | The fingerprint that detects a repeated issue set |
| `claim-research-scope-v1` | `lessons/content_verification/evidence_selection.rs` | Bindings of research sources to the claims that requested them |
| `semantic-entailment-v2` (`STRICT_INTERPRETATION_POLICY`) | `crates/lattice-core` claim verification | Non-supported claim receipts and provisional batch results |
| `consistency-v2`, `knowledge-v2` | `crates/lattice-core` `claim_verification/classes.rs` `CONSISTENCY_POLICY`, `KNOWLEDGE_POLICY` | The consistency and knowledge judges' receipts |
| `admission-v1` | `references/admission.rs` `POLICY` | The gate's verdict on a saved page: the sweep reads every machine-saved page again |
| `claim-ledger-v1:cuts`, `lesson-numbered-sources-v1` | `claim_ledger/cuts.rs`, `repair_loop.rs` checkpoint keys | A job's record of its cuts; the sources a repair numbered |
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
| L7 | `key_review::solve_items` (reason-first, through `batched::answers`) | questions without keys | correct indices and reasons | `question-review-v5:<digest>` |
| L7 | `teaching_review::review` (reason-first on its first attempt) | the changed sections with their passages, the authoring context with `lessonShape`, `openFindings` | `{openFindings:[{id, quote, verdict}], issues, blockChecks}` | `teaching-review-findings-v6:<digest>` (`section-review-v5`) |
| L7 | `review_evidence::check` | the reviewer's new findings and the answer-key findings; passages per finding | decisions with evidence IDs and `partId` | teaching receipt, `teaching-sources:<digest>` (`review-finding-protocol-v6`) |
| L7 | `contract::check` (reason-first) | the lesson, the course plan (`earlierLessons`, `laterLessons`, `courseOutline`), prior knowledge | `{planned, usedBeforeTaught, contradictions, unseen}` | `lesson-contract-answer:<digest>` (`lesson-contract-v6`) |
| L7 | `contract/confirm::settle` (reason-first, one call per negative) | one planned item, one use or one contradiction, the lesson | `{taught, quote, belongs}`, `{asked, taught, where, quote}` or `{case, contradict}` | `lesson-contract-confirm:<digest>` |
| L7 | `teaching/repair::ask` (one step; edits or the whole lesson) | candidate, `issuesToFix` (one entry per finding), quote checks, authoring context | `answers`, one entry (plan and edits) for each finding, or `lesson_schema` | `teaching-repair-answer:<digest>` (`teaching-repair-v3`); the draft with the loop's state |
| L8 | `inventory::extract` | section passages | `{units:{unit-N:{claims, nonFactualReason}}}` | `InventoryCheckpoint` (`lesson-evidence-v8`) |
| L9 | `coverage` mapping and fidelity | passages, inventory | coverage units, `missingAssertions` | inventory checkpoint (`passage-coverage-v9`), section receipts |
| L11 | `classify` | 20 statements the rules leave | a class each | the ledger's `Classified` events |
| L11 | `evidence_checks` (`Bridge`) | one Sourced statement, the passages its sentence cites | its parts: stated, common, specific missing, contradicted | `claim-v1:<digest>` per comparison |
| L11 | `class_checks` (`consistency-v2`, `knowledge-v2`) | one Reasoning or Common statement | consistent or not; three samples of the veto or the knowledge judge | receipts |
| L13 | `repair::candidate` (`repair_sections`) | section, failed statements, evidence table, history | text edits, or one validated section | `PendingRepair` in staged drafts (`pending-repair-v3`) |
| L13 | `sections::settle` | a section that took a cut and the sentences beside the gap | a flow repair | the job's cuts (`claim-ledger-v1:cuts`) |

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
| Claim classes §3.2: any digit makes a statement Sourced by rule | A figure beyond the part's givens in a part that states what is so is a floor (`Ruling::Checked`): the model sorts it and `Ruling::settle` takes Internal as Reasoning. Quoted wording still makes a statement Sourced by rule, and the lesson's own figures (`own_figures!`) are not held to a source |
| `2026-10-04-learning-content-verification.md` "First implementation slice": one evidence-driven repair, research only after checks | Superseded: repair repeats, and research runs before writing (§15) as well as after |

## 10. Known gaps

1. **The acceptance bar is not met yet.** No lesson has finished under the claim classes design on the real app: §3.11's run (lesson `f36400bc`, the 27B, nobody touching it) is scored by `scripts/score_lesson_acceptance.py`, and the admission gate's floors are provisional until its refused and short pages are read by eye.
2. **The live test does not wait out outages.** Only the app's outline path uses `PatientModel`; a 503 during the live test's outline stage still fails the run.
3. **Repair can drop evidence for one claim** when that claim's recorded passages alone exceed the window. It logs a warning and keeps the passages that fit.
4. **A non-transient outline error is not resumed automatically.** The draft waits for the learner to press Repair.
5. **The second checker question is closed for now.** Measurement found no second judge that helps (see the testing doc). The weak point is generation grounding, not the checker.
6. **Restoring a statement the machine removed is not built**, and the rest of what the branch did not do is in design §6.
