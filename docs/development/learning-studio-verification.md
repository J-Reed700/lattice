# Learning Studio verification and release evidence

Learning Studio records study activity and outcome evidence. It does not claim
that a learner has mastered a subject, is ready for employment, or will retain
material. A release result is valid only for the exact commit, platform, model,
and optional runtime named in its retained artifacts.

## Saved outline drafts and repair

Course generation saves the first structurally valid outline and its captured
references before reviewing it. The raw candidate, including its quotations,
review findings and ordered reference IDs, is checkpointed in SQLite. Each module
correction receives another checkpoint before rechecking, so cancellation, app
restart or a later provider failure does not discard completed work.

The draft's review state is **unchecked**, **needs repair** or **passed**. These
states describe the outline only. A saved outline is neither a prepared lesson
nor a factual correctness guarantee. Backend acceptance rejects unchecked or
unresolved drafts. Lesson preparation retains its separate verification gate;
this repair workflow does not edit already prepared lessons.

Initial generation and resumed repair automatically search for supporting pages
when references are missing or review reports unresolved findings. There is no
web-research checkbox. A topic-only draft is saved before research and stays
unresolved if no usable references can be captured.

Automatic repair edits affected modules and continues while the unresolved
finding count decreases. Newly captured references allow another attempt even
if the count has not decreased. Each operation searches the course goal and each
affected module at most once, using stable module positions so rewritten titles
cannot restart the same searches. Duplicate URLs and identical captures are not
added again. Without new evidence or fewer findings, the remaining issues stay
visible for another manual attempt. There is no
overall or per-call elapsed-time deadline for outline authoring/review/repair;
user cancellation, provider errors and transport stall detection still apply.
Each call uses a fresh prompt containing the current artifact and relevant
passages, not an accumulating conversation transcript.
If a module rewrite breaks a previously valid quotation attached to an unchanged
claim, the original citation is retained. Changed claims still need their evidence
checked. After a completed semantic review, repairs can reuse its checks for
unchanged modules. The next request contains the complete changed or unresolved
modules and their retrieved evidence, plus every module's instructional fields
(without repeated quotations) for checking cross-course effects. It can report
new problems anywhere in that sequence. Exact quotation checks still run across
the entire outline.

A separate persisted review receipt records module hashes and the context of the
last completed semantic check; merely saving an edited draft does not update it.
Changed source content or order, learner request, model name, review protocol,
module/lesson identities or course-wide findings require full review. Missing
receipts also require full review. Failed or interrupted checks cannot create a
receipt or approve the draft. The UI labels findings from an incomplete recheck
as previous findings awaiting verification, and displays whether the operation
is checking the whole outline or selected modules. Incremental checks reduce
repeated model input; they do not guarantee latency or factual correctness.

Before research or review, formatting-only quote mismatches are repaired by
copying an exact span from the same saved reference. This handles straight versus
curly quotation marks and whitespace without a model rewrite. Words, numbers,
operators and source indices must still match; fabricated or paraphrased quotes
remain findings. Restored quotations are checkpointed as unchecked and still
require factual/instructional review of the affected content. The strict quote
validator itself is unchanged.

Review and repair receive an explicit module-number map: JSON paths and
prerequisite indices start at zero, while learner-facing module numbers start
at one. Reviewers must not report a valid reference to an earlier displayed
module as a self-reference because its JSON array index differs. Repairs also
reconsider prior findings using that map instead of blindly applying proposed
renumbering; module titles are preferred in learner-facing prose.

Within the running renderer, generation and repair progress remain in the query
cache when the learner leaves and reopens a draft. Reopening reconnects the
progress and Cancel controls to the existing request rather than offering a
second repair. Restarting the application still requires resuming the saved
draft explicitly.

To try draft recovery:

1. Generate an outline. Once the first draft is saved, choose **Inspect saved
   draft** while review continues. The generation panel shows the current stage,
   elapsed time, model and received response text length.
2. Cancel during review, or reopen a draft after restarting the app. The draft
   remains in Programs. Its findings show the affected module/lesson, claim,
   unmatched quotation where applicable, and reason.
3. Choose **Repair remaining issues** to resume a stopped draft. Both initial
   generation and resumed repair automatically search using the goal and affected
   module titles. **Finding supporting references** appears during research.
   Research fetches complete pages through
   the existing safe web service and saves them in the course source library.
   Search snippets and truncated captures are not evidence. Fetched pages still
   require relevance/support review; search rank does not establish authority.
4. Inspect the revised findings. Acceptance becomes available only when the
   outline checks pass. Source quote matching remains deterministic even if an
   AI reviewer approves an invalid quotation.

Repairs retain module and lesson identities and counts, and use revision checks
to reject concurrent stale writes. Checkpoints retain previous candidates and
findings; a history/rollback browser is not part of this UI. Initial authoring
still requires a complete structurally usable outline; incremental module
creation and durable lesson-content repair are separate work.

Lesson preparation reports its durable job in the lesson workspace, including
reference indexing batches, writing, answer-key and teaching review, claim
extraction, coverage checking, and completed claim checks. Active jobs are polled
while browsing; reopening the course rejoins the saved job. Cancel and Retry
are available there, and completion refreshes the lesson content. Model response
character counts describe received answer text, not hidden reasoning or a
completion percentage. A waiting message is not proof that the model is moving.

The progress panel uses a persisted, typed activity record rather than guessing
the phase from status text. It shows the current lesson and phase, elapsed time
for the run and phase, factual checks completed/supported/unresolved/remaining,
reused comparisons, model output activity and retries, checkpoint time, and recent
phase transitions. Its determinate bar describes one claim-verification pass,
never an overall completion percentage or ETA. Research and repair are explicit
phases with an explanation of why verification can repeat. The same panel appears
in Lessons and Plan; details can be collapsed while the job continues. No recent
output is described as an observation, not proof of a stuck model. The history
contains phase labels and timestamps, not lesson answers or hidden reasoning.

Lesson preparation uses the persisted generation-job queue as a local outbox.
The command commits a pending job and acknowledges it immediately, before source
refresh or model requests. The UI reloads and observes the saved job; it does not
hold an IPC call open until publication. A renderer-independent dispatcher also
reconciles pending work. The desktop single-instance guard runs
before startup recovery, and an atomic database claim plus in-process registration
deduplicates delivery. Recovery runs once before dispatch, never from an elapsed
time heuristic that could steal a healthy, slow model request.

On macOS, an active lesson job holds a scoped `NSProcessInfo` user-initiated
activity so hiding the window or switching apps does not make the work eligible
for App Nap. The assertion allows normal system sleep and ends on completion,
cancellation, or failure; queued work and retry backoff hold no assertion.
Preparation has no focus-loss cancellation handler. Its UI polls in the
background, but the native worker does not depend on those polls to continue.

Quitting returns unfinished lesson jobs to pending. After a crash, startup
requeues running lesson jobs with the same ID and checkpoints. Explicitly
cancelled, failed, and completed jobs are not automatically restarted. A failed
or cancelled job can be retried explicitly with its saved work. Local preparation
does not execute while the app is closed; it continues on reopening. Outline
draft recovery still uses the separate explicit repair action described above.

The lesson UI exposes this durable stop/retry path as **Pause lesson preparation**
and **Resume lesson preparation**. Pause cancels the active request while retaining
the draft, activity, and completed checkpoints; the stored attempt is `cancelled`
and remains excluded from automatic recovery. Resume creates a pending attempt
with the saved work and reloads the current model settings. These controls remain
visible when progress details are collapsed. A failure still shows its error and
**Retry**, separately from an intentional pause. Lesson writing and factual
verification use the main model, not the utility model. The authoring input hash
and verification checks are bound to that model, so changing the main model
currently restarts drafting and verification. A utility-only change does not
invalidate those checkpoints. The paused UI explains this before resuming.

Temporary network, service-unavailable, and rate-limit errors defer lesson jobs
in that same outbox, preserving their ID, draft, and completed checkpoints.
Learning-repository connection-pool acquisition timeouts are classified as
temporary service failures; other database errors retain their failure status.
The persisted redelivery delay grows from 30 seconds to at most five minutes;
this limits retry frequency, not job duration or total attempts. A waiting job
releases its worker slot and displays the interruption with automatic retry.
Connection loss, model-service failures, and rate limits have distinct progress
messages. Repeated service failures are described as preventing progress, rather
than evidence that the laptop is offline. Sleep can break an in-flight request;
unfinished work repeats when connectivity returns. Cancellation wins over late
errors. Invalid input and unusable verification responses remain explicit
failures or unresolved checks, never automatic approval.

Learner progress does not invalidate background preparation. Completing a ready
lesson or submitting an assessment can advance the UI revision while the same
job resumes, retries, and publishes. A separate content revision advances
transactionally when course objectives, sequence, or saved teaching changes.
Such edits reject an old job's publication and require preparation for the updated
course. Publication preserves intervening learner progress. Source changes are
validated through evidence bindings separately, including research performed by
the job itself.

Every completed factual comparison is checkpointed. Approvals require both strict
evidence checking and independent challenge. Each key binds the exact claim, quote,
ordered source versions, passage bytes and offsets, checker policy, configured
model name and context size. Retrieval runs again on resume. Changed evidence or
checker inputs require a new comparison; failed or incomplete model responses
are never reusable. Concurrent checks save independently as they finish, even if
an earlier call is slow. A crash may repeat in-flight requests, so model-call
delivery is at least once; it is not an exactly-once billing guarantee.

Structured providers verify unfinished factual comparisons in groups of up to
eight. Exact repeated passages are sent once in a shared bank, with an explicit
allowed passage list for each claim. Both strict evidence checking and the
separate challenge pass retain individual decisions. Missing or duplicated IDs,
incomplete replies, or citations outside a claim's assigned evidence cannot
approve it. Unusable grouped decisions fall back to individual checks; valid
sibling results save before those fallbacks run. Non-structured providers keep
the individual path.

Groups split when their full evidence and response reserve do not fit the model
context, or when a server failure survives the provider's request retries. The
smaller groups keep each claim's complete assigned evidence and both checking
stages. The split plan is checkpointed against the exact comparisons, so reopening
does not repeat a known failing group. Connection loss, rate limits, and invalid
requests propagate without multiplying requests. An individual service failure
still defers the job; splitting cannot approve or skip a check. Passages are not
summarized or discarded to make a group fit. One
group runs at a time to avoid competing large prompt prefills. Each claim still
owns its receipt, so resuming
or changing one comparison does not require repeating its completed siblings.
The UI reports each completed claim immediately, including completed siblings
while a smaller group or individual fallback is still running. It reports claims
checked, not model request count. Verification policy v17
invalidates older factual approvals while preserving unchanged drafts and the
separately versioned claim inventory and coverage audit.

Completed research queries and verified lesson material also have durable
checkpoints. Reference captures and completed indexes retain their existing
persistence. Before reusing a prepared lesson, the worker validates its report,
content hash, model, policy, and current source collection. Publication still
validates evidence inside the transaction that saves lessons and completes the
job, so a restart cannot expose partially published material or duplicate it.

Durable lesson jobs have no overall or model-call elapsed-time deadline. Their
claim checks also opt out of the chat verifier's shared deadline. Cancellation
still drops in-flight requests, and failed or incomplete checks cannot publish
the lesson. Other interactive material calls and chat keep their own budgets.
Structured lesson calls use the provider adapter's transient connection retries.
Explicit transient server statuses inside an SSE response also retry, including
a `500 server_error` received after HTTP 200. Context and protocol errors do not.
HTTP and streamed server failures retain the service-unavailable category; client
request errors remain terminal instead of becoming connection retries. After an
inference service failure, the next attempt disables server prompt-cache reuse
and reprocesses the identical full input. This leaves the draft, local evidence
index, completed verification receipts, and output allowance unchanged. Only
fixed descriptions of recognized server errors are exposed; arbitrary server
messages can contain private input and are never echoed.
An abandoned partial JSON response is discarded, and progress shows the new
request attempt with its response-character count reset; fragments from separate
attempts are never joined into a candidate.

Teaching review uses application-assigned section and passage IDs. The complete
section body is split without changing any bytes, including Markdown, Unicode,
code indentation and string whitespace. Each section must receive a valid
finding tied to one of its own passages. IDs establish the location being
reviewed; they do not establish factual correctness. The separate answer-key,
claim-coverage and evidence checks remain required.
The teaching reviewer treats original authoring excerpts as a partial selection.
New research can support a repaired detail that is absent from those excerpts;
absence alone is not a teaching defect or proof of a false claim. Concrete
errors, internal contradictions and misrepresented citations remain defects.
The full evidence gate still checks every extracted claim against the current
saved collection. The teaching receipt policy changes when this scope changes,
so an earlier teaching review cannot be reused as a new-policy approval.
For prepared lessons, model review findings are proposals, not authoritative
corrections. Before a proposal can trigger rewriting, a separate evidence check
compares it with the candidate, authoring requirements, and passages retrieved
from the current saved reference collection. Factual corrections require reference
evidence; contract violations require both the stated requirement and affected
content; internal conflicts require candidate evidence. Unsupported reviewer
assertions do not drive edits. This is another fallible model judgment, not a
guarantee. It cannot override deterministic schema or quotation failures, and
the complete factual-verification gate still runs before publication. Teaching
receipts include the review policy and hashes of the current reference snapshots,
so new evidence invalidates a previous teaching approval. Regression fixtures
exercise valid and invalid criticisms across subjects; their expected answers
and subject labels are never supplied to the production checker.
Lesson quotations also run through the publication validator during teaching
review, before claim extraction and factual checking. Exact failing field paths
are supplied to the repair, and a remaining mismatch blocks further work even
if the model approves it. This avoids spending a full factual pass on a draft
that is already known to fail publication's quotation check.
Before that check, typography-only mismatches are restored from exact bytes in
the same cited excerpt, including curly quotes and original whitespace. This
uses the outline citation restoration logic and preserves the lesson text,
source index, words, quantities and operators. The restored span must still pass
the ordinary publication validator; unrelated or fabricated quotes need repair.

Factual claim extraction also selects application-owned passage IDs. Sections
have explicit indices, and the extractor receives four sections per request.
Extraction records what the lesson asserts, including false or inconsistent
assertions, without silently repairing them. Attributed quotations include their
exact wording and attribution as claims; a paraphrase of the meaning is not a
complete record of a quotation. The later evidence check determines support.
Passages preserve the original text, including whitespace in code and data;
the server resolves each selected ID back to that section's exact text. A
section-specific schema branch binds each returned index to its own passage
IDs, preventing constrained generation from pairing an index with another
section's otherwise-valid ID. Runtime validation still enforces the same rule
for providers that do not enforce the response schema. A
malformed response receives a targeted correction request while valid section
inventories are retained. Coverage correction handles one affected section at a
time, with its original text, suspected passages, finding and current assertions.
The model returns additions or explicit replacements using application-owned
claim IDs; the application retains unmentioned claims and rejects foreign IDs,
duplicate replacements and invalid passage locations. A finding may name the
wrong passage, so correction locates the actual assertion within that section.
It does not repeatedly regenerate complete inventories or repeat unrelated
sections' findings without their text. Each changed inventory still requires
a fresh coverage audit and factual verification.
Missing sections, duplicate indices and invented or
cross-section passage IDs cannot approve a lesson. A separate coverage audit
still checks the inventory against every complete section. Missing assertions
receive targeted inventory correction and another coverage audit, rather than
a lesson rewrite. The correction retains complete section inventories verbatim;
only corrected sections need another audit while the lesson itself is unchanged.
Correction continues while the audit reduces incomplete sections or unresolved
passages, or captures more claims. These app-counted measures must improve on
each pass; otherwise the draft is retained with the unresolved findings. This
prevents an unchanged correction loop without a wall-clock or attempt cutoff.
Completed coverage batches also save findings for sections with omissions.
Restarting can reuse those findings instead of repeating completed reviews; an
omission remains unresolved. These receipts bind the original section, extracted
assertions, model and coverage policies. A corrected inventory requires a new
audit, and an interrupted batch cannot create a completed receipt.
The audit receives full section text and standalone claims
without repeating the same location passage for every claim.

Coverage review separately accounts for every one of these passages, in groups
of four sections. Its response uses required passage-ID keys, with claim IDs
restricted to the same section, explicit nonfactual reasons, and missing or
weakened assertions. A section-level summary cannot substitute for these
decisions. The review checks qualifiers and every asserted consequence,
including factual assertions in headings and persuasive language. It checks
faithful representation even when an assertion is false; evidence judgment
comes afterward. These are model judgments and can still miss errors.
For passages mapped to claims without reported omissions, the shared claim
checker separately compares the original wording with the extracted statements
selected by that passage's mapping. If the selected subset fails, the checker
also compares the passage against the complete inventory for that same section.
This distinguishes an omitted assertion from an assertion the mapper failed to
select. Only extracted assertions are evidence in this comparison; original
section text supplies interpretation context, and other sections are excluded.
An assertion found only in the original section text remains missing from the
inventory; it cannot serve as its own evidence of extraction.
Every inventory claim still goes through the separate factual-evidence gate.
This fidelity check can reject a complete mapping when an asserted
consequence or qualifier was lost. It treats instructor choices and stipulated
exercise inputs as instructions while still checking empirical guarantees.
Passing fidelity establishes representation only, never factual approval.
If an isolated teaching passage fails this comparison, a second comparison
receives its original section as interpretation context. This restores local
scope and references that extraction could read but the isolated comparison
could not. Context cannot override an explicitly broader claim or substitute
for a consequence missing from the selected inventory statements. An unusable
comparison still fails closed. The unchanged base comparison runs first, so
previous positive base checks remain valid. Negative checks from before this
fallback are reconsidered; a changed teaching-context policy expires approvals
that could have used an earlier fallback. These receipts remain bound to the
complete lesson content and separate from assessment-context receipts.
For assessment passages, the comparison also receives the question's scenario,
answer index and target field as interpretation context. A conditional answer
must be read within its question. This original context is explicitly separate
from the selected inventory statements and cannot supply missing assertions as
evidence. Distractors are distinguished from endorsed answers.
Conditional inventory statements also require the scenario to establish their
prerequisites at the same scope. A related local property cannot silently stand
in for an unstated condition about a surrounding structure or dependency.
Assessment-context checks have their own policy receipt: changing those rules
expires assessment extraction and audits while retaining teaching audits with identical inputs
and unchanged checking instructions. A change to the common coverage policy
still expires every coverage audit.
The application uses the validated answer index to keep unendorsed alternatives
as separate interpretation context, outside factual claim locations. Their bytes
are retained, but the checker is not asked to prove deliberately incorrect choices
true. Question premises, the selected answer, and every explanatory assertion
remain coverage targets. The independent blinded answer-key review still checks
all alternatives; this separation cannot validate an incorrect answer key.
Missing or invalid indices do not exempt any choice from coverage.
An unusable fidelity response preserves the draft and blocks publication.
Coverage-policy receipts are separate from extraction checkpoints. A policy
change may reuse the unchanged extracted claims as data, but must audit their
coverage again before any factual approval can publish a lesson.
Completed extraction and corrected inventories are checkpointed before the next
audit. Interruption or a failed audit preserves those claims as data without
retaining an audit approval for the changed inventory.
The checkpoint retains completed, unchanged section audits as well. Retry with
the same content and coverage policy resumes pending sections or the remaining
corrections; duplicate or foreign section and passage locations invalidate reuse.

Factual retrieval restores surrounding source text before judging a matching
passage. Nearby conditions, exceptions and introductory definitions must not be
lost just because they scored below the matching paragraph. Overlapping context
windows are merged without changing source bytes or dropping either original
hit. Verification allows 24,000 evidence characters, and the configured model's
context check still applies. Authoring and general search keep their existing
windows. More context reduces some retrieval omissions; it cannot establish
that a saved source is correct or guarantee that every relevant exception was
retrieved.

Strict factual judgments reuse the chat claim checker with application-assigned
evidence passage IDs. The judge still evaluates entailment and conflicts and
must supply an explicit verdict and reason; reported verdict confidence is
required for approval when the provider supplies it. An uncertain or inconsistent
label remains unsupported and can trigger research; it is not treated as a
broken request. Supported and contradicted judgments must select a valid
passage ID. The application attaches that passage's original bytes instead of
asking the model to transcribe line wrapping, code indentation or quotation
marks. Missing verdicts, incomplete responses or invalid locations receive one
format correction; unresolved responses cannot approve a lesson. Durable checks
use the model's remaining context and configured output allowance rather than
the small reply allowance used for chat checks. This adds no elapsed-time limit,
and output cut off by the provider is never accepted as a factual approval.
Durable strict judgments request model reasoning and return the evidence
comparison before a single final verdict. A premature or duplicate verdict is
invalid. When a provider joins labeled fields onto one line, the parser restores
field separators only if each label appears exactly once and in the required
order. Missing fields, ambiguous labels, trailing output and invalid passage
identifiers still cannot approve a claim. Correctly separated fields are parsed
by their line boundaries, so ordinary "source passage:" wording inside an
explanation is not mistaken for a second citation field. First-token probabilities are not used for this protocol because they
describe the comparison or model reasoning, not its final decision. The chat check's fast
reasoning-disabled setting does not apply to lesson publication. This advances
the factual verification policy without invalidating unchanged, source-independent
claim inventories. Existing factual approvals cannot satisfy the new policy.

The publication prompt omits the chat rule that one supporting source is enough.
It explicitly compares supporting passages with limitations and conflicting
passages before deciding. An unresolved conflict remains unsupported even when
another source repeats the claim. Chat and inventory-fidelity instructions keep
their separate policies. A canonical three-line judgment may echo the same
verdict at the end of its reason; conflicting echoes, extra result lines and
ambiguous field layouts remain invalid.

Publication also challenges each provisionally supported claim in a separate
model request. Source entailment alone can approve a false assertion repeated by
an unreliable reference. The challenge uses the same claim and passages to ask
whether exceptions, omitted conditions or source reliability require independent
research. It may use model knowledge to formulate public search questions, but
that knowledge cannot establish a contradiction or approve a claim. Open
questions mark the claim unsupported and enter the existing research/repair
workflow; they are explicitly distinguished from established errors. An empty
question list retains the original evidence judgment. Failed, malformed or
incomplete challenge responses leave the claim unchecked. The publication policy
versions this extra requirement, and unchanged comparisons may be reused only
inside the same operation. This is another fallible check, not a factual guarantee
or a separate independently trained model.

Extraction, evidence comparison, research and repair use shared subject-neutral
instructions. A particular lesson's failed claim and source passages are repair
data, not global rules for that subject. Scope and exception checks apply to
every claim, including quantifiers, conditions, lists and claimed consequences.
The opt-in `live_source_passage_judgments` test can exercise
`content_verification/claim_scope_cases.json` via `LATTICE_CLAIM_CASES`.
These are controlled evidence fixtures covering biology, baking and programming,
with both supported and overbroad claims. Domain labels and expected verdicts
are not sent to the checker. They test the same comparison protocol across
subjects; passing them does not establish broad factual accuracy. Executable
examples separately use the available language runtimes and their restrictions.
The same live harness can check assessment-context preservation with
`LATTICE_CLAIM_POLICY=fidelity` and
`content_verification/fidelity_context_cases.json`. These paired cases distinguish
an explicitly established prerequisite from an unrelated scenario fact across
subjects. The optional `context` field is interpretation data; expected verdicts
remain test assertions and are never included in model requests.
`content_verification/fidelity_teaching_context_cases.json` uses the same
fidelity harness with `contextKind: "teaching"`. It tests local scope against
explicit universal assertions and omitted consequences across subjects.
`content_verification/claim_prerequisite_cases.json` exercises the corresponding
conditional-source comparisons with `LATTICE_CLAIM_POLICY=strict`. The factual
judge also requires supplied premises to establish a source's prerequisite;
it cannot bridge different descriptions using unstated domain knowledge.
Before approval, it looks for a counter-scenario consistent with the claim's
premise and supplied evidence in which the conclusion fails. An unresolved
counter-scenario or missing connection remains unsupported. These are generic
comparison instructions, not subject-specific facts, and remain fallible model
judgments. Both the missing-premise and explicitly established-premise cases
must pass the live regression; rejecting every claim is not success.
These evidence-policy changes require fresh factual judgments while unchanged
coverage receipts remain valid under their own policies.
The live claim harness can also set `LATTICE_CLAIM_PROGRAM` and
`LATTICE_LESSON_DATABASE` to retrieve from an explicitly selected saved library
instead of using fixture passages. This exercises production hybrid retrieval,
context restoration and evidence comparison together, without editing lessons
or using expected verdicts as model input.
Set `LATTICE_CLAIM_POLICY=publication` to exercise the complete claim approval
path, including the independent challenge after strict source comparison.
`content_verification/evidence_challenge_cases.json` includes deliberately
misleading source assertions and correct controls across the same three
subjects. These fixtures test whether literal source agreement is challenged;
their subject labels and expected outcomes are never model instructions.

Unlabeled worked-example fences are classified before review. The classifier
selects a language or marks literal output/diagrams; the application inserts
only the opening fence label, preserving the fenced bytes. Missing, duplicate
or invented fence IDs block preparation. Python and JavaScript examples still
execute, while other languages retain the report's execution disclosure.

Unsupported and contradicted lesson claims automatically trigger focused reference research.
Complete captures are saved as immutable course sources and indexed using the
same retrieval model. Search snippets and truncated pages cannot become
evidence. Every claim retrieves evidence again from the expanded collection,
including previously supported claims, before content repair or publication.
Within a preparation operation or its resumed job, a completed judgment can be reused only
when the claim, its original lesson passage, section location, and ordered evidence passages are
unchanged. Evidence identity includes source version, exact text and byte range;
ranking scores are refreshed in the report. Selected passages have a stable
source/version/byte-range order in both the actual model request and the receipt,
so a ranking-only reorder cannot force another identical judgment. Selection
still runs against the current collection and retains contradictory passages.
Changed evidence requires a new
judgment, and failed or incomplete checks are never reused. This reuse does not
require repeating an identical comparison merely because another section was
repaired; publication still binds the report to the entire final lesson. Receipts
survive a retry or restart with the same checker policy and model identity.
They do not skip extraction, coverage, execution or retrieval checks; those stages
retain their own input validation. Progress explicitly counts reused comparisons. Adding a
source is not an approval; contradictory or unresolved checks still block the
lesson. Failed checker calls are kept distinct from missing source evidence.
Search results must respect explicit `site:` host/path constraints, including
after redirects; unrelated provider enrichment cannot bypass those constraints.
For every query, a separate selection step chooses relevant, credible references
from application-assigned result IDs before capture. It can reject all candidates;
result ranking and snippets cannot establish either relevance or factual approval.
Research continues while added references reduce unresolved claims. Repeated
queries are skipped; adding pages without reducing the gaps moves the draft to
content repair instead of researching indefinitely. Repair requests store each
exact evidence passage once in a shared table, with findings referencing its ID,
so repeated retrieval results do not multiply the request size.
Content repairs continue while the number of unresolved checks decreases. A
stalled repair leaves a saved draft rather than looping or publishing defects.
After research adds evidence, remaining contradictions trigger rewriting even
if another missing-evidence claim improved. Pure evidence gaps can continue
research while their count decreases.

Repair runs one affected teaching section or assessment item per request, with
only its recorded evidence and execution results. The application merges only
that position, preserves the other sections, and saves each patch together with
the remaining repair work. Interrupted repair resumes the remaining sections
before reviewing the assembled lesson. The complete result still receives
teaching, coverage, execution and factual checks before publication.

Completed claim extraction and its coverage audit are checkpointed for the
exact candidate, configured model, context size and verification policy. Retry can reuse this source-independent
inventory. Completed section inventories and coverage/fidelity audits also have
independent checkpoints bound to their exact section text, position, model,
context configuration and checking policies. Each finished audit batch is saved
before the next batch starts. A repair or interrupted audit therefore extracts
and audits only changed or unfinished sections; reordered sections and policy or
model changes invalidate their receipts. These representation checks cannot
approve factual content: current evidence is retrieved for every claim, changed
comparisons run again, and the complete assembled lesson still passes teaching
review, execution and publication validation.
Before a content repair, the failed checks and deduplicated evidence are saved
as a pending rewrite. If that request fails, Retry resumes the rewrite only when
the candidate, model, policy, instructions, schema and full reference contents
are unchanged. This checkpoint cannot approve a lesson: the rewritten candidate
still undergoes execution, extraction, coverage and evidence checks. A changed
reference collection requires diagnosis again.

A malformed review response receives one targeted correction request. Valid
section checks and reported defects are retained; the correction requests only
missing or invalid section checks and does not regenerate lesson content or
repeat the answer-key call. Unresolved response errors include diagnostic detail
and block publication without blaming learner input.
Blinded answer-key checks similarly retain valid checks and re-request missing,
duplicate or malformed question indices. Corrections still receive no authored
key or explanation. Duplicate indices are never silently reassigned by position,
and a repeated malformed response cannot approve a lesson. Both lesson repair
paths use the same review context so unchanged completed reviews remain reusable.

Durable lesson jobs save unpublished candidate content before review and after
content repairs. Retry carries that checkpoint into the new job. It reuses the
draft only when the model, lesson request, course revision and original complete
reference snapshots still match. Adding research references retains the original
authoring prompt and source-index mapping; replacing or removing an original
snapshot invalidates reuse. New references always enter factual verification.
A completed teaching/answer-key review also
saves a receipt tied to the exact candidate, reviewer, policy, schema and
authoring context; retry reuses that completed stage only while all those inputs
match. Content changes invalidate the receipt. Factual verification and the
publication report are still required. A saved draft is not publication approval
and is never exposed as a ready lesson. Jobs that failed before checkpoint
support was added have no candidate to recover.

Study also has a shared activity panel for other slow requests: diagnostics,
tutor feedback, assessments, practical activities, recall/flashcard generation,
environment setup, source acquisition, imports/exports, and slow reads or saves.
It observes the existing learning/study IPC calls without adding retries or
execution deadlines. Model and acquisition tasks appear after 800 ms; other
requests appear after four seconds, so routine reads and autosaves stay quiet.
These thresholds only control presentation.

The panel shows elapsed time, expandable task details, and bounded completion
and failure receipts. It stays available across Study screens and can reopen the
relevant course section after pending edits are saved. Collapsing or clearing
finished receipts cannot cancel active work. Receipts live in the query cache
for this app session; they are not persisted jobs or proof of model activity.
Requests without backend telemetry explicitly say that live stage updates are
unavailable. Outline/lesson generation and practical runs retain their existing
progress and cancellation controls; enqueue acknowledgments are never reported
as completed jobs by this panel.

## Reference collection MVP — 2026-10-04

Learning Studio now uses the same immutable reference collection for lesson
writing, claim checking, and the “Search related saved material” inspector.
The configured embedding model supplies persisted passage vectors; keyword BM25
and cosine rankings are combined with reciprocal rank fusion. The MVP reuses
`learning_source_retrieval_index`, with an exact scan of the course vectors,
rather than placing private historical snapshots in global document search.
The lesson writer retrieves for its objective; the verifier independently
retrieves for every extracted claim. Ranking scores are not correctness scores.

New course creation retains complete captured source text instead of only its
outline excerpts. Library documents must finish indexing before capture. Web
references use the existing safe DNS/redirect/body-limited reader with a separate
two-million-character allowance; a short chat cache cannot masquerade as a whole
book. The reference path does not use the bounded browser-display fallback.
Reference extraction keeps headings, ordered lists, code indentation, and table
rows. Initial imports are one page per URL; there is no website crawler.
The saved draft workflow automatically researches missing references and review
gaps as described above. The extraction can still omit information in images, complex tables,
or dynamic pages. Review the saved source text before relying on it.

Bounds: two million characters per source, twenty million per retrieval collection,
and the existing source-count limits. Old excerpt-only or truncated active
sources block new background lesson preparation until replaced with complete
material. Topic-only requests automatically seek references after saving the first
outline; missing evidence keeps that draft unresolved. Lesson preparation requires
references.
Completed per-source indexes are reused. Interrupted indexing cannot publish a
partial version; that version is retried, while completed versions are reused.
Changing the embedding identity or passage layout rebuilds its cached vectors.
If no embedding model is ready, the explicit keyword fallback remains available.
An inference error during indexing/querying fails the operation rather than
recording a successful hybrid check.

The production CSV branches have been removed. The CSV example remains only a
regression fixture. Python and JavaScript worked examples still run in restricted
runtimes. Other languages, including Rust, receive evidence checks, with an
explicit “not compiled or executed” disclosure in the lesson report.

To try the MVP:

1. Run the current desktop development build (`npm run tauri:dev`).
2. In Studio, create a focused course. Add an indexed library document or a
   reference URL in Materials. For a larger Rust collection, use the official
   [Rust Book print page](https://doc.rust-lang.org/book/print.html). A narrower
   goal such as ownership and borrowing is easier to evaluate first.
3. Accept the outline. In Sources, inspect the saved text and use Search related
   saved material to check that relevant passages can be found. You can also add
   pasted text, individual recipe pages, or biology reading here.
4. Prepare a lesson. Open **View lesson evidence** above its teaching sections.
   Inspect the claim, supporting quote, retrieved passages, original source
   version, checker, retrieval mode, and execution limitations.
5. Add/adopt/delete a source, then reopen the report: it should identify the changed
   collection. In-flight publication rejects any changed active source set. The
   old lesson retains its historical evidence; automatic re-verification is not
   part of this MVP.

The public evidence DTO omits assessment claims, private answer keys, and their
rationales. Existing or imported lessons without a report are explicitly labeled.
Reports remain bound to lesson content and full source hashes, using
`lesson-evidence-v2`. The author and checker still use the configured course model;
independently configured judges, human-calibrated accuracy measurements, curriculum
coverage evaluation, general website import, and additional executable validators
remain follow-up work. These are reliability mechanisms, not a correctness guarantee.

Verification results for this MVP:

- Learning backend suite: 130 passed; six opt-in tests skipped in the suite.
  Focused retrieval and verifier tests were rerun after final fixes.
- Web-service tests: 75 passed, three opt-in tests skipped. Existing chat verifier:
  50 passed, one opt-in test skipped.
- Learning frontend: 154 tests passed. Chromium and WebKit each passed the evidence
  report journey at desktop and narrow viewport widths.
- The opt-in live Rust Book fetch passed separately: 1,333,339 characters, no
  truncation, final appendix present, using the production reference reader.
- TypeScript, IPC contracts, desktop command permissions, SQL contracts, Rust
  architecture boundaries, formatting and strict library Clippy passed.

Retained logs and source hashes are under
`e2e-results/learning-reference-mvp/2026-10-04/`. Browser fixtures validate the UI;
deterministic embeddings and model fixtures validate retrieval, repair, and
publication boundaries. The live fetch verifies acquisition, not model quality.
No live-model lesson-generation or factual-accuracy evaluation was run for this MVP.

## Evidence-gated lesson preparation — 2026-10-04

The first content-verification slice reuses the chat claim checker and passage
ranker under a strict lesson policy. Preparation now extracts and audits claims,
checks immutable full-text evidence, executes complete Python/JavaScript worked
examples, and rechecks one evidence-driven repair. Both publication paths require
a revision-bound, backend-only report in the same transaction as the ready state.
The report includes source hashes, coverage findings, checker identity and runtime
observations; it contains private answer-key claims and is not a learner DTO.

Verification on the local working tree:

- Learning Studio library suite: **129 passed**, five optional container checks
  skipped. This includes six new verification regressions and real Python/WASI
  and JavaScript execution.
- Existing chat verification suite: **50 passed**, one optional live-model check
  skipped. Legacy chat judgment and lexical fallback behavior remain covered.
- CSV regression: a scripted checker rejects the false whitespace claim, the
  repair receives captured evidence and independently authored runtime fixtures,
  and the corrected revision passes rechecking and persists with its report.
- Missing/invalid evidence, incomplete claim coverage, fabricated quotes,
  truncated and uncertain judgments, changed content, inactive sources and
  absent reports block publication. Failed publication preserves the revision.
- SQL contracts: 1,226 statements prepared against 32 migrations; 74 dynamic
  fragments remain covered through repository tests. Rust layer boundaries,
  formatting, strict library Clippy (`-D warnings`), and patch whitespace checks passed.

Logs and source hashes are retained under
`e2e-results/learning-verification/2026-10-04/`.

Historical first-slice behavior included automatic Python CSV reference acquisition.
The reference collection MVP above removes that topic-specific behavior. General
topic/claim research and independent checker-model selection remain follow-up work;
all topics require suitable saved references. Existing ready/imported lessons are not
retroactively certified. These deterministic model fixtures establish pipeline
and persistence behavior, not live-model factual accuracy or repair quality on
unseen lessons; the live source-acquisition path and held-out evaluations still
need separate evidence. See the
[implementation scope and research plan](../design/2026-10-04-learning-content-verification.md).

## Interactive teaching and placement review — 2026-10-04

The teaching loop now includes saved, in-lesson guided exercises, progressive
hints, critique, task-specific rubric feedback, and revisions that retain the
original submission. Revision sessions copy the original task, rubric, sources,
and answer, expose previous feedback, and remain assisted practice.

Curricula now persist prerequisite links and structured project milestones.
Lesson authoring receives the course sequence and recent lesson recaps. Curriculum,
lesson, placement-task, and assessment-item authoring use an additional AI review
with one repair and re-review. Schema violations and short teaching blocks also
enter the repair path. Review is calibrated to the authoring stage: outlines do
not need finished exercises, and later lessons may rely on earlier learning.
The final review receives the original findings and checks their resolution plus
concrete newly introduced errors; optional new suggestions do not become
publication blockers. Unresolved blocking defects prevent publishing the candidate.
Lesson multiple-choice keys receive an additional blinded check: the model solves
the questions without seeing the proposed keys or explanations. Disagreements
and ambiguous options enter the same bounded repair, then are checked again.
Every lesson section also requires a review finding tied to an app-assigned
passage ID from its unchanged body. Missing sections and foreign or invented
passage IDs require response correction before review can finish, and a section
marked defective enters repair even when the overall issue list is empty. The
same factual and product-capability checks apply after repair. This remains an AI
check, not a guarantee of correctness.

Optional placement uses short performance tasks with private keys, autosaved
answers, revision conflict recovery, and feedback tied to exact submitted text.
Recommendations suggest study or an optional module challenge. Challenges can be
authored from accepted objectives before lesson preparation. The next-step control
uses unfinished work, placement, missed outcomes, newer evidence, guided and
independent submissions, and due recall; it never silently completes lessons.
The lesson-preparation job path accepts an explicitly selected unfinished lesson
outside course order, preserves ordered batches, and follows a stable retry chain
after a failure or interruption.

A live Qwen feedback matrix exposed an uncertain response receiving numeric zeros.
The grader now requires null scores for uncertain judgments and the backend strips
numeric scores from uncertain results. The retained initial failure is useful
regression evidence, not a passing model-quality result. The first complete-course
trial also exposed an overly broad focused-course goal and a reviewer confusing
outline requirements with prepared lesson content. The reviewer context was fixed,
and the revised synthetic goal explicitly scopes a small CSV expense summarizer.
Agent inspection and local Python execution of the first prepared lesson also found incorrect question keys
and an example whose leading newline changed its output. The general critique
missed these, motivating the blinded key check and explicit example tracing.
The initial harness disabled model reasoning; it now forwards the same reasoning
controls as the production llama.cpp adapter. A retained reasoning-enabled probe
identified both incorrect keys in that lesson. The earlier non-reasoning runs
remain labeled as such and cannot establish production model quality.

With production reasoning controls, the initial HTTP-based feedback matrix passed: correct work
scored 8/8, incorrect work 0/8, partial work 6/8, and uncertain work had null
scores. The orienting hint did not reveal the solution. These are four synthetic
answers and one hint, not a broad grading benchmark. A full-lesson request then
exceeded the former three-minute timeout. That historical iteration increased
material requests to five minutes and lesson preparation to fifteen minutes.
Those elapsed-time limits have since been removed; the current durable worker
reports progress and supports cancellation without an overall time cutoff.
The harness now delegates to the actual llama.cpp streaming/retry adapter rather
than duplicating its HTTP transport. The first full lesson completed through that
adapter in 218 seconds. Its general review exhausted the initial 3,000-token
allowance before producing a judgment, so that review now requests up to 6,000
tokens within the provider's configured ceiling. Blinded checks also reject
impossible question premises; the quality review rejects unsupported promises
that written tutoring will execute code or provide external expert review.

The first actual production-adapter grading run rejected abbreviated evidence
quotes that used ellipses. Grading now explicitly requests continuous exact
quotes, budgets output for the rubric size, and makes at most one targeted repair
before rejecting invalid feedback. Deterministic checks cover successful evidence
repair and repeated invalid feedback; uncertain judgments still lose all numeric
scores. The subsequent production-adapter matrix passed all four cases (8/8,
0/8, 6/8, and null scores) and the orienting hint in 95 seconds. The source
configuration and saved application settings were not modified.

The nine-lesson live course is **not a passing end-to-end result**. The initial
review accepted a first lesson that still incorrectly described whitespace
handling and memory usage, and promised that the written tutor would run code.
The run was stopped during lesson two after these defects were identified. Its
saved first lesson is retained as failure evidence, not approved learning
material. That finding motivated the section-by-section review above and a
separate opt-in retained-lesson regression. Full-course completion, the final
checkpoint, and a native learner journey remain unverified for this version.

The retained-lesson regression also **failed** (236 seconds). The stricter review
identified header-whitespace and tutor-execution defects, but duplicated a section
index, omitted the recap, and reformatted several quotes. The backend rejected
that invalid review before repair or publication. The model additionally claimed
that empty CSV lines produce spurious rows; local Python execution shows that
`DictReader` skips empty data lines. It also missed the false memory-use claim.
These findings establish a remaining model-quality limitation. Requiring section
coverage detects incomplete review output; it does not make a model's factual
judgments reliable. This run is retained without retrying until it passes, and no
full-course or Coursera-level content-quality claim is supported by these checks.



Deterministic and native validation:

- All 151 component tests passed; the additional submission-conflict test also
  passed (152 unique component tests across the retained runs).
- All 42 Chromium/WebKit journeys passed, including guided attempts, placement,
  390px layouts, source selection, drafts, retry, and renderer restart.
- The final Learning Studio Rust suite passed 123 tests; five optional container tests
  remained excluded.
- A fresh isolated macOS app bundle passed all eight native tests,
  including real Learning Studio IPC, migrations, file access, and close/reopen
  persistence. Bundled Python and JavaScript runtime checks also passed.
  A run of the rebuilt bundle first timed out on the reopened Journal title's
  visibility check, despite the retained screenshot and database containing the
  saved content. One confirmation run passed all eight checks. Both results are
  retained; the intermittent visibility failure was not reproduced or explained.
  That native bundle predates the final generation/review changes, which are covered
  by the subsequent Rust checks and provider probes rather than that UI run.
- Application and browser TypeScript checks passed after bindings regeneration.
  ESLint, IPC contracts, SQL statement checks, command inventory, and patch
  whitespace checks passed.
- The native suite validates integration and lifecycle behavior; its course
  commands use an empty test library. Live course authoring below uses production
  services and a separate SQLite database, not a full native learner journey.

Logs and reviewed screenshots are retained under
`e2e-results/teaching-course-review-2026-10-04/`.

The opt-in provider harness reads an explicitly selected OpenCode configuration,
passes configured credentials only through sensitive HTTP headers, and records
synthetic prompts, outputs, model identity, latency, and token usage. Ordinary
tests never contact this endpoint. Reproduce with a provider-compatible config:

```bash
LATTICE_TEACHING_CONFIG_PATH=/path/to/opencode.json \
LATTICE_TEACHING_EVAL_DIR=/path/to/new-evaluation-directory \
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml \
  --features bindings-export --test teaching_course_evals \
  -- --ignored --nocapture --test-threads=1 --skip live_retained_lesson_review
```

For an interrupted retained evaluation, `LATTICE_TEACHING_RESUME=1` resumes the
same saved outline and skips already prepared lessons. Optional
`LATTICE_TEACHING_REPLAY=1` reuses only successful production-adapter calls whose
model, messages, reasoning effort, and output ceiling match exactly; each reuse
is printed in the log. Both are evaluation-only controls. Ordinary fresh runs
contact the provider for every authoring and review stage.

The separate retained-lesson regression requires `LATTICE_TEACHING_REVIEW_CALL`
to point to a synthetic `call-NNN.json` containing an authored lesson. Run only
`live_retained_lesson_review` with `--exact --ignored --nocapture`, the provider
config, and the evaluation directory. It uses the production review/repair path
and retains the reviewed lesson separately; it does not overwrite the course or
mark a failed full-course run as passed.


The full suite attempts to prepare every lesson in a synthetic course, authors starting-point
tasks, grades correct/incorrect/partial/uncertain responses, checks an orienting
hint, then attempts to author and persist a written/application checkpoint. The
checkpoint requires a successfully completed course; it is not implied by a
passing grading matrix. Raw answer-key
artifacts are explicitly named private. These checks do not measure human learning
outcomes or constitute independent subject-expert review.

## Course authoring and Space selection review — 2026-10-04

The creation form and backend previously required a document or URL. The picker
read all Library documents without an explicit Space, and lesson generation was
limited to three teaching blocks and six multiple-choice questions. Written
practice also discarded lesson-specific assignments in favor of a generic prompt.

Creation now supports a topic without documents, with optional materials selected
through the same Space-scoped document API used by Chat. The picker starts in the
currently selected Space (General when none is selected), searches that Space,
and retains visible, removable selections across Spaces. Only checked IDs are
sent for acquisition. The same picker is used when adding materials to an
existing course. A failed acquisition of explicitly selected material never
silently falls back to general knowledge.

The authoring contract distinguishes topic-based AI content from externally
sourced material. Topic-only content must not invent citations; source-backed
content still needs validated quotes. This distinction continues through lessons,
written practice, practical activities, checkpoints, and recall. Generated recall
without external references quotes the prepared lesson itself and retains empty
external source IDs.

Course depth is enforced by the backend: focused courses have 2–3 modules with
2–3 lessons each, complete courses have 4–6 modules with 3–5 lessons each, and
deep dives have 6–10 modules with 4–6 lessons each. Older clients retain their
existing 2–6 module bounds. Session length controls lesson size independently.

New lessons require 8–12 sections: at least two explanations, two worked examples,
guided practice, an independent assignment, reflection, and a recap. Teaching and
practice sections have minimum content lengths. The independent assignment is
frozen as the actual written-practice task. A full syllabus, lesson section links,
module milestones, written/applied checkpoints, and prefilled project/capstone
briefs connect the existing workspaces. Existing prepared lessons remain readable.

Verification on the working tree:

- Rust Learning Studio suite: 113 tests passed; five optional container checks
  were not rerun. New integration coverage exercises topic-only creation,
  preparation, saved assignments, recall acceptance, checkpoint creation and
  form startup, depth bounds, rejected thin lessons, and invalid citations.
- Learning Studio and section error-boundary component suite: 20 files, 140 tests
  passed in a serial run, including saving generated recall without external sources.
  An existing recall test timed out under parallel load and passed in isolation
  and in the complete serial run.
- The initial Chromium/WebKit suite passed 33 of 36 checks. Two layout checks
  exposed excessive course-map height at 800×600, which was corrected with a
  compact expandable map. One Canvas retry click raced with an autosave.
- All eight follow-up browser checks passed, covering the corrected layout,
  Canvas retry, topic-only creation, and new 390px creation journeys in both
  browsers. Reviewed screenshots include the builder and active workspace.
- ESLint passed for the Learning Studio components and browser fixture/spec.
- TypeScript checks passed for the application and browser tests.
- API bindings were regenerated and their consistency check passed.
- IPC contract guard, Rust formatting, and patch whitespace checks passed.

Logs and reviewed screenshots are retained under
`e2e-results/study-course-review-2026-10-04/`.

These are deterministic renderer, model-fixture, and real SQLite checks. Live-model pedagogical
quality, native packaged-app execution, and learning efficacy were not measured
by this review; course length and section validation alone cannot establish them.

## Studio usability and draft safety review — 2026-10-02

The local working tree based on `c25d7fb55062e459ba2692783085583215b2fd9f`
completed the following follow-up checks on macOS Apple Silicon. These results
cover the durable lab editor and Studio navigation changes, with additional
visual checks after the final theme-color adjustment.

| Boundary | Result |
|---|---|
| Full frontend Vitest with enforced coverage | 174 files, 1,331 tests passed |
| Learning Studio renderer coverage | 80.40% statements, 70.60% branches, 79.73% functions, 88.65% lines |
| Whole-renderer coverage | 54.06% statements, 48.95% branches, 49.12% functions, 54.95% lines |
| Complete renderer Playwright | 80 checks passed across Chromium and WebKit, with no retries or skipped checks |
| Final layout and theme verification | 4 additional browser checks passed after the last color adjustment |
| Rust Learning Studio library | 108 passed; 5 optional container checks were not rerun |
| Rust draft persistence | Revision, replay, private-file rejection, and file-backed SQLite close/reopen checks passed |
| TypeScript, E2E TypeScript, ESLint | Passed |
| Rust formatting and strict library Clippy | Passed with warnings denied |
| Generated bindings and command/SQL/IPC contracts | Passed; 353 registered commands and 27 migrations checked |

Lab edits now save in SQLite independently of execution. Tests exercise edits
during loading and saving, operation replay after a lost response, stale-revision
conflicts, both conflict recovery actions, undo back to saved content, activity
isolation, and reopening the database. Pending saves gate activity creation,
activity switching, program navigation, and route exits. Failed saves preserve
the editor and provide a retry path. No draft save is counted as a code run.

The editor provides local C#, Rust, Python, JavaScript, TypeScript, JSX/TSX,
and JSON syntax support, line numbers, indentation, search, undo, and a Run
shortcut. Browser journeys use the real CodeMirror editor and verify an escape
from its Tab-indentation behavior. Runnable lab creation requires an explicit
execution environment; review-only projects remain available. The activity
dialog traps focus, closes with Escape, and restores focus to its opener or the
requested environment setup panel.

Five primary navigation tabs group the full workspace, with practice activities
in a secondary row and Plan, Canvas, and Import & export under More. Module and
lesson selectors wait for saves before changing context. Canvas and all prior
workflows remain covered by the browser suite. Final visual review covered
1280×800, 800×600, and 390×844 in both browsers; the first lesson card starts at
410 pixels at the two desktop sizes. Tests inspect inner scroll containers as
well as the page, so a clipped app shell cannot hide horizontal overflow.
Search controls and primary buttons are checked for at least 4.5:1 text contrast
in light and dark themes.

Logs, the complete browser result, coverage totals, and reviewed screenshots are
retained under `e2e-results/studio-polish-2026-10-02/`. Browser journeys use a
deterministic stateful IPC fixture; the separate Rust tests exercise real SQLite
and migrations. This follow-up did not rerun the packaged native, live-model,
or real-container checks. Their earlier evidence and remaining platform limits
are recorded below.

## Earlier runtime integration evidence — 2026-10-01–02

The integrated local working-tree candidate completed these deterministic checks
on macOS Apple Silicon:

| Boundary | Result |
|---|---|
| Rust Learning Studio library | 106 passed; 5 optional container tests also executed separately below |
| Existing Study library | 11 passed |
| Article extraction and its boundary tests | 16 passed, including the shared QuickJS dependency change |
| Evaluation integration | 2 passed, 1 ignored opt-in live-model benchmark |
| Frontend Vitest with enforced coverage | 171 files, 1,305 tests passed |
| Learning Studio renderer coverage | 80.49% statements, 69.79% branches, 79.08% functions, 88.68% lines |
| Whole-renderer coverage | 53.50% statements, 48.32% branches, 48.49% functions, 54.36% lines |
| Complete renderer Playwright | 70 checks passed across Chromium and WebKit |
| Rust formatting and strict library Clippy | Passed with warnings denied |
| TypeScript, E2E TypeScript, ESLint | Passed |
| Generated bindings and command/SQL/IPC contracts | Passed |
| Real Docker execution | All 4 language presets accepted correct and rejected incorrect solutions |
| Container containment | Passed: protected evaluator, read-only host input/root, bounded writable guest storage, network isolation, output cap, cancellation cleanup, and missing-image status |
| Installed Python and JavaScript | Passed correct/incorrect exercises and host-access checks inside the signed application |
| Packaged macOS native UI | Passed: 9 recorded checks across startup, IPC, file access, editing, close/restart persistence, and resource churn |

The Rust library run includes 133 passing tests across Learning Studio, Study,
and article extraction. The five real-container tests ran from that same freshly
built test executable with `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, exercising engine
discovery with a typical desktop launch environment. Docker's CLI and credential
helper directories are resolved for the child process without a login shell or
global environment changes. No container daemon is needed for the built-in
Python and JavaScript providers.

Completed check logs are retained under
`e2e-results/learning-runtime/2026-10-02/`. The installed-runtime result includes
the exact candidate identifier, executable, resource directory, architecture,
and profile. The packaged UI result is also retained at
`e2e-results/desktop/reports/1790917832494/result.json`; its 25-cycle resource
workload grew sampled resident memory by approximately 0.10 MiB, below the
64 MiB regression budget. This is evidence for that workload, not proof of zero
leaks. These are isolated, instrumented debug candidates, not notarized shipping
installers.

Earlier candidate evidence remains available for the CPU sidecar's tiny-model
execution (`e2e-results/desktop/reports/cpu-generation-local.json`) and the
allocator diagnostic (`e2e-results/desktop/reports/1790910887552/`). Those checks
were not repeated for this runtime integration. The opt-in live-model benchmark
has no configured endpoint and remains unverified. The Windows/Linux packaged
matrix and manual assistive-technology review also remain release evidence to
collect; configured CI jobs are not passing results.

## Fast and interactive test workflows

Use the focused commands while developing Learning Studio:

```bash
# Automated component and interaction tests
npm run test:learning:unit

# The same unit tests in Vitest's interactive browser UI
npm run test:learning:unit:ui

# Automated production-renderer journeys in Chromium and WebKit
npm run test:learning:e2e

# Step through, inspect, and replay those journeys in Playwright UI
npm run test:learning:e2e:ui
```

`npm run test:coverage` runs the complete renderer unit suite and enforces both a
whole-renderer baseline and stronger Learning Studio floors. The Studio floors are
77% statements, 67% branches, 75% functions, and 87% lines. CI runs this command
instead of an unenforced unit-test pass and retains the text, JSON summary, LCOV,
and HTML reports as the `frontend-coverage` artifact. Failed tests still produce a
report when the runner reaches coverage collection.

## Deterministic release gates

Run these from the repository root after generating bindings:

```bash
npm run bindings:generate
npm run bindings:check
npm run contracts:check
npm run contracts:commands
npm run contracts:sql

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
SQLX_OFFLINE=true cargo check --manifest-path src-tauri/Cargo.toml --lib
SQLX_OFFLINE=true cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --lib features::learning::
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml --test learning_studio_evals

npm run type-check
npm run type-check:e2e
npm run lint
npm run test:coverage
npm run test:learning:unit
npm run test:learning:e2e
```

The Rust Learning Studio tests use the real migration set and SQLite foreign
keys. They cover replayed operation IDs, compare-and-swap conflicts, immutable
history, answer-key isolation, source/version ownership, assessment uncertainty,
generation interruption, pack validation, scheduler replay, and practical-run
recovery. The browser suite uses a strict stateful IPC fixture: unsupported
commands, unexpected external requests, console errors, page errors, and
narrow-screen overflow fail the journey. Its stateful journeys cover saved
assessment responses with pause and fresh submission, curriculum preview and
acceptance, an explicitly unavailable lab runtime, and recall card versioning,
review, scheduler selection, and duplicate decisions. Assessment grading is
shown as unavailable in the fixture; these journeys do not claim model grading.
Browser fixtures validate renderer behavior; they do not establish native
service safety.

The packaged native suite exercises the real Tauri plugin registration and
migrated database as part of its isolated install, close, and restart journey:

```bash
npm run test:desktop:build
npm run test:desktop
```

This build is intentionally separate because it produces and launches an
instrumented application bundle. See `e2e/desktop/README.md` for platform setup,
isolation, and the CI evidence boundary.

The build helper verifies the bundled CPython interpreter and standard-library
hashes, then executes Python and JavaScript from the installed executable. On
macOS this exercises the signed hardened-runtime binary, including its Wasmtime
executable-memory entitlement. The fixed probe requires correct solutions to
pass, incorrect solutions to fail, and host access to remain unavailable. Its
report is `e2e-results/desktop/reports/embedded-runtime.json`. The native UI suite
also verifies the actual runtime catalog IPC and installed resource path.

## Language execution coverage

Python standard-library exercises run in the bundled CPython WASI interpreter;
JavaScript module exercises run in QuickJS. Both work without Docker, a local
language installation, or a network connection. Tests execute real learner code
and cover failed assertions, timeouts, cancellation, bounded output, and denied
host access. JavaScript tests also exercise memory exhaustion. Python compiles
its trusted interpreter once per application process; that initialization does
not consume the learner's exercise time limit.

C#, Rust, and React/TypeScript use optional Docker or Podman environments prepared
from the app's catalog. Preparation can download toolchains; actual exercises
run offline against the recorded immutable image ID. C# supports package-free
.NET projects, Rust uses the standard library, and React includes pinned React,
TypeScript, jsdom, and Testing Library dependencies. React exercises type-check
and run component interaction tests; a live visual preview is not implemented.

Run the real Docker catalog checks after starting Docker:

```bash
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml \
  --features bindings-export --lib features::learning::runtime::runtime_catalog::tests::docker_ \
  -- --ignored --test-threads=1
```

Each language executes both a correct and an incorrect solution through the
production container runner. The React check renders a component and clicks its
button. These tests prepare missing images and therefore may download them.
The Ubuntu CI job runs these checks and the containment smoke test below.

## Opt-in contained-runtime smoke test

Ordinary tests inspect the exact Docker/Podman argv without requiring either
runtime. The opt-in smoke test uses an already-local, immutable image and never
pulls. The selected image must provide `/bin/sh` and `cat`.

```bash
LATTICE_LAB_SMOKE_ENGINE=docker \
LATTICE_LAB_SMOKE_IMAGE_ID=sha256:<64-lowercase-hex-digest> \
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml \
  --features bindings-export --lib \
  features::learning::runtime::lab_runtime::tests::opt_in_container_smoke_proves_runtime_containment_and_read_only_checks \
  -- --ignored --nocapture
```

The smoke test verifies that the learner can write to the bounded temporary
workspace inside the container while host input files remain unchanged.
Evaluator files cannot be changed through their read-only mounts, the container
root cannot be written, effective capabilities are empty, `NoNewPrivs` is set,
and the isolated network namespace exposes no `eth0`. It then cancels a busy
run and verifies that the deterministically named container no longer exists.
Production execution also
sets a wall-clock timeout, memory plus swap ceiling, CPU ceiling, process and
file-descriptor limits, bounded output, a read-only root filesystem, no image
pull, no network, and a deterministic container name for cancellation/recovery.

When a container exercise has no available Docker or Podman engine, its recorded
status is `runtime_unavailable`. Built-in Python and JavaScript practice,
authoring, and artifact review remain available.

## Opt-in live-model benchmark

The committed benchmark contacts no model during an ordinary test run. Configure
an Ollama-compatible local or private endpoint explicitly:

```bash
LATTICE_LEARNING_EVAL_MODEL=qwen3:8b \
LATTICE_LEARNING_EVAL_ENDPOINT=http://127.0.0.1:11434 \
LATTICE_LEARNING_EVAL_LOG_DIR="$HOME/lattice-eval-results" \
SQLX_OFFLINE=true cargo test --manifest-path src-tauri/Cargo.toml \
  --test learning_studio_evals -- --ignored --nocapture
```

Optional authentication uses
`LATTICE_LEARNING_EVAL_AUTH_HEADER_NAME` and
`LATTICE_LEARNING_EVAL_AUTH_HEADER_VALUE`. The credential is passed to the
client and is never written to the trace.

The benchmark calls the production open-response grader with correct,
incorrect, ambiguous, and incomplete synthetic attempts. It records rubric
agreement, exact submitted-text support for numeric scores, false-confidence
count, latency, and peak observed process RSS. It then calls the production
practical-authoring pipeline, including independently generated hidden checks,
and records frozen-source grounding, public/check file counts, a withheld
solution canary, latency, and RSS. Each record is appended before an assertion,
so a failing run remains reviewable at
`lattice-learning-studio-evals-<pid>.jsonl`.

Retain the JSONL file with the model identifier and candidate commit. A passing
run establishes only that this model followed these bounded synthetic contracts
on that run. Cancellation, crash recovery, pack restoration, and operation
replay remain deterministic repository/runtime gates rather than model-quality
metrics.

## Platform behavior

| Capability | macOS | Windows | Linux |
|---|---|---|---|
| Programs, lessons, assessments, notes, Canvas, recall, sources, packs | Supported by the local Tauri/SQLite application | Supported by the local Tauri/SQLite application | Supported by the local Tauri/SQLite application |
| Built-in Python and JavaScript execution | Bundled interpreters; no Docker dependency | Same implementation; packaged CI required | Same implementation; packaged CI required |
| Container lab authoring and artifact review | Available without a runtime | Available without a runtime | Available without a runtime |
| Container execution | Optional Docker or Podman capability, probed at use time | Optional Docker or Podman capability, probed at use time | Optional Docker or Podman capability, probed at use time |
| Unsupported runtime state | Shown as unavailable; no evidence event is created | Shown as unavailable; no evidence event is created | Shown as unavailable; no evidence event is created |
| Pack import | Checksum and conflict preview required before writing | Same | Same |

The repository command builder and migration tests are portable. Cross-platform
support claims still require a green packaged-desktop matrix for the candidate
commit. Windows Server CI does not replace a Windows 11 clean-machine run, and
Linux package extraction does not test every distribution's container setup.

## Accessibility review

Automated journeys run both Chromium and WebKit at desktop and 390-pixel widths.
They use role/name locators, keyboard focus assertions, live status/error regions,
and overflow checks. Before tagging a release, retain a manual review record for:

- complete keyboard traversal, visible focus, modal focus entry/return, and
  logical focus order at 100%, 200%, and 400% zoom;
- VoiceOver on macOS plus NVDA or Narrator on Windows for headings, landmarks,
  form labels, error recovery, assessment disclosure, and card reveal state;
- light and dark themes, high-contrast settings, reduced motion, and text-only
  comprehension of Canvas through its written outline;
- 390-pixel layout with long titles, file paths, source quotes, and translated
  date/number strings.

Record failures with the platform, browser/webview, assistive technology, exact
control, and screenshot or trace. An unchecked manual item is unverified; it is
not a passing result.
