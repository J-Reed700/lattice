# Course generation pipeline (as built)

This is the reference for how Lattice builds a course from a topic request: the outline, then each lesson, and how both are checked and repaired. It describes the code at `62b3151f` on `learning/outline-evidence` (2026-10-15). Where the code and the design records disagree, the code wins and the difference is listed in [section 9](#9-where-the-code-differs-from-the-design-records).

- Testing, live runs and checker measurement: [course-generation-testing.md](course-generation-testing.md).
- Every live run and what it taught: [course-generation-history.md](course-generation-history.md).
- Design records (history of decisions, not current behaviour): [2026-10-14-outline-evidence-rebuild.md](../design/2026-10-14-outline-evidence-rebuild.md) (§12 and §14 are binding amendments), [2026-10-04-learning-content-verification.md](../design/2026-10-04-learning-content-verification.md).
- Release evidence for the wider Learning Studio: [learning-studio-verification.md](learning-studio-verification.md).

Paths are under `src-tauri/src/features/learning/` unless they start with `src/` (frontend), `crates/` or `migrations/`. Code is named by path and function, not line number.

## 1. Principles

1. **Evidence comes before writing.** The outline is drafted after references are gathered. A lesson is written after its topics are planned, retrieved, judged for coverage and researched.
2. **Code retrieves; the model plans, chooses and judges.** The model never chooses stored identifiers. Every quote attached to an outline item is the saved page's own text, so the exact-match check cannot fail. A lesson quote must match its excerpt exactly or it is re-anchored to the real passage.
3. **No arbitrary caps.** Josh's rule, in his words: "I don't want any arbitrary caps in this code... like this shit that is just chipping off the first 2400 characters". A fixed limit may never discard content. Sizes are derived from the model's context window. What does not fit is split across calls or paged. See [section 6](#6-budgets).
4. **Every judgement has a receipt.** Model decisions are saved under keys that include a policy string, the model name and the exact inputs. A resumed or repeated run reuses them and makes no model call. Bumping a policy string invalidates exactly the receipts that carry it ([section 7](#7-policy-strings-and-what-a-bump-invalidates)).
5. **Optional steps never stop the run.** An answer that strays from its schema in evidence linking, outline review, the lesson evidence plan or the coverage judgement is logged and treated as "no decision". Publication is the opposite: it fails closed.
6. **Loops end by construction.** The outline loop has per-finding attempts and per-module rewrite budgets. (The lesson verify-and-repair loop does not yet; see [section 10](#10-known-gaps).)

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
 ├─ L6 Write lesson .......... structured_call::complete_reasoned_json ...... reason-first (2 calls)
 ├─ L7 Teaching review ....... teaching::review_lesson ....................... quotes (code), answer keys, section review, <= 1 repair, final review
 │
 │ content_verification::verify_and_repair_with_references (repair_loop.rs)
 ├─ L8 Claim extraction ...... content_verification/inventory::extract ..... 1 call per 4 sections
 ├─ L9 Coverage audit ........ content_verification/coverage ............... mapping + fidelity; missing assertions adopted as claims
 ├─ L10 Examples ............. content_verification/execution::observe ..... runs Python and JavaScript examples
 ├─ L11 Verification ......... content_verification/evidence_checks::check . strict judge per batch of 8 claims, then challenge
 ├─ L12 Research after writing research::expand .............................. for unsupported and contradicted claims
 ├─ L13 Section repair ....... content_verification/repair::candidate ...... per section, split by token budget; then L7 again
 │     L8-L13 repeat until the report has no issues
 │
 └─ L14 Readiness gate ....... service::validate_prepared, report validation, publish
```

All model calls go through `lessons/structured_call.rs` (`complete_json`, `complete_json_with_progress`, `complete_reasoned_json`) and `model_call::send`. A **reason-first** call (`CallKind::Authoring`, `model_call::REASON_FIRST_AUTHORING = true`) makes two requests: a prose draft, then a schema-constrained answer that continues the same conversation. If the window cannot hold the draft beside the answer, it answers in one step. A `CallKind::Classifier` call is one step (`REASON_FIRST_CLASSIFIERS = false`). Inside a lesson job the prose draft is checkpointed under `reason-first-draft-v1:<digest of model, system, prompt>`, so an interrupted job resumes at the answer step.

## 3. Course outline

### O1. Reference plan

`planning/outline_references::plan`. Runs only when a web service is available.

- **Input:** goal, prior knowledge, minutes per session, depth bounds (`LearningCourseDepth::bounds`), and the catalog of existing sources (`ReferenceCollection::catalog`).
- **Schema:** `{"subtopics":[{"title":1..120 chars,"queries":[3..240 chars, max 3]}]}`, `subtopics` 1..`max_modules`. Output reservation `400 + 220 x max_modules` tokens.
- **Guard:** a query that repeats eight or more consecutive words of the goal or prior knowledge (`copies_private_text`, `COPIED_RUN = 8`) is dropped and logged. The learner's goal text never leaves the machine.
- **Errors:** "The reference plan was malformed; the draft was not started." (shape violations). "The reference plan contained no usable searches; the draft was not started." (no queries left after the guard, unless existing sources made new ones unnecessary).
- **Kept:** `OutlineDraft.reference_plan`. Its subtopic titles become the drafting retrieval queries.

### O2. Gather references

`planning/outline_references::gather` (wrapper of `gather_for` with `GatherPurpose::Course`).

For each subtopic, in order:

1. Run each query through `references/web_research::search` (`max_results: 12`, providers `duckduckgo` and `bing`, Wikipedia off, `site:` scope enforced by `within_search_scope`). Failures are logged and skipped.
2. Merge results by URL, dropping URLs already saved.
3. One `content_verification/research::select_references` call per subtopic chooses credible results (`decisions[].useForGap`). Model calls stay sequential: the server is single-slot.
4. Capture the chosen pages with `web_research::capture`, at most four fetches at once (`FETCH_CONCURRENCY`). A capture that is truncated, empty, or redirected out of the query's scope is skipped (`Ok(None)`).
5. Deduplicate by resolved URL and by `hash_text` of the text. Mirrors and redirects are not two sources.

Nothing is persisted here. The captures are saved with the program in `LearningRepository::create_outline_draft`, in the same transaction. A cancel during gathering leaves nothing behind. There is no cap on the number of sources; the bound is the plan.

**Progress:** stage `GatheringReferences` ("Gathering references"); detail "Subtopic n of m: searching for ...", "choosing among k results", "saving k selected pages".

### O3. Draft the outline

`lessons/generation/outline::draft_outline`, called from `service::generate_with_references_and_progress`.

- **Evidence in the prompt:** `planning/outline_evidence::select` runs each subtopic title (or the goal, when no plan exists) against all saved text with `ReferenceCollection::retrieve(query, 4)`, adds passages round-robin by rank, then fills remaining room with the first 600 characters of each source. Budget: a quarter of the model's window. Source indexes refer to the complete collection.
- **Prompt:** goal, prior knowledge, minutes per session, `sources` (the selected passages), `sourceCatalog`, and design requirements (backward design from a capstone, measurable outcomes, milestones, lesson sizing).
- **Citation instructions:** `OUTLINE_REFERENCE_INSTRUCTIONS` (cite only where a passage directly addresses the item; otherwise `sourceIndex: null`, `quote: ""`). With no sources the topic-only instructions apply and every citation must be null.
- **Schema:** `outline_schema(depth, sources)`; `sourceIndex` is nullable.
- **Call:** reason-first, subject "the course outline", brief `OUTLINE_DRAFT_BRIEF`. Output reservation `max_modules x max_lessons x 240 + 1600` when a depth is set, else `OUTLINE_OUTPUT_RESERVE` (4,000).
- **After the call (code):** `outline_citations::restore_source_quotes` fixes typography only; `outline_linking::strip_unmatched` removes any author quote that does not match its source exactly; `decode_outline` validates structure (a quote without an index is fabrication and an error); `service::validate_outline`. The program is saved as `Draft` with the draft state and the gathered sources.
- **Errors:** "The goal and source excerpts exceed this model's context window. Choose fewer or shorter sources." (does not fit), "No learning sources could be acquired." (user sources requested but none acquired), parse failures ("The model returned malformed or incomplete learning material. Try again.").

Author citations are provisional. They only survive if evidence linking chooses them.

### O4. Evidence linking

`planning/outline_linking::link`. Runs once at the start of `outline_draft::finish` over all items, and again on one module after each rewrite (`outline_repair`).

- **Items:** every module summary, outcome and lesson objective.
- **Candidates (code):** `ReferenceCollection::retrieve(item text + module title [+ lesson title], CANDIDATES_PER_ITEM = 4)`. Each candidate is cut to a citable span by `outline_linking/spans::citable_span`: whole sentences of prose (not headings, tables, link lists or code), at most `MAX_QUOTE_CHARS` (1,200) and at least five words. An author citation that matched exactly joins the candidates.
- **Call:** `CallKind::Classifier`, `LINK_SYSTEM`, batches of `BATCH_SIZE = 8` items, halved while the request does not fit. Schema `{"links":[{"id":enum(item paths),"candidate":enum(p1..pn, "none"),"reason":1..300}]}` with `minItems = maxItems = batch size`. Output `200 + 120 x items`.
- **Applying (code):** the chosen passage becomes the citation with a positive receipt. "none" records a negative receipt keyed on the declined candidate set, so only new references reopen the question. An author citation that was not chosen is removed. An unreadable answer leaves the batch unlinked without receipts.
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
| `factual` | Targeted research (`outline_repair::research`): `web_research::plan_queries` with the claim and reason as the gap and `alreadySearched` from `OutlineDraft.research_log`, then `outline_references::gather_for` (`GatherPurpose::Finding`), saved with the draft. Then judge (`resolve_factual`): `retrieve_for_verification(claim, 8)` plus `retrieve(claim, 8)` over all sources, `content_verification::judge_for_publication`. **Supported**: the located span is attached as the item's citation, receipts recorded, finding closed by code and never sent back to the reviewer (§12.1). **Contradicted**: module rewrite with `CORRECT` and the contradicting passage shown in the finding. **Unsupported** (or no passage): rewrite with `NARROW`. | Rewrite with `REMOVE` ("Remove the assertion or restate it as a learning activity rather than a fact."). No research. |
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
- There is no automatic resume. Any error inside `finish` is caught: the committed draft is reloaded, its status set to `Unchecked`, and the note becomes "Review or repair stopped: {reason} Completed work is saved. Use Repair to continue." Receipts make the next run cheap.
- No overall deadline applies to outline generation. Cancellation and transport failures still do.

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
- **Checkpoint:** `lesson-evidence-v1:plan:<digest of model, system, lesson + module>`. The catalog is not part of the key, so references saved by research do not reopen the plan.
- **Progress:** phase `References`, "Finding evidence for {lesson}".

### L2. Retrieval per topic

`evidence::retrieve` runs every query of a topic against all saved references with `ReferenceCollection::retrieve(query, depth)`, merges by rank and stores each passage once (`intern`: a passage overlapping one already held for the same source reuses it). The depth is the evidence room shared among every search, in passages of `PASSAGE_CHARS` (1,800 characters) measured with the model's tokenizer (`evidence::depth`).

### L3. Coverage judgement

`evidence::judge`.

- Lists each topic with its candidate passage IDs and each passage once.
- **Schema:** `{"topics":[{"id":enum(t1..),"covering":[enum(p1..)]}]}`, `minItems = maxItems = topics in the group`. Output `64 + 24 x topics + 8 x pairs`.
- **Call:** `CallKind::Classifier`, `COVERAGE_SYSTEM` ("A passage covers a topic when it states what a lesson must teach about that topic... Shared vocabulary, a passing mention, an adjacent topic... does not qualify").
- Groups are sized to fit and halved while they do not. A single pair that cannot fit is an error: "A lesson evidence check exceeds this model's context window."
- Unknown IDs are ignored. A topic missing from the answer counts as uncovered. An unreadable answer leaves its topics uncovered.
- **Checkpoint:** `lesson-evidence-v1:coverage:<digest of model, system, prompt>`.
- **Progress:** "Checking which of n passages cover the lesson's topics".

### L4. Research before writing

`content_verification/research::research_topics`, called from `evidence::gather` with the uncovered topics.

- Gaps are `{"topic", "missingEvidence": "No saved reference covers this lesson topic"}`.
- The shared `research` function plans up to three public queries (`web_research::plan_queries`, with `alreadySearched`), searches, calls `select_references` per query, captures, deduplicates against saved sources by URL and text, and saves each page to the program's source library (`LearningSourceLibraryRepository::add`). It saves at most two pages per query. It then reloads the collection.
- Completed queries are checkpointed under `research-v1:<digest of topic, model, lesson evidence policy>`, so a resumed job does not repeat them.
- `evidence::gather` re-retrieves the uncovered topics against the expanded collection and judges only passages not judged before. It repeats research rounds while a round covers at least one more topic.
- Without web research, or when planning or selection gives no usable answer, the topics stay uncovered. This step never stops the lesson.
- **Progress:** phase `Research`, "Finding references for lesson topics no saved reference covers".
- **Error:** "No saved passages match this lesson. Add relevant references in Sources, or narrow the lesson objective." when no topic has any passage at all.

### L5. Evidence pack

`evidence::pack` fills the writer's evidence room round-robin across topics: each topic offers its covering passages first, then its other passages, best first, so every topic is represented before any topic gets another passage. `evidence::assemble` turns the packed passages into the writer's `sources`: one entry per source whose excerpt is its packed passages in document order, plus `evidenceByTopic` (each topic with the `sourceIndex` values whose passages cover it; an empty list means no evidence). `prepare_lesson_with_references` then builds the prompt and drops the last packed passage until the measured prompt fits.

The authoring context (prompt, sources, reference hashes) is saved (`lesson_drafts::authoring`). It is reused while the scope hash matches (policy `lesson-authoring-v2`, prompt scope, revision, model) and every original source is unchanged, so references added by research do not cause a rewrite.

### L6. Write the lesson

- **System:** `lesson_system(true)`: "Teach one rigorous, accessible lesson..." plus `grounding_instructions(true)`.
- **Grounding rules** (`lessons/generation.rs::grounding_instructions`): every block, outcome, summary and question cites a `sourceIndex` with a verbatim quote; state a fact only when a supplied excerpt states it or it follows necessarily; add no statistics, typical figures, intensifiers (purely, entirely, always, never, guarantees) or causal explanations the excerpts do not give; present illustrations as explicit hypotheticals ("Suppose...", "In this example...") that say nothing about real products, versions or behaviour beyond the excerpts; for a topic without evidence, say what the learner should find out and why, asserting no facts.
- **Prompt** (`lesson_prompt`): program goal, prior knowledge, course sequence (recaps of earlier lessons), current module, lesson, `sources`, `evidenceByTopic`, `referenceCatalog`, and requirements: 8 to 12 blocks (at least two explanations and two worked examples, exactly one guided practice, independent practice, reflection and recap), minimum body lengths, rubrics for practice blocks, runnable Python or JavaScript examples, and exactly two questions each of practice, quiz and test.
- **Schema:** `lesson_schema(sources)`; `sourceIndex` is required (non-null) when sources exist.
- **Call:** reason-first, brief `LESSON_DRAFT_BRIEF` + `CITATION_DRAFT_BRIEF`, output `LESSON_OUTPUT_RESERVE` (10,000).
- **Checkpoint:** the raw draft is stored in the job's staged drafts (`lesson_drafts::save`, in `jobs.staged_result_json`) under an input hash of policy `lesson_draft_v1`, prompt, schema, revision, model and sources. A resumed job reuses it ("Reusing the saved lesson draft; continuing verification").
- **Progress:** phase `Writing`, "Writing {lesson}".

### L7. Teaching review

`teaching::review_lesson` then `review_material`. Runs after writing and after every section repair.

1. **Quotes (code, `teaching/quotes.rs`):** restore typography; on the second attempt, re-anchor a quote the author could not match to the nearest real passage (`reanchor_lesson_quotes`: the span sharing at least 60% of the quote's terms, else the span sharing the most terms with the item). Citations are provenance; claims are checked separately.
2. **Answer keys:** `answer_review::check` solves each question blind (reason-first) and compares with the key.
3. **Section review:** `teaching_review::review` (policy `section-review-v3`, findings bound to app-owned passages by `review_evidence`, policy `review-finding-protocol-v2`).
4. **One repair** of listed issues (one `complete_json_with_progress` call, the lesson schema), then a final review.
- **Receipts:** `teaching-sources-v1:<digest>` and the teaching receipt (policy `lesson-teaching-v5`). An unchanged candidate with unchanged sources is not reviewed again.
- **Error:** "The teaching review still found an issue: ... No material was published. Try again."
- **Progress:** phase `Review`.

### L8. Claim extraction

`content_verification/inventory::extract` (and `extract_remaining`).

- **Input** (`inventory::inputs`): each section as app-owned passages with IDs. For assessment items, wrong options are passed as `distractors` and never extracted as facts.
- **Call:** `complete_json`, sections in batches of four, up to two attempts per batch (the second includes the response errors), output 12,000.
- **Schema:** one `unit-N` slot per requested section: `{"claims":[{"passageId":enum(that section's passage ids),"statement"}],"nonFactualReason"}`. The app supplies the quote from the passage ID.
- **Rules (system text):** extract every independently checkable factual assertion, preserving scope, conditions, defaults and quantifiers; course organization, chosen exercise constraints, deliverables and preferences are nonfactual with a reason; record assertions even when false; do not extract quiz-label claims (the answer-key check owns those).
- **Checkpoint:** `InventoryCheckpoint` (policy `INVENTORY_POLICY`, model, context size, content hash, inventory, coverage, coverage policy), saved per candidate (`lesson_drafts::record_claim_inventory`). Section receipts (`section_checkpoints.rs`) keep extraction for byte-identical sections across repairs.
- **Progress:** phase `Inventory`.

### L9. Coverage audit

`content_verification/coverage.rs` (policy `passage-coverage-v6`). A mapping review accounts for every passage under its exact ID (`coverage/mapping.rs`). A fidelity judge then lists, per passage, the factual assertions the inventory fails to represent (`missingAssertions`). The application adopts those as claims for the evidence check (commit `2885fd55`); it does not argue with the judge. A malformed answer gets one correction and is never read as a gap. Phase `Coverage`.

### L10. Examples

`content_verification/execution::observe` runs complete Python and JavaScript examples in the built-in runtimes. A failed example is an issue ("Example {id} did not complete: ..."). An unavailable runtime stops publication: "An example runtime is unavailable or cancelled. No lesson was published; retry when the runtime is available." Passed observations become evidence for their section's claims. Phase `Examples`.

### L11. Verification

`content_verification/evidence_checks::check`, judge in `content_verification::judge_batch_provisionally` and `challenge_batch_for_publication`.

- **Evidence per claim** (`content_verification::evidence_for`): passed execution observations for its section, plus `ReferenceCollection::retrieve_for_verification(claim, 8)` over the whole collection, widened to surrounding context. Contradicting passages are not filtered. `evidence_selection.rs` keeps passages already compared stable as research adds sources.
- **Judge:** `ClaimChecker` (`crates/lattice-core`, `application/services/claim_verification`) with `CheckPolicy::Strict`, deterministic sampling, 512 output tokens per claim. Claims go in batches of `BATCH_SIZE = 8`; a batch that does not fit is halved; a claim a batch answer leaves unusable is checked alone.
- **Verdicts** (`ClaimVerdict`): `Supported` (with a supporting quote that must appear in the evidence), `Unsupported`, `Contradicted`, `Unverified` (the checker could not finish).
- **Challenge** (`content_verification/challenge.rs`): every provisional `Supported` is challenged. A separate call plans up to three research questions about omitted conditions, exceptions or conflicting evidence. Any question turns the verdict into `Unsupported` with the questions as the reason, so research follows. A failed challenge leaves the claim unverified.
- **Receipts:** per claim `claim-v1:<digest of POLICY, model, context size, comparison>` (`ClaimReceipt`: verdict, reason, supporting quote, interpretation policy); provisional batch results under `PROVISIONAL_BATCH_POLICY`.
- **Report** (`LessonVerificationReport`): policy, checker model, source bindings (id + sha256), coverage, coverage audit, findings, executions, issues, retrieval mode. Every non-`Supported` finding becomes an issue "Section n: {statement} — {reason}" at `62b3151f` (see [section 5](#5-labelling-general-knowledge-being-built) for the change in progress).
- **Progress:** phase `Evidence`, "Checking claims".

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

- `prepare_lesson_with_references` decodes the final candidate and enforces the shape: 8 to 12 blocks with the required kinds and minimum lengths, unique titles, six questions (two per kind), four distinct options, valid source IDs from this program, and binds the report to the lesson (`report.bind`).
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

## 5. Labelling: general knowledge (being built)

**Status: being built on this branch; not in `62b3151f`; never run against a real model.** This section is written from design §16. The as-built details will be added when the builder lands it.

- **The gate.** After the research rounds, a claim that is still `Unsupported` and not contradicted is kept and shown as general knowledge. It is no longer a lesson issue and never triggers repair. `Contradicted` claims and failed examples remain issues and are repaired. `Unverified` stays an error.
- **Exceptions that stay issues:** an unsupported claim that states a specific detail the evidence does not give (a number, version, date or percentage), or an absolute ("always", "never", "must", "all", "every", "only", "guarantees", "purely", "entirely"). Repair removes or softens that detail. These are where invented specifics concentrate.
- **Lesson-internal statements.** Claim extraction classifies statements about the lesson's own example code, scenario or exercise as nonfactual with the reason "lesson-internal", while still extracting any real-world fact they rely on. Examples still run; quiz answers still go through the answer-key check.
- **What the learner sees.** Unsupported claims appear in the lesson evidence panel as "General knowledge · not checked against a source" in neutral styling. Section and lesson summaries count source-checked and general-knowledge statements separately.
- **Outlines are unchanged.**
- **Policies.** The builder's working tree moves `content_verification::POLICY` from `lesson-evidence-v17` to `v18` and `INVENTORY_POLICY` from `lesson-evidence-v5` to `v6`, which invalidates claim receipts, inventories and `prepared-v1` reuse. It adds `Finding::general_knowledge()` (in `content_verification/general_knowledge.rs`) and uses it in `check_evidence` and the repair loop.

Why: a sample of 40 claims the failed final2-1 lesson left unsupported, judged against world knowledge, was mostly correct but unsourced, plus 13 statements about the lesson's own code that no outside source can confirm. On the 85 labelled .NET pairs the shipped judge wrongly blocked none of 34 supported claims, so these were gaps in evidence, not checker errors. See [course-generation-testing.md](course-generation-testing.md#qwen-factual-accuracy-sample).

## 6. Budgets

**Principle.** "I don't want any arbitrary caps in this code... like this shit that is just chipping off the first 2400 characters" (Josh). No fixed limit may discard content that a model call needs. Sizes derive from the model's context window. What does not fit is split across calls or paged. Fixed numbers are allowed only for the shape of model output (schema bounds), for termination budgets, and for display.

**How the window is divided.** `model_call::fits(llm, request)` asks the grounded-generation service whether the system text, prompt and output reservation fit the model's window (`LLMPort::max_context_tokens`, counted with `count_tokens`). `model_call::source_room(llm, request, ceiling)` returns the room left for evidence once the prompt skeleton and the output reservation are paid for. A request that does not fit is never sent; `model_call::send` maps it to the caller's error text.

| Call | Evidence or input size | Output reservation |
|---|---|---|
| O1 reference plan | catalog: window/16 (see caps audit for its heading and title limits) | 400 + 220 x max modules |
| O3 outline draft | passages: window/4, 4 per subtopic query, then 600-char source prefixes as filler | max modules x max lessons x 240 + 1600, or 4,000 |
| O4 linking | 8 items per call, halved while it does not fit; 4 candidates per item | 200 + 120 x items |
| O5 review | passages for lessons in scope: window/4 | 6,000 + 200 x open findings |
| O6 module rewrite | passages for the module's findings and lessons: window/4 | 6,000 |
| O6 factual judge | 8 verification passages + 8 retrieved passages per claim | 512 per claim |
| L1 evidence plan | lesson, module, catalog | 2,000 |
| L2 retrieval depth | room / (searches x tokens of a 1,800-char passage), at least 1 | |
| L3 coverage | groups sized to fit, halved on overflow | 64 + 24 x topics + 8 x pairs |
| L5/L6 writer | `evidence_room`: window minus prompt skeleton minus (3 x 10,000 + briefs + 512) for the two-step call; if that leaves nothing, minus 2 x 10,000. No ceiling. The pack is measured and trimmed until the prompt fits | 10,000 |
| L8 extraction | 4 sections per call | 12,000 |
| L11 verification | 8 claims per batch, halved on overflow; 8 passages per claim with surrounding context | 512 per claim |
| L13 repair | room = window minus section context minus 2 x output; claims grouped to fit | 2 x 10,000 reserved |

`SOURCE_EVIDENCE_CEILING` (7,000 tokens) and `bounded_sources` (12 sources, 2,400 characters each) now apply only to recall-card drafts (`generation/recall.rs`). Lesson authoring no longer uses them (commit `4f4a2ef6`).

**Remaining fixed caps.** [docs/design/2026-10-14-content-caps-audit.md](../design/2026-10-14-content-caps-audit.md) lists 36 rows of fixed limits that discard content. The learning rows are being fixed on this branch; the chat and web rows on branch `fix/no-content-caps`. The learning rows that touch this pipeline at `62b3151f`: `retrieve_for_verification(claim, 8)` per lesson claim; `ReferenceCollection::retrieve` clamping its limit to 50 and the keyword fallback to 100; widening of 300 and 1,700 characters; the catalog's 40 headings and 120-character titles; `CANDIDATES_PER_ITEM = 4` in linking; `outline_evidence::select`'s 4 passages per query and 600-character prefixes; the outline factual judge's 8 + 8 passages; `review_evidence`'s `retrieve(issue, 8)`; recall-card `bounded_sources`.

## 7. Policy strings and what a bump invalidates

| String | Where | A bump invalidates |
|---|---|---|
| `outline-evidence-link-v2` | `planning/outline_linking.rs` `POLICY` | All link receipts; every item is linked again |
| `outline-incremental-review-v2` | `planning/outline_review_scope.rs` `REVIEW_VERSION` | The review receipt; the next pass is a full review |
| `outline-claim-v1` | `planning/outline_repair.rs` `CLAIM_POLICY` | Factual-claim receipts from outline repair |
| `reason-first-draft-v1` | `lessons/structured_call.rs` `DRAFT_CHECKPOINT_POLICY` | Saved prose drafts of two-step calls in lesson jobs |
| `lesson-evidence-v1` | `lessons/generation/evidence.rs` `POLICY` | Lesson evidence plans and coverage judgements |
| `lesson-authoring-v2` | `lessons/generation/lesson.rs` `AUTHORING_POLICY` | The reusable authoring context; evidence is gathered again and the lesson rewritten |
| `lesson_draft_v1` | `lessons/generation/lesson.rs` (draft input hash) | The saved lesson draft |
| `lesson-teaching-v5`, `section-review-v3`, `review-finding-protocol-v2` | `lessons/teaching.rs`, `teaching_review.rs`, `review_evidence.rs` | Teaching review receipts |
| `lesson-evidence-v5` (`INVENTORY_POLICY`) | `lessons/content_verification/mod.rs` | Claim inventory checkpoints |
| `passage-coverage-v6` | `lessons/content_verification/coverage.rs` | The coverage audit (the inventory is kept) |
| `lesson-evidence-v17` (`POLICY`) | `lessons/content_verification/mod.rs` | Claim receipts (`claim-v1:`), the research query log (`research-v1:`), and every report: `prepared-v1` checkpoints stop being reusable and must be verified again |
| `claim-research-scope-v1` | `lessons/content_verification/evidence_selection.rs` | Bindings of research sources to the claims that requested them |
| `STRICT_INTERPRETATION_POLICY` | `crates/lattice-core` claim verification | Non-supported claim receipts and provisional batch results |
| `prepared-v1` | `lessons/generation_jobs.rs` checkpoint key | The verified-lesson checkpoint |

Change the string whenever the prompt, schema or meaning of the stored answer changes. Never reuse an old receipt under a new protocol.

## 8. Model calls

| Stage | module::function | Input | Output / schema | Checkpoint key |
|---|---|---|---|---|
| O1 | `outline_references::plan` | goal, prior knowledge, depth, catalog | `{subtopics:[{title, queries<=3}]}` | `OutlineDraft.reference_plan` (after save) |
| O2 | `research::select_references` (from `outline_references::gather_for`) | subtopic, queries, gap, merged results | `{decisions:[{id, useForGap, reason}]}` | none (sources saved with the draft) |
| O3 | `generation::draft_outline` (reason-first) | request, passages, catalog, requirements | `outline_schema` (nullable citations) | none; the draft is saved as the program |
| O4 | `outline_linking::link` | items with candidate IDs, passages | `{links:[{id, candidate|none, reason}]}` | `OutlineDraft.citation_receipts` (`outline-evidence-link-v2`) |
| O5 | `outline_review::review` (reason-first) | candidate or scoped modules, passages, open findings, verified assertions | `{verdicts, newFindings}` | `OutlineDraft.completed_review` |
| O6 | `web_research::plan_queries` (from `outline_repair::research`) | claim, reason, catalog, already searched | `{queries<=3}` | `OutlineDraft.research_log` |
| O6 | `research::select_references` | finding, results | decisions | saved sources |
| O6 | `content_verification::judge_for_publication` (from `outline_repair::resolve_factual`) | claim, passages | verdict, reason, quote; then challenge | `OutlineDraft.claim_receipts` (`outline-claim-v1`) |
| O6 | `outline_repair::rewrite_module` | module, findings with instructions, passages, course sequence | `{module}` | `learning_outline_checkpoints` row |
| L1 | `evidence::plan` | lesson, module, catalog | `{topics:[{topic, queries}]}` | `lesson-evidence-v1:plan:<digest>` |
| L3 | `evidence::judge` | topics with candidate IDs, passages | `{topics:[{id, covering}]}` | `lesson-evidence-v1:coverage:<digest>` |
| L4, L12 | `web_research::plan_queries` (from `research::research`) | topic or claims as gaps, catalog, already searched | `{queries<=3}` | `research-v1:<digest>` (completed queries) |
| L4, L12 | `research::select_references` | gaps, query, results | decisions | sources saved to the library |
| L6 | `structured_call::complete_reasoned_json` (from `prepare_lesson_with_references`) | lesson prompt with sources and `evidenceByTopic` | `lesson_schema` | prose step: `reason-first-draft-v1:<digest>`; answer: staged draft (`lesson_draft_v1` input hash) |
| L7 | `answer_review::check` (reason-first) | questions without keys | correct indices and reasons | teaching receipt |
| L7 | `teaching_review::review` (reason-first) | sections, questions, authoring context | section checks and findings | teaching receipt, `teaching-sources-v1:<digest>` |
| L7 | `teaching::review_material` repair (one step) | candidate, issues, quote checks | `lesson_schema` | staged draft |
| L8 | `inventory::extract` | section passages | `{units:{unit-N:{claims, nonFactualReason}}}` | `InventoryCheckpoint` (`lesson-evidence-v5`) |
| L9 | `coverage` mapping and fidelity | passages, inventory | coverage units, `missingAssertions` | inventory checkpoint (`passage-coverage-v6`), section receipts |
| L11 | `judge_batch_provisionally` | up to 8 claims with passages | verdicts | provisional batch key, `claim-v1:<digest>` |
| L11 | `challenge::guard_batch` | provisionally supported claims | `{queries<=3}` | `claim-v1:<digest>` |
| L13 | `repair::candidate` (`repair_sections`) | section, failed claims, evidence table, history | text edits, or one validated section | `PendingRepair` in staged drafts |

## 9. Where the code differs from the design records

| Design record says | The code does |
|---|---|
| §4.3: a new `planning/outline_citation_check.rs` judges author citations for coverage | Replaced by evidence linking (§13): `planning/outline_linking.rs`. No citation-check file exists |
| §5.1, §6.3, §11: unresolved findings block acceptance; no dismiss | An exhausted `plan` finding no longer blocks acceptance (`blocks_acceptance`, commit `37c6ef57`); it is shown as advisory |
| §15.1: the evidence plan returns 3 to 8 topics with 1 to 3 queries each | The schema sets only minimums (at least one topic, at least one query); there is no maximum |
| §15.3: one research round before writing | Rounds repeat while a round covers at least one more topic |
| §15.4: the writer's evidence ceiling rises from 7,000 to 48,000 tokens | There is no ceiling: `evidence_room` uses `usize::MAX` and is bounded by the window alone |
| §15.6: at most three passages per failed claim in repair | No per-claim count; claims are grouped by token budget, and only a claim whose own evidence exceeds the room loses passages |
| `repair_loop.rs` doc comment: "Continue while defects decrease; preserve a draft when repairs stop helping" | No code enforces it. The loop ends only when the report has no issues, a claim is `Unverified`, or an error is raised |
| `2026-10-04-learning-content-verification.md` "First implementation slice": one evidence-driven repair, research only after checks | Superseded: repair repeats, and research runs before writing (§15) as well as after |

## 10. Known gaps

1. **The acceptance bar is not met yet.** No live run has prepared the .NET lesson to Ready since the §15 rebuild (`4f4a2ef6`). §15 and §16 have not been run against a real model. See [course-generation-history.md](course-generation-history.md).
2. **§16 labelling is being built.** At `62b3151f` every unsupported claim is an issue and drives repair.
3. **The lesson verify-and-repair loop has no pass budget.** It can repeat until an error stops it. `repeatedDefects` asks for removal but does not bound the loop.
4. **Fixed caps remain** in retrieval and verification ([section 6](#6-budgets) and the caps audit). Research before and after writing saves at most two pages per query.
5. **Repair can drop evidence for one claim** when that claim's recorded passages alone exceed the window. It logs a warning and keeps the passages that fit.
6. **Outline generation has no automatic resume.** A stopped outline waits for the learner to press Repair.
7. **The second checker question is closed for now.** Measurement found no second judge that helps (see the testing doc). The weak point is generation grounding, not the checker.
