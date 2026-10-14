# Fix: the prompt budget could not trim evidence

Branch `fix/budget` (from origin/main 90bbbcb3), uncommitted.

Defect: `evals/retrieval/2026-10-12-SHIPPED-PATH-BASELINE.md`, "Defects found" §1. At `--ctx-size 8192`, 31 of 34 turns were refused before generation with `PromptBudgetExceeded`. At 16,384, 3 turns per arm were refused.

## Root cause

Two planners disagreed about what the request would hold, and the second could not give anything back.

1. **The turn's budget left out the tools and most of the policy.** `turn/budget.rs` sized the evidence pool with `BudgetRequest::new(system, 0, question)`. It charged no tool schemas, the system prompt without the memory rules the assembler appends, and the bare question without the date line, the template or the passage labels. On the 9B that left about 2,100 tokens of tools and policy unpaid (baseline §4), plus a few hundred of template and labels.
2. **The assembler charged that evidence as fixed.** The chat path rendered the evidence into the enhanced message and passed the whole message as `current_input`. `BudgetAllocation::plan` counts `current_input` as fixed. So the overflow surfaced as `PromptBudgetExceeded` before any eviction ran. The failed run's rows say exactly that: "The system policy, tools and current message need 8691 tokens, above the 5734 this request may occupy" (required 5,847 to 8,691, median 6,733). Eviction (evidence, recall, summary, oldest turns) only ever saw the separate evidence section, which the chat path left empty.
3. **A side effect of (2).** Because the evidence was charged as fixed, the assembler's history pools were fractions of the room left after the evidence, while the turn's budget had planned them as fractions of the room before it. When the evidence filled its pool, the assembler kept only about a third of the history the turn had planned for, which could force a compaction.

## What changed

### Context assembler (`src-tauri/src/application/services/context_assembler/`)

- `mod.rs:71` new `InlineEvidence { items, render }`: evidence the caller renders into the current message, best first. `ContextRequest` gains `inline_evidence` (`mod.rs:89`). With it, `current_input` is the bare message.
- `mod.rs:141` new `ContextAssembler::fixed_parts(system_policy, current_input, tool_schema_tokens, today)`. It prices the policy (memory rules appended), the tools and the bare message under its date line, through the same `fixed_messages` helper (`mod.rs:660`) that `assemble` uses. The turn's budget plans with it.
- `mod.rs:399` step 8: the full message is rendered and its extra cost charged as evidence, not as fixed. It has first claim on what the history left, ahead of saved memory (`mod.rs:427`). Only the policy, tools and bare message reach the `PromptBudgetExceeded` check.
- `mod.rs:561` `evict_until_fits`: after the evidence section, the inline evidence is cut one item at a time, lowest-ranked first (`mod.rs:579`), re-counting only the final message. Then recall, the summary and the oldest turns, as before. `ContextAccounting.inline_evidence_kept` (`plan.rs:68`) reports how many items survived.
- `render.rs:261` `FinalUserMessage::set_input`. The date line and input are stored apart; the rendered bytes are unchanged.
- `budget.rs` doc: evidence is never a fixed part.

### Chat turn (`src-tauri/src/features/conversation/chat/`)

- `turn/message.rs` (new): `TurnMessage` holds the message frame and its evidence as ranked items with the citation numbers each shows. Order, best first: attachments, the Explorer folder, prior evidence, the follow-up document, library passages in rank order, linked pages, web results. `render(keep)` builds the message through `PromptMessageBuilder` with the first `keep` items. `keep_sources` drops sources only cut items showed.
- `turn/assemble.rs:256` `render_prompt` builds the `TurnMessage`. `enhanced_message` is `render(items)`, byte-identical to the old builder output.
- `turn/assemble.rs:134` passages are priced as the prompt shows them (`kb_passage(result, 0)`: name and Document ID lines), not as bare text.
- `turn/assemble.rs:408` `plan_request` passes the bare message plus `InlineEvidence` to the memory plan. When the plan cut evidence, `RenderedPrompt::keep_evidence` (`assemble.rs:61`) drops those sources and refreshes the saved trace counts (`assemble.rs:475`). `plan_request` now takes `&mut RenderedPrompt` (`turn/mod.rs`).
- `turn/assemble.rs:514` `grounded_tool_tokens`: the schema cost of the tools a grounded turn is offered (grounded subset with page opening, history and folder tools). `select_tools` now shares `offered_tools` (`assemble.rs:593`) with it.
- `turn/budget.rs:45` `TurnBudget::plan` takes the tool tokens and `today`, and prices fixed parts with `fixed_parts` when bounded memory is on. The string path also pays for tools and the date line.
- `turn/prepare.rs:202` reserves `grounded_tool_tokens` and charges the RAG frame (`prompting::evidence_frame`, `prompting.rs:348`) as the message.
- `prompting.rs`: `CONTINUITY_NOTE` and `EVIDENCE_CUT_NOTE` constants; `kb_passages`/`kb_passage` (`prompting.rs:449`, `:463`); `with_evidence_cut` (`prompting.rs:70`). A cut message says lower-ranked evidence was left out for room (`prompting.rs:238`). A message with every item cut says evidence was found but did not fit (`prompting.rs:308`), not "no passage came back"; the forced-search notes no longer fire for it.
- `retrieval/source_citations.rs:80` `web_context_entries`, one entry per page. `render_web_context` and `build_kb_context` are now test-only (`#[cfg(test)]`); `chat.rs` drops the unused import.
- `memory_context.rs:152` `build_memory_plan_for_query` takes `inline_evidence`. `build_memory_plan` passes `None`.

## Tests added

- `context_assembler/tests/turn_evidence.rs`:
  - `an_8k_window_cuts_the_lowest_ranked_evidence_instead_of_refusing_the_turn`: policy plus tools about 2,100 tokens, 24 passages. Charged whole it is refused at 8,476 against 5,734 (inside the baseline's 5,847 to 8,691). As inline evidence it fits, keeps the best passages and drops the lowest (passage 24 goes, passage 1 stays), and leaves less than one passage (300 tokens) unused. By hand, that is 13 of 24 kept.
  - `a_16k_window_assembles_the_turns_the_baseline_refused`: 11,658 and 13,250 against 11,468, both refused whole, both assemble.
  - `evidence_that_fits_is_sent_whole_and_the_request_is_unchanged`: with history, summary, recall and saved memory at 32K, the plan is byte-identical to the old whole-message plan.
  - `the_turns_evidence_is_cut_before_recall_and_the_summary`.
  - `a_window_with_no_room_for_any_evidence_sends_the_bare_message`.
  - `the_policy_tools_and_bare_message_alone_still_refuse_a_turn_they_do_not_fit`.
- `memory_context/tests.rs`: `evidence_rendered_into_the_message_is_cut_to_fit_instead_of_refusing_the_turn` through the real repository and recall.
- `turn/message.rs`: the whole message equals the builder's; web results go before library passages, lowest first; the bare message says evidence did not fit; only cut items' sources are dropped.
- `turn/budget.rs`: `at_8k_the_evidence_pool_leaves_room_for_the_tools_and_the_policy` (tools come out of evidence token for token; fixed parts plus evidence equal the 5,734 input budget), `the_string_path_also_pays_for_its_tools_before_evidence`, and the three existing tests on the new signature.
- `tests/prefix.rs` is unchanged.

## Verified

All cargo commands ran from `src-tauri` with `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2`.

- `cargo check --lib` → `Finished`, exit 0, no warnings.
- `cargo test --lib -- context_assembler conversation::chat::turn conversation::chat::retrieval conversation::chat::memory_context conversation::chat::prompting` → `test result: ok. 213 passed; 0 failed; 1 ignored` (the ignored one is the live corpus test that needs `LATTICE_RETRIEVAL_DB`). No compiler warnings. All new tests above passed, and so did the unchanged `tests/prefix.rs` (4) and `tests/eviction.rs` (3).
- `cargo clippy --lib` → `Finished`, exit 0, no warnings or errors (the crate denies `indexing_slicing`, `unwrap_used`, `expect_used`, `panic`).
- `bash scripts/check-repository-barrier.sh` → "Repository Barrier check: clean (272 production feature files)."
- `bash scripts/check-rust-layer-boundaries.sh` → "Rust layer boundary check: clean (syntax-aware)." The first run flagged one new `features::conversation -> features::settings` reference (an import in `turn/message.rs`). It now goes through the chat feature's own re-export, and the edge count is back to the baseline 345.
- `rustfmt --edition 2021 --check` on every touched file, and on `chat.rs` (which walks the whole chat module tree) and `context_assembler/mod.rs` → clean. I formatted with `rustfmt` on the touched files rather than `cargo fmt`, so no file outside this change was rewritten.

## Not verified

- No live turn and no eval re-run (no model process was allowed). The fix is not yet measured against the 8K and 16K shipped-path runs.
- No `cargo test` outside the listed modules, no integration tests, no `--all-targets`. No crate-wide `cargo fmt --check` (stopped for disk space).
- The one-line import change in `turn/message.rs` (for the boundary check) came after the test run. `cargo clippy --lib` compiled it, but the `turn::message` tests were not re-run against it. The type it names is the same.
- The live "retrieval" stream event is emitted before planning and is not re-sent after a cut. The saved trace and sources are corrected.

## Risks

- **History pools now come from the room after the bare message.** In a long conversation on a small window the assembler can carry more history than before and less evidence. This matches what the turn's budget already planned. When all history and evidence fit, nothing changes (the 32K identity test).
- **Citation numbers can have gaps** after a cut, because sources are numbered before planning. The model only sees the numbers it is given.
- **The tool reservation assumes a grounded turn with page opening.** A turn that sends fewer tools reserves up to about one schema too many. A "query"-mode turn reserves the full set, as it sends.
- **Passage pricing now includes labels**, so a tight window selects slightly fewer passages than before.
- **The string path (bounded memory off) still has no trimming.** It now reserves tools and the date line, but an overflow there still reaches the provider.
- Each cut item re-renders and re-counts the final message once. With tens of items this is negligible next to the model call.

## Suggested commit

```
Let the prompt budget cut evidence instead of refusing the turn

At an 8K window 31 of 34 shipped-path turns failed before generation. The
turn sized its evidence without paying for the tool schemas, the memory
rules or the template, and the assembler then charged that evidence as part
of the fixed message, so nothing could be cut and the plan was refused.

The turn's budget now reserves the tools a grounded turn sends and prices
the policy and message exactly as the assembler does. The chat message keeps
its evidence as ranked items, and the assembler charges only the policy,
tools and bare message as fixed. When the request will not fit it cuts the
lowest-ranked evidence first, then recall, the summary and the oldest turns,
and the turn drops the sources of what was cut. A refusal now means the
policy, tools and bare message alone do not fit.
```
