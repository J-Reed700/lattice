# Lesson acceptance: one policy, read by every stage

Date: 2026-11-03. Status: design; nothing built. Code read at `lesson/kinds-and-shape` `5cb727a3`. Evidence: the removals of job 1 of lesson `9ad43b26` on `/models/Qwen3.8-27B-Q5_K_S.gguf` (the copy of its database taken when it failed at publication, read-only). Supersedes the mechanism of the claim classes design (2026-11-01) §3.1–§3.3 and §3.11's call count, and keeps its measured judges, prompts and intent.

## 0. The finding in one paragraph

The claim classes design decided, per class of statement, which check runs, what passes and what happens on failure: one table (§3.1). The code holds that table nowhere as one thing. Each stage keeps the column it needs, in its own words: the classifier the class, `class_checks.rs` the routing and the falsity veto, `bridge.rs` the pass rule of a cited sentence, `ClaimLedgerEntry::may_repair` and `ClaimLedger::cuts_next` the failure path, `repair/confine.rs` what a repair may change, `call_plan.rs` a hand copy of every path's cost, `sections.rs` and the writer's rules the lesson's shape. Where two columns meet they disagree, and the pipeline either goes round or deletes true statements (§1). Another patch at another seam leaves the next seam. The fix is to make the table a thing: **each class's check is one finite graph of steps, the graphs are the policy, every stage asks the policy, and what each stage used to copy is derived from the graphs**: the next step, what a repair is asked to achieve, the call ceiling, a statement's end state, and the proof that the loop ends.

## 1. Split brain, located

Every seam where two stages judge the same unit by different rules or with different context, read at `5cb727a3` (paths under `content_verification/` unless rooted):

| # | One stage says | Another says | Effect | Answer |
|---|---|---|---|---|
| 1 | a cited sentence is Sourced by the writer, never lightened (`Class::prevails`, core `claim_review.rs:173`; `class_checks.rs:93`) | pass A fails a statement with no stated part (core `bridge.rs:102`, pinned by `nothing_stated_is_nothing_supported_whatever_is_common`) | a true all-common sentence is researched, rewritten, cut | §2.1 |
| 2 | Common passes on three unanimous closed-book samples (core `classes.rs:147`) | a common part beside a stated one passes on one part-wise reading (core `bridge.rs:98`) | the bar for "common" depends on whether the writer cited the sentence | §2.1, §2.3 |
| 3 | the knowledge judge sees `Statement: …` alone (core `classes.rs:240`) | the consistency judge saw the passages and passed it (`class_checks.rs:255`) | a closed-book majority overrules a reading of the sources | §2.2 |
| 4 | a vetoed Reasoning statement is saved only if the strict judge finds it stated outright (`class_checks.rs:267`, `stated_by`) | an explanation goes beyond its passages by definition (core `classes.rs:14`) | the overrule never fires for the class it exists for | §2.3 `r.ground` |
| 5 | a Common statement most samples call false fails at once (`class_checks.rs:302`), alone on its sentence it is cut without a rewrite (`claim_ledger.rs:565`) | a Reasoning one gets its passages read first | which class the model picked decides whether the sources are asked | §2.3 `c.ground` |
| 6 | a rewritten sentence keeps its saved statement (`inventory/revisions/locations.rs:64`, pinned by `relocation_never_changes_an_assertion_even_if_it_is_now_false`; prompt `inventory/revisions.rs:270`) and receipts are reused by statement across quotes (`claim_reuse.rs:56`, `mod.rs:283`) | the repair changed the assertion on purpose | the re-check judges the old words, or copies the old verdict | §2.7 |
| 7 | the repair prompt promises that changes elsewhere are discarded (`repair/confine.rs:15`) | confinement refuses the whole section on one mixed hunk (`confine.rs:255`, `:276`) and every target in the group is charged (`repair.rs:575`) | one stray edit sends a section's every target to the cut | §2.6 |
| 8 | a check judges a claim (`inventory.rs:362`) | the cut removes every sentence the claim's quote touches (`repair/cut.rs:44`), so a quote that spans a passage takes the passage, and every claim on a removed sentence ends Removed (`cut.rs:209`, core `ledger.rs:539`) | true statements leave with a false neighbour | §2.6 (cut) |
| 9 | the ledger does not re-judge a verified claim on reselected evidence (core `rules.rs:109`) | the report re-judges every claim each pass and `publishable()` reads the latest (`passes.rs:147`, `resolution.rs:20`) | two verdicts per claim; `Reconcile` patches one case (`claim_ledger.rs:428`) | §2.9 |
| 10 | the citation repair moves the claim's citation | with no entry of its own it overwrites the section's quote (`repair/citation.rs:263`), the evidence of the section's other claims (`citations.rs:102`) | a repair for one claim changes what others are judged on | §2.6 (recite) |
| 11 | `call_plan.rs` restates every path's cost (`:14`, `:18`, `released`) | the paths live in `class_checks`, `claim_ledger`, `repair` | the ceiling and the calls drift; it is only logged when exceeded (`lesson_progress.rs:118`) | §2.4 |
| 12 | "specific" per the classifier (`classify.rs:24`), the rule layer (`:150`), the bridge (core `bridge.rs:20`), the consistency (core `classes.rs:16`) and knowledge (`:26`) prompts; the bridge's guard ignores the givens the rule layer honours (`evidence_checks/bridge.rs:69`, `Givens::NONE`) | — | a part is a specific or not depending on who asks | §2.6 (guard), §6 (prompts) |
| 13 | the writer is told to add no explanation the excerpts do not give (`generation.rs:159`) and to explain why it works (`generation/lesson.rs:166`); uncited specifics are advisory to it (`lesson_rules.rs:5`) | the checker classes explanations Reasoning and removes uncited specifics | the writer cannot know what will pass | §6 |
| 14 | the repair history forbids reintroducing every non-publishable finding, unjudged ones and ones never sent for repair included (`repair_loop.rs:450`) | only statements that failed and were repaired are known false | the rewrite avoids true statements | §2.6 (rewrite) |
| 15 | the design gives every lighter class the falsity veto (classes design §3.1) | a model-Internal statement gets no check (`class_checks.rs:207`) | a false statement the model calls a scenario is never read | §3 |
| 16 | the ledger ends a claim that no re-extraction continues `Superseded { None, Machine }`, replayed as Removed (`claim_ledger/observe.rs:380-423`, core `ledger.rs:539`) | the draft still holds its sentence; nothing compares the two | a sentence stays in the lesson with no live claim: unchecked text, recorded as removed | §2.9 |
| 17 | a statement is sorted once (classes design §3.1) | later passes record new `Classified` events (`class_checks::sort`, and the routes that send a lighter class to the strict check as Sourced by the model), and a stricter class from the same authority prevails (`Class::prevails`) | one noisy later sort ratchets a statement that passed as Reasoning or Common into Sourced for good, where it fails | §2.9 |

Outside the lesson's statements and left to their own designs: the teaching review's citation rules beside the claims' (`teaching.rs:441`), the Verifier skipping `Consistent` claims and disputing on an unanswered check (`audit/ledger.rs:54`, core `replay.rs:136`), and the outline repair still failing a supported claim on the gate (`planning/outline_repair/evidence.rs:74`).

What the 27B's job 1 did with it (lesson `9ad43b26`: 16 claims removed, 6 with a failing verdict of their own; 23 with the retry that followed):

- **A citation's fault taken as the sentence's.** A sentence the writer cited is Sourced by the writer's authority, which nothing below it can lighten (`Class::prevails`). Its check is the part-wise judge, whose pass rule is "every part stated or common, *and one stated*" (`BridgeOutcome::decision`, core `bridge.rs:98`). A cited sentence whose every part is standard knowledge therefore fails as unsupported ("a standard concept in machine learning … No passage states any part of it"), research finds nothing for common knowledge, and the sentence is rewritten and cut. "One stated" asks whether the citation is honest; the pipeline answers a question about the citation by deleting the sentence.
- **Two bars for "common".** A part the part-wise judge calls common passes on that one reading beside a stated part. A Common statement passes only when three closed-book samples agree (`KnowledgeOutcome::is_common`). The same words meet a different bar depending on whether the writer happened to cite their sentence.
- **Sources outrank the model, for one class, with the wrong judge.** A Reasoning statement a majority of closed-book samples call false is read against its section's passages before it fails, but by the strict judge, "stated outright" (`class_checks.rs:267`, `stated_by`), which an explanation never is. A Common statement gets no reading at all: a majority `false` fails it at once (`class_checks.rs:302`) and, alone on its sentence, it is cut without a rewrite (`ClaimLedger::cuts_next`). Job 1 lost two true statements about an acronym its own sources define, one Reasoning and one Common by the model classifier's choice, to a closed-book judge that did not know the term.
- **The statement checked is not the sentence written.** After a rewrite ("must be routed" → "you would route") the section's revision kept the saved statement, so the re-check judged the old words and repeated the old verdict.
- **A repair refused for a change the policy would accept.** Four claims in two requests were refused at their first attempt because a passing neighbour sat one character away in the same hunk (`confine.rs:255`); each was charged (`RepairOutcome::Rejected`), so its next attempt was the cut in code, and none was repaired. The teaching review's repair hit the same refusal three times.
- **Ten removals with no failing verdict, all through one funnel** (#16): a claim no re-extraction continued was ended Removed whether or not its sentence stood. One shared a sentence with a failed claim whose rewrite left neither continued. Four went with sections marked to go, three of them in a block that fell under two teaching sentences because its three cited all-common and closed-book-vetoed statements had been cut (the first and third items above, compounding). Five went when a later preparation revised sections against the saved inventory of the section that used to sit at their index after an excision renumbered the lesson (that trigger is fixed, `8bcf1080`; the funnel is not): four of those sentences are still in the final draft with no live claim on them.
- **The second 27B run** (`5cb727a3`, 2026-11-03, 152 minutes, 210 calls under a ceiling of 286 that never rose) did not publish: 48 statements, 19 removed, and 1 waiting for the user. That last one is a Reasoning statement contradicted three times whose escalated attempt rewrote instead of cutting, and which the replay's attempt budget then queued (`replay.rs`, `charge`): the third copy of the policy overruling the classes design's "failure ends in removal". Of the 19 removed, 12 had passed their last check and were removed by the record (#16), one of them a rewrite that had just fixed it; 10 had been sorted again into Sourced after their first check, 8 after passing it (#17); 6 went down the designed path, failing, rewritten, cut.
- **The ceiling is a copy.** `call_plan.rs` restates each path's cost by hand (`SOURCED_FOLLOW_UP = 4`, `REASONING_FOLLOW_UP = 3 + SAMPLES`, `released`); a change to a path that misses the copy makes the panel's "at most T" wrong, which the acceptance run fails on.

## 2. The design

Six concerns, each behind its own seam, each free to change without the others:

| Concern | Question it answers | Pattern | Where |
|---|---|---|---|
| Policy | for a statement of this class, after these outcomes, what next; when is it done | Strategy (one graph per class; the policy is the set) | `lattice-core` `domain::learning::acceptance` |
| Readers | what does this judge, on this evidence, make of the statement | Adapter (each judge to one outcome vocabulary) | `lattice-core` `claim_verification` (interpretation), desktop (calls) |
| Remedies | how the draft changes: cite, uncite, research, rewrite, cut | Adapter (each existing mechanism to one step) | desktop `content_verification` |
| Round | in what order and batches steps run; sync points; when the lesson settles | Template Method | desktop `content_verification::round` |
| Shape | what a lesson must keep to be a lesson | Strategy | `lattice-core` (rule), read by writer, cut, save, settle |
| Ledger | what happened | (record) | `claim_review`, unchanged in kind |

### 2.1 Two questions, not one

A cited statement raises two questions, and they have different remedies:

- **Standing**: may the lesson say this? Its parts are stated by evidence, standard knowledge, or explanation consistent with the sources; nothing contradicts it; it asserts no specific that nothing states. Remedies: research, a rewrite, a cut.
- **Attribution**: is its citation honest? A cited passage states at least one of its parts (AIS, Rashkin et al. 2023: attribution is a relation between a statement and a source, "according to P, s"). Remedies: move the citation to the passage that states it, or drop it. **A citation's fault never rewrites or removes a sentence.**

"One stated" stays, as the attribution test of a cited statement and as the meaning of "grounded" (§2.2). A cited statement whose parts are all common is a statement with a dishonest citation: the citation goes and the statement meets the bar every statement that rests on no passage meets, the knowledge judge's (§2.3, Sourced, `AllCommon`).

### 2.2 Evidence has a rank

A reading is grounded in one of three ways, weakest first: closed-book (the model alone: the knowledge judge), read (a judge that read the passages and named none for the point: "consistent", "common"), named (a passage, an example's run, or the user's evidence, named for a part). A negative finding is overruled only by a positive one of a stronger rank, and a named contradiction is overruled by nothing the machine has. In the graphs this is topology, not a rule each stage applies: **every edge on which a closed-book reading fails a statement leads to a part-wise reading of its passages before any rewrite or cut**, and a part-wise reading that grounds the statement (one part stated, none missing, contradicted or common) ends it passed. Retrieval outranks parametric memory wherever retrieval has something (Mallen et al., ACL 2023); the closed-book judge is cheap and good on standard knowledge, so it runs first and the passages are read only when it objects (a cascade in the sense of FrugalGPT, Chen et al. 2023).

### 2.3 Each class's check is a graph (Strategy)

A graph is nodes; a node is one step and its cost in model calls; its edges map each outcome of the step to the next node, to another class's entry (a change of class the evidence makes), or to an end. Steps:

- `Read(Parts)`: the part-wise judge (`BRIDGE_SYSTEM`) on the statement's own citation: the passages its sentence cites, or its section's when it cites none. It answers both questions of §2.1, so it is the one reading that can say `AllCommon`. `Read(Ground)`: the same judge on the section's passages, asked only whether they ground a statement a closed-book reading called false (one part stated, none missing, contradicted or common): yes stands, anything else leaves the veto standing. Same judge, different question, so a different reading. `Read(Page)`: the strict judge on the course's page for it (`page_one`). `Read(Consistency)`: the consistency judge on the section's passages. `Read(Knowledge)`: the knowledge judge's three closed-book samples, passing on unanimity. `Read(Veto)`: the same samples, failing only when every sample says `false`.
- `Research` (once in a statement's life, at most two pages), `Recite` (its citation moves to the passage found), `Uncite` (its citation goes), `Rewrite(goal)`, `Cut`.

Outcomes of a reading, interpreted once per reading in `lattice-core` (§2.6): `Stands`, `StandsElsewhere` (on a passage it does not cite), `AllCommon` (the part-wise judge found every part common, none stated, guarded by the specifics test), `Unproven` (a specific nothing states; nothing supports it), `Refuted` (a passage, or every sample, says otherwise), `NeedsSource` (Common: a `needs_source` or a split with a `false`; Reasoning: an unfounded specific), `Unjudged`. Outcomes of a remedy: `Done`, `Saved`/`FoundNothing` (research), `Rewritten`/`NotApplied` (rewrite), `Cut`/`CannotCut`.

Only `Refuted` shows a statement false, so only it leads to a `Rewrite` or a `Cut`. A reading that finds the sources silent (`Unproven`, `NeedsSource`) removes the citation it cannot confirm, or when nothing more can be tried ends the statement `Unverified`: it stays in the lesson as written, published not verified (policy `acceptance-v2`; `v1` rewrote and cut these). A contradiction the statement's own citation names goes straight to its rewrite (policy `acceptance-v3`): `v2` sent it to research first, where a page without the cited passage read silent and dropped the citation, so the named contradiction was overruled by silence (6 of the 8 statements refuted at `s.read` in the 2026-11-12 Haiku round, 3 of them then passed as general knowledge).

`Unjudged` is no edge of any graph: a reading whose answer cannot be read (after the judge's own second asking) leaves the statement where it is, to be read once more; a second unanswered reading of the same statement, at that node or any later one, stops the lesson with the honest reason the classes design gives (§3.2 there). One re-ask per statement in its life, not one per reading: the code today allows one per reading (`ClaimLedgerEntry::unjudged` counts a run), which the ceiling's `U` never counted. A rewrite the confinement could not apply is asked once more, told why, by its own node (`*.again`), and then takes the next rung.

The four graphs (`→ end X` is a terminal state; `⇒ Class` enters that class's graph at its entry, the class recorded `by: Evidence`; `⇒ Class after its rewrite` enters it at its post-rewrite node, which has no rewrite in it, so a rewrite's successor may change class once more without ever being rewritten again):

**Sourced** — entered by the writer's citation, the rule layer, the model, or evidence.

| Node | Step | Calls | Outcomes |
|---|---|---|---|
| `s.read` | `Read(Parts)` on the passages of the cited sentence it restates, else its section's quote (every block carries one) | 1 | Stands → end Resolved · AllCommon → `s.uncite` if the statement has not been in Common, `s.research` if it has · Unproven → `s.research` · Refuted → `s.rewrite` |
| `s.uncite` | `Uncite` | 0 | Done ⇒ Common |
| `s.research` | `Research` | 0 | Saved, FoundNothing → `s.reread` |
| `s.reread` | `Read(Page)` with what research saved | 1 | Stands → end Resolved · StandsElsewhere → `s.recite` · Refuted → `s.rewrite` · Unproven → `s.uncite` if the statement has not been in Common, end Unverified if it has |
| `s.recite` | `Recite` | 0 | Done → `s.citeread` |
| `s.citeread` | `Read(Parts)` on the new citation | 1 | Stands → end Resolved · AllCommon → `s.uncite` if the statement has not been in Common, end Unverified if it has · Refuted → `s.rewrite` · Unproven → as AllCommon |
| `s.rewrite` | `Rewrite(what the passage states, or the point without the specific)` | 1 | Rewritten → `s.final` (the successor) · NotApplied → `s.rewrite.again` |
| `s.rewrite.again` | the same, told why the first was not applied | 1 | Rewritten → `s.final` · NotApplied → `s.cut` |
| `s.final` | `Read(Parts)` | 1 | Stands → end Resolved · AllCommon → `s.final.uncite` · Refuted → `s.cut` · Unproven → end Unverified |
| `s.final.uncite` | `Uncite` | 0 | Done ⇒ Common after its rewrite (at `c.final`) |
| `s.cut` | `Cut` | 0 | Cut → end Removed · CannotCut → end Unverified |

A statement with no passage to be read against (no citation and no section quote, which the writer's schema does not allow) is read on the course's page wherever its node reads `Parts`, since the page is the only place its support can be: it stands where the page states it, as a sentence that cites nothing does at `s.reread`. `Unproven` without a call, it would go from `s.final` to its cut with nothing judged. A rewrite that keeps "the point without the specific", as its goal allows, reads all common on its citation at `s.final`: it loses the citation and meets Common's check from Common's post-rewrite node, so it is never rewritten twice; today the bridge's "one stated" fails exactly that success, and the cut follows.

**Reasoning** — entered by the model.

| Node | Step | Calls | Outcomes |
|---|---|---|---|
| `r.read` | `Read(Consistency)` | 1 | Stands → `r.veto` · Refuted → `r.rewrite` · NeedsSource ⇒ Sourced if the statement has not been in Sourced, → end Unverified if it has |
| `r.veto` | `Read(Veto)` | 3 | Stands → end Consistent · Refuted → `r.ground` |
| `r.ground` | `Read(Ground)` | 1 | Stands → end Consistent · Refuted → `r.correct` |
| `r.correct` | `Rewrite(correct what the veto found; keep the rest of the sentence)`, told the veto's reason and the ground's | 1 | Rewritten → `r.final` · NotApplied → `r.correct.again` |
| `r.correct.again` | the same, told why | 1 | Rewritten → `r.final` · NotApplied → `r.cut` |
| `r.rewrite` | `Rewrite(agree with the passages; correct what the reason names)` | 1 | Rewritten → `r.final` · NotApplied → `r.rewrite.again` |
| `r.rewrite.again` | the same, told why | 1 | Rewritten → `r.final` · NotApplied → `r.cut` |
| `r.final` | `Read(Consistency)` | 1 | Stands → `r.reveto` · Refuted → `r.cut` · NeedsSource → end Unverified |
| `r.reveto` | `Read(Veto)` | 3 | Stands → end Consistent · Refuted → `r.reground` |
| `r.reground` | `Read(Ground)` | 1 | Stands → end Consistent · Refuted → `r.cut` |
| `r.cut` | `Cut` | 0 | as `s.cut` |

**Common** — entered by the model, or by evidence from Sourced (`AllCommon`, its citation dropped).

| Node | Step | Calls | Outcomes |
|---|---|---|---|
| `c.read` | `Read(Knowledge)` | 3 | Stands → end GeneralKnowledge · NeedsSource ⇒ Sourced if the statement has not been in Sourced, end Unverified if it has · Refuted → `c.ground` |
| `c.ground` | `Read(Ground)` | 1 | Stands → end GeneralKnowledge, the passage on its check · Refuted → `c.rewrite` |
| `c.rewrite` | `Rewrite(state it as the field has it, per the reason)` | 1 | Rewritten → `c.final` · NotApplied → `c.rewrite.again` |
| `c.rewrite.again` | the same, told why | 1 | Rewritten → `c.final` · NotApplied → `c.cut` |
| `c.final` | `Read(Knowledge)` | 3 | Stands → end GeneralKnowledge · Refuted → `c.reground` · NeedsSource → end Unverified |
| `c.reground` | `Read(Ground)` | 1 | Stands → end GeneralKnowledge · Refuted → `c.cut` |
| `c.cut` | `Cut` | 0 | as `s.cut` |

**Internal** — a statement the rule layer leaves out (a rubric or option field, "The sources…") never enters the ledger and asks nothing. One the model calls Internal meets the falsity veto (build step 10):

| Node | Step | Calls | Outcomes |
|---|---|---|---|
| `i.veto` | `Read(Veto)` | 3 | Stands → end NotAClaim · Refuted → `i.ground` |
| `i.ground` | `Read(Ground)` | 1 | Stands → end NotAClaim · Refuted → `i.rewrite` |
| `i.rewrite` | `Rewrite(state it as the field has it, per the reason)` | 1 | Rewritten → `i.final` · NotApplied → `i.rewrite.again` |
| `i.rewrite.again` | the same, told why | 1 | Rewritten → `i.final` · NotApplied → `i.cut` |
| `i.final` | `Read(Veto)` | 3 | Stands → end NotAClaim · Refuted → `i.reground` |
| `i.reground` | `Read(Ground)` | 1 | Stands → end NotAClaim · Refuted → `i.cut` |
| `i.cut` | `Cut` | 0 | as `s.cut` |

A `Cut` takes the statement out of its sentence: the sentence itself when no other live statement stands on it; otherwise one narrowing rewrite that keeps what the others say (their successors are read once from their class's post-rewrite node, the M reserve of §2.4). When that rewrite cannot be applied, or the text cannot be taken out (the whole of its field, a code block), or the cut would leave its own section with fewer than two teaching sentences or without its answer, no cut is made and the statement ends `Unverified`. The rule is per section: only the statements whose cuts thin a section keep their sentences, and every other statement's cut is made: a sentence is never cut for what a neighbour says. Only a failed example's refused cut takes its section out. A rewrite's successor continues at its parent's post-rewrite node (`s.final`, `r.final`, `c.final`); so does a statement whose sentence a neighbour's rewrite changed.

What changed from the code, deliberately (§3): Sourced's `AllCommon` edges, at its first reading, its re-cited reading and after its rewrite (was: fails as unsupported); the part-wise judge in place of the strict one where the passages overrule a veto (`Read(Ground)`; was `stated_by`); Common's `c.ground` and its one rewrite (was: failed at once and, alone on its sentence, cut); the rewrite's retry twin and per-target confinement (was: one mixed hunk refused the section and charged every target); the narrowing cut (was: the cut took every statement on the sentence, and every sentence the quote touched). Everything else is the code's present path, written down once.

### 2.4 What the graphs decide, and what is derived from them

- **The next step** of a statement is the step of the node it is at. No stage decides it.
- **What a rewrite is asked** is the `Rewrite` step's goal, with the reading that failed the statement: the repair prompt says exactly what the next reading will test.
- **The call ceiling** is the calls made so far and the most still to come: per statement, the longest path in calls from where it is to an end, plus the costliest reading still ahead of it while its one re-ask is unused (U), plus two reserves the graphs cannot see: one flow repair per section that can take a cut (K), which each flow repair uses up as it is made, and M. The sorting calls are among the calls made; the plan is made once the pass's statements are identified, and every statement enters it the same way, at its place. For each statement whose sentence carries another live statement when the statement is taken into the plan, M holds two readings of its class from its post-rewrite node, one after a neighbour's rewrite and one after a neighbour's narrowing cut, each with the statement's re-ask still possible, plus that narrowing call: `2 × most_calls(post-rewrite node, re-ask unspent) + 1`, which is 2 × 8 + 1 = 17 for Sourced and Reasoning and 2 × 7 + 1 = 15 for Common and Internal. (A re-reading that leaves the re-ask unused gives its allowance up as it ends, so this is an upper bound by up to three calls, never a count that can be passed.) It is fixed when the statement is taken into the plan, at its place there (a continued claim at its ledger position, not at an entry it left), so it cannot rise. A statement whose successor reads again takes those calls from its own reserve, since that is what the reserve was held for; anything beyond it would be a rise, which is shown and counted, never hidden. The reserve is released once the statement and every other statement on its sentence have ended, since then none is left to rewrite the sentence or to be read again. `call_plan.rs`'s constants go. From the graphs of §2.3, at entry: Sourced 13 + 3 (its longest path re-cites, reads all common on the new citation and takes Common's whole check), Reasoning 12 + 3, Common 13 + 3 (sent to its source, rewritten there, then all common after the rewrite), model-Internal 10 + 3; from the post-rewrite node: Sourced 5, Reasoning 5, Common 4, Internal 4. `call_plan.rs` has Reasoning's and Common's by hand today and Sourced's as 1, because its post-rewrite reading had no path but the cut. Because a node's longest path is at least its own cost plus the longest path of any successor, the ceiling cannot rise as statements move; a test holds every edge of every graph to that. The teaching review may rewrite the sentence of a statement that ends, or waits, with a rewrite left; its successor is read again from the post-rewrite node. The rewrites a statement has left are part of its position: the most `Rewrite` steps any path of its class takes (a rewrite and its retry, so 2), one spent by each `Rewrite` step it takes whatever its outcome and one by each machine change of its sentence. The ceiling and the teaching release read that count through the same functions (`next_rewrite`, `successor`), so a statement that ends with a rewrite left still holds its successor's readings, and one whose repair took both steps is not rewritten again, which brings each statement's share at entry to 19 (Sourced, Reasoning, Common) and 16 (Internal).
- **A statement's end state** is the end it reaches: Resolved (`ResolvedByEvidence`), Consistent, GeneralKnowledge, Removed by the machine.
- **That the loop ends**: the graphs are acyclic (a statement enters each class at most once at its entry, and after a rewrite only into a post-rewrite node, from which no rewrite and no entry is reachable), each statement's path is finite, a rewrite's successor continues its parent's path (it does not start over), and a round in which no statement moved is a fault that stops the lesson with its reason (§2.7), not a count that runs out.

### 2.5 Invariants, held by tests on every graph of every policy

1. Acyclic, counting class changes: a statement enters each class at most once at its entry; after its rewrite it may change class once more, into a post-rewrite node.
2. Every path to end Removed passes a `Rewrite` (no statement is cut without one rewrite).
3. Every `Refuted` edge out of `Read(Knowledge)` or `Read(Veto)` reaches a `Read(Ground)` before any `Rewrite` or `Cut`.
4. A citation's fault is answered on the citation: every `StandsElsewhere` edge leads to a `Recite`, and every `AllCommon` edge to an `Uncite`, directly or as a first entry's `by`; only a first entry's `otherwise`, taken by a statement that has been in the class already, may lead elsewhere.
5. No node has an `Unjudged` edge (the walk keeps the statement in place once, then stops the lesson), and every `Rewrite` node's `NotApplied` edge leads to its `.again` node, whose own leads to the cut.
6. A class's ends are its own: Sourced Resolved, Removed or Unverified, Reasoning Consistent, Removed or Unverified, Common GeneralKnowledge, Removed or Unverified (the acceptance run's pairings, classes design §3.11), Internal NotAClaim or Removed.
7. For every edge `n → m` and either state of the re-ask: `cost(n) + most(m) ≤ most(n)`, and using the re-ask never raises `most` (the ceiling never rises).
8. A rewrite's successor is never rewritten again: from every class's post-rewrite node no `Rewrite` node is reachable, and the only change of class is into another class's post-rewrite node.
9. No `Rewrite` or `Cut` node is reachable from an entry without crossing a `Refuted` edge: nothing is changed or removed on a statement the sources are merely silent about.
10. A first entry leads into the class it names: its `by` is an entry into that class, or a node of the same graph whose every edge is.

A policy is a set of graphs; another policy (a stricter one for answer keys, one with the Verifier's reading before publication) is another set, and the same tests hold it.

### 2.6 Readers and remedies (Adapter)

A reader turns one judge's reply into the outcome vocabulary. The interpretation is pure and lives beside the judge in `claim_verification`; the call stays where it is.

| Reading | Judge (unchanged) | Interpretation |
|---|---|---|
| `Parts` | `ClaimChecker::check_bridge`, `BridgeOutcome::guarded` | any part contradicted → Refuted; any part specific-missing → Unproven; every part common, none stated → AllCommon; otherwise Stands. Replaces `BridgeOutcome::decision`. |
| `Ground` | the same | every part stated or common and one stated → Stands; anything else → Refuted (the veto stands). |
| `Page` | strict judge (`ClaimVerdict`) | Supported → Stands or StandsElsewhere by the passage's source (a sentence that cites nothing has no other source to be on, so its support Stands); Unsupported, NoSource → Unproven; Contradicted → Refuted; Unverified → Unjudged |
| `Consistency` | `check_consistency` | Consistent → Stands; Contradicted → Refuted; UnfoundedSpecific → NeedsSource |
| `Knowledge` | `check_knowledge` | unanimous `true_common` → Stands; all `false` → Refuted; otherwise (a `needs_source`, a split with a `false`) → NeedsSource. Replaces the routing in `class_checks::common`. |
| `Veto` | `check_knowledge` | all `false` → Refuted; otherwise Stands (`needs_source` never fails a Reasoning statement, classes design §4.5). |

The part-wise reading's specifics guard asks the rule layer's own test with the unit's givens and its hypothetical-case exemption (`classify::rule_class`'s inputs), not `Givens::NONE`: one answer to "is this a specific" in code (§1 #12).

A remedy is one existing mechanism behind one step:

- `Research`: `research::expand`, with the per-statement page count.
- `Recite`: `repair::cite_where_supported`, writing the sentence's own `citations` entry: it moves the entry the sentence has, and adds one where the sentence has none or where its entry is another live statement's citation too. It never overwrites the section's quote, which other statements are read against (§1 #10), and it cites one claim of a sentence per call, so that two cannot write one entry.
- `Uncite`: removes the sentence's own `citations` entry (new, beside it); a sentence that carries none has nothing to remove.
- `Rewrite`: `repair::candidate` for a section's targets, each told its goal and the reading that failed it; the history it may not reintroduce is the statements that failed a reading and were rewritten or cut, nothing unjudged (§1 #14). Confinement is per target: a hunk that touches a protected sentence is put back and only the targets in that hunk are `NotApplied`; the section's other targets keep their rewrites and nothing else is charged (§1 #7). What a rewrite or narrowing wrote is extracted again. The statement sharing most words with a target continues it; each other new statement written in the changed text is born of the repair (`BornOfRepair`, of the target whose replaced sentence shares most words with it): it is read from the post-rewrite node of its class with the successor's rewrites, so a split sentence adds no budget, and cut when its reading does not stand. Nothing is put back and nothing is charged.
- `Cut`: `repair/cut.rs`, on the sentence that states the claim. A quote that spans a passage is first narrowed to that sentence (`shared::text::claim_sentence`), so its other sentences stay (§1 #8); every copy of the sentence in the field still goes, each asserting what failed (`every_place_a_sentence_is_written_goes_with_it` stays true). A statement that draws on several sentences, which `claim_sentence` declines to name, and a sentence that carries other live statements are narrowed by the rewrite of §2.3 instead; when that rewrite cannot be applied the cut is refused, nothing is taken out and the statement ends `Unverified`.

### 2.7 The round (Template Method)

One function owns the order; a review supplies the steps. The preparation job is one review; a scripted review in the tests is another, which is what lets the loop's termination and its no-progress stop be tested without a model.

```
loop {
    review.sync()                    // the user's actions since the last round
    review.extract()                 // statements of new or changed sentences, from their current text alone
    round = steps of every machine-owned statement, from the policy, grouped by step
    if round is empty {
        match review.settle(shape)   // flow repairs, sections that go, the shape's stop
        { Again => continue, Done => return review.finish(policy.verdict(ledger, shape)) }
    }
    readings, then research, then recite/uncite, then rewrites by section, then cuts
    if the ledger did not move: return review.finish(Stopped(no progress, with what was pending))
}
```

`extract` carries the fix for the old words. What an edit changed is extracted from its own text: a section's claims on sentences that stand are carried as they were, those on sentences rewritten or cut leave, and the text the edit wrote (the alignment's hunks) is extracted with the rest of the section as context, never shown the saved statements. No model revises saved statements. The claims keep their identity by `claim_review::identities` (same unit, overlapping span), never by a model's echo of the old statement; a receipt is reused only for the same statement on the same quote. Relocation never changes an assertion: it carries a kept sentence's claim as it is.

### 2.8 The lesson's shape (Strategy)

One `ShapePolicy` answers, for a lesson: what the writer must produce (question kinds and counts, a first explanation), whether a cut may be taken (`admits_cut`), which sections go (too little to teach), and whether what is left can be published (a third of its sections, its first explanation, all its questions, every question of a kind it asks). The writer's rules, the cut, `sections::settle` and the publication gate read it; `LessonQuestionKind::ALL` was its first piece.

### 2.9 Where a statement is

The ledger records decisions, not only observations. Today it records what a check found (`Checked`, `Researched`, `RepairAttempted`) and `replay.rs` re-derives what follows by rules of its own (`first_failure`, `after_machine_failure`, `charge`: an attempt budget that queues a claim for the user when it runs out, which the classes design had already replaced by the cut). That is a third copy of the policy, and it disagrees with the other two. So each machine step is recorded twice over, once as what happened and once as what the policy made of it: beside the observation event, one `ClaimEvent::Stepped { policy, step, outcome, then }`, where `then` is the node or end the policy sent the statement to (`Then::Node(name)`, `Then::End(end)`, `Then::Stop`) and `policy` is the policy's version.

Everything reads `Stepped`. A machine-owned claim's state is read from its latest `then`: at a node it is `Open` (its kind and reason from the outcome, `machine_attempts_left` the rewrites still reachable from the node); at an end it is that end's state (`ResolvedByEvidence`, `Consistent`, `GeneralKnowledge`, `Removed { by: Machine }`). The replay rules that decide machine transitions go once every producer writes `Stepped`; the user's events (confirm, edit, remove, reopen, disputes) layer over it as they do now. The next step is the step of the `then` node; the ceiling is `most_calls` from it. `walk` replays a statement's recorded steps and must arrive where the last `then` says, and a statement whose record does not fit the graph (or names another policy version) is a fault that stops the lesson with its reason, never guessed past. A rewrite's successor starts at its parent's post-rewrite node: its first `Stepped` continues from there.

A statement is sorted once in its lineage. A successor, and a statement whose sentence a neighbour's rewrite changed, keeps its predecessor's class and continues from its position; no later sort and no route records a class, and a class changes only on the graph's `Enter` and `EnterAfterRewrite` edges, recorded `by: Evidence` (§1 #17).

Three more additions to the record: `ClaimEvent::CitationDropped { cited }`, `RepairAttempted` carries the goal it asked for, and `ClassifiedBy` gains `Evidence`, above `Writer`, which a change of class by `Enter` or `EnterAfterRewrite` records.

The report's per-claim verdict is read from the same place: a finding is publishable when its claim's walk has reached a passing end, so the report and the ledger cannot disagree and `Reconcile` goes (§1 #9).

**A claim ends Removed only when its sentence is gone from the draft**: cut, rewritten, or in a section that went. That holds for a claim that only waits for the user, queued or reopened from a check of the machine's: its sentence gone, it has nothing left to show them, and the words in its place are new and are read. A verdict is never carried onto words it did not read. A decision of the user's stays theirs, and a sentence changed under it reopens it. A claim an extraction misses while its quote stands is a fault of the extraction, not a removal: the inventory keeps it as it was extracted from that same text, and it continues. Such a claim reaching identification with no successor is a fault that stops the lesson. Publication holds the draft to the ledger the other way too: no claim the machine ended without a successor stands on a quote the published lesson holds, and the coverage audit answers for every passage with its claims, a non-factual reason, or the assertions missing (§1 #16). A rule that every sentence stand on a claim would refuse the sentences that assert nothing.

### 2.10 The switch: every decision the policy's

Today a statement's next step is decided in ten places:
- `class_checks::check`'s routes;
- pass A and B's selection and `combine`;
- `observe`'s class mapping of a check;
- `research_targets`;
- the findings `cite_where_supported` picks;
- `decide`'s `may_repair` and its rechecks;
- `cuts_next`;
- `begin_repairs`' attempt numbers;
- the replay's attempt rules;
- the sort, which runs every pass.

All ten go in one step. A half-switched loop strands statements: one the policy sends to `s.uncite` is one no old rule acts on. The loop keeps its shape (§2.7's template is step 9); what changes is who answers each of its questions.

**A pass.**
1. Extraction runs as now.
2. Before anything is read, the ledger matches what was extracted to the claims it holds (`identities` and a repair's heirs), recording nothing.
   - A statement that continues a claim takes that claim's class, so only the statements the match finds new are sorted (§2.9, §1 #17). Today the sort finds a held class by the statement's words, so a claim re-extracted in other words is sorted again.
   - Then the ledger records the identities: `Extracted`, a class for a statement new to the ledger, the user's edits, the orphans, and the outcome of a repair that awaited this pass (below). No check is recorded here.
3. Every live machine-owned statement then has a position (`ClaimLedger::position`):
   - At a reading node, `read_until_remedy` reads it on to a remedy or an end, and each reading is recorded with its decision (`record_reading`).
   - At any other node, nothing is read. The report carries its latest reading's finding, which the job keeps. When the job has no saved finding for it, as in a fresh job over a lesson the ledger already holds, the finding is rebuilt from the ledger: the latest check's verdict, reason and support (a support found elsewhere names where, which the recite needs), and the passages the claim's card shows. It is never asked for again, and a missing finding is never a fault. Either way it decides nothing.
   - A claim the user resolved with evidence of their own carries that evidence's check as its finding, never a reading the job saved before the user acted.
   - The progress counts each statement read as one check, and as a saved check reused when every reading of it in the round came from a receipt.
4. Three kinds of statement are outside the ledger's checks, as now: one the rule layer leaves out, a model-Internal statement until step 10, and a failed example, which keeps its own path (§6).

**The remedies, by node.** Each remedy takes the statements at its nodes, and the step's outcome is recorded with its `Stepped`:
- **Research.** The statements at `Research` nodes, plus a user's hint (below), go to `research::expand`. It records `Researched`, with outcome `Saved` when the round saved a source and `FoundNothing` otherwise. A round that reached nothing (`unreached`: every search and page read failed, or the planner gave no usable answer) is not research and records nothing, so the statements stay where they are.
- **Recite.** The statements at `Recite` nodes go to `cite_where_supported` with their page's finding. It records its `CitationRepaired` and the outcome `Done`.
- **Uncite.** For the statements at `Uncite` nodes, the sentence's own entry in `citations` goes.
  - A sentence with no entry of its own keeps what it has: that is the section's quote, which others are read against.
  - An entry another live statement on the same sentence restates also stays, since it is that statement's citation.
  - Either way the statement no longer stands on a citation. It records `CitationDropped { cited }` (empty when nothing went), the outcome `Done`, and `Classified { by: Evidence }` for the class the edge enters.
- **Recite, once per section per call.** The recite repairs one claim of a section per call, so that two cannot overwrite the section's quote. A second claim of that section at a `Recite` node waits for the next round, with nothing recorded. A claim the recite cannot cite at all (no run of the passage's whole sentences can be a citation) is still `Done`, and the reading after it tells. From step 7a the recite writes the sentence's own entry (§2.6), and the rule is one claim of a sentence per call.
- **Rewrite and Cut** are one repair, as now:
  - The statements at `Rewrite` nodes are its targets, each with its node's goal on `RepairAttempted`.
  - The statements at `Cut` nodes go on their last attempt (`lastAttempt`, which the code cuts).
  - The outcome is recorded where the repair's is now: when the next pass identifies what the repair left.
    - A target whose sentence changed → `Rewritten`.
    - A target whose quote is still in its field → `NotApplied`.
    - A sentence the cut took out, or that was out already → `Cut`.
    - A refused cut → `CannotCut`, ending the statement `Unverified`; only a failed example's refused cut takes its section out.
    - A refused cut of a statement in a question (its prompt, choices, rationales or explanation) takes the question out instead, the way a failed example's takes its section out: the statement is recorded `Cut` and ends `Removed` with its question, which the saved lesson lacks. When the lesson cannot lose the question (it is the last, or the last of its kind), the guard holds the cut back and the statement ends `Unverified` as above.

**A user's hint.** A claim the user holds may carry a hint, what to search for (`ResearchWithHint`). The claim is the user's, so the policy does not walk it. The hint is the user's request for one research round and one reading of what that round found:
- The hint goes to `research::expand` with the round's other targets and records `Researched`. The claim it names is looked for whatever its latest check found, since a reopened claim's last check may have passed, and what the round saves is in its scope. The claim takes no `Stepped` for it: a step is the machine's decision, and the claim is not the machine's.
- When the round saved a source, the claim is read once by the `Page` reader over the expanded references, whatever its class, since the user asked for a source. An unreadable answer is asked once more.
- The reading answers the hint. The first judged local check after a `ResearchWithHint` is not an observation (§2.9): a reading that stands verifies the claim, and any other leaves it the user's, with the reading's reason on its card. Every other local check of a decided claim stays an observation.
- Nothing else follows: the claim is not rewritten or cut, because the user holds it. Where the page that verifies it is a source its section does not cite, the section cites it, as for the user's own evidence (below).
- A round that saved nothing answers the hint with its `Researched` alone, which found nothing. A round that reached nothing records nothing, so the hint is tried again on the next run.

**Where the user's evidence is, the lesson cites it.** A claim the user resolved, with evidence of their own or by a hint's reading, whose support is on a source its section does not cite has its section's citation moved there by the recite, as a statement at a `Recite` node does, with no step and no reading after it: the user's evidence is its check (the claim review design's §5.4). The recite still cites one claim at a time where two would write one citation, and a citation it wrote is never written again. Today's pass recites every finding supported elsewhere; the switch keeps that for the user's claims, not only the machine's.

Today's hint path re-checked the whole pass, which re-asked only what the new evidence changed. A Sourced claim was read against the new page, and a Reasoning or Common claim met its own check again, restored from its receipt. So a hint helped a Sourced claim alone; read by the page, it helps any claim it can find a source for.

**What the loop does next** (`decide`), checked in this order:
1. A statement whose walk is `Stopped`, or whose record does not fit its graph, stops the lesson with its reason.
2. Statements at `Rewrite` or `Cut` nodes are repaired, along with failed examples by their own rule.
3. A statement at any other node is taken by the next pass.
4. Once every machine-owned statement has ended, the sections are settled as now, and the lesson publishes unless a claim waits for the user.

An iteration that records no `Stepped` and leaves the draft as it was stops the lesson with what was pending: §2.7's no-progress stop, here before the template exists. The stop names each statement and says in words what it waits for (research, a reading, its citation). When the round's research reached nothing, which is the usual cause, the stop says that and how to fix it. Node names go to the log, not to the message.

**What is new about a claim, without a check.** Today a pass decides whether to check a claim again from what is new about it. The policy now decides when a statement is read, so the evidence or checker having changed no longer brings a check. Two reopenings stay, because they need no check:
- a verified claim whose resolving source was deleted or replaced;
- a confirmed claim whose sentence or bound source changed.

Both are recorded at identification, from the claim's quote and the active sources, before anything is read. A reopened claim is the user's (`awaits_user`), and the round leaves it alone.

**A stop belongs to its run.** A second unreadable answer stops the lesson. Its `Checked` is recorded, but not as a step: the claim stays at its node with its re-ask spent.
- A resumed run, with the same checker or another, asks once more. The user's resume is a new ask, not the machine's one re-ask, which stays one per life.
- This keeps what the stop message promises: resuming with another checker starts the checks again.
- A change of checker otherwise continues every walk where it is. A decision stands with the checker that made it, and the history names that checker.
- A claim whose decisions are another policy version's (an update that changed a graph) stops the lesson with that reason, until step 6b-4 lets a walk continue across a change of policy.

**The report reads the policy.** A machine-owned claim's finding is publishable when its walk ended at a passing end (`Resolved`, `Consistent`, `GeneralKnowledge`, `NotAClaim`), and the user's resolution counts as now.
- `Reconcile` goes. Its premise, a pass that disagrees with a claim the ledger holds verified, cannot arise when an ended claim is never decided again.
- Kept, `Reconcile` would loop: it would re-record a check for a claim whose state only a decision can move.

**What goes:**
- the routes of `class_checks::check` (its judges stay, behind the readers);
- `check_with_citations`' choice of passes, and `combine` and `alone` as a decision;
- `check_of`'s class mapping;
- `research_targets`' rule;
- `may_repair`, `cuts_next` and `MACHINE_ATTEMPTS` for statements (failed examples keep them);
- `Reconcile`;
- the sort of statements the ledger already holds.

**What stays as it is:**
- **For step 7a, what each remedy does:**
  - the recite writes the section's citation;
  - a refused hunk refuses its section's targets;
  - the cut takes every sentence its quote touches;
  - the goal is recorded but not yet in the prompt.
- **For step 7b:** a sentence that carries other live statements, and a statement no single sentence states, are narrowed (v1 cut them whole; v2 refuses the cut once the narrowing is not applied); a rewrite may write a new statement.
- **For step 8:**
  - a rewritten sentence is extracted with its saved statements;
  - a claim no extraction continues is Removed.

## 3. What changes in behaviour, and what is measured first

| Change | Why | Measured before it ships |
|---|---|---|
| A cited, all-common statement loses its citation and is checked as Common: at its first reading, on a re-cited passage, and after a rewrite that kept the point without the specific (then from Common's post-rewrite node) | §2.1 | the job-1 removals of this kind and the replay's bridge rows, on the 27B: each such statement passes the knowledge judge or fails it for a reason a reader accepts |
| The passages overrule a veto by the part-wise judge, for Reasoning and Common | §2.2 | the 56 veto rows of `run1/veto.jsonl` and job 1's two vetoed statements about the acronym its sources define: the real catches (single pass/opposite of modular; fine-tuning) still fail, the two pass |
| A Common statement gets one rewrite before a cut | invariant 2 | none: it adds a call, never a removal |
| A refused hunk refuses only its own targets, and is asked once more, told why | §2.6 | none |
| A cut takes the statement, not its neighbours: the sentence that states it (a passage-wide quote narrowed to it, every copy of it going), narrowed by a rewrite when the sentence carries others | §2.3, §2.6 | the job-1 removals with no failing verdict of their own: each is kept, or goes for a reason of its own |
| A rewritten sentence is extracted from its text alone | §2.7 | none |
| A statement a rewrite or narrowing writes beside its target's successor is born of the repair: it is read from its class's post-rewrite node with the successor's rewrites, and cut when it does not stand | §2.6 | a correction can add a statement, which is read once and cut if it does not stand |
| One re-ask per statement in its life, where the code gives one per reading in a run | §2.3 | none; it can stop a lesson that today goes on, when one statement's answers twice cannot be read, each after the judge's own second asking. What it buys is a ceiling that holds (today's `U` undercounts) |
| A statement the model calls Internal meets the falsity veto (as the classes design's §3.1 already says) | §1 #15 | the replay's 16 Internal rows: no scenario, rubric line or remark fails; cost 3 calls each, in the ceiling |

The prompts of record are not touched by this design. One definition of "specific" and "common" across the three judge prompts, the writer's rule and `states_a_specific` (four wordings today) is the next design's, since each wording was measured.

## 4. Interfaces

The policy, pure (`lattice-core/src/domain/learning/acceptance.rs` and `acceptance/`):

```rust
pub trait AcceptancePolicy: Send + Sync {
    /// Part of every receipt's and checkpoint's key: a change to any graph changes it.
    fn version(&self) -> &str;
    /// The graph of a class's check. A statement the rule layer leaves out never asks.
    fn graph(&self, class: Class) -> &Graph;
}

pub enum Reading { Parts, Ground, Page, Consistency, Knowledge, Veto }
pub enum Goal { WhatThePassageStates, AgreeWithThePassages, AsTheFieldHasIt, KeepTheOthers }
pub enum Step { Read(Reading), Research, Recite, Uncite, Rewrite(Goal), Cut }
pub enum Outcome {
    Stands, StandsElsewhere, AllCommon, Unproven, Refuted, NeedsSource, Unjudged,
    Done, Saved, FoundNothing, Rewritten, NotApplied, Cut, CannotCut,
}
pub enum End { Resolved, Consistent, GeneralKnowledge, NotAClaim, Removed { section_goes: bool }, Unverified }
pub enum Target {
    Node(NodeId),
    /// Another class's graph, at its entry: the statement enters the class by evidence, once.
    Enter(Class),
    /// Another class's graph, at its post-rewrite node: a rewrite's successor
    /// changes class, and is never rewritten again.
    EnterAfterRewrite(Class),
}
pub enum Next {
    Go(Target),
    /// `by` unless the statement has been in `into`; `otherwise` once it has
    /// (a statement enters each class at most once).
    FirstEntry { into: Class, by: Target, otherwise: NodeId },
    End(End),
}
pub struct Node { pub name: &'static str, pub step: Step, pub calls: u32, /* edges: (Outcome, Next) */ }
pub struct Graph { /* class, entry, nodes, after_rewrite */ }

pub struct Position { pub class: Class, pub visited: Classes, pub reasked: bool, pub node: NodeId }
pub struct Taken { pub step: Step, pub outcome: Outcome }
pub enum Walked { At(Position), Ended(End), Stopped, Diverged { at: Position, taken: Taken } }

/// Where a statement that entered `class` is after `taken`, its steps in order.
pub fn walk(policy: &dyn AcceptancePolicy, class: Class, taken: &[Taken]) -> Walked;
/// The most calls a statement at `at` can still cost: its longest path to an end, and its
/// re-ask on the costliest reading ahead while it is unused.
pub fn most_calls(policy: &dyn AcceptancePolicy, at: Position) -> u32;
/// The longest path from `class`'s post-rewrite node: what a neighbour's rewrite can cost.
pub fn after_rewrite_calls(policy: &dyn AcceptancePolicy, class: Class) -> u32;
/// Every invariant of §2.5 that `policy` breaks.
pub fn violations(policy: &dyn AcceptancePolicy) -> Vec<Violation>;

pub struct StandardPolicy; // the graphs of §2.3
```

The readers' interpretation, pure, beside each judge (`lattice-core/src/application/services/claim_verification/`): `BridgeOutcome::outcome(&self) -> Outcome` and `BridgeOutcome::ground_outcome(&self) -> Outcome` (both after `guarded`), `KnowledgeOutcome::knowledge_outcome(&self)` and `veto_outcome(&self)`, `ConsistencyOutcome::outcome(&self) -> Outcome`, `page_outcome(ClaimVerdict, on_cited: bool) -> Outcome`.

The readers and the reading driver, in the desktop (`content_verification/readings.rs`, one file per reader beneath it). Each reader adapts an existing judge path: `Parts` is pass A's comparison (the quotation settled in code first, then the bridge on the cited sentence's passages and the section's passed example runs; no passage is `Unproven` without a call), `Page` is pass B's first page, `Ground` is the bridge on `Citations::of(unit)`, `Consistency` and `Knowledge`/`Veto` are today's class judges. Pass A and pass B stop being combined in one finding (`evidence_checks/outcome.rs`): they are different nodes.

```rust
#[async_trait]
pub(super) trait Reader: Send + Sync {
    fn reading(&self) -> Reading;
    /// One `Read` per statement, in order; an outage is an error the caller defers on.
    async fn read(&self, at: &ReadContext<'_>, statements: &[Statement<'_>]) -> Result<Vec<Read>>;
}
pub(super) struct Read { pub outcome: Outcome, pub finding: Finding }

/// Reads each statement along its graph until its node is a remedy or an end, one
/// batch per reading kind a round, and gives every reading it made with the policy's
/// decision (the `Then`, the position after it, `rewrite_ahead`), for `observe` to
/// record as a check and a `Stepped` each.
pub(super) async fn read_until_remedy(
    policy: &dyn AcceptancePolicy,
    readers: &Readers,
    at: &ReadContext<'_>,
    statements: Vec<(Statement<'_>, Position)>,
) -> Result<Vec<ReadSteps>>;
```

The check a reading records is its observation, named by the reading: `Parts`, `Ground` and `Page` give `Supported` (or `SupportedElsewhere` from a page) or a failure; `Consistency` gives `Consistent`; `Knowledge` gives `KnownCommon` only when unanimous; `Veto` gives `CheckResult::NotFalse` (new: the samples did not call it false), so a veto passed is never shown as a unanimous common-knowledge pass. `check_of`'s class-by-class mapping goes: the state is the `Stepped`'s, not the check's.

The round, in the desktop (`content_verification/round.rs`):

```rust
#[async_trait]
pub(super) trait Review {
    async fn sync(&mut self) -> Result<()>;
    async fn extract(&mut self) -> Result<()>;
    async fn read(&mut self, reading: Reading, claims: &[ClaimId]) -> Result<()>;
    async fn research(&mut self, claims: &[ClaimId]) -> Result<()>;
    async fn cite(&mut self, steps: &[(ClaimId, Step)]) -> Result<()>;
    async fn rewrite(&mut self, section: usize, targets: &[(ClaimId, Goal)]) -> Result<()>;
    async fn cut(&mut self, claims: &[ClaimId]) -> Result<()>;
    async fn settle(&mut self, shape: &dyn ShapePolicy) -> Result<Settled>;
    async fn finish(&mut self, verdict: LessonVerdict) -> Result<Finished>;
    /// Each machine-owned claim's class and the steps it took, for the policy's walk.
    fn walks(&self) -> Vec<(ClaimId, Class, Vec<Taken>)>;
    /// Advances whenever the ledger records anything: the no-progress test.
    fn revision(&self) -> Revision;
}

pub(super) async fn run(review: &mut dyn Review, policy: &dyn AcceptancePolicy, shape: &dyn ShapePolicy) -> Result<Finished>;
```

## 5. Build plan

Each step is a builder's (`model: sonnet`), on branch `lesson/acceptance-policy`, tests first and seen failing where the step changes behaviour; each step's diff is read in full and its tests run in the build pool before the next starts.

| Step | What | Where | Tests that come first |
|---|---|---|---|
| 1 | The policy: graph types, `StandardPolicy`, `walk`, `most_calls`, `violations` | core `domain/learning/acceptance*` | every invariant of §2.5 on `StandardPolicy`, and each one failing on a graph built to break it; the walk of every path of §2.3; the ceilings of §2.4 |
| 2 | The readers' interpretation | core `claim_verification/{bridge,classes}.rs`, the strict judge | one table per reading: every part mix, every sample mix, every verdict |
| 3 | The seams' tests at `5cb727a3`, through the loop's own harness: §1 #1, #5, #4, #6, #7, #8, #16 | desktop tests, worktree `acceptance-reds` | each fails on `5cb727a3` for the reason in §1 |
| 4 | The record of decisions: `Stepped`, `CitationDropped`, the goal on `RepairAttempted`, `ClassifiedBy::Evidence`; the state of a machine-owned claim read from its latest `then`; the walk's check of the record | core `claim_review/{claim_review,ledger,replay}.rs` | a claim's state follows each `then`; a record that does not walk to its `then`, or names another policy, is a fault; a successor continues at its parent's post-rewrite node |
| 5 | The ceiling from the graphs | `call_plan.rs`, `lesson_progress.rs` | the ceiling is the sum of `most_calls` from each recorded position plus the reserves; it never rises over any scripted run |
| 6a | The readers: one adapter per reading, unit-tested on scripted models, not yet called by the pass | new `content_verification/readings*`; visibility in `class_checks.rs`, `evidence_checks*` | each reading's outcomes and findings, from the judges' fixtures |
| 6b-1 | The reading driver (`read_until_remedy`) on scripted readers; the ledger's record of a reading (`record_reading`: the check named by its reading, the `Stepped`, a class change by evidence, the decision and the observation held to agree); `CheckResult::NotFalse` | `readings/driver.rs`, `claim_ledger/readings.rs`, core `claim_review` | every path through reading nodes; the re-ask in place and the stop; a class change read on in the same call; the state after each recorded `then` |
| 6b-2 | The switch (§2.10): identity before reading, the sort once, readings and remedies by node, `decide` by node, the report from the policy | `passes.rs`, `repair_loop.rs`, `claim_ledger/{observe,claim_ledger}.rs`, `class_checks.rs`, `evidence_checks*` | §1 #1, #4, #5 go green; the no-progress stop; every machine-owned claim's state read from its `then` |
| 6b-3 | Retire the A/B pass: its tests of what a reader still runs (paging, packing, receipts, the page budget) move to that reader; its tests of the A/B decision go with `check_with_citations`, `prepare`, `corroborate`, `remember` and `combine` | `evidence_checks*`, `two_pass_tests.rs`, `evidence_paging_tests.rs`, `readings_tests.rs` | the moved tests pass on the readers; nothing outside tests called what goes |
| 6b-4 | Built before the first change to a graph a recorded walk has crossed; until then a change of policy stops with `policy_changed`, and steps 7 to 11 change no such graph, so it waits until a step needs it. A walk across a change of policy: a claim's recorded steps are walked again under the new graphs, and it continues where they fit, or else starts again at its class's entry (a rewrite's successor at its post-rewrite node, so it is never rewritten twice), with research spent where it was spent | core `acceptance/walk.rs`, `claim_review/ledger/decisions.rs`, `claim_ledger/position.rs` | a record under v1 continues under a v2 that keeps its nodes; one under a v2 that drops them starts again, never rewritten twice and never researched twice |
| 7a | The remedies that need no fresh extraction (§2.6): the recite on the sentence's own entry, one claim of a sentence per call; the rewrite asked its goal, told why on its retry, and kept only from what failed and was repaired (§1 #14); confinement per target; the cut on the sentence that states the claim | `repair/*`, `repair_loop.rs` | §1 #7, #8 b, #10, #14 go green |
| 8 | What an edit changed extracted from its own text, with no revision of saved statements; a receipt only for the same statement on the same quote; a claim whose quote stands kept in the inventory; no machine-ended claim on a quote a published lesson holds | `inventory.rs`, `inventory/revisions*`, `claim_reuse.rs`, `claim_ledger/identify.rs`, the loop's publication | §1 #6, #16 go green |
| 7b | Built after 8, since both read the fresh extraction of what a rewrite wrote: the narrowing rewrite of a cut whose sentence carries other live statements or that no single sentence states, its outcome read when the next pass extracts the sentence; no repair writing a new statement | `repair/cut.rs`, `claim_ledger/{successor,identify}.rs` | §1 #8 a goes green |
| 9 | The round (Template Method) with a scripted review | `repair_loop.rs` → `round.rs` | termination and the no-progress stop on the scripted review |
| 10 | The veto for a model-Internal statement | the graph, `class_checks.rs`, the ledger's content rule | §1 #15 |
| 11 | Measure §3 on the 27B, then the acceptance run | scratchpad replay; `acceptance_run.sh` | — |

## 6. Not in this design

- One wording of "specific" and "common" for every prompt (§3).
- An example's run as a reading. Example claims keep their own path (`exampleClaims`, the run as pass A evidence, a failed run blocking publication) until a `Read(Run)` joins the graphs.
- A user restoring a statement the machine removed (classes design §6).
