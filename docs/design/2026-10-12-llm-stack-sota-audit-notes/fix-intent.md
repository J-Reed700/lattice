# Fix: intent sends vault questions to the web

Branch `fix/intent` in `../lattice-fix-intent`, cut from `origin/main` 90bbbcb3. Uncommitted.

Defect 3 of `evals/retrieval/2026-10-12-SHIPPED-PATH-BASELINE.md`. The intent step added web search on 4 to 6 of 41 vault turns per arm, and on 25 of 38 v1 questions. Up to 5 web pages joined the vault passages, and in two answers they changed what the model said.

## What was happening

- `apply_turn_intent` ORed `needs_web` into `force_web_search`. That is the same flag the user's toggle sets.
- `kb_can_run_alongside_external` then saw a forced web search that was not a fallback. So the pipeline ran the vault search and the web search together with `tokio::join!` (`retrieval/pipeline.rs`, the `kb_alongside_external` branch).
- The external-lookup gate was open on that path. The web pages were always merged, and the web query was sent before the vault had answered.

## The new rule

- **Intent never forces the web.** `needs_web` now sets a new one-turn flag, `web_fallback`. `force_web_search` means the user asked: the web-search toggle or the query turn mode.
- **The vault comes first.** A turn with `web_fallback` searches the vault on its own, not beside the web. The web is queried only if:
  - the vault was searched, and
  - it returned nothing, or the sufficiency check judged its evidence insufficient (the final verdict, after any corrective retry).
- **The vault answers: no web.** If the vault returns passages and the verdict is sufficient, no web query is sent. Passages with no verdict count as sufficient.
- **User-forced web is unchanged.** It still runs beside the vault, whatever the vault holds.
- **Closed-book turns stay closed.** The merge refuses to reopen them, and the pipeline returns before any search.
- **No carry-over between turns.** `from_preferences` never sets `web_fallback`. `apply_turn_intent` sets it from this turn's intent alone and ignores the flag passed in. A follow-up reaches the web only if its own intent call says so.
- **Routing is as before.** Any turn with the web in its plan, forced or fallback, skips the router and searches the vault afresh. That gives the fallback a verdict to act on.
- **The prompt now tightens `needsWeb`.**
  - True only when the answer depends on information that cannot be in the user's files: current events, live data such as prices or weather, or public facts the user did not write about.
  - False for the user's own documents, notes, policies, projects or work, including anything asked about "our", "my" or "the company's" rules, plans or records.
  - When unsure, false.
  - The schema and the parser are unchanged.

## What changed (file:line, all under `crates/lattice-desktop/src/features/conversation/chat/`)

- **`routing.rs`**
  - :14 adds the `web_fallback` field. :6 now documents `force_web_search` as user-only.
  - :67 adds `SearchFlags::web_planned()`.
  - :167 `apply_turn_intent` maps `needs_web` to `web_fallback` (:173), not to `force_web_search`. Its doc comment is rewritten.
  - :128 the intent `info!` log now records `web_fallback`.
  - :203 `resolve_router_decision` short-circuits on `web_planned()`.
- **`intent.rs`**
  - :14 has the new `needsWeb` definition.
  - :46 rewrites the `TurnIntent` doc, which used to say "Enable-only".
- **`turn/classify.rs:7`**: rewrites the `classify_turn` doc, which used to say "Enable-only".
- **`retrieval/mod.rs:250-286`**: `RetrievalPlan::from_router` reads `web_planned()` for the route, the clarify short-circuit, `should_search_kb` and `should_search_web`. The comment describes the fallback.
- **`retrieval/policy.rs`**
  - :30 `should_use_external_as_fallback` treats `web_fallback` as a fallback.
  - :60 `kb_can_run_alongside_external` counts it as planned, so it never runs beside the vault.
  - :68 adds the `WebSearchReason` enum (`Requested`, `VaultEmpty`, `VaultInsufficient`), with `step_label` at :81.
  - :98 adds `intent_web_fallback(kb_attempted, kb_has_results, kb_sufficient)`.
- **`retrieval/pipeline.rs`**
  - :295-340 is the post-vault gate. Intent fallbacks use `intent_web_fallback`; every other external search keeps `should_execute_external_lookup`.
  - The skip log gains `kb_sufficient` and `web_fallback`, and a new log line marks a fallback that ran.
  - `ExternalLookup::run` (:519) and `web_search` (:1104) take the reason and use its label for the web-search step.
- **Test-only changes**
  - `prompting.rs`, `tests.rs` and `retrieval/corpus_plan/tests.rs` add `web_fallback: false` to their `SearchFlags` literals.
  - `test_runtime.rs:21` adds an optional `TestVault` (records plus library search) to `FakeRuntime`. Citation lookups use the existing empty in-memory document and chunk mocks.

## How the turn record says why

The web-search step's `label` carries the reason. Its `detail` stays the query, because the research deck reads the query from there. No new field was needed.

| Why | Step label |
|---|---|
| User toggle or query turn mode | "Searching the web, as you asked" |
| Intent fallback, vault returned nothing | "Searching the web, since your documents had nothing on this" |
| Intent fallback, vault judged insufficient | "Searching the web, since your documents fell short" |

A web search the model starts in the tool loop keeps the plain "Searching the web" label.

In the fallback cases the record already shows the vault steps before the web step: plan, search documents, then sufficiency with its reasons. The shipped-path runner counts step kinds, not labels, so "Turns where intent added web search" still counts. The labels can now split that count into fallbacks and user requests.

## What I verified

- `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2 cargo check --lib`: finished with no warnings.
- `cargo fmt`, then `rustfmt --check --edition 2021` on every touched file: exit 0.
- `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2 cargo test --lib -- conversation::chat`: **409 passed, 0 failed, 2 ignored**, no warnings. New or changed tests:
  - **Routing (`tests.rs`)**
    - `turn_intent_web_becomes_a_fallback_and_never_forces_the_web`
    - `a_web_search_the_user_asked_for_stays_forced`
    - `a_follow_up_inherits_no_web_fallback_from_an_earlier_turn`
    - `turn_intent_merge_*`, extended for the new flag and the closed book.
  - **Gate and plan (`retrieval/tests.rs`)**
    - `an_intent_that_needs_the_web_searches_the_vault_first` (replaces the old "runs side by side" test)
    - `a_web_search_the_user_asked_for_still_runs_beside_the_vault`
    - `an_intent_web_search_waits_for_the_vault_and_its_verdict`
    - `a_web_search_step_says_why_it_ran`
    - `concurrent_decision_matches_the_gate_kb_retrieval_will_produce`, now over 32 flag combinations including `web_fallback`.
  - **Fallback order through the real `run_retrieval_pipeline` on `FakeRuntime` (`retrieval/tests/web_fallback.rs`)**
    - Vault answers: no web call, and the vault sources stay.
    - Vault insufficient: one web call, recorded after the vault search and the sufficiency step, with the "fell short" label.
    - Vault empty: one web call after the vault search, with the "had nothing" label.
    - User-forced web: one web call even when the vault answers, with the "as you asked" label.
    - Closed book with an eager intent: no vault search, no web call, no steps.
  - **Multi-turn (`turn/tests.rs`)**: `each_turn_decides_its_own_web_fallback` runs `classify_turn` on two turns of one conversation. Intent says web, then not. The second turn has no fallback.
  - **Prompt text (`intent.rs`)**: `needs_web_is_for_what_the_users_files_cannot_hold`.
- `bash scripts/check-repository-barrier.sh`: clean (272 files).

## What I did not verify

- **`bash scripts/check-rust-layer-boundaries.sh`.** It runs `cargo run`, and the coordinator stopped new cargo commands because the disk is low. I added no production import across a feature boundary. The only new cross-feature paths are in test code (`test_runtime.rs`, `retrieval/tests/web_fallback.rs`), and the ratchet skips test code.
- **`cargo clippy --lib`.** Stopped for the same reason. The production code adds no indexing or slicing.
- **No live run.** The shipped-path eval re-run is the live measure.
- **How the 9B model reads the new `needsWeb` text.** Not measured.

## Risks and findings

- **The fallback alone would not have saved the two diluted answers.** I matched the baseline's web turns to their vault verdicts:
  - In the utility arm, 3 of 4 intent-web turns had an insufficient vault (`no_reranker` + `low_term_coverage`). Those 3 still go to the web under the new rule.
  - In the default-roles arm, 3 of 6 had an insufficient vault.
  - The receipt-threshold and sick-child turns, the two diluted answers, are both in the insufficient group.
  - On the v1 set, all 25 intent-web turns had a sufficient verdict, so the fallback alone removes all 25.
  - So for the desktop-path set, the prompt change is what has to keep those turns home. If the re-run still shows them, the next lever is the sufficiency check. Without a reranker it calls paraphrased questions insufficient on term coverage alone. Two options for the coordinator to decide:
    - Require a reranker verdict before an intent fallback runs.
    - Do not count `low_term_coverage` alone as falling short when there is no reranker.
- **A wrong `needsWeb: false` now costs a real web lookup.** "When unsure, false" and the "my" wording could make the small model say false for an outside question such as "the weather in my city". That turn then gets no web search at all.
- **The record now lands on the label.** The user-forced pipeline label changed from "Searching the web" to "…, as you asked". No backend or frontend code matches on that label, only on the step kind.
- **One untested path inside the gate.** A fallback whose web search then fails falls through to the no-context prompt, which says the documents were searched. It does not say the web failed. Before, this path claimed "The user explicitly requested web search", which was false for an intent turn.
- **Pages from an earlier answer still reach later turns.** Pages an earlier answer cited stay linked to the conversation (`linked_web_sources_context`) and are carried into later turns. That is evidence, not a web flag: no new search runs. But a page from a fallback turn can still appear in a follow-up's prompt.
- **Saved deep-research jobs.** `SearchFlags` is serialized in a saved deep-research job (`AssembledTurn`). A job saved by an older build will not deserialize after this change. No serde default was added, per the no-legacy rule.

## Suggested commit message

```
Search the vault before the web when intent guesses the web is needed

The intent step turned needsWeb into a forced web search, which ran beside
the vault search. Company-handbook questions left the machine as public
queries, and the pages changed two answers in the shipped-path eval.

needsWeb now sets a one-turn web fallback. The vault is searched first,
and the web only when the vault returned nothing or the sufficiency check
judged its evidence insufficient. A web search the user asked for is
unchanged. The web step's label says why it ran, and the intent prompt
keeps needsWeb for what the user's own files cannot hold.
```
