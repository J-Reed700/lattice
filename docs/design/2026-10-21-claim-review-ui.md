# Claim review: one place, one claim at a time

Date: 2026-10-21. Status: design, decisions taken (§11); nothing built. Code read on `fix/claim-official-sources` at `b4e447ae` (main). Frontend paths are under `src/features/learning/` unless they start with `src/`; backend paths under `crates/lattice-desktop/src/features/learning/` unless they start with `crates/`. This document is the UI half of one plan; the checker half is `docs/design/2026-10-21-claim-official-sources.md` ("the checker design"), whose §17 names where the two meet. The combined build plan is §9 here.

The owner's words (verbatim brief: `scratchpad/ui-brief/owner-feedback-2026-10-21.md`, screenshot `images/17.png`): the course page has "100s of different modules all over the place", the 33 held claims "ballooned the UI", a click on a claim "expands into some weird verdict explanation then some garbled source", and "I have no idea how I would even resolve these". He wants two things: stop putting everything on one page, and make the checker stop failing claims like "Before a model can call a tool, it needs a structured description of what the tool does, its arguments, and its return values."

## 0. What he was looking at, and why it is wrong

Read from the two pastes and the screenshot, and verified in the code:

| what he saw | where it comes from | the defect |
|---|---|---|
| The course goal twice: a truncated line, then ≈1,900 characters in full | `curriculum/ProgramWorkspace.tsx:230`: the `<h1>` is `program.summary.title` (`line-clamp-2`), which for this course **is** the goal text, and the `<p>` below is `program.summary.goal` (`line-clamp-1`); the Study activity card above repeats the title (`workspace/StudyActivityPanel.tsx:21-22`) | the same text three times; clamps hide it on screen but it is still there, and the title *is* the goal |
| "Needs your review · 33 claims" twice, with the same 10 groups | `lessons/LessonPreparationProgress.tsx:60` renders `HeldLessonsReview` (one `LessonClaimReviewPanel` per held lesson) **and** `lessons/LessonCard.tsx:98-99, :134` renders a second `LessonClaimReviewPanel` for the same held lesson when its card is open | two instances of the same panel on one page |
| A 6-step stepper, "LESSON NOT READY YET", a 5-figure stats block, a "Claims" line, "Checkpoint saved", a paragraph about thorough preparation, "Recent activity" | `LessonPreparationProgress.tsx:85-150` and `LessonPreparationCurrentStep.tsx` | the job's whole telemetry on the course page, above the lessons |
| A claim that quotes a whole paragraph in raw markdown with `[0] [5] [12]` markers | `lessons/review/ClaimReviewCard.tsx:170` shows `item.location.quote` (the extraction's quote: the paragraph) in a blockquote | the one checked statement (`item.statement`) is shown only as a second line when it differs; markdown and citation markers are never stripped |
| "Saved version a3cc5271-… · bytes 1355–6326 · hybrid" then ≈4,900 characters of raw page text, no highlight | `lessons/LessonEvidencePanel.tsx:63-68` (`Passage`), used by the card at `ClaimReviewCard.tsx:214` with `defaultOpen` | internal identifiers, the whole widened passage open by default, the supporting sentence not marked |
| "Definition claim about Required parameters array semantics · supported only by a source whose publisher is unknown · accepted alone: a primary, authoritative or established source" | `sources/trust/ClaimSufficiency.tsx`, `trustPresentation.ts:52-96` | the gate's vocabulary (kinds, levels, bars) shown as the explanation |
| "WHAT WAS TRIED": every query of 13 research rounds and ≈250 saved page titles (LinkedIn login, a GPU console, a customer-service vendor four times) | `ClaimReviewCard.tsx:220-233` lists every `ResearchAttemptDto` with every `sourcesAdded` title | research saves whatever it fetched; the card lists it all, relevant or not |
| Actions buried in a sentence; "Confirm" is the first chip in a row of eight | `ClaimReviewCard.tsx:235-245`, `ACTION_ORDER` in `claimReviewPresentation.ts:40-42` | no obvious "what do I do now" |
| "74 resolved by you" after zero user actions | `content_verification/claim_ledger.rs:560` (fixed by the checker design, fix H) | machine rewrites counted as the user's |
| 15 of 33 claims rest on one book, resolved one by one | no grouping by source; `groupBySection` only (`claimReviewPresentation.ts:88-102`) | no bulk action |

The page also mounts `CheckerAccuracy`, `LessonAuditLine`, `TeachingFindings`, the `PASS_BOUND` sentence ("the machine gives up on a claim after two tries on the same evidence"), the `Pass 38 · …` counts line and the Verifier columns inside every card. Everything the system knows is on the page; nothing is in the order a person resolving a claim needs it.

## 1. Decisions at a glance

| # | Decision |
|---|---|
| 1 | Reviewing happens on a dedicated **review screen** (`screen: 'review'` in the Study page, deep link `/studio?course=…&lesson=…&review=claims`), never inline in the course page. The course page shows one line per lesson: "Function Calling & Tool Schemas — 33 claims need you · Review". |
| 2 | The review screen is a queue (grouped: by source by default when two or more claims share a source, else by section) beside **one claim in focus**. Resolving moves to the next claim. Keyboard throughout. |
| 3 | A claim in focus shows: the one checked statement in plain text inside its sentence (markdown and citation markers stripped, backend-side), one plain-language reason, the best evidence with the supporting sentence highlighted in a little context, the source's title, site and kind in words, one line on why it counts or does not, the full passage behind a disclosure rendered cleanly, and "what was tried" as one line with a details disclosure listing only relevant pages. |
| 4 | Five obvious buttons: **Accept**, **Accept this source for all N**, **Edit the sentence**, **Remove it**, **Add a source**. General knowledge, research with a hint, unconfirm, undo and history stay, out of the main row. |
| 5 | "Accept this source for all N" is the existing course-scoped trust override on the source (`set_source_trust_override`), previewed with its impact count and undone with `clear_source_trust_override`; "Accept" is the existing `confirm` bound to the source. |
| 5a | **Add a source** is a primary path at two levels: on one claim, and for a group or the whole queue ("Add a source for these 33 claims"). One link, file or pasted text becomes a course source and is checked against every claim in scope, with live progress and a summary ("12 of 33 verified"). It is the existing attach flow (`claim_reviews/attach.rs`) with a scope and a progress channel, moved to its own command. A page you add that states a claim always settles it (§3.6); the only way it "comes back not enough" is that it does not state the claim, and the screen then shows the closest passage and says so. A source added on the Sources tab runs the same check over every waiting claim of the course's held lessons. |
| 6 | The course page's preparation panel collapses to one status line with the progress bar; its telemetry (stepper, pass statistics, checkpoint time, recent activity) moves behind one "Details" disclosure. `HeldLessonsReview` is deleted. The header shows the title once; the goal lives in a "Goal" disclosure and is never shown when it is the title. `OutlineReviewPanel` is one line once passed. |
| 7 | Nothing renders twice: one component owns each piece of information, and a vitest guard asserts the Lessons tab renders at most one claim review region and one preparation region. |
| 8 | All copy comes from one deck (§4) with no internal vocabulary; the same words serve a biology, history or software course. |
| 9 | Three builders in parallel (§9): A the checker (the checker design), B the review screen and course page, C the data layer the UI needs (DTOs, view builder, mocks, bindings). B and C meet at `claim_reviews/dto.rs` and `src/lib/bindings.ts`; A and C meet at the ledger's counts and sufficiency DTOs. |

## 2. Information architecture

### 2.1 Where things live

```
Study home                      Course page (one course)                 Review screen (one lesson)            Reader
─────────────────────────       ──────────────────────────────────       ───────────────────────────────       ──────────────
Activity: one line per          Header: title, lessons complete,         Queue (grouped, counts)               Full source,
request, pending or done        Goal ▸ (disclosure)                      Claim in focus:                       passage
Programs grid                   Tabs: Lessons · Practice · Recall ·        statement in context                 highlighted
                                      Notebook · Sources · More            why it waits (one line)
                                Lessons tab:                               best evidence (highlighted)
                                  preparation: ONE status line + bar       what was tried (one line ▸)
                                  lesson rows, ONE line each               Accept · Accept source for all ·
                                  "… — 33 claims need you · Review" ──▶      Edit · Remove · Add a source
                                  milestones (one row)                   Resume verification (when done) ──▶ course page
```

Rules: a claim's text, evidence and actions exist only on the review screen; the course page knows counts and states; the reader shows sources. The Study activity card never shows the course goal (it shows the request's own title, "Preparing a lesson", and the lesson title).

### 2.2 The course page, component by component

Every component the Lessons tab, the header and the Sources tab render today, with its fate. "Keep" means unchanged; "Collapse" means the same information behind a disclosure or on one line; "Move" names the new home; "Delete" removes the component.

| component (file) | today | decision |
|---|---|---|
| `workspace/StudyActivityPanel.tsx` (`ActivityCard`) | a card per request with course title (= goal), title, status, duration, "Task details" disclosure, Open/Clear | **Collapse**: one line per request ("Preparing Function Calling & Tool Schemas · 12m 28s · Open"); details disclosure stays; `courseTitle` shows the program's short title and never the goal; a finished request whose destination is a held lesson says "33 claims need you · Review" and opens the review screen |
| `curriculum/ProgramWorkspace.tsx:230` header: "Active program · Revision N", title, goal, "Lessons complete n / m" bar | title and goal both visible | **Change**: drop "Revision N" (internal); title once; the goal in a "Goal ▸" disclosure, omitted entirely when `goal.trim() === title.trim()` or the title starts with the goal's first 80 characters; keep the completion bar |
| `curriculum/OutlineReviewPanel.tsx` | a green/amber card with heading, paragraph, findings | **Collapse**: when `status === 'passed'` and the course is active, one line "Outline checks passed · 49 references" with no paragraph; expanded card only for a draft or `needs_repair` |
| "Topic-based course · AI-authored…" line (`ProgramWorkspace.tsx:235`) | shown when no sources | **Keep** |
| `curriculum/CourseSyllabus.tsx` (full, draft only) and `compact` in the header | draft outline / jump menu | **Keep** |
| module select row + module header + "Module details" popover (`:238-254`) | | **Keep** (one row) |
| tabs `Lessons · Practice · Recall · Notebook · Sources · More(Plan, Canvas, Import & export)` (`:144, :255`) | | **Keep**; the review screen is not a tab (it is per lesson) |
| `sources/CourseIndexNotice.tsx` | index building/waiting notice | **Keep** (one line, already) |
| `lessons/LessonPreparationProgress.tsx` (header, pause/resume, stepper, current step, stats block, Claims line, checkpoint line, thorough-preparation paragraph, Recent activity) | ≈120 visible lines while a job runs | **Collapse** into `LessonPreparationStatus` (same file, rewritten): one status line "Preparing Function Calling & Tool Schemas · Fact checks · 12m 28s" + the existing progress bar + Pause/Resume button + "Details ▸". Behind Details: the current step (`LessonPreparationCurrentStep`), the stepper, the pass statistics, the checkpoint time and Recent activity, unchanged in content. The "Claims" tally line moves to the lesson row (below). The thorough-preparation paragraph becomes one sentence in Details. `PASS_BOUND` is deleted everywhere (it is a statement about the machine's budget, not the user's business). |
| `lessons/review/HeldLessonsReview.tsx` | the held-lesson takeover of the preparation panel | **Delete**. A held lesson is a lesson row state. `claimTallyLine` moves to `claimReviewPresentation.ts` and loses "resolved by you" for machine work (§4.4). |
| `lessons/LessonPreparationLine.tsx` | one line on other tabs | **Keep** (it already is the collapsed form; the Lessons tab now uses the same line plus the bar) |
| `lessons/LessonCard.tsx` | expandable card; for a held lesson it mounts `LessonClaimReviewPanel` inline (`:98-99, :134`) | **Change**: the closed row is one line: title · state · "Review" button for a held lesson ("33 claims need you · Review"), "Ready · 2 teaching notes · Review" for a published lesson with notes, "Preparing · 44 of 44 checked" while its job runs (from `activity.claims`), "Outline · Prepare" otherwise. Opening a *ready* lesson shows its sections as today. Opening a *held* lesson shows its draft sections read-only with a "Review 33 claims" button; never the claim panel. `AuditLessonNow` and `LessonEvidencePanel` stay inside the opened ready lesson. |
| Module milestones `01 · Learn … 04 · Build` (`:266-275`) | a 4-card grid | **Collapse** to one row of four short links |
| `(prepareError || completeError)` alert (`:289`) | | **Keep** |
| bottom `<details>` "Progress, source materials & authorship" (`:291-293`) | collapsed | **Keep** |
| `ProgramTangents` wrapper, `CitationVisibilityToggle`, `ProgramTangentsTrigger` (`:228`) | | **Keep** |
| **Sources tab**: `sources/SourcesPanel.tsx`, `SourceMaintenancePanel`, inline `SourceReader` (`:284`) | | **Keep**; the review screen's "Open the source" opens the same reader (`FilePreviewModal`), not this tab |
| **Plan tab**: `curriculum/PlanPanel.tsx:105` renders `LessonPreparationProgress` per job | | **Change** to the collapsed `LessonPreparationStatus`; `VerifierAuditSwitch` stays |
| `lessons/review/LessonClaimReviewPanel.tsx`, `ClaimReviewCard.tsx` | the inline panel and card | **Replace** by the review screen (§6) |
| `lessons/review/CheckerAccuracy.tsx` | a course-wide accuracy line at the bottom of every panel | **Move** to the Plan tab (course-level fact) |
| `lessons/review/LessonAudit.tsx` (`LessonAuditLine`, `AuditLessonNow`) | audit line in the panel header | **Move**: the line into the review screen's header only when an audit exists (`hold === 'audit' \| 'audit_stop'`, §3.7); `AuditLessonNow` stays in the opened ready lesson |
| `lessons/review/TeachingFindings.tsx` | after the claims in the panel | **Move** to the review screen as a second queue group "Teaching notes" (same component, restyled to the queue row) |
| `lessons/review/VerifierAuditSwitch.tsx` | Plan tab | **Keep** |
| `sources/trust/ClaimSufficiency.tsx` | the SUPPORT block | **Delete** from the claim; its facts become the one-line reason and the evidence's "why it counts" (§3.3, §4) |

Nothing renders twice: `LessonClaimReviewPanel` is deleted, so there is one review surface; `LessonPreparationStatus` is rendered by the Lessons tab and the Plan tab only, never both at once (they are different tabs); the goal is rendered once; the activity card never repeats the title it links to. A vitest test (§7.1) mounts the Lessons tab with a held lesson and a running job and asserts `getAllByRole('region', { name: /claim review/i })` is empty and `getAllByRole('region', { name: 'Lesson preparation' })` has length one.

### 2.3 Navigation

- `LearningStudioContent` (`workspace/LearningStudioPage.tsx:34`) `Screen` gains `'review'`; state `reviewing: { programId, lessonId } | null`. The Study page already parses `?course=&lesson=` (`:55-70`); it also parses `review=claims` and opens the review screen. The lesson row's Review button and the activity line call `openReview(programId, lessonId)`, which pushes `?course=…&lesson=…&review=claims` so the browser's back returns to the course page with the lesson row focused (`document.getElementById('lesson-<id>')`).
- Leaving the review screen (Back, Escape with nothing open, Resume verification) returns to the Lessons tab.
- The `useStudioNavigationGuard` (unsaved notebook work) applies as to any screen change.

## 3. The review screen

### 3.1 Layout, desktop (≥ 1024px)

```
┌─ ← Function Calling & Tool Schemas ───────────────────────────────────────────── Resume verification (disabled: 33 left) ─┐
│ 33 claims need you · 12 verified · 74 rewritten by the machine                                   Group by: Source ▾  ?  │
├──────────────────────────┬──────────────────────────────────────────────────────────────────────────────────────────────┤
│ QUEUE                    │ Section 2 · Defining Tool Schemas                                              3 of 33  ◀ ▶ │
│                          │                                                                                              │
│ ▾ Tool Use & Function    │ Before a model can call a tool, it needs a structured description of what the tool does,    │
│   Calling — The LLM      │ its arguments, and its return values. ░This is known as a tool schema or function schema,░ │
│   Stack · online book    │ ░typically defined using a subset of JSON Schema.░  A standard function definition …        │
│   15 claims              │                                                                                              │
│   [Accept this source    │ ■ Waiting for you: no source the course trusts states this.                                  │
│    for all 15]           │   The one passage that does is from an online book whose author is not identified.          │
│   ● Before a model can   │                                                                                              │
│     call a tool, it …    │ BEST EVIDENCE                                                                                │
│   ○ The `required`       │ Tool Use & Function Calling — The LLM Stack · prakashkagitha.github.io · online book         │
│     array lists …        │ "…Before the model can invoke anything, it is given a ▌structured description of each tool:  │
│   ○ Returning an         │ its name, what it does, its arguments and what it returns▐. The description is the contract…"│
│     execution error …    │ Counts as: an unidentified author's work · not enough on its own for this claim              │
│   (12 more)              │ Full passage ▸    Open the source ↗                                                          │
│                          │                                                                                              │
│ ▸ Function Schemas: JSON │ Searched the web 9 times and read 214 pages; none stated it. Details ▸                       │
│   Schema … · AI/TLDR     │                                                                                              │
│   article · 6 claims     │ ┌──────────┐ ┌──────────────────────────────┐ ┌──────────────────┐ ┌───────────┐ ┌─────────────┐│
│                          │ │ Accept   │ │ Accept this source for all 15│ │ Edit the sentence│ │ Remove it │ │ Add a source││
│ ▸ No source states it    │ └──────────┘ └──────────────────────────────┘ └──────────────────┘ └───────────┘ └─────────────┘│
│   4 claims               │ More ▾ (General knowledge · Search with a hint · History)                                     │
│                          │                                                                                              │
│ ▸ Teaching notes · 2     │ A · accept   E · edit   R · remove   S · add a source   J/K · next/previous   U · undo   ? │
└──────────────────────────┴──────────────────────────────────────────────────────────────────────────────────────────────┘
```

`░…░` is the checked statement, set in the sentence it was extracted from (the rest of the paragraph in the secondary colour). `▌…▐` is the supporting sentence highlighted inside the passage excerpt.

### 3.2 The queue

- Groups, with counts, collapsible; the active claim's group is open. Group modes, in a `Group by` select: **Source** (default when ≥ 2 waiting claims share a support source; a group per support source, titled by the source's title, site and kind; claims with no support source form "No source states it"), **Section** (lesson order; questions after sections), **What fixes it** (the copy deck's fix classes, §4.2). Teaching notes are always their own last group.
- A row is one line: a state dot (● waiting · ○ waiting, not yet opened · ✓ resolved this session), the statement in plain text (clamped to two lines), and the section chip on the right. Rows are a single roving tab stop (`ArrowUp/Down`, `Home/End`, as today's headers, `LessonClaimReviewPanel.tsx:164-172`).
- A source group's header carries the bulk button **Accept this source for all N** (§3.5) when N ≥ 2.
- Resolved claims drop out of the queue after a short settle (they stay reachable under a "Show resolved (n)" toggle at the bottom), so the count only goes down.
- The header line is the honest tally: "33 claims need you · 12 verified · 74 rewritten by the machine" (§4.4); "resolved by you" appears only when the user did something.

### 3.3 One claim in focus

Top to bottom, nothing else:

1. **Place**: "Section 2 · Defining Tool Schemas" (or "Question 3", "Question 3 · answer key"); position "3 of 33"; previous/next.
2. **The statement in context**: `presentation.sentence` set in `context_before` + sentence + `context_after` (the paragraph, plain text, §5.1), with the checked statement marked. No markdown, no `[n]` markers, no code-fence noise. For a question, the prompt or option text.
3. **Why it waits**: one line from the deck (§4.1) with the state as a chip: "Waiting for you". A second short line names the specific lack when there is one ("The one passage that does is from an online book whose author is not identified."). Never a kind, level, bar, pass number, uuid, byte or "the machine".
4. **Best evidence** (§3.4), or, when no passage was found, one line: "No saved source mentions this" and the "What was tried" line.
5. **What was tried**: "Searched the web 9 times and read 214 pages; none stated it." / "…; 2 pages mention it but do not state it." with **Details ▸** listing only the relevant pages (`tried.relevant`, §5.2): title · site, each with "Open". Never login pages, consoles or unrelated titles (those are not relevant pages by definition).
6. **Actions** (§3.5) in one row, then "More ▾".
7. A one-line keyboard legend at the bottom (hidden on touch devices).
8. After an action: the live region announces it ("Accepted. 32 left.") and focus moves to the next claim's statement; Undo is in the announcement bar for the last action.

### 3.4 The evidence view

- Source line: `support.source.title` · `support.site` · `support.kind` word (§4.3): "online book", "documentation", "peer-reviewed paper", "reference work", "article", "forum", "your file", "pasted text".
- Excerpt: the supporting sentence (`support.quote`) highlighted, with up to one sentence of context on each side taken from the passage (`support.quote_span` inside `support.passage.text`, sentence bounds found client-side with the same sentence rule as the backend, §5.1), rendered as plain text.
- **Counts as**: one line from the deck (§4.3): what this source is for this claim and whether it is enough: "an unidentified author's work · not enough on its own for this claim", "the course's strongest source on this subject · enough", "the owner's own documentation · enough".
- **Full passage ▸**: the whole passage rendered through `TiptapViewer` with `formatSourceForReader` (`sources/sourceReaderFormat.ts`), which already repairs flattened markdown, code fences and tables for the Sources tab reader; the highlight is kept by wrapping the quote in a `<mark>` before rendering.
- **Open the source ↗**: `FilePreviewModal` with `fromLearningEvidencePassage(passage, 0, fullText)` (`src/features/reading/model/sourceAdapter.ts:446`) and `locator.text` = the supporting sentence, so `PassageHighlighter` (`src/features/reading/components/PassageHighlighter.tsx:98`) scrolls to and marks it; `fullText` comes from the source detail query the Sources tab already uses (`sources/useLearningSources.ts`, `SourcesPanel.tsx:327`). A web source also offers the frozen page (`WebPageSnapshotView`) as today.
- When a claim has two or more supports (the checker design's `also_supported`), the best one is shown and "Also stated by: <title>" lists the others as links that swap the excerpt.
- A contradicting passage (verdict contradicted) is shown the same way with the heading "SAYS OTHERWISE" and the contradicting sentence highlighted.

### 3.5 Resolve flows

| button | what it means | backend | new? |
|---|---|---|---|
| **Accept** | this source is good enough for this claim | `resolve_learning_claim` with `{ action: 'confirm', note, sourceVersionIds: [support.source.sourceVersionId] }` (`plugin/claims.rs:134`; the ledger's `Confirm { bound }`). A contradicted claim asks for the note first (today's rule) | no |
| **Accept this source for all N** | trust this source in this course; every claim it supports is re-decided | opens a dialog: source title and kind, "N claims rest on this source in this course" (`get_learning_source_trust` with `includeImpact: true`, `impact.claims`), a level choice in words (default "a reliable identified source" = `established`; "an authority on this subject" = `authoritative`; "the subject's own account" = `primary`), optional note; then `set_source_trust_override` `{ scope: 'source', programId, sourceId, level, note }` (`plugin/trust.rs:47`, `claim_reviews/trust_overrides.rs:104`); the result (`LearningTrustChangeResultDto`) says "12 claims verified, 3 still need you"; **Undo** calls `clear_source_trust_override` with the same scope | no new command; a dialog component |
| **Edit the sentence** | rewrite it | `{ action: 'edit', before, after }` (the sentence, not the paragraph: `presentation.sentence` is `before`) | no |
| **Remove it** | delete the sentence | `{ action: 'remove' }` | no |
| **Add a source** | a page address, a file or pasted text, for this claim (§3.6) | the new `attach_learning_claim_evidence` command (§5.3) with `scope: { kind: 'claim' }`; the report's lines are shown in the announcement bar | the command is new; the service behind it is `claim_reviews/attach.rs` as it is |
| **Add a source for these N** (group header, queue header) | the same, checked against every waiting claim of the group or the queue (§3.6) | `attach_learning_claim_evidence` with `scope: { kind: 'claims', claimIds }` | same command |
| More ▾ **General knowledge** | accept the proposal | `{ action: 'general_knowledge' }`; a group-level "Accept all n proposals" when a "No source states it" group holds proposals (`accept_all_general_knowledge`) | no |
| More ▾ **Search with a hint** | | `{ action: 'research_with_hint', hint }` | no |
| More ▾ **History** | the event list, as today's `ClaimHistory` | — | no |
| Undo | | `{ action: 'undo' }`, or `clear_source_trust_override` after a bulk accept | no |
| **Resume verification** | when nothing is left for the user | `resume_learning_lesson_review` (`plugin/claims.rs:222`), disabled with the reason until something changed (today's rule) | no |
| Unconfirm | on a confirmed claim under "Show resolved" | `{ action: 'unconfirm' }` | no |

Resolving moves to the next claim in queue order; the last resolved claim's Undo stays available until the next action. Keyboard: `J`/`K` or `ArrowDown`/`ArrowUp` next/previous claim; `A` accept; `Shift+A` accept the source for all; `E` edit; `R` remove; `S` add a source; `G` general knowledge; `U` undo; `Enter` on a queue row opens it; `Escape` closes a form, then the screen; `?` shows the legend. Registered with `useKeyboardShortcuts` (`src/hooks/useKeyboardShortcuts.ts`) in the `review` context, inactive while a text field has focus.

### 3.6 Add a source, at two levels

What exists today and he could not find: `ClaimActionKind::AttachEvidence` (`claim_reviews/attach.rs`): a link, a file from the library, pasted text or a saved passage becomes a course source (`added_by = user`), the claim is checked against it by the publication judge, and every other open or waiting claim of the lesson that the source mentions is checked against it too (`check_others`, `attach.rs:343-411`; `open_for_recheck`, `:133-135`). The report already carries the tally of those other claims (`OtherClaimsCheckedDto { tried, verified, not_verified, no_passage, stopped }`, `attach/report.rs:32-43`). Its UI is the "Add evidence" chip under the wall (`ClaimReviewCard.tsx:274`). "Research with a hint" (`ResearchWithHint`) is the other buried path.

The design makes it a first-class resolve path:

**Level 1, on a claim.** The **Add a source** button opens a sheet in the focus pane (not a modal: the claim stays visible):

```
┌ Add a source for this claim ─────────────────────────────────────────────────┐
│ Drop a file or a link here, paste an address or text, or choose a file.      │
│ ┌──────────────────────────────────────────────────────────────────────────┐ │
│ │ https://                                                                 │ │
│ └──────────────────────────────────────────────────────────────────────────┘ │
│ [Paste text instead]  [Choose a file…]  [Pick a saved source…]               │
│ ☐ Also check the 14 other claims that wait on this source's subject          │
│                                                     [Cancel]  [Add and check]│
└──────────────────────────────────────────────────────────────────────────────┘
```

The checkbox is on by default when the claim's group has other waiting claims; it sets `scope` to the group. "Pick a saved source…" lists the course's sources (the Sources tab's list, `useLearningSourceWorkspace`) and attaches a saved passage (`attach_saved_source`) chosen in the reader.

**Level 2, for a group or the queue.** The group header's **Add a source for these 15** and the queue header's **Add a source for these 33** open the same sheet with `scope: { kind: 'claims', claimIds }`; the first claim of the scope is the one the attach is recorded on (`EvidenceAttached`), the rest are the "others" (`Checked` events), exactly as `check_others` records them today, scoped to the chosen ids instead of the whole lesson.

**Drag and drop and paste.** The sheet, the focus pane and the queue accept drops and pastes anywhere (one `onDrop`/`onPaste` on the screen's root, with a visible drop ring): a dropped or pasted `http(s)` address becomes a link; dropped files use the Tauri drag-drop event with file paths and go through the same import the chat panel uses for dropped files (`src/features/chat/hooks/useChatFileDrop.ts`: the file becomes a library document, which gives the `documentId` that `attach_document` takes); pasted plain text longer than one line becomes pasted text with "Where it is from" prefilled from the clipboard's source address when one is present. A drop with nothing open opens the sheet with the item filled in and the scope set to the claim in focus.

**Progress.** The command takes a Tauri `Channel<ClaimAttachProgressDto>` (as `generate_learning_program` does, `plugin/program.rs:58`): stages `fetching` ("Reading nature.com…"), `saving` ("Saving it to your sources"), `checking` ("Checking claim 1 of 33: Before a model can call a tool…", with `done`/`total`), and `done`. The sheet shows the stage line and a bar over `done/total`; the queue rows flip to ✓ as their claims verify. No timeout: the fetch's own failure (`AttachFailureDto`) or the judge's error (`others.stopped`) ends it, and Cancel abandons the client side while the server finishes the claim it is on (the ledger is append-only; nothing half-written).

**Result.** One line in the announcement bar, from the report: "Added {title}. Verified 12 of 33 claims; 18 do not match what it says; 3 are not mentioned by it." For a single claim: "Added {title}; it states this. 32 left." / "Added {title}; it does not say this. The closest passage is shown." The closest passage (the judge's compared passages, `AttachOutcomeDto::Checked.passages`) becomes the claim's evidence view with the heading "CLOSEST PASSAGE IN WHAT YOU ADDED", and the buttons offer Edit the sentence (to what the page says) and Add another source.

**Failure states** (`AttachFailureKindDto`, `attach/failures.rs:43-58`, each with its `next_steps`): `refused` → "{host} refused the request (HTTP 403)." with "Open it in your browser, then paste the relevant text" and "Save it as PDF and add the file"; `paywall` → "{host} shows a sign-in or subscription wall." same steps; `thin` → "The page has almost no text to check." paste or file; `browser_unavailable` → "The page needs a browser to read and none is available." paste; `blocked` → "That address is private or local and cannot be fetched." paste or file; `unreachable` → "{host} could not be reached." Retry. "Nothing relevant" is not a failure: it is the `no_passage` outcome ("Nothing in {title} mentions this claim. It is saved in your sources.") with the claim unchanged.

**What wins when the page is yours.** `attach.rs:7-8` says a source the user added "is never weighted differently: it is checked like any other". That sentence is about the **judge**: the model reads the user's page like any page and says whether it states the claim; the user cannot make a page say what it does not. The **gate** is the other half: `decide` (`core/domain/provenance/sufficiency.rs:230-244`) accepts any usable support whose `added_by` is `user` as sufficient before any bar is read ("Supported by material you added; its standing is shown, not weighed"). So a page the user adds that states a claim always verifies it, whatever its standing; the checker design's fix B (user-added material levelled at least `Established`, §5.2 there) governs only what the Sources tab shows for it, how it corroborates other claims and the course's ceiling. Nothing changes in either rule; this document states the order: the judge decides whether the page says it; if it does, your source is enough. The copy deck carries it (§4.6).

**Sources added on the Sources tab.** Today `add_learning_web_source`, `add_learning_document_source` and `add_learning_text_source` (`plugin/sources.rs:67-208`) save the source and nothing checks waiting claims until the lesson is resumed, when the normal pass reads it like any source (and the gate's user rule applies if it supports a claim). The design adds the immediate check: after saving, each of the three commands calls `claim_reviews::recheck::waiting_claims(program_id, version_id)` (new, §5.3), which runs the `check_others` loop over every waiting or open claim of every lesson the course holds for review, and the Sources tab shows one line: "Checked 33 waiting claims against it: 12 verified." with "Open the review" when some were. The same progress channel reports it. A source added with no held lesson checks nothing and says nothing.

### 3.7 States

- **Loading**: the header with the lesson title and a two-column skeleton.
- **Empty (nothing waits, not yet resumed)**: the queue shows "Everything is resolved" and the focus pane says "Every claim is verified, confirmed by you, removed or edited. Resume verification to check your changes and publish the lesson." with the Resume button enabled.
- **Done (resumed; the job is running)**: the screen stays open with the status line "Checking your changes · 2m 10s" and the live tally; claims that need the user while it runs appear in the queue (today's `jobActive` behaviour).
- **In progress (job running, claims waiting)**: as the normal screen, with the status line at the top; actions work now (today's rule).
- **Held by the Verifier audit** (`reviewHold` `audit` / `audit_stop`): the focus pane shows the audit's message and fix and the Resume button; the queue is empty.
- **Stopped** (`review.state === 'stopped'`): the stop reason and fix, "Retry after fixing: …" (today's `StopReason`, kept, with its request facts behind "Details").
- **Error**: the review failed to load: the message and "Load again".
- **Published lesson with teaching notes only**: the queue has the "Teaching notes" group alone; each note shows its passage and "Keep as written".

### 3.8 Narrow widths (< 768px)

Two stacked screens, not two columns: the queue list first (grouped rows, full width); tapping a row opens the claim as a page with "← 3 of 33" at the top and the action buttons in a sticky bottom bar (two rows of large targets: Accept / Edit / Remove on the first, Accept source for all / Add a source on the second). Swiping is not implemented; Next/Previous arrows are in the top bar. No horizontal scroll at 390px (the existing e2e assertion, `e2e/lesson-claim-review.spec.ts:118-119`, is kept).

```
┌ ← Function Calling …  33 ┐      ┌ ← 3 of 33            ◀ ▶ ┐
│ Group by: Source ▾        │      │ Section 2 · Defining …   │
│ ▾ The LLM Stack · book 15 │      │ …░This is known as a tool │
│   [Accept for all 15]     │      │ schema…░ …                │
│   ● Before a model can …  │      │ ■ Waiting for you: …      │
│   ○ The `required` array… │      │ BEST EVIDENCE             │
│ ▸ AI/TLDR · article 6     │      │ … ▌structured descr…▐ …   │
│ ▸ No source states it 4   │      │ Counts as: …              │
│ ▸ Teaching notes 2        │      │ Tried: 9 searches… ▸      │
└───────────────────────────┘      ├───────────────────────────┤
                                   │ [Accept] [Edit] [Remove]  │
                                   │ [Accept source for all 15]│
                                   │ [Add a source]   More ▾   │
                                   └───────────────────────────┘
```

## 4. Copy deck

House style: plain, short, present tense, second person, no internal names. "Source" not "version"; "page" not "snapshot"; "the checker" only when it must be named ("The checker could not read this"); never "the machine", "pass", "hybrid", "bytes", "uuid", a level word, a kind word or a bar word. Examples run across subjects.

### 4.1 Why a claim waits (verdict × what is missing → chip, reason, suggested fix)

| verdict / missing (DTO) | chip | one-line reason | second line, when the facts are known | suggested fix (first button) |
|---|---|---|---|---|
| `insufficient_standing` + `no_identified_source` | Waiting for you | No source the course trusts states this. | The one passage that does is from {kind} whose author is not identified. | Accept this source · or Add a source |
| `insufficient_standing` + `no_primary_for_subject` | Waiting for you | This needs the subject's own account, and the course has none that states it. | It is stated by {kind} from {site}. (biology: "the species account states it; the nomenclature code the course has does not") | Add a source |
| `insufficient_standing` + `stronger_source_silent` (new, checker design §6.1) | Waiting for you | The course has a stronger source on this subject, and it does not state this. | {stronger title} ({kind}) was read and does not say it; {support title} does. | Accept this source · Edit the sentence |
| `insufficient_standing` + `no_independent_corroboration` | Waiting for you | Only one source states this, and this kind of statement needs a second one. | (history: "One chronicle says it; a second independent account would settle it.") | Add a source |
| `insufficient_standing` + `only_testimonial` | Waiting for you | The sources that state this are reporting what someone else said. | {title} attributes it to {attributed_to}. | Add a source (the original) |
| `insufficient_standing` + `only_stale` | Waiting for you | The sources that state this are older than the newest one on the subject. | | Add a source · Accept |
| `insufficient_standing` + `no_measurement_origin` | Waiting for you | This number needs the source that measured it. | {title} reports it without saying who measured it. | Add a source |
| `unsupported` | Waiting for you | No saved source states this. | {n} passages mention the subject; none state it. | Edit the sentence · Remove it · Add a source |
| `no_source` | Waiting for you | No saved source mentions this. | | Add a source · Remove it |
| `contradicted` | Says otherwise | A source says otherwise. | {title}: "…sentence…" | Edit the sentence · Accept (with a note) |
| `general_knowledge_proposed` | Common knowledge? | No source states this, and it looks like common knowledge. | Accept it as such, or add a source. | General knowledge · Add a source |
| `could_not_judge` | Could not check | The checker could not give an answer for this one. | | Accept · Edit the sentence |
| `disputed` (Verifier) | Disputed | An independent check disagrees with the first one. | {verifier reason} | Accept (with a note) · Edit |
| state `reopened` (cause) | Reopened | The sentence changed / Its source was deleted / … (today's `REOPEN_CAUSES`, reworded without "the Verifier") | | per the underlying verdict |

### 4.2 "What fixes it" group names (the third grouping mode)

"Accept a source" (insufficient standing of any kind), "Needs a source" (unsupported, no source, measurement origin), "Says otherwise" (contradicted, disputed), "Common knowledge?" (proposals), "Could not check", "Teaching notes".

### 4.3 Sources: kind, standing and "counts as"

Kind words (`support.kind`, §5.2), from the v2 standing's nature and publisher kind and the source's kind: `online book`, `documentation`, `reference work`, `peer-reviewed paper`, `report`, `article`, `guide`, `forum`, `wiki`, `marketing page`, `your file`, `pasted text`, `page` (unknown).

Standing words (`support.standing`), replacing `LEVEL_WORD` in the claim context (the Sources tab's trust badge keeps its own, more technical, words):

| level (DTO) | words |
|---|---|
| `primary` | the subject's own account |
| `authoritative` | an authority on this subject |
| `established` | a reliable identified source |
| `community` | a community page (forum, wiki, Q&A) |
| `unknown` | an unidentified author's work |
| `distrusted` | a source you distrust |

"Counts as" line = `{standing words}` · `{enough | not enough on its own for this claim | not counted: {reason}}`, where the second part comes from the decision: sufficient → "enough"; `BestAvailable { level }` label → "the course's strongest source on this subject · enough"; insufficient → "not enough on its own for this claim"; excluded (distrusted/misdated) → "not counted: {plain reason}".

Labels on verified claims (`sufficiency.label`), shown as a small chip on resolved rows and in the lesson's evidence panel: `practice` → "A recommendation", `attributed` → "Attributed", `community_consensus` → "Several community sources agree", `reported_by` → "As reported by {name}", `secondary` → "From a secondary source", `mirror` → "From a copy of the original", `best_available` (new) → "The course's strongest source on this".

### 4.4 Tally (course page line, review header, activity line)

Order and words: "{waiting} claims need you · {verified} verified · {confirmed} confirmed by you · {general} common knowledge · {rewritten} rewritten by the machine · {removed_by_you} removed by you · {open} being checked". A zero count is omitted; "resolved by you" is never a sum of machine work (checker design fix H). The lesson row shows only the first non-zero of: "{n} claims need you", "{n} teaching notes for you", "Ready", "Preparing · {checked} of {total} checked", "Outline".

### 4.5 Announcements

"Accepted. 32 left." · "Sentence edited. It is checked when you resume." · "Removed. 32 left." · "Added {title}; it states this. 32 left." / "Added {title}; it does not say this. The closest passage is shown." · "Trusted {title} for this course: 12 claims verified, 3 still need you." · "Undone. 33 left." · "Everything is resolved. Resume verification to publish the lesson."

### 4.6 Adding a source

| moment | words |
|---|---|
| sheet title | Add a source for this claim · Add a source for these 15 claims · Add a source for all 33 claims |
| hint | Drop a file or a link here, paste an address or text, or choose a file. A page you add is read as it is; if it says this, that settles it. |
| progress | Reading {host}… · Saving it to your sources · Checking claim {n} of {m}: {statement…} |
| one claim, stated | Added {title}; it states this. |
| one claim, not stated | Added {title}; it does not say this. The closest passage is shown: edit the sentence to match it, or add another source. |
| one claim, not mentioned | Nothing in {title} mentions this claim. It is saved in your sources. |
| many claims | Added {title}. Verified {v} of {t} claims; {n} do not match what it says; {p} are not mentioned by it. |
| stopped part-way | Checked {done} of {t}; the rest were not checked: {reason}. Add and check again to continue. |
| already in the course | {title} was already one of your sources; the claims were checked against it. |
| refused | {host} refused the request (HTTP {status}). Open it in your browser, then paste the relevant text, or save it as PDF and add the file. |
| paywall | {host} shows a sign-in or subscription wall. Open it in your browser, then paste the relevant text, or save it as PDF and add the file. |
| thin | The page has almost no text to check. Paste the relevant text, or add the file. |
| browser unavailable | The page needs a browser to read and none is available. Paste the relevant text. |
| blocked | That address is private or local and cannot be fetched. Paste the relevant text, or add the file. |
| unreachable | {host} could not be reached. Try again, or paste the relevant text. |
| Sources tab, after adding | Checked {t} waiting claims against it: {v} verified. Open the review ↗ · (none waiting: nothing) |
| standing of your source (Counts as line) | a source you added · enough, it states this |

## 5. Data

### 5.1 `ClaimReviewItemDto` additions (`lessons/claim_reviews/dto.rs`, built in `view.rs`)

```rust
/// The claim as the review shows it: plain text, in its sentence and paragraph.
pub struct ClaimPresentationDto {
    /// The checked statement, markdown and citation markers removed.
    pub statement: String,
    /// The sentence of the lesson field the statement was extracted from, plain.
    pub sentence: String,
    /// The paragraph around it, plain, split at the sentence.
    pub context_before: String,
    pub context_after: String,
    /// "Section 2 · Defining Tool Schemas", "Question 3", "Question 3 · answer key".
    pub place: String,
}
/// The best support the checker found, or the contradicting passage.
pub struct ClaimSupportDto {
    pub role: ClaimSupportRoleDto,             // supports | contradicts
    pub source: SourceRefDto,                  // exists: source_id, source_version_id, title, url, added_by
    pub site: Option<String>,                  // registrable domain, else host, else None (a file)
    pub kind: SourceKindDto,                   // §4.3 words, as an enum
    pub standing: Option<LearningTrustLevel>,  // the level for this claim's subject, when weighed
    pub counts: ClaimSupportCountsDto,         // enough | not_enough | not_counted { reason: String }
    pub quote: String,                         // the supporting (or contradicting) sentence(s), as the judge cited
    pub quote_span: Option<(usize, usize)>,    // byte span of `quote` inside `passage.text`
    pub passage: LearningEvidencePassageDto,   // exists
}
/// Research, summed up.
pub struct ResearchSummaryDto {
    pub searches: u32,                 // rounds with at least one query
    pub pages_read: u32,               // distinct source versions research saved for this claim
    pub pages_mentioning: u32,         // of those, versions whose passages the checker compared for this claim
    pub stated: bool,                  // a research-saved page supports the claim now
    pub relevant: Vec<SourceRefDto>,   // the `pages_mentioning` versions, newest first
}
```

New fields on `ClaimReviewItemDto`: `presentation: ClaimPresentationDto`, `support: Option<ClaimSupportDto>` (the strongest counted support, else the contradicting passage), `also_supported: Vec<ClaimSupportDto>`, `tried: ResearchSummaryDto`. The existing `evidence`, `research`, `repairs`, `history`, `localCheck`, `verifierCheck` stay (History and the Verifier columns read them).

How `view.rs` fills them: `presentation` from `location.quote` and the unit's field text (`claim_ledger/observe.rs::field_text`, made reachable through the review's repository) with a new pure `lattice_core::shared::text::plain_sentence_context(field_text, quote) -> (before, sentence, after)` that strips emphasis, inline code fences, link syntax and `[n]` markers (`crates/lattice-core/src/shared/text/mod.rs`, beside `normalize_whitespace`) and finds the sentence with the rule `content_verification/repair/citation.rs::sentences` uses (moved to `lattice_core::shared::text::sentences` so both crates share it). `support` from the latest check's support and the stored `TrustAssessment` (`entry.trust()`: the support's `trust.level`, its `counted` outcome in `decision.have`/`met_by`), `quote_span` by locating the supporting quote in the shown passage's text. `kind` from the source's v2 standing (`nature`, `publisher_kind`) and library `kind` via one mapping function `source_kind()` in `claim_reviews/dto/trust.rs`. `tried` from the `Researched` events and the checks' `evidence` source ids.

### 5.2 Other DTO changes

- `LearningSufficiencyDto` (`dto/trust.rs:171`) gains `missing: Option<String>` (`no_identified_source`, `no_primary_for_subject`, `stronger_source_silent`, `no_independent_corroboration`, `only_testimonial`, `only_stale`, `no_measurement_origin`), `label` gains `best_available`, and `ceiling: Option<LearningTrustLevel>` (the checker design's `Available.ceiling`). `dto_map::sufficiency` fills them.
- `ClaimCountsDto` (`claim_reviews/dto.rs:69`) follows the checker design's `ClaimCounts`: `removed` becomes `removed_by_you`, `edited_by_you`, `rewritten`. `LearningClaimTally` (`planning/curriculum.rs:232`) gains `rewritten`.
- `ClaimActionRequestDto`: unchanged. `SourceRefDto`: unchanged (it already has `source_id` for the override scope).
- `LearningGenerationJob.activity.claims`: unchanged.
- Bindings regenerated with `npm run bindings:generate`; `scripts/check-ipc-contracts.mjs` and `scripts/check-tauri-command-inventory.py` pass unchanged because no command is added.

### 5.3 The attach command and the Sources-tab re-check

- New command `attach_learning_claim_evidence(request: AttachLearningClaimEvidenceRequestDto, on_progress: tauri::ipc::Channel<ClaimAttachProgressDto>) -> ClaimAttachReportDto` in `plugin/claims.rs`, registered in `generate_handler!`, `build.rs` and `capabilities/main.json` (`scripts/check-tauri-command-inventory.py` enforces all three). `resolve_learning_claim` stops accepting the four `attach_*` variants (pre-release, no shim): `ClaimActionRequestDto` loses them, and `AttachEvidenceDto { attach_link { url } | attach_document { documentId } | attach_text { title, text } | attach_saved_source { sourceVersionId, startByte, endByte } }` carries them in the new request. `useLessonClaimReview.ts`'s `isAttachAction` routing goes away with it.

```rust
pub struct AttachLearningClaimEvidenceRequestDto {
    pub program_id: String, pub lesson_id: String,
    /// The claim the attach is recorded on; the first of `scope` when a scope is given.
    pub claim_id: String,
    pub expected_revision: i64,
    pub evidence: AttachEvidenceDto,
    pub scope: AttachScopeDto,           // claim | claims { claim_ids: Vec<String> }
}
pub enum ClaimAttachProgressDto {
    Fetching { host: Option<String> }, Saving,
    Checking { done: u32, total: u32, statement: String },
    Done,
}
```

`attach::attach` (`attach.rs:212`) takes the scope: `check_others` (`:343`) filters its candidates to the scope's ids when given, and reports progress through a new `AttachDeps.progress: &dyn Fn(ClaimAttachProgressDto)` after each claim. The report is unchanged (`ClaimAttachReportDto`, `attach/report.rs:48-63`); `others` is the summary the UI shows.

- New service `claim_reviews/recheck.rs`: `waiting_claims(deps, program_id, version_id, progress) -> WaitingClaimsRecheckDto { lessons: u32, tried, verified, not_verified, no_passage, stopped: Option<String> }`, which loads every lesson of the program whose review state is `needs_review` or `verifying` and runs the `check_others` loop over their `open_for_recheck` claims (the attached-to claim being none). `add_learning_web_source`, `add_learning_document_source` and `add_learning_text_source` (`plugin/sources.rs:67, :86, :155`) call it after saving and gain the same `on_progress` channel; `LearningSourceWorkspaceDto` gains `recheck: Option<WaitingClaimsRecheckDto>` for the line the Sources tab shows. A lesson's `changed_since_stop` (`claim_reviews/service.rs:186`) is true after a re-check that appended a `Checked` event, so Resume is enabled.

### 5.4 Mocks and fixtures

- `e2e/helpers/learningStudioBackend.ts`: the `claimReviews` fixtures (`:446-448`) and the `holdLessonForReview` path produce the new fields; `get_learning_source_trust` (`:1731`) answers `includeImpact` with an `impact` per source; `set_source_trust_override` and `clear_source_trust_override` are added to the mock: they record the override, re-decide the fixture's claims whose `support.source.sourceId` matches to `resolved_by_evidence`, and return `LearningTrustChangeResultDto` counts; `attach_learning_claim_evidence` replaces the mock's attach handling inside `resolve_learning_claim` (`:1202-1248`), honours `scope`, emits `Checking` progress on the channel for each claim, verifies the fixture claims whose statement the pasted text contains, and returns the report with `others`; the fixture's refused-link (`paywalled.example.com/403`) path is kept; `add_learning_web_source`/`_document_source`/`_text_source` return `recheck` counts; `unsupportedCommands` must stay empty (the spec asserts it).
- `lessons/review/claimReviewTestData.ts`: `claimItem()` gains the new fields with a biology-shaped default (statement "Monarch butterflies migrate up to 3,000 miles", a field-guide support, kind `online book`); `review()` gains the new counts.
- The UI rig (`~/.claude/projects/-Users-josh-Code-lattice-temp/ui-rig/`, `tauri-mock.js`) gets the same handlers and a `review-*` fixture family (§7.3).

## 6. Components

| today | decision | new / changed file |
|---|---|---|
| `lessons/review/HeldLessonsReview.tsx` | delete | — |
| `lessons/review/LessonClaimReviewPanel.tsx` | delete (its `StopReason`, `AuditStop`, resume logic move) | `lessons/review/ClaimReviewScreen.tsx` (layout, header, states, keyboard, announcements, Resume) |
| `lessons/review/ClaimReviewCard.tsx` | delete | `lessons/review/ClaimInFocus.tsx` (statement in context, reason, evidence, tried, actions), `lessons/review/ClaimQueue.tsx` (groups, rows, group-by select), `lessons/review/ClaimEvidence.tsx` (excerpt with highlight, counts-as, full passage, open), `lessons/review/ClaimActions.tsx` (the button row, forms for edit/confirm-note/hint), `lessons/review/AcceptSourceDialog.tsx` (bulk accept over `set_source_trust_override`) |
| `lessons/review/claimReviewPresentation.ts` | keep and extend: the copy deck functions (`whyItWaits(item)`, `countsAs(support)`, `kindWord`, `standingWords`, `triedLine(tried)`, `fixGroupOf(item)`, `groupBySource`, `tallyLine`) replace `verdictLabel`/`verdictMeaning`/`countsLine`/`claimTallyLine`; `groupBySection`, `isWaitingForYou`, `describeEvent`, `reviewHold`, `heldLessonIds`, `jobHoldingLesson` stay | same file (split `claimReviewCopy.ts` if it passes 400 lines) |
| `lessons/review/ClaimEvidenceAttach.tsx` | rewrite as `AddSourceSheet.tsx`: the §3.6 sheet (link / paste / file / saved source, scope checkbox, drop ring, progress bar, result and failure states), used on a claim, a group and the queue; its refusal paths and tests carry over | `lessons/review/AddSourceSheet.tsx`, `lessons/review/useAddSource.ts` (the mutation over `attach_learning_claim_evidence` with the progress channel), `lessons/review/useReviewDrop.ts` (drop/paste on the screen root, file import through the chat drop hook's path) |
| `lessons/review/ClaimHistory.tsx` | keep; under More ▾ | — |
| `lessons/review/TeachingFindings.tsx` | keep; rendered as the last queue group and in focus | small restyle |
| `lessons/review/CheckerAccuracy.tsx`, `LessonAudit.tsx`, `VerifierAuditSwitch.tsx` | keep; moved as in §2.2 | — |
| `sources/trust/ClaimSufficiency.tsx` | delete from the claim; the Sources tab's trust section keeps its own components | — |
| `lessons/LessonPreparationProgress.tsx` | rewrite as `LessonPreparationStatus` (one line + bar + Details), keeping `useReloadWhenPrepared`, `preparationState`, pause/resume/retry and the existing test names where the behaviour is the same | same file |
| `lessons/LessonCard.tsx` | the one-line row with the Review button; no inline review panel; held lesson shows read-only draft sections | same file |
| `curriculum/ProgramWorkspace.tsx` | header (title once, Goal disclosure, no revision), milestones row, collapsed outline line | same file |
| `curriculum/OutlineReviewPanel.tsx` | one-line passed state | same file |
| `workspace/StudyActivityPanel.tsx`, `workspace/LessonPreparationLine.tsx` | one line; Review link | same files |
| `workspace/LearningStudioPage.tsx` | `screen: 'review'`, `?review=claims` | same file |
| `src/lib/studyActivity.ts` | `StudyDestination.review?: 'claims'` | same file |

Primitives and tokens: `Button` (`src/components/ui/button.tsx`; primary for Accept, outline for the rest, danger variant for Remove), `Dialog` (the bulk accept and the reader), `Badge` (chips), `ScrollArea` (the queue), `Tooltip`/`IconButton` (previous/next), the `.kbd` class for the legend, `--surface` / `--surface-raised` rungs for the two panes, `shadow-sheet` for the focus pane, `.row-hover` for queue rows; `<mark>` styled with the accent at 25% alpha for highlights in both themes. No new library.

## 7. Tests

### 7.1 vitest (`src/features/learning/...`)

| file | test | asserts |
|---|---|---|
| `lessons/review/ClaimReviewScreen.test.tsx` | `opens on the first waiting claim grouped by source with counts, and moves to the next after accepting` | 33 fixture claims, 15 on one source: the first group is the source with 15 and its bulk button; Accept calls `resolve_learning_claim` with `confirm` bound to the support's version id; focus moves to the next statement; the announcement reads "Accepted. 32 left." |
| same | `shows the checked statement in plain text inside its sentence, never markdown or citation markers` | the fixture's quote holds `*`, `**`, backticks and `[5]`; the rendered statement has none and the paragraph context surrounds it |
| same | `explains why a claim waits in one plain line with no internal vocabulary` | for each verdict/missing of §4.1 the reason text matches the deck and contains none of: uuid pattern, "bytes", "hybrid", "Pass", "established", "community", "primary", "authoritative", "machine", "kind" |
| same | `highlights the supporting sentence with context, names the source's kind and site, and keeps the full passage behind a disclosure` | `<mark>` holds `support.quote`; the raw passage text is not in the document until "Full passage" is opened; it then renders without `jsonjson` fence artefacts |
| same | `sums what was tried into one line and lists only relevant pages in its details` | "Searched the web 9 times and read 214 pages; none stated it."; the details list equals `tried.relevant`; a fixture title "LinkedIn Login" is absent |
| same | `accepts a source for every claim it supports with a count and undoes it` | the dialog shows "15 claims rest on this source"; confirm calls `set_source_trust_override` with `{ scope: 'source', level: 'established' }`; the announcement names the re-decided count; Undo calls `clear_source_trust_override` |
| same | `works from the keyboard: J/K, A, E, R, S, U, Escape` | handlers fire; none fire while a textarea has focus |
| `lessons/review/AddSourceSheet.test.tsx` | `adds a link for one claim with progress, then shows the result` | the mutation is called with `scope: { kind: 'claim' }`; the `Fetching`, `Saving`, `Checking 1 of 1` stages render in order; "Added {title}; it states this." |
| same | `adds one page for a whole group and sums the result` | `scope.claimIds` equals the group's ids; the bar advances with `Checking n of 15`; "Verified 12 of 15 claims; 2 do not match what it says; 1 is not mentioned by it." |
| same | `shows the closest passage when the page does not say it` | the evidence heading "CLOSEST PASSAGE IN WHAT YOU ADDED"; Edit the sentence is offered first |
| same | `names each failure kind with its next steps` | the six kinds of §4.6, each with its buttons (paste / file / retry) |
| same | `accepts a dropped link, a pasted address, pasted text and a dropped file` | a drop of `text/uri-list` fills the address; a multi-line paste switches to text with "Where it is from" prefilled; a dropped file goes through the import and arrives as `attach_document` |
| `sources/SourcesPanel.test.tsx` | `adding a source checks the waiting claims and links to the review` | the `recheck` line "Checked 33 waiting claims against it: 12 verified." with "Open the review" |
| same | `renders the empty, done, running, audit-held, stopped and error states` | each state's words from §3.7 |
| same | `stacks the queue and the claim at narrow widths with a sticky action bar` | with `matchMedia` at 390px the queue is a list; opening a row shows the claim page with the bottom bar |
| `lessons/review/claimReviewPresentation.test.ts` | `the tally never counts the machine's rewrites as the user's` and `kind and standing words cover every DTO value` | per §4.3–§4.4 |
| `curriculum/ProgramWorkspace.test.tsx` (new) | `the Lessons tab renders the goal once, one preparation status, and no inline claim review` | the §2.2 guard; the title equals the goal → no Goal disclosure |
| `lessons/LessonCard.test.tsx` (new) | `a held lesson is one line with a Review button that opens the review screen` | `onReview(programId, lessonId)` is called; no `Claim review` region |
| `lessons/LessonPreparationProgress.test.tsx` | existing 12 tests rewritten to the status line + Details form: the same facts are reachable after opening Details; `shows a lesson held for review as waiting for the user, with its claims, never as a failure` becomes `shows a held lesson as one line with Review, never as a failure` |
| `lessons/review/LessonClaimReviewPanel.test.tsx` | deleted; each of its 12 behaviours has a home in `ClaimReviewScreen.test.tsx` above (note required to confirm a contradiction, stale revision refusal, accept-all proposals, Verifier two columns under More ▾ › History/Checks, teaching notes, keep refused) |
| `workspace/StudyActivityPanel.test.tsx` | `a finished preparation that holds claims links to the review` | |

### 7.2 e2e (Playwright, `e2e/`)

- `e2e/lesson-claim-review.spec.ts`: rewritten to the new flow: open the course → the lesson row says "2 claims need you" → Review → the screen shows the queue grouped by source → the contradicted claim shows "A source says otherwise." and the contradicting sentence highlighted → Accept (note required) → Undo → Add a source on the second (the refused-link path, kept verbatim) → narrow viewport screenshot with no horizontal scroll → Resume → the course page shows "Preparing …". The `unsupportedCommands` assertion stays.
- `e2e/lesson-preparation-repair.spec.ts`: the repair progress is reached through Details; the "visible repair progress" assertion reads the status line.
- New `e2e/claim-review-bulk-accept.spec.ts`: 5 fixture claims on one source → "Accept this source for all 5" → 4 verified, 1 still needs you (a contradicted one) → Undo → 5 again.
- New `e2e/claim-review-add-source.spec.ts`: the queue header's "Add a source for these 5" → paste text that states three of them → progress "Checking 1 of 5" … → "Verified 3 of 5 claims" → the three rows show ✓ → then the Sources tab: add a text source that states a fourth → "Checked 2 waiting claims against it: 1 verified." → Open the review → 1 left.

### 7.3 Screenshot states for the coordinator's rig

Fixture family `review-*` in `tauri-mock.js` and shots in `shots.mjs`, both themes, 1280×800 and 390×844: `review-queue-by-source` (33 claims, 15 on one book), `review-focus-insufficient` (the 7cf7155f-shaped claim), `review-focus-contradicted`, `review-focus-unsupported`, `review-focus-proposal`, `review-bulk-dialog`, `review-empty-resolved`, `review-running`, `review-audit-held`, `review-stopped`, `review-add-source-sheet`, `review-add-source-progress`, `review-add-source-result`, `review-add-source-refused`, `review-add-source-closest-passage`, `review-narrow-queue`, `review-narrow-claim`, `course-lessons-held` (the course page with a held lesson row and a collapsed preparation line), `course-lessons-running` (status line + bar), `course-header-goal` (title once, Goal disclosure). The coordinator captures these after the build; the builders do not run the rig.

## 8. The checker, re-checked against the screenshot claim

Claim `7cf7155f` (copy of 2026-10-21): statement "Before a model can call a tool, it needs a structured description of what the tool does, its arguments, and its return values." Profile: kind `general`, subject "Tool calling prerequisites" (a concept), timeless. Its only support is the course book `5e2254bb` ("Tool Use & Function Calling — The LLM Stack", `prakashkagitha.github.io`, research-added), a direct passage, levelled `unknown` (reason `unknown_publisher`). Decision: insufficient, bar `any`, missing `no_identified_source`; reason "needs one established source or better" (the static bar for a general claim is `Established`, `core/domain/provenance/sufficiency.rs:166`). Pages that state it, saved in the course: Claude Academy's "Tool schemas" (`41a78e0e`, `academy.claude.com`, standing `unknown` today), OpenAI's function-calling guide (`90527362`, judged `institutional · documentation · vendor`), Claude's "Define tools" (`b38cddc6`, unjudged). A stored ownership judgement exists for this subject: `prakashkagitha.github.io × "tool calling prerequisites" → independent`, hints `[openai.com, anthropic.com, langchain.com]`.

Under the checker design:

1. **Standing v2 on the book** (§4): the content evidence gives `name_in_domain` ("prakashkagitha" ↔ a byline or the site's author line), a numbered chapter path (`/llm-stack-book/08-agents-harness/01-…`), many headings, and citations to arXiv papers and vendor documentation (`stands_on_sources`, `structured`). The judgement either identifies the author (`Identified`, nature `Reference` or `Documentation`) → `Established` by `standing_level`, or still cannot (`Unknown`) → row 1 of the §5.3 ladder (a cited, structured reference work) → `Established`. Either way the book is `Established` for this claim.
2. **The bar**: a general claim's static bar is `Established`; `alone` (`sufficiency.rs:359`) finds the book at `Established` → **Sufficient**, no label. The claim is verified by the book alone, before any standing reach runs. The ledger's `redecided` turns the queued claim into `ResolvedByEvidence` (`replay.rs:292`). Expected outcome: **verified, not waiting for the user**.
3. If the judgement were harsher than expected (the book `Community`, row 2 or 3), the standing reach (§7) still settles it: the subject's hints put `openai.com` and `docs.anthropic.com` pages in scope (`b38cddc6`, `90527362`, `e0bcd56c`); the first standing page carries the OpenAI guide's "`description` … what the tool does, when to use it, and what it returns" or Claude's equivalent; the judge supports (the pairs eval accepted the same passages, `primary` 6/9); the OpenAI guide is `Institutional · documentation` → `Established` at least (`Authoritative` with an editorial signal, `Primary` if the ownership judgement makes OpenAI the owner of this subject, which for a generic concept it may not) → `alone` passes at the `Established` bar → verified with the OpenAI passage as the citation. And if even that failed, `best_available` (§6.1) would accept the book as the course's strongest source on the subject only if nothing stronger on the subject was read; here something stronger was read and stated it, so the claim is verified by it.

So the claim verifies on the first re-run by path 1, with path 3 as the backstop. The review screen then shows it under "Show resolved" with the chip "Verified" and "Counts as: a reliable identified source · enough". If a real run shows it still waiting, the failure is in the v2 judgement of the book (check the `work` summary and the reasons in the source's trust view) or in the hint scope (check the standing reach's log line for the claim), and the fix belongs in the checker design, not the UI.

## 9. One combined build plan

Three builders, each in its own worktree from `main` after the checker design's doc lands, each running only `cargo check -p lattice-core -p lattice-desktop` (B also `npx tsc --noEmit`); the coordinator runs every test, gate, eval and the rig once at the end.

| builder | owns | touches | must not touch |
|---|---|---|---|
| **A — checker** (`../lattice-claims-a`, branch `fix/claim-checker`) | the checker design §4–§12 (standing v2, ladder, best-available, standing reach, combine, claim-id rule, confinement, ledger counts, `Superseded.by`) | `crates/lattice-core/src/domain/provenance/**`, `crates/lattice-core/src/application/services/source_trust/**`, `crates/lattice-core/src/domain/learning/claim_review/**`, `content_verification/**`, `references/trust/judgements.rs`, `references/trust/source_view.rs`, `references/trust/repository.rs`, the migration, `research.rs` (selection note), `teaching.rs`/`teaching/passes.rs`, the evals (harness copy, `standing` order, README lines) | `claim_reviews/**` except `ClaimCountsDto` wiring (C's), `planning/curriculum.rs`, anything under `src/`, `e2e/` |
| **B — review UI** (`../lattice-claims-b`, branch `ui/claim-review`) | §2, §3, §4, §6, §7.1–§7.2 of this document | `src/features/learning/lessons/review/**`, `lessons/LessonCard.tsx`, `lessons/LessonPreparationProgress.tsx`, `lessons/LessonPreparationLine.tsx`, `curriculum/ProgramWorkspace.tsx`, `curriculum/OutlineReviewPanel.tsx`, `curriculum/PlanPanel.tsx`, `workspace/*`, `src/lib/studyActivity.ts`, `sources/trust/trustPresentation.ts` (kind/standing words), `e2e/lesson-claim-review.spec.ts`, `e2e/lesson-preparation-repair.spec.ts`, `e2e/claim-review-bulk-accept.spec.ts` | `crates/**`, `src/lib/bindings.ts` (B codes against C's published binding diff, §9.1), `e2e/helpers/learningStudioBackend.ts` (C's) |
| **C — data** (`../lattice-claims-c`, branch `data/claim-review-dto`) | §5 of this document (DTOs, the `attach_learning_claim_evidence` command with its scope and progress channel, `claim_reviews/recheck.rs` and the Sources-tab re-check) and the checker design's §17 | `lessons/claim_reviews/dto.rs`, `claim_reviews/dto/trust.rs`, `claim_reviews/view.rs`, `claim_reviews/attach.rs` (scope, progress), `claim_reviews/recheck.rs` (new), `claim_reviews/dto_tests.rs`, `claim_reviews/attach/tests.rs`, `plugin/claims.rs`, `plugin/sources.rs`, `crates/lattice-desktop/build.rs`, `crates/lattice-desktop/capabilities/main.json`, `dto/trust.rs` (`LearningSufficiencyDto`), `dto.rs` (`LearningSourceWorkspaceDto.recheck`), `references/trust/dto_map.rs`, `planning/curriculum.rs` (`LearningClaimTally.rewritten`), `crates/lattice-core/src/shared/text/` (`plain_sentence_context`, `sentences`), `e2e/helpers/learningStudioBackend.ts`, `lessons/review/claimReviewTestData.ts`, `src/lib/bindings.ts` via `npm run bindings:generate`, the rig's `tauri-mock.js` fixtures | `content_verification/**`, `src/features/**` other than the test data file |

### 9.1 Where they meet, and the merge order

1. **Day 0, before anyone codes**: C publishes the DTO shapes of §5 as a Rust stub commit (types with `todo!()`-free default fills) and the regenerated `bindings.ts`; A and B both start from that commit, so B's TypeScript compiles against the final names and A's `ClaimCounts` split lands in C's `ClaimCountsDto` by name.
2. **A ↔ C**: `ClaimCounts { removed_by_you, edited_by_you, rewritten }` (A, lattice-core) ↔ `ClaimCountsDto`/`LearningClaimTally` (C). `Decision::Sufficient.label = BestAvailable`, `Missing::StrongerSourceSilent`, `Available` (A) ↔ `LearningSufficiencyDto.{label, missing, ceiling}` (C, `dto_map::sufficiency`). `TwoPass.also_supported` (A) ↔ `also_supported` on the item (C reads `entry.trust().supports`, so it needs no new field from A). The `work` summary on `StandingAssessment` (A) ↔ `source_kind()` (C) — C's mapping takes `(nature, publisher_kind, library kind)` and treats a missing v2 row as `page`.
3. **B ↔ C**: `bindings.ts` and `claimReviewTestData.ts` (C) ↔ every component (B). The e2e mock (C) ↔ the specs (B): B writes the specs against the mock contract in §5.4; C makes the mock honour it.
4. **Merge order**: C first (DTO fields with default fills; nothing visible changes), then A (fills the sufficiency/counts facts), then B (the screen). Each is a PR to `main` ("PR-only main", no force-push, merge `main` into a branch rather than rebasing a pushed branch). The coordinator combines locally in `../lattice-claims-c` before the PRs: merges A and B on top of C, runs `cargo test` for both crates, `npm run test`, `npm run lint`, `npx tsc --noEmit`, `npx playwright test e2e/lesson-claim-review.spec.ts e2e/claim-review-bulk-accept.spec.ts e2e/lesson-preparation-repair.spec.ts`, `bash scripts/check-rust-layer-boundaries.sh`, `python3 scripts/check-sql-contracts.py`, `python3 scripts/check-tauri-command-inventory.py`, `node scripts/check-ipc-contracts.mjs`, `npm run bindings:check`, then the evals and the rig shots of §7.3, then the first-start check on a copy (checker design §13 and §10 here).

### 9.2 Order of work inside each builder

A: the checker design §16 steps 1–8 (step 8 without running). B: `claimReviewPresentation.ts` copy deck and tests → `ClaimQueue` → `ClaimInFocus` + `ClaimEvidence` → `ClaimActions` + `AcceptSourceDialog` → `AddSourceSheet` + `useAddSource` + `useReviewDrop` → `ClaimReviewScreen` states + keyboard → `LearningStudioPage` navigation → `LessonCard` row → `LessonPreparationStatus` → `ProgramWorkspace` header/milestones/outline line → `StudyActivityPanel` → `SourcesPanel` re-check line → e2e specs. C: `shared/text` helpers + tests → DTO types (including the attach request/progress/scope and `recheck`) → `view.rs` fills + `dto_tests.rs` → `dto_map::sufficiency` → counts/tally → `attach.rs` scope + progress, `recheck.rs`, the command and its three registrations, the Sources-tab calls → mock + fixtures + bindings → rig fixtures.

## 10. What changes for you on first start

Nothing in storage. No migration from this document (the checker design adds one). On first start after the merge:

- The Study activity list shows your past requests as one line each.
- The course page's Lessons tab shows "Function Calling & Tool Schemas — 33 claims need you · Review" on the lesson row and no claim text anywhere on the page; the preparation panel is one line with Details. The header shows the title once; your course's title *is* its goal, so no Goal disclosure appears.
- Review opens the new screen. Your 33 claims are grouped by source: "Tool Use & Function Calling — The LLM Stack · online book · 15" first. After the checker's first re-run (checker design §13), most of them are under "Show resolved" instead.
- The tally reads "33 claims need you · 12 verified · 74 rewritten by the machine"; "resolved by you" is gone until you resolve something.
- Nothing you did before is lost: confirmations, notes, attached evidence and history are the same ledger events, shown under More ▾ History.

## 11. Decisions needed from the owner

1. **"Accept this source for all N" as a course-wide trust of the source** (verified claims then read "verified against X, which you trust for this course"; future claims it supports verify too), rather than N separate confirmations. Confirm, or ask for N confirmations instead (then it is a new bulk `confirm` action with a bulk undo, about two days more).
2. **The default level that bulk accept sets** is "a reliable identified source" (`established`), which settles definition, practice and general claims and corroborates the rest; the dialog offers "an authority on this subject" and "the subject's own account" as choices. Confirm the default.
3. **Keyboard letters** `A E R S G U J K ?` (single keys, inactive in text fields). Confirm, or prefer modifier combinations only.
4. **Adding a source on the Sources tab re-checks every waiting claim of the course's held lessons at once** (one judge call per waiting claim that the source mentions, as the claim-level attach already does per lesson). Confirm that this should run unasked on every add, or only when the add dialog's "Check the waiting claims against it" box is on (default on).

**Answered by the owner, 2026-10-21:** all four as recommended. (1) "Accept this source for all N" trusts the source for this course, with one undo. (2) Default level `established` ("a reliable source"). (3) Single keys, inactive in text fields. (4) The add dialog's "Check the waiting claims against it" checkbox, on by default.
