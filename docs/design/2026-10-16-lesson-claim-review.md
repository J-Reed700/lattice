# Lesson claim review: transparency, human resolution, convergence

Date: 2026-10-16. Status: design, nothing built. Code read at `/Users/josh/Code/lattice-main-ro` (main `c1cd385e`, includes #68 and #71); in-flight `feat/per-chat-model` read at `/Users/josh/Code/lattice-chat-model` (`docs/design/2026-10-16-per-chat-model-override.md`). All `file:line` below are in `src-tauri/` of main unless prefixed `src/` (frontend) or `crates/`.

## 0. The rule, in one paragraph

A claim is checked by a machine at most twice per piece of evidence it has seen; after that it is the user's, with everything the machine knew laid out: where the sentence is, what it asserts, what passages were compared, why the checker said no, what research and rewrites were tried. The user resolves it with an action that is recorded, undoable and never re-litigated by the machine unless the sentence or the cited source changes. A lesson with any unresolved claim is not ready (decided by Josh, §2.1). A model request that fails the same way twice is a bug report, not an outage: the job stops with the server's words, sanitized, and the numbers that explain them. No prompt is sent that the server's real per-request window cannot hold.

## 1. Verified current state

### 1.1 The loop and why it looks like insanity

| Fact | Where |
|---|---|
| verify → research for unsupported/contradicted → recheck only claims with new evidence → repair; repeats while the issue count drops; a non-improving round escalates (`repeatedDefects` = remove the assertion); an escalated round that fails saves the best candidate and returns `InvalidInput("Repairs stopped reducing…retry to continue")` | `lessons/content_verification/repair_loop.rs:36-199, 219-241` |
| `repeatedDefects` is also set when the issue-set fingerprint equals the previous round's | `repair_loop.rs:155-158, 322-331` |
| The repair prompt gets `failedClaims` (unit, statement, verdict, reason, evidence ids), the de-duplicated evidence table, executions, `repairHistory` (rejected variants), `repeatedDefects` | `repair_loop.rs:358-397`, `repair.rs:142-144` |
| Repairs are per section, split into claim groups that fit (`source_room`/`fits` against `llm.max_context_tokens()`), down to one claim with fewer passages | `repair.rs:155-253, 358-410`; `learning/model_call.rs:104-122` |
| Any `Unverified` claim (window too small, unusable answer) fails the whole job | `repair_loop.rs:255-283` |
| The saved pending repair resumes on retry if model+prompt+schema+candidate+sources match | `repair_loop.rs:49-85, 332-370` |
| Claim receipts are keyed by policy + model + `max_context_tokens` + (unit, claim, passages); reuse is exact-input only | `content_verification/mod.rs:239-249`, `claim_reuse.rs:62-100` |
| "General knowledge" = unsupported claim whose text has no absolute and no digit-bearing token absent from its evidence; decided by word lists, never shown to the user as a decision | `general_knowledge.rs:14-27, 50-55` |
| Deeper evidence pages: unsupported claims are rechecked against the next page of their evidence while a round settles at least one | `evidence_checks.rs:700-741` |
| User exits: Pause = job cancel keeping checkpoints; Retry = new attempt from checkpoints | `src/features/learning/lessons/LessonPreparationProgress.tsx:22-26, 71-74`; `crates/lattice-core/src/shared/runtime/jobs/runtime.rs:537-541` |

The loop is a feedback loop in the narrow sense (rejected variants are fed back). It is not a converging one: the stop condition is "the lesson-wide issue count did not drop twice", so one stubborn claim can cost two full rewrite-and-recheck passes of the whole lesson, and the user's only exit reruns exactly that.

### 1.2 What reaches the UI and what is persisted

| Surface | Content | Where |
|---|---|---|
| Preparation panel | `verificationPass`, `checksTotal/Completed/Reused/Unresolved`, deeper-page counters, recent phases; no claim, reason, passage or action | `LessonPreparationProgress.tsx:44-49, 91-103`; `learning/planning/curriculum.rs:188-216` |
| `verification_pass` | +1 on every `begin_checks`, i.e. every `check_evidence` call including rechecks after research — "Pass 35" counts rechecks, not rewrites | `lessons/lesson_progress.rs:187-199` |
| Published lesson | `LearningLessonEvidenceDto` (per claim: section index, quote, statement, verdict string, reason, supporting quote, passages with `sourceVersionId` + byte span + url); only for `ready` lessons; general knowledge labelled | `lessons/lesson_evidence.rs:12-51`, `learner_view.rs:19-60`, `LessonEvidencePanel.tsx:62-121`, `LessonCard.tsx:35-41, 79` |
| Report persistence | `learning_lesson_verifications.report_json` written inside the publication transaction only; `validate` demands zero issues and every finding Supported or general-knowledge | `report.rs:36-100, 103-121`; `migrations/20261004120000_learning_lesson_verification.sql` |
| Drafts and checkpoints | `learning_generation_checkpoints(job_id, lesson_id, key)`; the draft row carries `pending_repair` and `claim_inventory` | `lessons/lesson_drafts.rs:18-33, 192-240`; `migrations/20261006000000_learning_generation_checkpoints.sql` |
| Passages open in the reader | `fromLearningEvidencePassage` → `FilePreviewModal` (web pages render the stored text as a snapshot) | `src/features/reading/model/sourceAdapter.ts:446-475`, `LessonEvidencePanel.tsx:7-11` |
| Research attempts | the search key `research-v1:` is checkpointed, but which queries ran for which claim is nowhere a user can see | `content_verification/research.rs:191-230` |
| Rejected repair variants | only inside the pending-repair checkpoint (`repairHistory`), never exposed | `repair_loop.rs:170-176, 302-320` |

### 1.3 Today's 180-second 500 loop: cause, confirmed as far as the client can see

Log `~/Library/Application Support/lattice/logs/lattice.log.2026-10-16`, lines 17548-17620:

1. 22:03:21 section 1 of 2 repaired: `evaluated_prompt_tokens=64532 prompt_ms=121055` (533 tok/s), `slot=1`, cache pinned (`pins_slots=true`, scheduler sized `slots=2 window=Some(196608)` at line 5286).
2. 22:03:21.497 section 2 of 2: `Capped the llama.cpp output reservation … requested=8192 allowed=1024 prompt_tokens=178491 window=131072` (`llm/llama_cpp/request.rs:42-56`). That figure is chars/4 × 1.5 (`request.rs:23-37`), so the raw estimate was ≈119k tokens. `measured_body` then re-counted with the server tokenizer and logged no second cap line (`request.rs:66-91`), which bounds the exact count at ≤ 122,880 (131,072 − 8,192): the request went out with a prompt of 98k–123k tokens and `max_tokens=8192`.
3. 22:06:22, 22:09:22, 22:12:22, 22:15:23: `500 server_error`, each exactly ~180 s after send. 180 s × 533 tok/s ≈ 96k tokens: the server processed one slot's worth and failed. With 2 slots over 196,608, the per-slot capacity is 98,304 unless the cache is unified; with a unified cache slot 1 still held the pinned 64,532. Either way the request exceeded what the server would hold for one request, while every client check used **131,072** (`LlamaCppLlm.config.context_window` from settings, `llm/llama_cpp/mod.rs:88`, reported by `max_context_tokens` `mod.rs:359` and `scheduler/scheduled.rs:212-213`, used by `model_call::fits` `model_call.rs:104-122`). The scheduler knows 196,608 but only caps a reservation at the whole window (`scheduler/mod.rs:204-208`) and only gates when another request is in flight (`mod.rs:292-294`); it never derives a per-request ceiling.
4. The server's message was discarded: `generation_error` keeps a fixed sentence for three known strings and nothing else (`llm/llama_cpp/streaming.rs:80-96`); the SSE path marks any 5xx inside the stream retryable (`streaming.rs:148-154`), `attempt` likewise (`retry.rs:203-206`); 5 attempts (`retry.rs:117-178`); then `lesson_failure` defers every `ServiceNotAvailable` without limit (`learning/planning/generation_recovery.rs:23-35`, `crates/lattice-core/src/shared/runtime/jobs/mod.rs:125-132, 158-174`), resuming the same pending repair (`repair_loop.rs:49-85`) with the same 178,491-token estimate at 22:19, 22:37, 22:52, 22:58 (log lines 17570-17617).

Confirmed: the cause is a prompt sized against the configured 131,072-token window and sent to a server whose per-request capacity is smaller, a floor that sends the request anyway, a message that is thrown away, and a retry policy with no notion of "identical request, identical failure". Not recoverable from the log: the server's exact words. The design makes all four impossible to repeat.

### 1.4 Worse than the brief

- The per-request ceiling defect is in the llama.cpp client, so every consumer (chat turns, research, outlines) on any multi-slot server has it; the log shows the same ≥131k-token repair prompts since 05:22 (lines 8083, 13809, 13822, 17238-17277).
- When such a prompt *does* fit, the 1,024-token floor truncates a section rewrite; the JSON is unusable and the retries spend the budget again (`request.rs:42-45`, `retry.rs:241-247`).
- Learning web references are never frozen as `web_page_snapshots`; only chat citations are (`conversation/di/chat_runtime.rs` is the only non-web caller of `web_page_snapshots`). The reader shows the captured text (`learning_source_versions.full_text`), not a frozen page.
- The publication gate refuses when the active source set changed during verification: "The reference collection changed during verification. Prepare the lesson again." (`report.rs:103-110`). Any evidence added during a run would fail the lesson rather than be used.
- "General knowledge" is a regex decision that ships unsupported claims with a label (`general_knowledge.rs:14-27`). Under "no factually incorrect information" that is a human decision (§13, D1).
- `preparation` is `CHECK(preparation IN ('outline','ready'))` (`migrations/20260930020000_learning_studio.sql:15`); adding a state means a table rebuild with CASCADE children (`learning_blocks`, `learning_questions`), which sqlx migrations cannot do safely. §7.1 avoids it.
- The job status set is also a CHECK (`migrations/20260930080000_learning_curriculum_jobs.sql:91-93`); "needs review" cannot be a job status. §6.1 avoids it.
- The Pass counter counts every recheck (`lesson_progress.rs:187-199`); 35 passes were mostly research rechecks and resumed repairs of one section, not 35 rewrites.

## 2. Decisions taken by Josh (not open)

### 2.1 BLOCKED

A lesson with any unresolved claim is not publishable. Unresolved claims leave the lesson in **Needs your review**, a preparation outcome, not a failure. Each claim must end in exactly one of: **verified** (machine, against a saved source), **confirmed by you**, **removed**, or **edited and re-verified** (§5). Code examples that could not run and claims the checker could not judge are claims in this sense. End state, copy, Retry and Resume: §6.

### 2.2 User-supplied evidence on every unverified claim

Each open claim accepts evidence from the user: a link (fetched with the existing web fetch and browser-reader fallback, frozen as a snapshot), a file (ingested through the existing import and indexing pipeline), or pasted text (included, with "text you pasted" provenance, §5.4). The source is indexed, the claim's passages are retrieved from it, the claim is re-verified by the same checker, and the other open claims the source might cover are checked too. A supported result resolves the claim with that citation. A user-supplied source is never a bypass. Full specification: §5.4.

### 2.3 Also decided (2026-10-16, later the same day)

D1-D5 of §13 as recorded there; a Verifier audit by a cloud-capable model before publishing, opt-in per course (§14); the local checker's accuracy measured from human verdicts and audit disagreements (§15).

## 3. Domain model (lattice-core)

New module `crates/lattice-core/src/domain/learning/claim_review.rs` (+ `mod.rs` export). Pure types and rules, no I/O.

```rust
pub struct ClaimId(pub String);                       // uuid, assigned at first extraction

pub enum UnitKind { Block, Question, AnswerKey }      // AnswerKey = the explanation/correct index of a question
pub struct ClaimLocation {
    pub unit_index: usize,                            // the report's unit index (blocks then questions)
    pub unit_kind: UnitKind,
    pub field: String,                                // "body" | "title" | "rubric/2/description" | "prompt" | "options/1" | "explanation"
    pub quote: String,                                // the exact lesson text the claim was extracted from
    pub span: Option<(usize, usize)>,                 // byte span of `quote` in `field`; None if the quote occurs more than once
}

pub enum VerdictKind {                                // honest wording, one per case
    Unsupported,            // evidence was compared, none supports it
    Contradicted,           // a passage says otherwise
    NoSource,               // retrieval found no passage for the claim's terms in any saved source
    CouldNotJudge,          // the checker could not answer: window, unusable answer, runtime missing (code)
    GeneralKnowledgeProposed, // machine proposes the label; the user accepts or not (D1)
    Disputed { local: Box<VerdictKind>, verifier: Box<VerdictKind> }, // the Verifier audit disagrees (§14.5)
}

pub struct EvidenceSeen { pub source_version_id: String, pub start_byte: usize, pub end_byte: usize, pub text_sha256: String }
pub struct EvidenceFingerprint(pub String);           // digest(policy, checker model, sorted EvidenceSeen)

pub enum ClaimState {
    Open { kind: VerdictKind, reason: String, machine_attempts_left: u8 },   // machine may still act
    Queued { kind: VerdictKind, reason: String },                             // waiting for the user; kind may be Disputed { local, verifier } (§14.5)
    ResolvedByEvidence { source_version_id: String, supporting_quote: String }, // machine-verified
    Confirmed { at: i64, note: Option<String>, bound: HumanBinding },        // human verdict
    Removed { at: i64, by: Actor },                                           // sentence/claim deleted
    Edited { at: i64, successor: Option<ClaimId> },                           // text changed; successor claims own the state
    GeneralKnowledge { at: i64, by: Actor },                                  // accepted label
    Reopened { cause: ReopenCause, previous: Box<ClaimState> },
}
pub enum Actor { Machine, User }
pub struct HumanBinding { pub quote_sha256: String, pub source_version_ids: Vec<String> }
pub enum ReopenCause { SentenceChanged, SourceDeleted(String), SourceReplaced { from: String, to: String }, Unconfirmed, VerifierDispute }
pub enum Checker { Local { model: String }, Verifier { model: String } }   // §14: both write Checked events

pub enum ClaimEvent {   // append-only audit, one row each
    Extracted { location }, Checked { checker: Checker, kind, reason, fingerprint, evidence, supporting_quote },
    Researched { queries: Vec<String>, sources_added: Vec<String>, passages_found: usize },
    RepairAttempted { attempt: u8, escalated: bool, before: String, after: Option<String>, outcome: RepairOutcome },
    Executed { observation_id, passed, evidence },
    UserAction { action: UserAction, note: Option<String> },
    EvidenceAttached { source_version_id, provenance: Provenance, result: Box<ClaimEvent> },
    Reopened { cause }, Undone { event_ordinal: u32 },
    Stopped { reason: StopReason },   // §8.4
}
```

**Rules (pure functions, unit-tested):**

- `new_information(prev: &ClaimLedgerEntry, now: &Observation) -> Option<NewInformation>` returns `Some` only for: a different `EvidenceFingerprint` (new passages retrieved, including from a user-added source), a changed `quote` (the sentence changed), a cited source's active version changed or deleted, a different checker model name or request window, a user hint for research, a user-attached source. Anything else is not new.
- `may_repair(entry) -> bool`: `Open` with `machine_attempts_left > 0` and kind ∈ {Unsupported, Contradicted}. `NoSource` and `CouldNotJudge` never repair; they research once, then queue.
- `after_machine_failure(entry, fingerprint)`: same fingerprint → decrement attempts; 0 → `Queued`. New fingerprint → attempts reset to 2 (one correction, one removal) because the machine has genuinely new material.
- `relitigate(confirmed, change) -> bool`: true only for `SentenceChanged` (the bound `quote_sha256` no longer occurs in the unit field), `SourceDeleted`/`SourceReplaced` of a bound source id, or `Unconfirmed`. A checker model change, policy change, new research, or new sources never reopen a human verdict.
- `identity(prev_claims, new_claim) -> ClaimId`: same unit index and (same statement or same quote) → the prior id; else, if the prior quote still occurs in the unit field and the two quote spans overlap → the prior id (largest overlap wins); else a new id. Prior ids with no successor in a re-extracted unit become `Edited { successor: None }` → shown as "removed by the edit".

Port `crates/lattice-core/src/application/ports/claim_review.rs`: `ClaimReviewStore { load(lesson_id) -> Ledger; append(lesson_id, claim_id, event, expected_revision) -> Result<Revision>; set_lesson_review(lesson_id, LessonReviewState) }`. Implemented in the learning feature (§7).

*As built (track L0):* `domain/learning/claim_review.rs` (types) with `claim_review/{ledger.rs, replay.rs, rules.rs}`. Where the sketch above could not hold, the code differs:
- `ClaimState::Unchecked` (extracted, not yet checked; the machine's) gives every claim a state from its first event.
- `VerdictKind::Disputed { verifier }` has no `local`: `VerdictKind` cannot spell "supported", and the local side of a dispute is always "verified by the local checker", because a human verdict the Verifier disputes (a confirmation, or accepted general knowledge) is reopened as `Reopened { VerifierDispute }` instead (§14.5).
- `Checked(Check)` carries `result: CheckResult { Supported { source_version_id, supporting_quote } | Failed { kind } }` in place of a flat kind and optional quote, plus `window_tokens` (a different window is new information). `Extracted` also carries the `statement`. `EvidenceAttached.result` is `AttachOutcome { Checked(Check) | NoPassage }`. `Executed` is a record only: code claims are judged by the `Checked` event over their execution passages, as today.
- New event `Superseded { successor }`: re-extraction's successor, or `None` for a claim whose sentence went with an edit (the "become `Edited { successor: None }`" above needed an event). A user edit is `UserAction::Edit` (→ `Edited { successor: None }`), then `Superseded` once re-extraction names the successor.
- `RepairOutcome` is `Rewritten | Removed | Rejected { reason } | SupersededByUser`. `after_machine_failure` is the ledger's replay, not a public function: a rejected repair uses an attempt at once, a rewrite uses one at the check that follows, and a user-superseded repair is not charged (§9); `RepairAttempted` is refused unless `may_repair` holds and `attempt`/`escalated` match the attempts left.
- Every event is validated against the state it lands on (actor per kind, §5.1 preconditions, note on a contradiction, undo target), so a log that loads is a log the rules allowed. Undo returns to the state before the undone event; undoing an `Undone` redoes it; undoing the machine's removal of a sentence queues the claim (§4).
- A claim waiting for the user stays the user's: a recheck on new information may verify it or update its reason, never return it to automatic repair (§5.1's "no automatic repair" after user research or evidence, applied to every queued claim).
- `relitigate(bound, change) -> Option<ReopenCause>` returns the cause to record; `reopen_cause(state, change)` adds §5.3's reopen of a machine verification whose source was deleted or replaced. `identity` is `identities(prior, extracted, field_text)` over a whole re-extraction, so no prior id is given twice (`claim_id` is a primary key); a new claim gets `None` and `ClaimId::generate()`.
- Also in the domain: `available_actions`/`allows` (§5.1's preconditions), `may_research`, `next_repair_escalates`, `confirm_requires_note`, `end_state` (BLOCKED, §6.1) and `ClaimCounts`.
- Port: `append(lesson_id, claim_id, provenance, event, expected_revision)`; `set_lesson_review(lesson_id, LessonReview { program_id, job_id, state, draft_sha256, checker_model, stop_reason }) -> Revision`, since the row needs more than its state; `load` returns `Ledger { review, revision, claims }`.

## 4. Convergence: the rewritten loop

`verify_and_repair_with_references` (`repair_loop.rs:36-199`) keeps its shape; the lesson-wide issue count and `RepairProgress` (`repair_loop.rs:202-241`) are replaced by the per-claim ledger.

```
loop:
  report = verify(candidate)                                   # unchanged mechanics, receipts reused
  ledger.observe(report)                                       # identity match, fingerprints, states
  research once per (claim, fingerprint) for Open{Unsupported|Contradicted|NoSource}   # research.rs:156-190
  if research added passages: recheck only those claims        # claim_reuse keeps the rest
  repairable = ledger.open().filter(may_repair)
  if repairable.is_empty():
      if ledger.open().is_empty() and no Queued: publish
      else: return NeedsReview(ledger)                         # §6.1, never an error
  candidate = repair(repairable grouped by section; escalated iff attempts_left == 1)   # repair.rs
  ledger.record_repair_attempts(before/after per claim)
  # the next verify re-extracts only sections that changed (section_checkpoints.rs reuses the rest)
```

Properties:
- **Bounded:** each repair round consumes one attempt of every claim it targets; a claim gets 2 attempts per fingerprint; a fingerprint changes only when evidence changes, which research does at most once per claim per fingerprint. Rounds ≤ 2 × (claims that ever failed) + research rounds, and every round either closes a claim or moves it to the user.
- **No re-litigation:** a claim that failed twice on the same evidence is `Queued`; the machine never touches it again without `new_information`.
- **Could-not-judge is not fatal:** `unchecked_claims` (`repair_loop.rs:255-283`) is deleted; those claims are `Queued { CouldNotJudge, reason }` with the cause text the checker produced (window, unusable answer).
- **Contradictions still get rewritten first**, as today (`repair_loop.rs:131-147`), but only within the attempt budget.
- **Escalation** survives as attempt 2 (`repeatedDefects`), per claim rather than per lesson. A removal that deletes the sentence closes the claim as `Removed { by: Machine }`, visible under "Resolved by the machine" so the user can object (undo restores the sentence and queues the claim).

**Second-opinion check by another model before queueing: decided, never automatic (D5).** Reasons: it needs a second resident model (memory admission, per-chat-model §5.4); its verdict is already "new information" by the receipt key (`mod.rs:239-249` keys on model), so it costs nothing to add later; and the two cases that reach the queue, "nothing in the sources says this" and "the source says otherwise", are not ambiguity a second model resolves — they are source gaps or author errors the user is best placed to settle. It becomes a manual card action, "Get a second opinion", **LATER**, built on the Verifier role (§14) and running the audit for that one claim. The lesson-level audit of §14 is the only machine second opinion in this change, and it runs after the local pipeline and the user have resolved everything, not instead of them.

*As built (track L1):* `content_verification/repair_loop.rs` runs the loop above over `claim_ledger.rs` (with `claim_ledger/observe.rs`), `review_pass.rs` and `support_gate.rs`; `passes.rs` holds `verify_with_references`/`check_evidence`, moved out of `mod.rs` for length. `RepairProgress`, the issue-set fingerprint, the best-candidate stop and `unchecked_claims` are gone. Where the code differs from the sketch:
- **A check is recorded** only for a new claim, the check that decides a repair, or `new_information` (incl. a cited version no longer active, as `SourceChanged(Deleted)`); unchanged claims are judged from receipts, not the model. Settled claims are not filtered out of the pass: their checks are receipt reuses unless the evidence changed, which §3 counts as new information, and the domain never lets a check move them. A confirmation is reopened by `SentenceChanged` only when its old quote is gone from its field, not when re-extraction merely picked another quote.
- **Deviation (domain, `replay.rs`):** the check that decides a rewrite uses the attempt even when the rewritten sentence retrieved other passages. What the machine's own rewrite brings is not new information; otherwise every rewrite would restore the budget and a claim the machine keeps rewriting would never reach the user, against "Bounded" above. Only an uncharged check on new evidence (research, the user, a source change, another checker) restores it. L0's `new_evidence_restores_the_budget_and_the_same_evidence_does_not` is updated to say so.
- **Identity** compares a prior claim's span in the text it was extracted from (`Pending.previous`, the lesson before a repair or the user's edits) with the new claim's span in the new text. A repaired claim the rules lose (its statement changed and its sentence has no single span) continues as the unmatched new claim its repair left in the same field; with none, the repair removed it (`RepairAttempted { Removed }`).
- **Repair outcomes** are recorded by the pass that shows them: `Rewritten` (then the check), or `Removed`. The repairs awaiting that pass are saved as the lesson checkpoint `claim-ledger-v1:pending`, so a restart still charges them. A section request whose answer cannot be used (`InvalidInput`: schema, wrong section count, does not fit) is `Rejected` and the loop goes on; an outage or cancellation still propagates. Before each section the ledger is read again: a claim the user resolved is withdrawn (`SupersededByUser`, not charged), also from a resumed pending repair (§9 Restart).
- **Escalation** is per claim: each `failedClaims` entry carries `claimId` and `repeatedDefect`; a request's `repeatedDefects` is true when any of its claims or failed examples is on its last attempt.
- **Research** runs for claims that `may_research`, plus claims with an unresearched user hint; hints are searched verbatim before the planned queries. Every researched claim gets `Researched { queries, sources_added, passages_found }`, also when research found nothing new (an empty plan, or no web: no queries, no passages), so it is not researched again on that evidence.
- **General-knowledge proposals** are queued at once, never researched or repaired (D1). A finding publishes only when supported or carrying the user's `resolution` (`general_knowledge::Resolution`, set from the ledger by `ClaimLedger::bind`).
- **A failed example is a claim** ("The code example `<id>` runs to completion with the output it shows.", quoting its code), checked over its run; a confirmed failed example is listed in the report's `confirmed_examples`.
- **P4 seam:** `support_gate::SupportGate::judge` is the one place a supported verdict becomes `ResolvedByEvidence`; `CheckerVerdict` (today's rule) is the default, and an `Insufficient { reason }` makes the claim unsupported with that reason.

## 5. Human resolution

### 5.1 Actions

All actions go through one command, `resolve_learning_claim(request)`, with `expected_revision` on the lesson review (optimistic concurrency, §9). Every action appends a `UserAction` event; `undo` appends `Undone` and restores the prior state (the ledger is append-only; state is replayed).

| Action | Preconditions | Exact effect | What the machine does next | Learner-visible result |
|---|---|---|---|---|
| **Confirm supported** (+ optional note) | state Queued/Open/Reopened; for `Contradicted` (local or Verifier, D3) a note is required and is shown with the claim and kept in its history | `Confirmed { bound: quote sha + the source ids the user picked (may be none) }` | nothing; never re-litigated unless §3 `relitigate` or a §14 Verifier dispute | "Confirmed by you · 16 Oct" + note; counts separately (§5.6) |
| **Attach evidence** (saved source/passage, link, file, pasted text) | any open state; also allowed on Confirmed (adds a citation) | §5.4 pipeline; event `EvidenceAttached` with the check result | checks this claim and other open claims against the source; no automatic repair | if Supported: "Verified against <title>" with the passage |
| **Edit the sentence** | any state; edits the lesson field at `location.field` | writes the draft (`text_edits` semantics: before/after substring, `lessons/text_edits.rs:8`), re-extracts claims for that unit only (`section_checkpoints.rs`), old claims → `Edited { successor }`, successors checked against evidence | the successors are Open with fresh attempts; teaching review for the unit reruns at Resume | the new text |
| **Remove the claim** | any state | removes `quote` from the field if it is a whole sentence; else opens the editor pre-selected (the user must say what goes); re-extracts the unit | successors (if any) checked | sentence gone |
| **Mark as general knowledge** | Queued/Open with kind Unsupported/GeneralKnowledgeProposed; never Contradicted | `GeneralKnowledge { by: User }` | nothing | "General knowledge · not checked against a source" (as today) |
| **Research again with a hint** | any open state | event `Researched` with the user's hint as an extra query (`research.rs:230` `plan_queries` gets the hint verbatim as one query) | retrieval → fingerprint → recheck; if still failing: Queued with the new reason (no automatic repair after a user-driven research) | — |
| **Undo** | the last user event on this claim | `Undone`; state replays to before it; a draft edit is reverted by applying the inverse edit if the field is unchanged since, else refused: "The section changed after this edit; undo it by hand." | — | — |
| **Unconfirm** | Confirmed | `Reopened { Unconfirmed }` → Queued | — | label removed |
| **Accept all general-knowledge proposals** (bulk) | ≥1 GeneralKnowledgeProposed | one `UserAction` per claim | — | §13 D1 |

Provenance: every event stores `actor`, `at`, `job_id` (if a job was running), the app version and the checker model at the time. Human verdicts carry the user's note verbatim and are exported as such.

### 5.2 Audit trail, backup, export

- Audit: `learning_lesson_claim_events` (§7.1), append-only, shown on the card under "History".
- Backup: the encrypted backup archives the whole SQLite database; the two new tables ride along (no change).
- Learning packs: `portability/pack_repository/export.rs` exports lessons, verifications and source versions (`export.rs:239, 313`); add the ledger and events per lesson and remap claim/source ids on import next to `remap_archived_ids` (`content_verification/portability.rs:27`). A confirmed claim imports as confirmed, with its note and "confirmed on another device" is not a distinction we make: the actor is the pack's author.

### 5.3 Source deleted after a human confirmation (decided, D2)

`delete_learning_source` (`learning/plugin/sources.rs:264-276`) soft-deletes (`deleted_at`). Before deletion the confirm dialog says: "This source supports N verified claims in M lessons. Deleting it reopens them for review." After deletion: claims whose `ResolvedByEvidence.source_version_id` or `Confirmed.bound.source_version_ids` belong to that source → `Reopened { SourceDeleted }` → Queued; the lesson's review state → NeedsReview; a published lesson stays readable (its content was verified when published) but shows "N claims lost their source — review" at the top of the lesson card and the learner view marks those claims "source removed". A confirmation bound to no source is untouched (the user said it was true, not that this source proved it).

### 5.4 User-supplied evidence (decided, §2.2)

**Inputs and the pipelines they reuse**

| Input | Reused pipeline | Snapshot | Notes |
|---|---|---|---|
| Link | `add_learning_web_source` (`learning/plugin/sources.rs:41-53`) → `source_workflows::add_web_source` (`learning/references/source_workflows.rs:10-22, 81`) → `fetch_reference_content` (`web/services/web.rs:424-430`) → `read_page_as(url, PageRead::Reference)` (`web/services/web/page.rs:161`) which falls back to the hidden-webview `BrowserReader` (`page.rs:325-328`) | **new linkage**: after the version is saved, request `capture_web_page_snapshot` (`web/snapshots/commands.rs:16`, job `web/snapshots/job.rs:218`) and store `snapshot_hash` on the version (§7.1); the reader opens the frozen page through the existing `lattice-snapshot://` path; until the capture is ready the reader shows the captured text as today | the thin-page refusal (`web.rs:426-428`) stays and becomes a card message (below) |
| File | the library import pipeline the Sources tab already uses (`LearningDocumentPicker.tsx` → library import), then `add_learning_document_source` (`sources.rs:57-123`) → `sources::capture_document` (`learning/references/sources.rs:90`) from `container.library_passages()` | n/a (the file is in the library) | the claim card's "Add a file" opens the same picker; a file not yet imported is imported first and the card shows the import's own progress |
| Pasted text | `add_learning_text_source` (`sources.rs:124`), kind `pasted` | none | included; provenance "text you pasted" |

**Provenance.** `learning_source_library.added_by` (new column, §7.1): `user` | `research` | `outline` | `import`. Today research and the user both write `operation_kind: "add_web"` (`research.rs:310-320`, `source_workflows.rs:81`), so `operation_kind` cannot tell them apart. Sources list (`src/features/learning/sources/SourcesPanel.tsx`) shows a chip: "Added by you", "Found by research", "Pasted by you". The learner evidence panel shows the same chip on the passage. Pasted text additionally reads "Pasted text has no public provenance; it is checked like any other source." Decided (D4): pasted text is included. The user is the author of the lesson's truth claims either way (they can Confirm with no source at all); pasted text at least records *what* convinced them, and the checker still has to find entailment in it. It joins the program's references and is reusable by other lessons, same as a link.

**Dedupe.** Same resolved URL → reuse the existing source (offer "Refresh" if the live page differs; `refresh_learning_source`, `sources.rs:171`); same `content_sha256` → reuse the existing version (`learning_source_versions` UNIQUE(program_id, source_id, content_sha256)); a library document already captured for this program → reuse. The card says "Already in your references as <title>; checking against it."

**Index and retrieve for that claim.** `ReferenceCollection::reload` (`lessons/reference_collection.rs:299-306`) rebuilds the collection over the active versions (library vectors for documents, keyword/snapshot passages for web/pasted; `reference_collection.rs:228-283`). New `retrieve_from(source_version_id, query, fits)` restricts `verification_page` (`reference_collection.rs:416-421`) to one version. Query = the claim statement; the check is `judge_for_publication` (`content_verification/mod.rs:521`) under the strict policy, exactly as in a pass; the receipt is saved like any other (`claim_receipt_key`), so a later pass reuses it.

**Other open claims.** After the targeted check, every other open claim of the lesson runs `retrieve_from` against the new version; claims with ≥1 passage are checked (bounded by open claims, one request each, batched as `evidence_checks.rs:557-560` batches). The card reports: "Also checked 4 other open claims against this source: 1 verified, 3 no passage."

**Outcomes.**
- Supported → `ResolvedByEvidence`; the unit's `source_ids` (`learning/dto.rs:66`) gains the source id so the citation line shows it; no rewrite (Supported means the sentence as written is entailed). If the supporting quote was found but the sentence's citation marker differs (lessons cite by index in `source_ids`), the minimal repair is the citation update only, applied as a `text_edits` edit, never a model call.
- Unsupported/Contradicted → stays Queued, reason replaced by the new one, the exact passages compared shown with "from the source you added".
- No passage → "No passage in <title> mentions this claim's terms. The source is in your references; the claim is unchanged." (kind stays what it was; event recorded).

**Fetch failures (honest messages, next step).** All from the existing fetch errors, mapped on the card:

| Failure | Message | Next step offered |
|---|---|---|
| HTTP 401/403/429 | "<host> refused the request (HTTP 403)." | "Open it in your browser, then paste the relevant text" (pasted text input prefilled with the URL as origin); "Save it as PDF and add the file" |
| Paywall / login wall (browser reader returned a page with little article text) | "The page needs a login or subscription; only N words of text could be read." | same two |
| Thin/empty extraction (`web.rs:426-428`) | the existing text: "This page did not provide enough static reference text…" | same two |
| Browser reader unavailable (`page.rs:328`) | "The page needs a browser to render and the in-app browser is not available right now." | retry; paste; file |
| SSRF-blocked URL (`web/traits/web.rs:128-141`) | "That address is private or local and cannot be fetched." | file / paste |
| Snapshot capture failed | source saved, check runs on the text; card notes "Frozen copy not captured: <reason>; the text was saved." | "Capture again" |

**Adding while the job runs.** Allowed. The attach is an action on the ledger (§9): the source is saved and the targeted check runs in the command's own task (it is one or a few model requests through the same scheduler, priority Interactive); the running job sees the new state at its next sync point. A supported result closes the claim; the job's next pass skips it. The publication gate's "collection changed" error (`report.rs:103-110`) is replaced by: recheck only claims with new evidence from the added sources (the `research` path already does exactly this reload-and-recheck, `research.rs:323-330`), then re-run the gate.

**Irrelevant / contradicting source.** Irrelevant → "no passage" above. Contradicting → `Contradicted` with the passage; the user sees it and chooses Edit, Remove, or Confirm (note required). A source the user added is never weighted differently by the checker.

**Backup and export.** Sources are in the DB (backup) and in packs (`export.rs:239, 313`); `added_by` and `snapshot_hash` are new columns exported alongside; the snapshot file itself is exported by the existing snapshot export path if present, else the text stands.

### 5.5 Code claims

Executions (`content_verification/execution.rs:147-170`) are shown per claim: language, command, exit code, stdout/stderr (bounded), "could not run: no <language> runtime on this machine" (`execution.rs:156, 293`). A failed example is a Queued claim of kind `CouldNotJudge` (no runtime) or `Contradicted` (ran, wrong output); the user's actions are the same; "Run again" is a card action when a runtime appears.

### 5.6 Confirmed vs machine-verified in the learner view

`LearningClaimEvidenceDto` gains `resolution: 'verified' | 'confirmed_by_you' | 'general_knowledge' | 'source_removed'` and `note`. `LearningLessonEvidenceDto` gains `confirmed_count`. The panel shows "Confirmed by you · 16 Oct · <note>" in a neutral chip (not the emerald "verified" chip), and the lesson header line reads "12 claims verified against sources · 3 confirmed by you · 2 general knowledge". The distinction is visible, never hidden, never alarming.

*As built (track L1):* `resolution` is a string (`verified`, `confirmed_by_you`, `general_knowledge`, `source_removed`), with `note`; `LearningLessonEvidenceDto` gains `confirmedCount`. `src/lib/bindings.ts` is not regenerated here (track S/I).

## 6. End states, the Pass counter, Retry and Resume

### 6.1 End states

| Outcome | Job | Lesson | Shown |
|---|---|---|---|
| All claims closed by machine or user | `Completed` (publication transaction as today) | `preparation='ready'` | "Ready" |
| All local claims closed, course opted in, audit running | job running, phase `audit` | review row `state='verifying'`, audit row `running` | "Auditing with <model> · batch 3 of 7 · 48,200 tokens sent" |
| Audit disputes ≥1 claim | `Completed { result_ref: "needs_review:…" }` | review row `needs_review` | "Needs your review · N claims disputed by the Verifier" |
| ≥1 claim Queued, no machine work left | `JobOutcome::Completed { result_ref: "needs_review:<lesson_id>", message: "Needs your review: N claims" }` — not a failure; the draft is in checkpoints; `learning_lesson_reviews.state='needs_review'` | `preparation='outline'` (unchanged) + review row | "Needs your review · N claims" on the lesson card and the panel |
| Deterministic model failure (§8) | `Failed { code: "model_rejected" }`, `stop_reason_json` on the review row | review row `state='stopped'` | "Stopped: <cause>. Fix: <fix>." + Retry |
| Outage | deferred as today, but §8.3 bounds it | — | "Waiting to retry after a service outage (attempt 3, last error …)" |
| User paused | cancelled, checkpoints kept | — | as today |

UI copy for Needs your review (panel header): **"Needs your review"** — "Everything the lesson says was checked against your references. N claims could not be verified or were contradicted; they are listed below with what was compared. Resolve each one, then choose **Resume verification**. The lesson becomes ready only when every claim is verified, confirmed by you, removed, or edited and re-verified."

### 6.2 Resume and Retry

- **Resume verification** (replaces Retry while a review row is `needs_review`; enabled when ≥1 claim changed state or gained evidence since the job ended): `retry_learning_generation_job` as today (new attempt, `runtime.rs:537-541`); the attempt loads the draft and the ledger; it runs a *review pass*: changed units are re-extracted and checked; `Confirmed`/`GeneralKnowledge`/`Removed` are skipped; still-Queued claims stay Queued; if anything is still open the job ends `needs_review` again with the updated list; else teaching review for changed units (`teaching_review.rs`), coverage audit for changed units, then the Verifier audit if the course opted in (§14.3; changed claims only), then publication. Checkpoints resolve through the retry chain (`jobs/repository.rs:184` already hides superseded attempts; the draft lookup must follow `retry_of_job_id` — verify at build time, the `lesson_drafts` base-revision mechanism at `lesson_drafts.rs:72-79` is the hook).
- **Retry** (review row `stopped` or job `failed`): reruns from checkpoints as today, carrying the ledger, so no claim is re-litigated. The button copy names the stop cause's fix: "Retry after fixing: <fix>".
- **Resume with nothing changed**: the button is disabled with "Nothing changed since the last pass. Resolve a claim or add evidence first." There is no path that reruns the same inputs.

### 6.3 The Pass counter

`verification_pass` increments only when a *new candidate text* is verified (after a repair or a user edit), in `verify_and_repair_with_references` before `verify`, not in `begin_checks`. Research rechecks and deeper-page rounds are sub-steps shown as "checking against new evidence" inside the pass. Display: "Pass 3 · 2 claims being repaired · 5 waiting for you · 108 verified". The bound is stated in the panel's help text: "Each pass fixes or hands over at least one claim; the machine gives up on a claim after two tries on the same evidence."

*As built (track L1):* when nothing is left for the machine and a claim is open, the loop saves the draft, writes the review row `needs_review` and unwinds with `content_verification::claim_ledger::needs_review`; `generation_jobs.rs` reads the review row (`held_for_review`), holds the lesson, prepares the others, and publishes them with `publish_prepared(…, held)`; with every lesson held the job returns `JobOutcome::Completed { result_ref: "needs_review:<id>[,<id>…]", message: "Needs your review: N claims[ in M lessons]" }`. The review pass is the loop's sync step (`review_pass.rs`): the user's edits and removals are carried into the draft there, then the teaching review, so a waiting lesson's draft only ever changes inside a job and the claim service never writes it. "Resume with nothing changed" is `ClaimReviewService::changed_since_stop` (a user event at or after the review row's last write by the job). Open for track S: `JobStore::retry_in` admits only failed, interrupted or cancelled jobs, so resuming a completed `needs_review` job needs a new attempt that inherits its staged draft and checkpoints. The Pass counter counts new candidate texts (`lesson_progress::new_pass`); `LearningGenerationActivity.claims` (`LearningClaimTally`) carries the panel's counts. Publication marks the review row `ready` inside its transaction (`persist_report`); when that gate finds the references changed since verification, the job verifies again instead of failing (`report::sources_changed`).

## 7. Persistence, DTOs, commands, bindings

### 7.1 Migration `src-tauri/migrations/20261016150000_lesson_claim_reviews.sql` (after the per-chat-model `20261016120000`)

```sql
CREATE TABLE learning_lesson_reviews (
  lesson_id TEXT PRIMARY KEY REFERENCES learning_lessons(id) ON DELETE CASCADE,
  program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
  job_id TEXT NOT NULL REFERENCES learning_generation_jobs(id) ON DELETE CASCADE,
  state TEXT NOT NULL CHECK(state IN ('verifying','needs_review','stopped','ready')),
  revision INTEGER NOT NULL DEFAULT 0,
  draft_sha256 TEXT NOT NULL,
  checker_model TEXT NOT NULL,
  stop_reason_json TEXT CHECK(stop_reason_json IS NULL OR json_valid(stop_reason_json)),
  opened_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE TABLE learning_lesson_claims (
  claim_id TEXT PRIMARY KEY,
  lesson_id TEXT NOT NULL REFERENCES learning_lesson_reviews(lesson_id) ON DELETE CASCADE,
  unit_index INTEGER NOT NULL, unit_kind TEXT NOT NULL CHECK(unit_kind IN ('block','question','answer_key')),
  field TEXT NOT NULL, quote TEXT NOT NULL, span_start INTEGER, span_end INTEGER,
  statement TEXT NOT NULL,
  state_json TEXT NOT NULL CHECK(json_valid(state_json)),          -- ClaimState, replayed from events on load; stored for listing
  evidence_fingerprint TEXT, verdict_kind TEXT, reason TEXT,
  evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),    -- Vec<EvidenceSeen> + texts shown
  updated_at INTEGER NOT NULL
);
CREATE INDEX learning_lesson_claims_lesson ON learning_lesson_claims(lesson_id, unit_index);
CREATE TABLE learning_lesson_claim_events (
  claim_id TEXT NOT NULL REFERENCES learning_lesson_claims(claim_id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL, actor TEXT NOT NULL CHECK(actor IN ('machine','user')),
  kind TEXT NOT NULL, payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  job_id TEXT, at INTEGER NOT NULL,
  PRIMARY KEY(claim_id, ordinal)
);
ALTER TABLE learning_source_library ADD COLUMN added_by TEXT NOT NULL DEFAULT 'research'
  CHECK(added_by IN ('user','research','outline','import','verifier'));
ALTER TABLE learning_source_versions ADD COLUMN snapshot_hash TEXT;
-- §14 Verifier audit (opt-in per course) and §15 checker accuracy
ALTER TABLE learning_programs ADD COLUMN verifier_audit TEXT NOT NULL DEFAULT 'off' CHECK(verifier_audit IN ('off','on'));
ALTER TABLE learning_programs ADD COLUMN verifier_web_search INTEGER NOT NULL DEFAULT 0 CHECK(verifier_web_search IN (0,1));
ALTER TABLE learning_programs ADD COLUMN verifier_consent_at INTEGER;
ALTER TABLE learning_programs ADD COLUMN verifier_consent_provider TEXT;
CREATE TABLE learning_lesson_audits (
  lesson_id TEXT NOT NULL REFERENCES learning_lesson_reviews(lesson_id) ON DELETE CASCADE,
  draft_sha256 TEXT NOT NULL, verifier_model TEXT NOT NULL, policy TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('running','completed','stopped')),
  batches_json TEXT NOT NULL CHECK(json_valid(batches_json)),
  input_tokens INTEGER NOT NULL DEFAULT 0, output_tokens INTEGER NOT NULL DEFAULT 0,
  started_at INTEGER NOT NULL, finished_at INTEGER,
  PRIMARY KEY(lesson_id, draft_sha256, verifier_model)
);
CREATE TABLE learning_checker_examples (
  id TEXT PRIMARY KEY, program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
  lesson_id TEXT NOT NULL, claim_id TEXT NOT NULL,
  checker_model TEXT NOT NULL, context_window INTEGER NOT NULL, policy TEXT NOT NULL,
  claim TEXT NOT NULL, passages_json TEXT NOT NULL CHECK(json_valid(passages_json)),
  local_verdict TEXT NOT NULL, label TEXT NOT NULL CHECK(label IN ('true','false','retrieval_miss')),
  label_source TEXT NOT NULL CHECK(label_source IN ('user_confirm','user_remove','user_edit','evidence_added','verifier_agree','verifier_disagree')),
  at INTEGER NOT NULL
);
CREATE INDEX learning_checker_examples_model ON learning_checker_examples(checker_model, at);
```

The `learning_programs` ALTERs are plain column adds; the triggers on that table (`migrations/20261006002000_learning_generation_content_revision.sql:34-43`) list columns explicitly and are unaffected.

No table rebuild; `preparation` and the job status CHECKs are untouched. Pre-release: existing rows get `added_by='research'`, which is wrong for sources the user added before this change; acceptable under "data deletable", and the Sources panel can show "added before provenance was recorded" for rows older than the migration if Josh wants (one line, LATER).

*As built (track L0):* the file is `migrations/20261017150000_lesson_claim_reviews.sql`, after `20261017120000_chat_images.sql`. `learning_lesson_reviews.job_id` references `jobs(id)`: `learning_generation_jobs` was dropped by `20261009140000_jobs.sql`. Everything else is as above, §14's opt-in and consent columns and §15's `learning_checker_examples` included, so no later track needs a migration of its own. Test: `learning/lessons/claim_reviews/migration_tests.rs` (fresh database; populated database before and after, rows kept, defaults, checks, cascades, and the opt-in leaving `content_revision` alone).

### 7.2 Repository and service (feature layer)

`src-tauri/src/features/learning/lessons/claim_reviews/` (new): `repository.rs` (implements `ClaimReviewStore` over sqlx), `service.rs` (actions → events, `expected_revision` check, draft edits via `text_edits`), `dto.rs`, `attach.rs` (the §5.4 pipeline, composing the sources repository, `ReferenceCollection::reload`, `retrieve_from`, `judge_for_publication`), `mod.rs`. DI through the container/bootstrap (#71's composition root), no global state. Each file ≤ 800 lines.

*As built (track L1):* `repository.rs` implements `ClaimReviewStore`; each claim row's `evidence_json` is the passages its card shows, with their texts (`ShownPassage`), written by `show_passages` (display only: no revision). The payload of an event row is the event plus the provenance the columns lack (app version, checker model). `service.rs` (`ClaimReviewService::resolve`) records Confirm, Unconfirm, Edit, Remove, GeneralKnowledge, AcceptAllGeneralKnowledge, ResearchWithHint and Undo; it validates an edit's `before` and a removal's whole sentence against the draft, refuses both on a published lesson, and refuses an Undo once the edit or removal is in the draft (restoring a sentence a repair removed is not built). The attach actions belong to `attach.rs` (track S), which records `EvidenceAttached` through the store and calls `CheckerExamples::record_evidence_added`. Not built here: the `LessonClaimReviewDto` assembly for `get_learning_lesson_claim_review` and every command (track S).

### 7.3 DTOs (specta, camelCase)

```rust
pub struct LessonClaimReviewDto { lesson_id, program_id, state: LessonReviewState, revision: i64,
  counts: ClaimCounts { verified, confirmed, general_knowledge, queued, open, removed },
  pass: u32, stop_reason: Option<StopReasonDto>, claims: Vec<ClaimReviewItemDto> }
pub struct ClaimReviewItemDto { claim_id, location: ClaimLocationDto { unit_index, unit_kind, unit_title, field, quote, span },
  statement, state: ClaimStateDto, verdict_kind: Option<VerdictKindDto>, reason: Option<String>,
  evidence: Vec<LearningEvidencePassageDto> /* reused, + `added_by` */, supporting_quote: Option<String>,
  research: Vec<ResearchAttemptDto { queries, sources_added: Vec<SourceRefDto>, passages_found, at }>,
  repairs: Vec<RepairAttemptDto { attempt, escalated, before, after, outcome, at }>,
  executions: Vec<ExecutionDto>, human: Option<HumanVerdictDto { at, note, bound_sources }>,
  available_actions: Vec<ClaimActionKind>, history: Vec<ClaimEventDto> }
pub enum ClaimActionRequestDto { Confirm{note, source_version_ids}, Unconfirm, Edit{before, after}, Remove, GeneralKnowledge,
  AcceptAllGeneralKnowledge, ResearchWithHint{hint}, AttachSavedSource{source_version_id, start_byte, end_byte},
  AttachLink{url}, AttachDocument{document_id}, AttachText{title, text}, Undo }
pub struct StopReasonDto { code, message, fix, at, request: Option<RequestFactsDto { prompt_tokens_exact, prompt_tokens_estimated, window_tokens, output_reserved, server_kind, server_message_sanitized, attempts, step }> }
```

*As built (track L0):* `learning/lessons/claim_reviews/dto.rs` and `dto/history.rs`; JSON fixtures in `claim_reviews/fixtures/`, pinned by `dto_tests.rs` (each fixture round-trips, and the domain mirrors produce it). `evidence` is `ClaimEvidencePassageDto { passage: LearningEvidencePassageDto, addedBy }` (the reused DTO is nested, not extended). `ClaimStateDto` carries the status only; verdict, reason, supporting quote and the user's note are the item's own fields. `history` is `ClaimEventDto { ordinal, actor, at, jobId, appVersion, checkerModel, event }` with one payload per event kind. Unions are internally tagged (`state`, `kind`, `cause`, `action`, `outcome`, `verdict`, `input`). No command is registered and the bindings are unchanged.

### 7.4 Commands (`learning/plugin/claims.rs`, new; registered in `learning/plugin.rs` and `src/bin/export_bindings/main.rs`)

`get_learning_lesson_claim_review(program_id, lesson_id)`, `resolve_learning_claim(program_id, lesson_id, claim_id, action, expected_revision, operation_id)`, `resume_learning_lesson_review(program_id, lesson_id, operation_id)` (wraps `retry_learning_generation_job`). Attach actions return the check result inline (they may take a minute: progress through the existing `lesson_progress` event channel keyed by lesson, so the card shows "Fetching… / Indexing… / Checking 1 of 5").

*As built (track S):*
- **Commands** (`learning/plugin/claims.rs`; registered in `learning/plugin.rs` and `src/bin/export_bindings/main.rs`, permitted in `build.rs` and `capabilities/main.json`; bindings and `gen/schemas` regenerated). They take request DTOs, the codebase's convention, not positional arguments: `get_learning_lesson_claim_review(programId, lessonId)`; `resolve_learning_claim({programId, lessonId, claimId, action, expectedRevision, operationId})` returns `{revision, attach: ClaimAttachReportDto | null}` (the frontend reloads the review after any action); `resume_learning_lesson_review({programId, lessonId, expectedRevision, operationId})`, where `expectedRevision` is the course's revision because `retry_job` checks it; and `capture_learning_source_snapshot({programId, sourceId, versionId})` for the card's "Capture again" (§5.4), returning `SnapshotNoteDto`. Resume is refused with §6.2's text when the review is `needs_review` and no claim event is later than the job's end. It then wraps `retry_job`, which accepts only a failed, interrupted or cancelled job, so how L1 ends a `needs_review` job decides whether Resume works.
- **Left for integration with L1:** three functions in `plugin/claims.rs` return `ServiceNotAvailable` until L1's code lands. `claim_ledger` becomes `Arc::new(ClaimReviewRepository::new(pool))`, `review_of` becomes the service's DTO assembly, and `resolve_by_service` becomes the service's non-attach actions. The `export_learning_checker_examples` and `get_learning_checker_accuracy` commands (L1's addition) are not registered yet. The attach pipeline is written against the `ClaimReviewStore` port alone.
- **The pipeline** (`claim_reviews/attach.rs` + `attach/{acquire, check, failures, report}.rs`):
  - *Link:* `source_workflows::web_source_url` validates the address. If the course already has a live web source at that address (compared by URL identity: scheme, host case, default port and fragment do not count), it is reused. Otherwise the page is read with `fetch_reference_content` (browser-reader fallback included), checked for a wall, captured by `source_workflows::captured_page`, deduped by content digest and saved with `added_by = user`. Then the frozen copy is requested.
  - *File:* an already-captured library document is reused. Otherwise `capture_document`, digest dedupe, then save.
  - *Pasted text:* digest dedupe, then save as kind `pasted`.
  - *Saved passage:* must lie in the active version of a live source. Exactly that span is judged.
  - *Then, for all four:* `ReferenceCollection::load` over `verification_sources` and `retrieve_from(version, statement, fits)`. `fits` is the single-claim judge's evidence room (`claim_share`). Only passages that `mentions_subject_of` the claim count, so a vector neighbour alone is "no passage". The claim goes to `judge_for_publication`, and the result is recorded as `EvidenceAttached` (actor user, `Checked` or `NoPassage`). A provider outage is an error, never a verdict.
- **Concurrency:** `expectedRevision` must be current when the attach starts. Fetching and checking take a while, so each event is then appended at the ledger's revision of that moment, after re-reading that the claim still allows it. A refusal at a revision the job has since moved past is retried, and one at an unchanged revision is the ledger's rule. If the claim was settled meanwhile, the user is told and the source stays saved.
- **Other open claims:** these are the claims that are `Open` or waiting for the user. `Unchecked` claims are left to the pass that checks them. A claim with no passage gets no event. A checked one gets `Checked` (actor machine), one request each. An outage stops the rest and says so on the card.
- **Fetch failures:** these are mapped from the existing errors (`HTTP <status>` network errors, the cached refusal included; the thin-page refusal verbatim; `InvalidUrl … blocked`; the browser reader's unavailable error). There are two deviations:
  - The paywall row needs the page's text, which the thin refusal never returns. So a wall is detected on a page that *was* read: at most 250 words (`WALL_MAX_WORDS`) together with a sign-in or subscribe phrase. A thinner wall ends as the thin message.
  - `read_page_as` never surfaces the browser reader's unavailability: it falls back to the HTTP result, so the card shows the refusal or thin message instead.
  - Other HTTP statuses and network errors are "unreachable", with Retry, paste and file offered.
  - The fixtures are in `claim_reviews/fixtures/attach_link_failures.json` (403, cached 403, 401, 429, thin, paywall, private address, no browser, 404).
- **Frozen copy:** the `ReferenceSnapshotPort` port (lattice-core) is implemented by `WebPageSnapshotService` (`web/snapshots/reference.rs`) and exposed as `Container::reference_snapshots()`, so learning names no web type. Copies are filed under the library conversation (`repository::LIBRARY_CONVERSATION`, now created by `library_conversation()`), so deleting a chat never takes a reference's copy with it. A copy of the same page kept for a chat is reused. The hash is recorded at attach if the copy is ready. Otherwise `get_learning_source_version` records it the first time it finds the copy ready, and the DTO carries `snapshotHash`; the reader opens the copy by its page URL.
- **Provenance:** `LearningSourceLibraryRepository::add_by(.., added_by)`. `add` keeps its signature and records `research`, so L1's `research.rs` and the tests that add sources are unchanged. The user's add-link, add-file and add-text commands record `user`, and the references saved at course creation (`repository.rs`) record `outline`. A pack import keeps the exported value (§5.2: the actor is the pack's author), so no path writes `import` today. `LearningSourceLibraryItemDto.addedBy` and `LearningSourceVersionSummaryDto.snapshotHash` are new, read by the Sources chip and the reader, and exported and imported with packs (`importers.rs`).
- **Not built here:**
  - A supported attach does not add the source to the unit's `source_ids`; that is a draft edit through L1's `text_edits`/`lesson_drafts`.
  - The check's receipt is not saved under `claim_receipt_key`, and no `learning_checker_examples` row with `evidence_added` is written (both L1's).
  - No progress events are sent: `lesson_progress` is task-local to the job, and the attach runs in the command, so the card shows its own pending state.
  - There is no "Refresh" offer when a saved page's live copy differs.
  - The fingerprint names `CHECK_POLICY = "strict"`. Integration should switch it to `content_verification::POLICY`, which L1's ledger hook uses, so that the two fingerprints agree.
- **Files outside the S row**, kept to a line or two each: `web/di.rs` (the accessor), `learning/repository.rs` (`outline`), `dto/sources.rs` (the two fields), the four `LearningSourceVersionSummaryDto` constructions in practice and `content_caps_tests.rs`, `claim_reviews/dto.rs` (`From<SourceAddedByDto>`), and `pack_tests.rs` (round-trip assertions).
- **Tests (written, not run):**
  - `claim_reviews/attach/tests.rs`: every §5.4 failure fixture through `attach` (message, next steps, nothing saved or recorded); a long article with a subscribe footer is not a wall; supporting pasted text verifies the claim and reports the other claim as "no passage"; the same text again is deduped; a contradicting source keeps the claim queued with its verdict; a link saved as the user's and frozen, and not fetched again; a failed frozen copy is noted; a stale revision is refused before anything is saved.
  - `source_library/user_evidence_tests.rs`: `added_by` for user, research and outline; lookups by address, digest and version; `link_snapshot`.
  - `reference_collection/tests.rs`: `retrieve_from` reads one version only.
  - `pack_tests.rs`: `addedBy` and `snapshotHash` survive a pack copy.

## 8. Deterministic-failure detection and request sizing

### 8.1 Classification

| Error | Retryable? | Classification | Message to the user |
|---|---|---|---|
| Network/connect/timeout, 502/503/504 from a proxy, 429 | yes, bounded (§8.3) | outage | "Waiting for llama.cpp at <host> (attempt n: <error>)" |
| llama.cpp SSE/HTTP error with code 5xx, first time | once | unknown | retried once with `cache_prompt=false` (today's `retry.rs:131-137`) |
| the same request digest + same error kind + same sanitized message, second time | **no** | deterministic → `AppError::ModelRejected` (new variant in `crates/lattice-core/src/shared/error/types.rs`, non-transient in `JobFailure::is_transient` `jobs/mod.rs:137-146`) | "llama.cpp rejected this request twice: (500 server_error) <sanitized>. The request was 127186 tokens: a prompt of 118994 (counted exactly) and 8192 for the answer; the configured context window is 131072. <fix>" |
| 400/422 | no | deterministic | same shape |
| `finish_reason=length` with empty text | no (already permanent, `retry.rs:241-247`) | deterministic | "answer budget of N tokens used before any answer; prompt P of W" |
| prompt plus a fixed answer exceeds the configured window (§8.2) | never sent | refusal (`AppError::InvalidInput`) | "This request needs N tokens: a prompt of P (counted exactly) and R for the answer; the configured context window is M. It was not sent. …" |

**Sanitizing the server message** (`streaming.rs:80-96` and `mod.rs:492-517` `check_status`): keep `error_kind` as is; add `sanitized_message(server: &str, request_texts: &[&str]) -> String`: (1) whitespace-collapse, (2) drop any run of ≥ 6 consecutive words that occurs in any request message text (prompt echo), replacing it with "[…]", (3) keep ASCII printable only (others → "?"), (4) cut at the first 240 chars with "…". The raw message is never logged; its sha256 is, so two logs can be compared. The three fixed sentences stay as prefixes for the known strings. Chat and learning share this.

*As built (track R):* the sanitizer, the request-size type, the refusal and the rejection text are provider-neutral (`llm/scheduler/rejection.rs`); the llama.cpp client reads a failure into `streaming::ServerFailure` (HTTP status or SSE error) and sanitizes it against every string of the request body. A message the client recognises (the three generation errors, the two template errors) is told in its fixed sentence *instead of* the server's words, which add nothing to it; an authentication failure (401/403/407) shows no server words, which may repeat the credential sent. 408/429/502/503/504 are outages (`Network`/`RateLimitExceeded`; 503 was `ServiceNotAvailable`); 400/413/422 are `ModelRejected` at once; other 5xx are retried once with `cache_prompt=false`, and the same status and error type twice is `ModelRejected`. The server's words are shown but never compared: they may carry a request id or a slot number that changes on every attempt.

### 8.2 Request ceiling: the configured window, and nothing that cannot fit it is sent

Decided by Josh on 2026-10-16, replacing an earlier design that read each server's per-slot capacity from `/slots` and `/props`: "Don't add a bunch of additional code to work around a configuration on a server. This app needs to be server agnostic."

- **The configured context window is the single ceiling.** What a request may use is the window the user set in Settings (`LLMPort::max_context_tokens`, unchanged, for every provider). Nothing asks a server how it is configured: no `/slots`, no per-request limit from `/props`. The scheduler's existing `/props` sizing of its slots is left as it was and never feeds request sizing.
- **Refused before sending, whichever backend** (`llm/scheduler/scheduled.rs` `ScheduledLlm::ensure_fits`, ahead of admission): a request whose prompt plus a *fixed* answer reservation (`max_output_tokens`) exceeds the window, or whose prompt leaves no room for any answer, is refused as `AppError::InvalidInput` and never queued or sent: "This request needs N tokens: a prompt of P (counted exactly|estimated) and R for the answer; the configured context window is M. It was not sent. Shorten the request, or raise the context window in Settings if the model's server holds more." The calibrated estimate settles a request that plainly fits; one that does not is counted exactly (`count_tokens_exact`) when the backend has a tokenizer, before it is refused. Opaque provider state (images, signed reasoning) is left out of both counts, so it never causes a refusal.
- **llama.cpp body** (`llm/llama_cpp/request.rs`): the 1,024-token floor is gone. An open reservation takes the room the prompt leaves and never more; a fixed one is kept whole or refused, since a section rewrite cut short is unusable JSON. `measured_body` counts exactly with the server's tokenizer (the existing `/tokenize` call, made only when the estimate does not settle it) and refuses with the exact numbers. The estimate that sizes the reservation carries a 3/2 margin for dense text; it never refuses on its own: when the server cannot count, the plain estimate (four characters a token, no margin) decides, so a server without `/tokenize` is not refused a request that fits.
- **A server that holds less than the window says so by refusing.** A context or size error on a request that fits the configured window ends as `ModelRejected` (§8.1) with: "The server refused a request of N tokens even though it fits the configured context window of M, so the server's real per-request limit is lower: lower the context window in Settings to match the server." Only a prompt counted exactly supports that conclusion; when it was estimated, the message says so instead: the request may not fit the window after all, so shorten it, and lower the window only if the server also refuses requests that fit. Telling a size error apart reads only the error (HTTP 413, an error type naming the context, words such as "context … exceeded" or "prompt is too long"), never the server's configuration, and only chooses which fix to suggest; the server's sanitized words are shown either way.
- The repair splitter (`repair.rs:380-410`, L1) sizes against the same `max_context_tokens`, so a section whose claims and evidence do not fit is split or trimmed as it already is. No fixed caps anywhere: everything derives from Settings and the measured prompt.
- Logging: `llm.request { model, prompt_tokens, counted_exactly, output_reserved, window, slot }` on send; `llm.request_refused { model, prompt_tokens, counted_exactly, output_tokens, reserved, window }` on refusal.

### 8.3 Jobs never retry forever invisibly

`JobFailure::Defer` stays unlimited for genuine outages, with two changes: (1) `Deferral.message` carries the attempt number and the last error, and the panel shows it (today's "Waiting to retry after a service outage" is replaced by "Waiting to retry after a service outage · attempt 4 · last error: <sanitized>"); (2) `lesson_failure` (`generation_recovery.rs:26-35`) returns `Fail` when the error signature (kind + sanitized message + request digest) equals the previous attempt's **and** `activity.last_checkpoint_at` did not advance between them: identical failure with no progress is deterministic by definition. No count-based magic number: progress is the criterion.

*As built (track R):* the rule lives in the job runtime (`shared/runtime/jobs/runtime.rs` `record_failure`) for every job kind, not only lessons, because `lesson_failure` sees one attempt and cannot compare it with the last. An outage (`JobFailure::is_outage`: `Network`, `RateLimitExceeded`) always defers. An Ollama that cannot be connected to, refused or unreachable, is `Network` from the loader on, so a lesson waits it out rather than ending on its second attempt. Any other deferral is compared with the job's previous one: the same error code (its kind, never its text, which may carry an id that changes on every attempt) **and** the same saved work (a SHA-256 of the job's checkpoint contents and staged result; content, not save times, so a checkpoint rewritten unchanged is not progress) ends the job as failed with code `repeated_failure`. The request digest is implicit: with nothing saved, the job resumes at the same step and sends the same request. The previous failure is held in memory by the runtime, and forgotten when the job finishes or is cancelled, so after a restart a job gets one more attempt. Every deferral's progress message gains " · attempt n · last error: <error>". Within one call, the llama.cpp client already turns the second identical server failure into `ModelRejected` (§8.1), which is never deferred.

### 8.4 Observability: "why did this lesson stop"

- The review row's `stop_reason_json` (`StopReasonDto`) is written by the job handler on every terminal error with the step (`lesson_progress` phase + stage text), the request facts, the attempt history and the fix.
- Tracing events, structured: `llm.request`, `llm.request_rejected { status, kind, sanitized, sha256, prompt_tokens, counted_exactly, output_tokens, window, attempt }`, `lesson.stopped { lesson_id, job_id, code, step }`, `claim.state { claim_id, from, to, cause }`, `claim.user_action { action }`.
- The preparation panel's "Stopped" block shows `stop_reason` verbatim with a "Copy details" button (the JSON), so a report to a log is one paste.

## 9. Concurrency

- **Ledger-first.** User actions write the ledger (and the draft, for edits) in one transaction with `expected_revision`; the job reads the ledger at sync points: before each verify, before each repair group, before publication. Between sync points the job holds no lock on the ledger.
- **Edit vs in-flight repair of the same unit.** The repair carries the unit's text sha it started from; `merge` (`repair.rs:260-286`) becomes compare-and-set: if the draft's unit sha differs (the user edited), the repair result is discarded, logged `repair.superseded_by_user`, and the unit is re-verified from the user's text at the next sync point. The user's edit always wins; the machine's attempt is not charged to the claim.
- **Attach while running** (§5.4): the attach command runs its own check; both it and the job go through the scheduler; the job picks up the closed claim at its next sync point. If both check the same claim with the same fingerprint, receipts dedupe (`claim_receipt_key`).
- **Pause**: cancel as today; actions while paused are ordinary ledger writes; Resume = §6.2.
- **Restart**: ledger and draft are in SQLite; `RecoveryPolicy::Requeue` (`generation_recovery.rs:17-21`) resumes; the pending-repair checkpoint resumes only if its claim set is still `may_repair` under the ledger (a claim resolved while the app was closed is dropped from the resumed repair context before sending).
- **Two lessons of one program**: the per-subject job lock stays (`generation_jobs.rs:325-345` test documents it); a source added for lesson A is new evidence for lesson B's next pass only for claims that retrieve passages from it.

## 10. Frontend

Files (new) under `src/features/learning/lessons/review/`: `LessonClaimReviewPanel.tsx` (list, filters, header, Resume), `ClaimReviewCard.tsx`, `ClaimEvidenceAttach.tsx` (link / file / paste, progress, failure messages), `ClaimHistory.tsx`, `useLessonClaimReview.ts` (queries + mutations through `src/features/learning/api/client.ts`), `claimReviewPresentation.ts` (copy, grouping, counts), tests beside each.

- **Where it lives.** (a) In `LessonPreparationProgress.tsx`: while running, a live "Claims" section lists Queued claims as they appear (so the user can act during the run) and counts per state; when the job ends `needs_review`/`stopped`, the panel header becomes the §6.1 copy and the list is the main content. (b) In `LessonCard.tsx` for a lesson in `needs_review`: the card shows "Needs your review · N" instead of "Ready/Outline" (`LessonCard.tsx:62`) and expands to the same panel; for a `ready` lesson the existing `LessonEvidencePanel` shows resolution chips (§5.6) and "source removed" flags.
- **Card.** Header: section number + title, unit kind ("Section 3 · Worked example" / "Question 2 · answer key"), the lesson sentence with the claim quote highlighted, "Open in lesson" (scrolls to `#lesson-<id>-section-<n>`, `LessonCard.tsx:84`). Body: the claim statement; verdict chip with the honest kind (Unsupported / Contradicted / No source found / Could not judge / General knowledge?); the checker's reason; "What was compared": passages (reuses `Passage` from `LessonEvidencePanel.tsx:26-60`) with source title, version, byte span, "Added by you" chip, open-in-reader; "What was tried": research attempts (queries, pages saved) and repair attempts (before → after, why rejected); executions for code. Footer: action buttons in a fixed order (Confirm · Edit · Remove · General knowledge · Add evidence ▾ · Research with a hint · History). Confirm opens an inline note field; for Contradicted the note is required and the button says "Confirm anyway (note required)".
- **1 claim vs 43.** One claim: the card renders inline, no filters, the header says "1 claim needs your review". Many: grouped by section in lesson order with sticky section headers and per-section counts; filter chips (Needs you · Being repaired · Resolved by the machine · Confirmed by you · General knowledge · All) with counts; "Accept all N general-knowledge proposals" appears when applicable; keyboard: `role="list"`/`listitem`, roving `tabIndex`, ↑/↓ move between cards, Enter expands, each action is a real button (no key-only actions); focus returns to the next card after an action; `aria-live="polite"` region announces "Claim confirmed. 42 remain."; the Resume button is disabled with the §6.2 reason as its `aria-describedby`.
- **Progress copy** replaces the paragraph at `LessonPreparationProgress.tsx:95` with the §6.3 bound sentence.

## 11. Failure-mode matrix

| Case | System | User sees | Fix offered |
|---|---|---|---|
| Claim fails twice on the same evidence | Queued, no third attempt | the card with everything compared | Confirm / Edit / Remove / Add evidence / Hint |
| Research finds nothing (no web, or no results) | event `Researched { passages_found: 0 }`; NoSource → Queued | "Searched: <queries>. No page added." | Add evidence / Confirm / Remove |
| Checker cannot judge (window) | Queued CouldNotJudge with the cause | "The checker could not fit this claim's evidence (P of W tokens)." | Add a smaller passage / Confirm / larger Learning model |
| Prompt exceeds the configured window | never sent (`InvalidInput` with the numbers); split/trim; if one claim with one passage still does not fit → that claim CouldNotJudge, not a job failure | per-claim | as above |
| Server refuses a request that fits the configured window (context or size error) | `ModelRejected` at once (400/413/422) or on the second identical 5xx; job Failed | "llama.cpp rejected this request…: the server refused a request of N tokens even though it fits the configured context window of M…" | lower the context window in Settings to match the server |
| Identical 5xx twice | `ModelRejected`, job Failed, stop reason | "llama.cpp rejected this request twice: … Fix: …" | Retry after fixing |
| Outage | deferred, attempt + last error visible | "Waiting to retry · attempt 4 · last error …" | Pause; Settings |
| User edits a unit mid-repair | repair result discarded, unit re-verified | nothing surprising; History shows "your edit superseded a repair" | — |
| Two actions on one claim from two windows | second gets `revision` conflict | "This claim changed; reloaded." | act again |
| Source deleted that resolved claims | reopen (§5.3) | dialog count; card "source removed" | re-attach / confirm |
| Link 403 / paywall / thin | §5.4 table | honest cause | paste / file |
| Added source contradicts the claim | Contradicted with passage | the passage | Edit / Remove / Confirm with note |
| Added source irrelevant | no passage; source kept | "No passage mentions…" | — |
| Learning model changed | receipts invalid (model in key); pass reruns checks; human verdicts stand | "Checker model changed; rechecking 112 claims; 3 confirmed by you are kept." | — |
| Pack imported with a ledger | ids remapped; states kept | same chips | — |
| App quits mid-attach | the source is saved (its own transaction); the check reruns on next open of the card ("Check not finished; run again") | — | button |
| Code example, no runtime | Queued CouldNotJudge | "Could not run: no Python runtime" | Confirm / Remove / install runtime |

## 12. Test plan

**lattice-core unit (`claim_review.rs`)**: identity matching (same statement, same quote, overlapping span, vanished sentence); `new_information` for each cause and for each non-cause (model unchanged, research with no passages); attempt budget 2 → Queued, reset on new fingerprint; `relitigate` true/false table; event replay = stored state for every transition; undo restores; JSON round-trips.

**src-tauri unit**: `sanitized_message` (echo removal, truncation, non-ASCII); `generation_error` keeps kind and adds sanitized text; `attempt` second-identical-failure → Permanent `ModelRejected`; a request that exceeds the configured window is refused before sending (the provider-neutral gate and the llama.cpp body; the server never receives it); an identical 500 twice produces exactly one `ModelRejected` and no third attempt; a transient failure (an outage, or a failure that changes) still retries; `output_room_after` refuses instead of flooring; the job runtime fails a job whose non-outage failure repeats with no saved progress, and keeps deferring outages and progressing jobs with the attempt and last error shown; `merge` compare-and-set; repair splitter uses the exact count (fake tokenizer).

**src-tauri integration (`content_verification/tests.rs` style, fake LLM)**: a claim that fails twice ends `needs_review` with one card and two `RepairAttempted` events; a contradicted claim gets correction then removal then queue; CouldNotJudge never fails the job; Confirm → Resume publishes without rechecking it; Edit → only that unit re-extracted (section checkpoints reused elsewhere); attach a saved passage that supports → ResolvedByEvidence and `source_ids` updated; attach a contradicting text → Contradicted with the passage; delete that source → reopened; Resume with nothing changed → refused; publication gate after a source added mid-run → recheck, not error; restart mid-repair with a claim confirmed meanwhile → repair context drops it. Migration applies on a populated DB.

**Verifier audit (fake cloud `LLMPort` + fake web)**: role resolution (unset, no key, provider changed since consent); batching against a measured window (oversize claim → `could_not_verify`); results written per batch and resumed after a simulated crash; revoke mid-run stops at the boundary and keeps results; a dispute → Queued `Disputed`, blocks publish, two-column DTO; agreement closes; a confirmed claim disputed → Reopened with the old note present; human confirm with note over a Verifier contradiction publishes; usage tokens summed. **Checker accuracy**: each §15 row writes one example with the right label source; export produces the `claim-pairs-v1` shape and `live_claim_pairs_eval` parses it; accuracy numbers computed per checker model.

**Frontend (Vitest)**: panel renders 1 vs 43 (grouping, filters, counts); card actions call the right request with `expectedRevision`; Confirm on Contradicted requires a note; attach failure messages per §5.4 table; keyboard navigation and live region; Resume disabled reason; LessonCard state chip; evidence panel resolution chips; consent dialog requires the confirm button and names the provider; audit progress line; dispute block shows both reasons; accuracy line under the role rows.

**Live manual script (Josh)**: 1) Prepare a topic-only course lesson on the remote 27B; watch the panel list claims as they queue; confirm that no pass number exceeds 1 + 2 × failed claims. 2) Pick a Queued claim: open both passages in the reader; Confirm with a note; Resume; lesson becomes ready; evidence panel shows "Confirmed by you". 3) Edit a sentence on another; Resume; only that section re-extracts (log `section_checkpoints` reuse). 4) Add a link that 403s → message; paste the text → claim verified with "Pasted by you". 5) Add a PDF via the picker → verified; delete that source from Sources → claim reopens with the dialog count. 6) Set the remote window to 131,072 on a server that holds less per request and prepare a long lesson: the log shows `llm.request` with `window=131072`; the server's size error ends in one `ModelRejected` naming both numbers and the fix; after lowering the window in Settings, section repairs split into parts; no 500 loop. 7) Point Settings at a server that returns 500 for everything: the job fails once with the sanitized message and numbers; "Retry after fixing" is the only button.

## 13. Decisions taken (nothing open)

All decided by Josh on 2026-10-16:

- **D1. General-knowledge labels: the machine proposes, the user accepts.** `general_knowledge()` (`general_knowledge.rs:50-55`) no longer closes a claim; it yields `VerdictKind::GeneralKnowledgeProposed`, a Queued state. The panel offers "Accept all N general-knowledge proposals" (one `UserAction` per claim) and per-card "General knowledge". Only accepted labels publish. The learner label stays "General knowledge · not checked against a source".
- **D2. A source deleted behind a published lesson: reopen its claims; the lesson stays readable with those claims flagged** (§5.3 as written). The delete dialog carries the counts.
- **D3. Confirm on Contradicted: allowed, note required.** The note is shown on the claim card and in the learner evidence panel next to "Confirmed by you", and stays in the claim's history even after Unconfirm or Undo (events are never deleted).
- **D4. Pasted text: included**, kind `pasted`, provenance "pasted by you" (`added_by='user'`), indexed and checked exactly like any source (§5.4).
- **D5. Second opinion: LATER**, as a manual "Get a second opinion" action on the claim card, never automatic (§4). It will reuse the Verifier role (§14): the action runs the §14 audit for one claim only. LATER note kept so the card's action slot and the ledger event (`EvidenceAttached`-style `SecondOpinion { model, verdict, reason }`) are reserved now and the button is not shown until built.
- **D6. Verifier audit by Claude before publishing, opt-in per course**: §14, decided and specified in full.
- **D7. Measure the local checker's accuracy from human verdicts and audit disagreements**: §15.

## 14. Verifier audit (decided): a final grounded audit before publishing, opt-in per course

### 15.1 The Verifier role

Extends the per-chat-model design, not a parallel mechanism. Touch points in that branch (sequence after it lands):

| Per-chat-model item | Extension |
|---|---|
| `ModelRole { Chat, Utility, Router, Learning, Maintenance }` (§2.3) | add `Verifier` |
| `ModelOrigin` (§2.3) | add `VerifierRole` |
| `ModelRef::Cloud { provider: CloudProvider, model }` (§2.1) | used as-is; the Verifier role is the first role that *allows* a Cloud ref. Local, Ollama and llama.cpp refs are also allowed (a user without cloud access may audit with a second local model). |
| `LLMSettingsDto.learning_model: Option<ModelRef>` (§3.3) | add `verifier_model: Option<ModelRef>` next to it, same `update_settings` path and shape validation |
| `resolve_learning(&LLMSettingsDto)` (§4.1) | add `resolve_verifier`: unset → `InvalidConfig("No Verifier model is set. Choose one in Settings → Models under Verifier.")`; `Cloud` with no key in the keyring (`loading.rs:157-183`) → `InvalidConfig("Your Verifier model <provider>/<model> has no API key saved.")` |
| `Container::learning_llm(requirements)` (§4.2) | add `verifier_llm(requirements) -> Result<PinnedModel>`; cloud refs load through `CloudLlm::new` (`llm/cloud.rs:44`) via `loading::load_ref` (A1) — the pool key is `ModelKey { model: Cloud{…}, launch: empty }`, so a cloud instance is a cheap client, never a sidecar |
| `ModelRequirements { min_context_tokens, tools }` (§2.3) | audit batches set `min_context_tokens` from the measured batch (§14.4) and `tools: Required` when web search is enabled for the audit (cloud tool calling exists, `cloud.rs:91-92`) |
| Roles UI `roleConfig.ts` fourth descriptor (§6.3) | a fifth: `{ id: 'verifier', label: 'Verifier', allowedTypes: ['language_model','chat','cloud'], hint: 'Audits every claim of a lesson against its cited passages before it publishes. May be a cloud model.' }`; the cloud provider row appears in the grid for this role only |
| Boot prewarm (§6 table) | Verifier is never prewarmed |
| `warm_up_learning_model` | no warm-up command for Verifier (cloud needs none; a local Verifier warms on first use) |

Lattice-core keeps the pure parts (`ModelRole`, `resolve_verifier`); the container does the loading; the audit service (feature) only sees an `LLMPort`.

### 15.2 Per-course opt-in and consent

- Storage: `ALTER TABLE learning_programs ADD COLUMN verifier_audit TEXT NOT NULL DEFAULT 'off' CHECK(verifier_audit IN ('off','on'))` and `verifier_consent_at INTEGER` in the same migration as §7.1. Pre-release default off.
- Where: the course's Plan tab and the lesson preparation panel both show the switch. Turning it on opens a dialog that must be confirmed:
  > **Audit lessons with your Verifier model before publishing**
  > Before a lesson of "<course>" becomes ready, its full lesson text (every section, question, answer and explanation), every factual claim extracted from it, and the exact cited passages from your references are sent to **<provider>** (<model>) for a second check. Nothing else from your library is sent. If the Verifier is allowed to search the web, the claim statements are also sent to your configured web search. Your provider's usage terms and pricing apply; Lattice shows the tokens sent and received for each audit. You can turn this off at any time; audits already running stop at the next batch.
  > [ ] Allow the Verifier to use web search for this course
  > [Cancel] [Turn on and send lesson text to <provider>]
- Changing the Verifier model after consent invalidates it when the provider changes (consent names a provider): the switch shows "Consent was for Anthropic; confirm again for OpenAI" and the audit refuses until confirmed. A local Verifier needs no consent dialog (nothing leaves the machine) but the switch copy still says what is sent to which process.
- Revocation: switching off writes `verifier_audit='off'` immediately; a running audit stops at its batch boundary (§14.6) and the lesson returns to `needs_review` with "Audit stopped: you turned the Verifier off" — the local pipeline's result stands and the lesson publishes without the audit at the next Resume (opt-in off means no audit gate).

### 15.3 When it runs

In `verify_and_repair_with_references` after the ledger reports no Open and no Queued claim (all verified/confirmed/removed/general-knowledge-accepted) and before `persist_report`/publication: `review_pass → audit → publish`. The audit is keyed by `draft_sha256` + Verifier model + policy; a lesson edited after an audit (any ledger `Edited`/`Removed` event, or a Resume with changed text) re-runs the audit for the changed units' claims only (unchanged claims keep their audit receipt, same reuse mechanism as `claim_receipt_key`, with `checker = verifier`). Opt-in turned on after a lesson is already ready does not retroactively audit published lessons; it applies to the next preparation or Resume (a "Audit this lesson now" button on a ready lesson runs it on demand and may send the lesson back to Needs review).

### 15.4 What is sent, how it is sized, what it costs

- Per batch: a system prompt (the audit policy, fixed text), the lesson units the batch's claims come from (full text of those units only, since a claim's context is its section), the claims (statement, quote, unit), and for each claim the exact passages the local checker compared plus the passages that resolved it (`evidence_json`), each with source title/version id. Never: other lessons, the program's other sources, chat history, learner answers, notes.
- Sizing with no constants: `verifier_llm.max_context_tokens()` (for cloud, `cloud.rs:480` from settings `context_window`) minus the measured system prompt minus the answer reservation (`claims × measured per-verdict output on the fixtures`, same approach as `model_requirements.rs` in track B) = room; claims are packed in lesson order into batches whose measured (exact where the backend counts exactly, else estimated and inflated as `request.rs:23-37` does) size fits the room; a single claim whose unit + passages exceed the room is split by passages first, then unit text is reduced to the paragraph containing the quote; if it still does not fit it is recorded `CouldNotVerify { reason: "the Verifier's window of W tokens cannot hold this claim's section and passages (P tokens)" }` and goes to the queue.
- Cost visibility: every batch records `input_tokens`/`output_tokens` from the provider's usage (`cloud.rs:534-536, 628-631`) into `learning_lesson_audits` (§14.7); the panel shows "Audit: 7 batches · 118,420 tokens sent · 6,140 received · <provider>" and the course Plan tab shows the course total. No price table is built in (prices change and are per account); the switch's help text links to the provider's pricing page.

### 15.5 What it does and what it outputs

- Policy (system prompt, fixed): judge each claim **only** against the passages supplied and, if enabled, pages fetched by web search during the audit; a verdict the model cannot tie to a quoted passage is `could_not_verify`, never `supported`; `contradicted` requires a quoted passage; general-knowledge-accepted claims are audited for contradiction only (they have no passage by definition) and reported `could_not_verify` is not a dispute for them, `contradicted` is.
- Web search: through the app's `WebServiceTrait::search_web` (`web/traits/web.rs:105`) and `fetch_reference_content`, exposed to the model as tools (cloud tool calling, `cloud.rs:91-92, 159`); fetched pages are saved as learning sources with `added_by='verifier'` (add that value to the CHECK in §7.1) so the citation is reproducible; the model never sees a page the app did not save.
- Output schema per claim: `{ claim_id, verdict: supported|contradicted|unsupported|could_not_verify, reason, passages_used: [{source_version_id|url, quote}] }`, validated against the schema as `structured_call.rs` does.
- Into the same ledger: each result is a `ClaimEvent::Checked { checker: Verifier(model), … }`. Agreement with the local result (supported/supported) closes the audit for that claim. Any **dispute** — Verifier says contradicted or unsupported where the local pipeline or the user said supported/confirmed, or `could_not_verify` on a machine-verified claim — sets the claim to `Queued { kind: Disputed { local: …, verifier: … } }`, which blocks publishing like any Queued claim. The audit never edits the lesson.
- Human-confirmed claims are audited too (the user may be wrong), and a dispute on one reopens it as `Reopened { cause: VerifierDispute }` → Queued; the user's earlier note is shown beside the Verifier's reason.
- Card: a two-column "Local checker said / Verifier said" block, each with verdict, reason and the passages it used; the user resolves with the same actions (§5.1). Confirming over a Verifier contradiction requires a note (D3).

### 15.6 Failure modes

| Case | Behaviour | Shown | Fix |
|---|---|---|---|
| Verifier unset while opt-in is on | the pass stops before sending; lesson `needs_review` with `stop_reason` | "Audit cannot run: no Verifier model is set." | Settings → Models; or turn the audit off for this course |
| No API key | `InvalidConfig` from `resolve_verifier` | "…has no API key saved." | add key |
| Provider down / network | outage: deferred as §8.3 (bounded by progress), audit resumes at the next unfinished batch | "Waiting to retry the audit · batch 3 of 7 · last error …" | wait / turn off |
| Rate limit (429 with Retry-After) | honoured per batch (`retry.rs:146-152` semantics in the cloud client's retry) | same line with the wait | — |
| Too large for one request | batching §14.4; never a failure | "Audit batch 3 of 7" | — |
| Identical provider error twice | `ModelRejected` (§8.1) | sanitized message | retry after fix |
| Partial completion | each batch's results are written before the next is sent (`learning_lesson_audits.batches_json`); restart/Resume continues from the first unfinished batch; a changed draft invalidates only batches whose claims changed | progress line | — |
| Consent revoked / course opted out mid-run | stop at the batch boundary; results so far are kept (they are real verdicts); lesson `needs_review` if any dispute, else publishable | "Audit stopped: Verifier turned off after batch 3 of 7" | — |
| Provider changed since consent | refusal before sending | "Consent was for <old>; confirm again" | reconfirm |
| Verifier returns unusable JSON | one repeat per batch (same as `structured_call.rs`), then each claim in the batch `could_not_verify` with the cause → Queued | per claim | user resolves |
| Web search unavailable | audit runs without it; claims that needed it come back as the model reports (never fabricated) | note on the audit line | — |

### 15.7 Persistence and commands

`learning_lesson_audits (lesson_id REFERENCES learning_lesson_reviews ON DELETE CASCADE, draft_sha256, verifier_model, policy, state CHECK IN ('running','completed','stopped'), batches_json, input_tokens, output_tokens, started_at, finished_at, PRIMARY KEY(lesson_id, draft_sha256, verifier_model))`; `learning_programs.verifier_audit`, `verifier_consent_at`, `verifier_consent_provider`, `verifier_web_search`. Commands: `set_learning_program_verifier_audit(program_id, on, web_search, consent_confirmed)`, `audit_learning_lesson_now(program_id, lesson_id, operation_id)`. Exported in packs (audit rows ride with the ledger).

## 15. Checker accuracy, measured (decided)

Every human verdict and every audit disagreement is a labelled example of the local checker's behaviour:

| Event | Label | Local checker outcome | Counts as |
|---|---|---|---|
| user Confirms a Queued Unsupported/Contradicted claim | true | flagged | **false flag** |
| user adds evidence and the checker then says Supported (same passage was retrievable before? only if the passage existed in the collection before — else it is a retrieval miss, labelled separately) | true | flagged | false flag (judge) or retrieval miss |
| user Removes or Edits a flagged claim | false (as written) | flagged | true flag |
| Verifier contradicts a locally Supported claim and the user agrees (removes/edits) | false | passed | **false pass** |
| Verifier contradicts and the user confirms with a note | true | passed | true pass (Verifier false flag; recorded for the Verifier too) |
| user accepts a general-knowledge proposal | n/a | — | not scored |

- Recording: `learning_checker_examples (id, program_id, lesson_id, claim_id, checker_model, context_window, policy, claim, passages_json, local_verdict, label, label_source CHECK IN ('user_confirm','user_remove','user_edit','evidence_added','verifier_agree','verifier_disagree'), at)` in the §7.1 migration; written by the claim service on each event. Passages are the exact texts compared (they are already in `evidence_json`).
- Reuse of the existing eval: `evals/grounding/claim-pairs-v1.jsonl` is the format the live eval reads (`lessons/live_claim_pairs_eval.rs:29-35`: `{id, claim, passage, label}`); a command `export_learning_checker_examples(program_id?)` writes the table as that JSONL into `evals/grounding/lesson-claims-<date>.jsonl` (the repo's documented eval home), so `live_claim_pairs_eval` scores any candidate checker on the user's own labelled claims; results append to `evals/grounding/results/` as today.
- Visible to the user: Settings → Models, under the Learning role's row: "Checker accuracy on your lessons: 412 labelled claims · false flags 9.2% · false passes 1.1% (from 38 Verifier audits) · retrieval misses 14 — [Export for evaluation]"; computed by `get_learning_checker_accuracy()` from the table, per checker model, so switching the Learning model shows each model's own numbers. The Verifier's own false-flag rate (user confirmed over it) is shown on the Verifier row. No threshold or verdict is attached to the numbers; they are data for choosing.

*As built (track L1):* `claim_reviews/checker_examples.rs`. `ClaimReviewService` writes an example for Confirm (`true`), Remove and Edit (`false`) when the claim's latest local verdict was a judge's flag over passages (unsupported or contradicted); `record_evidence_added` (for the attach pipeline) labels `true` when the passage was in the collection before and `retrieval_miss` otherwise; an Undo deletes the example its verdict gave. `export_lines`/`export` write the `claim-pairs-v1` shape: a true claim is `supported`, a false one keeps the checker's verdict, the passages compared are joined into one `passage`, `difficulty` is the label source, and retrieval misses are left out. `accuracy()` gives `CheckerAccuracyDto` per checker model. The commands (`export_learning_checker_examples`, `get_learning_checker_accuracy`) are track S's to register; the recipe is in `evals/grounding/README.md`.

## 16. Implementation plan

Rules: at most 3 build agents at once; every cargo invocation as `lockf -k /Users/josh/Code/.lattice-cargo.lock cargo <args>`; `cargo check -p lattice-core -p lattice` and lib tests only until integration; grep before read; frontend `npm run type-check`, `npm run lint`, `npm run test:unit`. Base: `main` after `feat/per-chat-model` merges (track B rewrote the lesson loader to `PinnedModel` in `generation_jobs.rs` and added `model_requirements::ensure_window`; A0 defines `ModelRole`/`ModelRef`/`PinnedModel` that V extends; building on top avoids a second rewrite). Track R can start now on main.

```
R (runtime: errors, ceiling, retries) ──────────────┐
L0 (contract: domain types, port, DTOs, migration) ─┼─► L1 (ledger + loop + review pass + accuracy table) ─┐
                                                    ├─► S  (sources/attach, commands, bindings) ──────────┼─► F (frontend) ─► I (integration)
                                                    └─► V  (Verifier role + audit; after feat/per-chat-model) ─┘
```
R, L0 first (2 agents; L0 is small). Then L1 and S concurrently (+ R if still running = 3). V starts when one of L1/S finishes **and** `feat/per-chat-model` is on main (it needs A0's `ModelRole`, A1's `load_ref`, B's `learning_llm` call shape). F after S and V regenerate bindings. I alone.

| Track | Owned files (exclusive) | Depends on | Done when |
|---|---|---|---|
| **R** | `llm/llama_cpp/{streaming.rs, retry.rs, request.rs, mod.rs}`, `llm/scheduler/{mod.rs, scheduled.rs, rejection.rs (new)}`, `crates/lattice-core/src/shared/error/types.rs` (`ModelRejected`) and its `ipc/conversions.rs` arm, `crates/lattice-core/src/shared/runtime/jobs/{mod.rs, runtime.rs}`, `learning/planning/generation_recovery.rs`, `src/lib/bindings.ts` (regen), their tests | nothing; R touches neither `llm_port.rs` nor `max_context_tokens` | §12 src-tauri unit list green; a request that exceeds the configured window is refused before sending; identical 500 twice → one `ModelRejected`; a transient failure still retries |
| **L0** | `crates/lattice-core/src/domain/learning/{claim_review.rs, mod.rs}`, `crates/lattice-core/src/application/ports/claim_review.rs` (+ `ports/mod.rs` export), `migrations/20261016150000_lesson_claim_reviews.sql`, `learning/lessons/claim_reviews/{mod.rs, dto.rs}` (types only) | nothing | `cargo test -p lattice-core` green; DTO JSON fixtures committed |
| **L1** | `learning/lessons/content_verification/{repair_loop.rs, repair.rs, evidence_checks.rs, general_knowledge.rs, learner_view.rs, report.rs, research.rs (hint query), mod.rs (ledger hooks)}`, new `content_verification/{claim_ledger.rs, review_pass.rs}`, `learning/lessons/{lesson_drafts.rs, lesson_progress.rs, lesson_evidence.rs, generation_jobs.rs (outcome), text_edits.rs}`, `learning/planning/curriculum.rs` (activity counts), `claim_reviews/{repository.rs, service.rs}` | L0; after per-chat-model B (`generation_jobs.rs`, `model_call.rs`) | §12 integration list green; `grep -rn "Repairs stopped reducing" src-tauri/src` empty |
| **S** | `learning/plugin/{claims.rs (new), sources.rs}`, `learning/plugin.rs` (register), `learning/references/{source_workflows.rs, source_library.rs, sources.rs}`, `learning/lessons/reference_collection.rs` (`retrieve_from`), `claim_reviews/attach.rs`, `web/snapshots` (expose a capture-by-url service to the container; no feature→feature edge: learning calls the web service port it already holds, `generation_jobs.rs:43 research_web`), `learning/portability/pack_repository/{export.rs, importers.rs}`, `src/bin/export_bindings/main.rs`, `src/lib/bindings.ts` (regen) | L0; after per-chat-model A0/B (`export_bindings/main.rs`, `plugin.rs`) | commands callable; §5.4 fixtures (403, thin, paywall) produce the table's messages |
| **F** | `src/features/learning/lessons/review/**` (new), `src/features/learning/lessons/{LessonCard.tsx, LessonPreparationProgress.tsx, LessonEvidencePanel.tsx, lessonPreparationActivity.ts}`, `src/features/learning/sources/SourcesPanel.tsx` (chips), `src/features/learning/api/{client.ts, evidenceQueries.ts, learningQueryKeys.ts}`, tests | S bindings; no overlap with per-chat-model C (C owns chat/settings/model files and `src/lib/api.ts`; F uses `learning/api/client.ts`) | §12 frontend list green |
| **V** | `crates/lattice-core/src/application/ports/llm_port.rs` (`ModelRole::Verifier`, `ModelOrigin::VerifierRole`), `crates/lattice-core/src/application/services/chat_model_resolution.rs` (`resolve_verifier`), `crates/lattice-core/src/application/contracts/settings/llm.rs` (`verifier_model`), `src-tauri/src/features/llm/di.rs` (`verifier_llm`), `learning/lessons/content_verification/audit/{mod.rs, batches.rs, policy.rs, tools.rs}` (new), `learning/plugin/audit.rs` (new; commands §14.7), `learning/planning/curriculum_repository/programs.rs` (opt-in columns), `learning/dto/programs.rs` (`verifierAudit`, consent), `src/features/settings/components/modelRoles/roleConfig.ts` + `RoleButton.tsx` (fifth role; **C's files, so V edits them only after C lands**), their tests | L0 (ledger event types), **`feat/per-chat-model` merged** (A0 types, A1 `load_ref`/pool, B `learning_llm` pattern, C roles UI) | §14.6 fixtures (no key, 429, oversize batch, revoke mid-run, partial resume) green against a fake cloud; a dispute reaches the ledger and blocks publish |
| **L1 (addition)** | `learning_checker_examples` writes in `claim_reviews/service.rs`; `export_learning_checker_examples` + `get_learning_checker_accuracy` in `learning/plugin/claims.rs` (S registers); `evals/grounding/README.md` gains the export/run recipe | — | an exported JSONL runs through `live_claim_pairs_eval` unchanged |
| **F (addition)** | course Plan tab switch + consent dialog (`src/features/learning/curriculum/PlanPanel.tsx`), audit progress and the two-column dispute block in `ClaimReviewCard.tsx`, accuracy line under the Learning/Verifier rows (`src/features/settings/components/modelRoles/LocalModelRow.tsx`, after C) | V, S | — |
| **I** | integration: bindings `--check`, `cargo test` workspace once, `npm run test:all` minus desktop, manual script (§12 plus: opt in with a fake provider, see a dispute, confirm with note, publish; revoke mid-run), as-built notes in this doc, PR | all | PR open |

**Collision surface with `feat/per-chat-model` (sequence, do not merge-race):**

| File | Theirs | Ours | Order |
|---|---|---|---|
| `crates/lattice-core/src/application/ports/llm_port.rs` | A0 adds `ModelRole`, `ModelRequirements`, `PinnedModel` | none (R stays in `scheduled.rs`) | — |
| `learning/lessons/generation_jobs.rs` | B: `PinnedModel` loader, `ensure_window` | L1: `needs_review` outcome, ledger at sync points | after B |
| `learning/model_call.rs` | B: +12 lines (refusal) | R/L1: exact count before send | after B |
| `learning/plugin.rs`, `src/bin/export_bindings/main.rs` | A0/B register commands | S registers three | after B |
| `learning/lessons/generation.rs`, `generation/*.rs` | B | none | — |
| `learning/model_requirements.rs` (new in B) | B: floors from measured sizes | none; the configured window stays the ceiling B's refusal measures against | — |
| `src/lib/bindings.ts` | regenerated by A0/C | regenerated by S | regen at I |
| `llm/di.rs`, `instances.rs`, `loading.rs` | A1 | V adds `verifier_llm` to `di.rs` | after A1 |
| `crates/…/ports/llm_port.rs`, `services/chat_model_resolution.rs`, `contracts/settings/llm.rs` | A0 | V adds the Verifier role, resolver and setting | after A0 |
| `src/features/settings/components/modelRoles/{roleConfig.ts, RoleButton.tsx, LocalModelRow.tsx}` | C | V/F add the fifth role and the accuracy line | after C |

## 17. Citation index (verified in `lattice-main-ro` at `c1cd385e`)

Additional for §14-15: `llm/cloud.rs:16, 44, 91-92, 159, 480, 534-536, 628-631` · `llm/loading.rs:157-183` (keyring key lookup) · `web/traits/web.rs:105` (`search_web`) · `lessons/live_claim_pairs_eval.rs:1-11, 29-35, 37-45` · `evals/grounding/{claim-pairs-v1.jsonl, results/, README.md}` · `migrations/20260930020000_learning_studio.sql:1-5` (`learning_programs`) · per-chat-model design §2.1, §2.3, §3.3, §4.1-4.2, §5.1, §6.3, §13 (A0/A1/B/C ownership).



`repair_loop.rs:1-13, 36-199, 155-158, 170-176, 202-241, 255-283, 302-331, 332-370, 358-397` · `repair.rs:142-144, 155-253, 260-286, 358-410` · `content_verification/mod.rs:239-249, 287-302, 521` · `claim_reuse.rs:62-100` · `general_knowledge.rs:14-27, 50-55` · `evidence_checks.rs:547-560, 700-741` · `research.rs:156-190, 191-230, 300-330` · `evidence_selection.rs:64-94` · `section_checkpoints.rs:13-24` · `learner_view.rs:19-60` · `report.rs:36-100, 103-121` · `execution.rs:147-170, 293` · `text_edits.rs:8` · `lesson_evidence.rs:12-51, 55-66` · `lesson_drafts.rs:18-33, 72-79, 192-240` · `lesson_progress.rs:160-199` · `reference_collection.rs:228-283, 299-306, 403-421` · `generation_jobs.rs:40-78, 325-345` · `planning/generation_recovery.rs:17-48` · `planning/curriculum.rs:188-216` · `planning/curriculum_repository/preparation.rs:103-116` · `learning/plugin/sources.rs:41-53, 57-123, 124, 171, 264-276` · `learning/references/source_workflows.rs:10-22, 81` · `learning/references/sources.rs:90` · `learning/references/source_library.rs:97-107` · `learning/dto.rs:53-66, 243` · `learning/model_call.rs:75-122` · `llm/llama_cpp/request.rs:23-37, 42-56, 66-91` · `llm/llama_cpp/streaming.rs:43-60, 80-96, 148-154` · `llm/llama_cpp/retry.rs:117-178, 203-206, 241-247` · `llm/llama_cpp/mod.rs:88, 196-245, 359, 492-517` · `llm/scheduler/mod.rs:60-90, 204-208, 292-310` · `llm/scheduler/registry.rs:86-157` · `llm/scheduler/scheduled.rs:58-65, 113, 212-230` · `web/services/web.rs:424-430` · `web/services/web/page.rs:161, 325-328` · `web/traits/web.rs:128-141` · `web/snapshots/commands.rs:16` · `web/snapshots/job.rs:218` · `crates/lattice-core/src/shared/runtime/jobs/mod.rs:125-174` · `jobs/runtime.rs:476-491, 537-541` · `jobs/repository.rs:184` · `portability/pack_repository/export.rs:239, 313` · `content_verification/portability.rs:27` · `migrations/20260930020000_learning_studio.sql:11-16` · `migrations/20260930050000_learning_source_library.sql:1-40` · `migrations/20260930080000_learning_curriculum_jobs.sql:53-95` · `migrations/20261004120000_learning_lesson_verification.sql` · `migrations/20261006000000_learning_generation_checkpoints.sql` · `src/features/learning/lessons/LessonPreparationProgress.tsx:22-26, 44-49, 71-74, 91-103` · `LessonEvidencePanel.tsx:7-11, 26-60, 62-121` · `LessonCard.tsx:35-41, 62, 79, 84` · `src/features/reading/model/sourceAdapter.ts:446-475` · `src/features/learning/curriculum/useLearningPlan.ts:85-86` · `src/features/learning/api/client.ts` · per-chat-model design §5.4, §6.2, §6.3, §13 (worktree `lattice-chat-model`).
