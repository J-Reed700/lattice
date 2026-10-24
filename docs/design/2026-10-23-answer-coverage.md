# Answer coverage: read every passage, then write — design

Date: 2026-10-23. Code read at #167 (per-document cap removed). Data: the shipped-path run of #167 on Qwen3.5-9B, a replay of its captured answer requests across passage depth, tools and seeds, and a replay of the same requests with thinking on. The replay harness is `scripts/replay_answer_round.py`.

## 1. Verdict

The diagnosis stands: retrieval delivers every fact, and the 9B drops passages while composing. The fix is to split the answer round into **a reading round and a writing round on the same cached prompt**: the model first writes one line per numbered passage (the sentences that bear on the question, copied verbatim, or a hyphen), then writes the answer from its own notes; afterwards the app checks, deterministically, that every passage the model's notes quote is cited in the answer, and asks for a continuation covering the ones left out, until no progress is made. This is Chain-of-Note applied to the passages already in the prompt, with a coverage check that is the exact dual of the grounding check: grounding asks "is every cited claim in a passage"; coverage asks "is every quoted passage in the answer".

It costs one extra generation per grounded turn (the prefill is paid once and shared), about 5 tokens per passage plus the copied sentences, 9–20 s on the local 9B, and a continuation only when the check finds an omission. It applies to every question without classifying any; the only inputs are the labels in the prompt, the model's own notes, and the labels in its answer.

Trimming to the first N passages, thinking, and dropping the tools are all rejected by the data (sections 2 and 5). Offering tools on the answer round should not change yet; section 5 says what settles it.

## 2. Diagnosis: confirmed, with two corrections

What I checked myself (the captured household request; hall, chronicle, lab-notebook and choir fact positions; the replay rows for household, hall and chronicle):

- **Every rubric fact is in the prompt, in passages [2]–[8].** Household: facts in [2],[3],[4],[5],[6],[7] of 33. Hall: [2]–[7] of 16. Chronicle: [2]–[7] of 29. Lab notebook: [2]–[7] of 33. Choir: [3]–[8] of 26. Within a passage the fact sits 280–1,520 characters in, after filler, in passages of 860–1,680 characters. The "neighbours" PR already puts hits ahead of neighbours, which is why hits are [2]–[8].
- **The answer is a one-pass skim that stops early.** Household full prompt, tools, seed 1: four bullets citing [2],[3],[4],[5]; seed 2: five bullets, skips [7]. Both stop at `finish_reason=stop` with max_tokens untouched (4,096). Hall misses `£6,497.92` (passage [2], the *second* passage) on both seeds, with and without tools. Chronicle seed 1 skips [2] and [4]; seed 2 skips [3],[6],[2]. Which passage is skipped varies by seed; that one or two are skipped does not.
- **Prompt size is not the cause.** The thinking-replay "hits-only" prompts (10 passages, ~4k tokens, no tools) still miss 3 of 6 on chronicle and 3 of 6 on choir. Top-10 with tools misses 4 of 6 on hall seed 2. So the owner is right twice over: a cap is a band-aid, and it is not even a working band-aid.
- **Correction 1 — tools are not established as a cause.** Full prompt, seed 1, tools → no tools: household 4→6, hall 5→5, chronicle 4→3, lab 5→5, allotment 6→6. One win, one loss, three ties. The household data point was sampling.
- **Correction 2 — thinking is unsafe at this window, not merely slow.** Full-prompt thinking on chronicle ran to `finish_reason=length` at 7,690 completion tokens and produced no answer at all; the other full-prompt thinking runs took 133–337 s.
- **Latency facts for the cost model** (same Mac, 9B Q4_K_M, 16k): a cold 11k-token prompt takes 40–47 s; the same prompt warm (seed 2 on the same slot) takes 1.5–14 s. So prefill ≈ 300 tok/s and generation ≈ 20 tok/s. Everything below is sized from those two rates and the passage count, not from this machine.

**Per-item weighing of the consistent misses** (coordinator's update at 13 of 18 replayed; I checked each passage and the answers):

| Item | Fact | What the passage says | Verdict |
|---|---|---|---|
| hall-transcript | £6,497.92, missed 6 of 7 runs, every arm | `[18:56:58] Councillor Reallhollow: I'm putting £6,497.92 from my ward budget towards the new kitchen, and that is settled.` — one line of a timestamped multi-speaker transcript in passage [2], after seven other speakers' lines | **Genuine omission.** The one run that used it wrote "He committed to contributing £6,497.92 … [2]", so the model reads it as a commitment when it looks. It skims past it because it is not shaped like the other five ("I will…", "I'll…"). A per-passage reading of [2] alone is the remedy |
| chronicle | 1887-12-21, missed 7 of 7 | `1887-12-21. The last miller at Greassby Mill retired, and the building was let as a store.` in passage [2], under the heading "Entries gathered in June" | **Genuine omission in 6 runs; a scoring miss in the 7th**, which wrote "December 1887" (the rubric's substring wants the ISO form). The same drift ("December 1850" for 1850-11-18) costs other chronicle runs. Two consequences: the rubric should accept the written-out date forms as it accepts "82 days"; and the verbatim notes of section 4 carry the exact date string into the writing round |
| lab-notebook | 2025-01-10, missed in every arm | `Went back to the site of the Throassmere core on 2025-01-10 to take a second core, labelled with a B suffix.` vs the ask "List every reading and observation … for the Throassmere core" | **Correct exclusion.** The answer lists five readings and leaves the site visit out; a site visit is neither a reading nor an observation. Rubric mismatch (section 7), not a model fault. A reading round would, rightly, hyphen it |
| household | 2025-03-19, missed with tools on both seeds, found without tools and at top-10 | the chimney-sweep line in [7], plainly about the stove | Genuine, variable omission — the ordinary skim |
| allotment, cookbook | shipped failures | pass on both replay seeds | Sampling noise, like the short items |

So the population is: variable skim omissions (household, chronicle's other dates, choir), one passage-shaped consistent omission (hall [2]), one date-form scoring artefact (chronicle), and two rubric mismatches (lab-notebook, choir). The design below fixes the first two and makes the third rarer; the last two are rubric edits.

The mechanism is the known multi-needle shortfall: a small model asked to aggregate k facts scattered across a long context reliably returns k−1 or k−2, independent of where the needles are (RULER's multi-needle tasks and the "Needle Threading" results show recall falling with the number of needles well inside the window; SummHay measures the same thing as "insight coverage" and finds even large models far from complete). It is not "lost in the middle" — the misses here are at positions 2 and 7 of 33.

## 3. What the research offers, and what fits this pipeline

| Approach | What it does | Fit |
|---|---|---|
| **Chain-of-Note** (Yu et al. 2023) | A note per retrieved document before the answer, including "irrelevant" verdicts; improves robustness to noise and completeness, because the model attends to each document in turn instead of skimming the set | **Adopted.** The forced per-passage verdict is exactly what the skim lacks. One extra generation; the prompt is already in the cache |
| Quote-first prompting (Anthropic long-context guidance; extract-then-answer) | Pull relevant quotes into a block, then answer from them | **Adopted as the note format.** Verbatim copies are language-neutral, make a deterministic coverage check possible, and cannot drift the way paraphrased notes do |
| Chain-of-Verification (Dhuliawala et al. 2023) | Draft, generate verification questions, answer them independently, revise | Its structure (draft → independent check → revise) is kept; its model-generated questions are not: the check here is deterministic, using the labels the model itself wrote |
| Map-reduce reading | One call per passage, then reduce | Rejected: N calls on a single-slot server |
| Lost in the Middle / query-aware contextualization (Liu et al. 2023) | Put the query before and after the data; reorder by relevance | Not the failure here (misses at position 2); layout hygiene only (section 6) |
| Context compression (RECOMP, FILCO) | A trained compressor selects/abstracts passages before the reader | The reading round *is* an extractive compressor, done by the same model on the cached prompt, with no second model |
| Thinking | Reason before writing | Rejected by the data: 2–5× latency, one window overflow |
| Self-critique loops (Self-Refine, Reflexion) | The model critiques its own output | Rejected as a free-form critique on a 9B; kept only in the form where the critique is computed by the app from structure (noted labels − cited labels) |

Coverage checking in the literature is an evaluation tool (RAGAS context recall, SummHay coverage, QA-based recall). Here it becomes a runtime check because the model's own notes give it an oracle: the set of passages the model said bear on the question.

## 4. The fix

### 4.1 Shape of a grounded turn

Today (tool_loop.rs, iteration 0): `[system][user: frame + passages + "Question: … Answer:" + citation rule]` → answer, streamed, tools callable.

Designed:

1. **Prompt** gains one closing instruction, appended by the prompt builder after the citation rule, only when the message carries numbered passages (library, web pages, attachments, prior evidence, follow-up document — anything with a citation label):

   > Before answering, go through every numbered passage above in order and write one line for each: its label, then the sentences of that passage that bear on my question, copied exactly as written and in the passage's own language. For a passage that says nothing bearing on my question, write its label and a single hyphen. Account for every label. Write only these lines; the answer comes after.

2. **Reading round** (new, before iteration 0): the planned request as is, tools kept with `ToolChoice::None` (so the cached prefix holds — the pattern `research_notes::condense` already uses, for the same reason), `SamplingOverride::deterministic()` (a transcription), `ThinkingMode::Off`, `max_output_tokens` = the round's reservation. Output: the notes. Shown in the UI as a `Generate` step "Reading the passages" with the notes as its disclosure, the way condensing shows "Condensing what it has read".

3. **Sweep completeness** (deterministic): parse the notes into `(label, remainder)` lines. Labels present in the prompt but absent from the notes are asked for again in a continuation of the reading round ("You did not account for [12], [19] …"), and a reading cut at `finish_reason=length` keeps its complete lines and continues from the first unaccounted label. The loop ends when every label is accounted for, when a continuation accounts for none, or at the turn's clock. The bound is the label set, not a count.

4. **Writing round** = today's iteration 0, with the notes appended as the model's own assistant message (the provider's `replay` shape, as tool rounds already do) and one user instruction said last:

   > Now answer my question. Your reading notes above say what each passage states about it: cover every passage you noted, cite each with its label, and take the wording from the passages themselves. Leave out a note that was mistaken.

   Tools callable as today (section 5), `ThinkingMode::for_answer` as today, the app's sampling, streamed as today. The "reading" instruction in the prompt stays in place: the model sees it was asked to read, read, and is now asked to write; the prefix is byte-identical to the reading round's.

5. **Coverage check** (deterministic, after the round that produced an answer with no tool calls, before `emitter.done()`):
   - `noted` = labels whose note, whitespace-normalised, occurs verbatim in that passage's text (`SourceDto` content, which the loop already holds in `sources`). A note that is not a copy — the model wrote "none", paraphrased, or quoted the wrong passage — does not count. That is the only guard the check needs, and it is exact-text, language-neutral and threshold-free.
   - `cited` = labels in the answer, via the verifier's citation extractor (`verification/lexical.rs::extract_sentence_citation_ids`, exposed as `cited_labels(text)`).
   - `left_out = noted − cited`. If empty: done, as today.
   - Else one **continuation** request, prefix cached (planned prompt + notes + instruction + the answer so far): tools kept, `ToolChoice::None`, `ThinkingMode::Off`, said last:

     > Your answer does not use these passages, which your notes quote as bearing on my question: [6] "…" [7] "…". Continue your answer with what each of them says, in the same form as above, citing each with its label; do not repeat what you have written. If one of those notes was mistaken, write nothing for it.

     The continuation is streamed as appended text after a paragraph break (the emitter has `content` and `done` only; `OUT_OF_ROUNDS_NOTE` sets the precedent for appending). It is repeated while each continuation cites at least one label that was still left out; it stops on the first that cites none, or at the clock. Termination is monotone progress on a finite set.

6. **Verification** runs unchanged on the final text (answer + continuations), from `finalize_turn`.

### 4.2 Why this works for any question, with no classification

Nothing inspects the question. The reading round runs whenever the prompt carries labels; its output size is set by how many passages bear on the question. A single-fact question (dp-paraphrase-clock, fact in [16] of 30) produces 29 hyphen lines and one quote; the writing round is unchanged; the coverage check is satisfied by the one citation. An abstain question produces only hyphens, and the answer then follows the system prompt's existing rule for that case — say the answer is not in the documents, or, where tools are offered, search further. The writing instruction deliberately says nothing about abstaining: a sentence telling the model to report the answer unavailable would override the tools on research turns and on any turn whose retrieval came up short. A multi-fact question produces k quotes and the check holds the answer to k citations. Section neighbours that say nothing about the question get a hyphen and never reach the answer — which is the relevance check the owner asked why we never had.

The notes also make the two failure kinds distinguishable in the turn record, which today's single pass hides: a hyphen on hall [2] means the model judged the line out (a wording or rubric question); a quote on [2] with no `[2]` in the answer means it skipped what it had found, and the continuation repairs it. The eval should report both counts.

### 4.3 Every number, and where it comes from

| Quantity | Source |
|---|---|
| Passages read | the labels in the prompt (evidence budget → `retrieval_depth`) |
| Reading output ceiling | the answer's output reservation R, for all sweeps together: each sweep asks for R less the notes kept so far (measured with the backend's tokenizer where it has one), and the reading ends at zero |
| Writing round ceiling | min(R, window − measured writing request); the notes are withdrawn when that room is smaller than the notes' own measured size |
| Coverage continuation ceiling | min(R, window − measured continuation request); skipped when no room is left. Written whole before it is shown, then shown and saved as one piece; a failure or the clock keeps the answer as it was |
| Reading continuation bound | the set of unaccounted labels; stop on no progress |
| Coverage continuation bound | the set of left-out labels; stop on no progress |
| Time | the turn's `TurnClock` (deadline/Stop), as every round |
| Note validity | exact substring of the passage after whitespace normalisation |

No thresholds, counts, ratios or timeouts are introduced. The reading instruction adds ~80 tokens to the message and is priced by `TurnBudget::plan` automatically because the builder renders it into the message.

### 4.4 Cost on a single-slot local server

Per grounded turn, at the measured rates (prefill ~300 tok/s, generation ~20 tok/s on the 9B; both scale with the server):

| | Calls | Prefill | Generation | 9B wall-clock |
|---|---|---|---|---|
| Today | 1 | 11k tokens once | answer 150–300 tok | 45–52 s |
| Reading round | +1 | the same 11k, paid here instead | ~5 tok × N passages + ~40 tok × k quoted passages: 185 tok (30 passages, 1 fact) to 375 tok (33 passages, 6 facts) | +9–19 s |
| Writing round | — | notes + instruction, ~300–500 tok (cached prefix) | answer, as today | +1–2 s |
| Coverage continuation (only on an omission) | +1 | answer + instruction (cached) | 50–120 tok | +3–6 s |
| Typical total | 2 | | | **55–73 s, i.e. +10–20 s** |

Why it is justified: today one in six facts is dropped on every long item, on both seeds, and the only other thing that recovers them (thinking) costs +90–290 s and overflowed the window once. The reading round converts silent waiting into a visible "Reading the passages" step, and the answer then streams faster because the writing round's prefill is cached. On the 60–180 s turns measured, the planner's ~70 s remains the largest single cost and is untouched here.

Why the prefix sharing holds on Qwen 3.5: the reading request is the planned request plus nothing; the writing request diverges only after the notes, which the server just generated (llama.cpp keeps the hybrid model's checkpoint at the end of a generation — the condense note in `research_notes.rs` measured the next round re-reading "only the 150 tokens of notes"). The depth replay's warm seed-2 timings (1.5–14 s against 40–47 s cold) are the same effect measured on this model.

llama.cpp refuses `json_schema` together with `tools` in one request, and the tools must stay in the request for the prefix to hold; so the notes are plain lines parsed structurally (label, then either no alphanumeric character or text), not a schema.

### 4.5 UI, journal, resume

- Steps: `Generate` "Reading the passages" (disclosure = notes), `Generate` "Writing the answer" as today, `Generate` "Adding what was left out" for a continuation, each with its result line.
- The saved assistant message is the answer text including continuations; the notes live in the turn record only.
- Research turns: the notes and the writing instruction sit after `planned_len`, so `journal.round` saves them with the first round's transcript; a resumed turn with a saved transcript skips the reading round (it is in the transcript). A resumed turn whose saved rounds hold no notes (the reading failed or was withdrawn) has the reading instruction withdrawn from its message, as the live turn did.
- Explorer turns without library search have no labels and are untouched. A turn whose prompt has no labels (greeting, closed-book without evidence) is untouched.

### 4.6 Risks, and the safe failure for each

- The reading round itself writes a hyphen for a bearing passage. This is the residual; the eval measures it. Judging one passage against one question is the easy case of the task the skim fails, which is the whole bet.
- A copy is inexact → it fails the verbatim test → that passage cannot trigger a continuation. Safe: the note still reaches the writing round. If the eval shows many inexact copies, that is a prompt-wording problem, not a reason for a similarity threshold.
- "none"/"nichts" instead of a hyphen → treated as a note, fails verbatim, "leave out a mistaken note" applies. Harmless.
- A whole attachment under one label → a long note, bounded by the reservation, continued by the sweep-completeness loop.
- An "in order" answer (chronicle) gets its left-out items appended out of order. Accepted: a complete answer with a seam beats an ordered one with a hole; the emitter cannot replace streamed text and buffering the answer would cost the streaming.

## 5. Tools on the first answer round

**Decision: no change now.** The data (section 2, correction 1) is one win, one loss, three ties on one seed; the hall item misses the same fact with and without tools. Dropping tools would also remove the model's only path when retrieval is short, which the system prompt relies on.

**What settles it:** the replay harness, with the reading round in place (the decision belongs to the shipped shape), arms {tools kept with `tool_choice: none` on the reading round and callable on the writing round} × {no tools in either}, 3 seeds, all 12 long items and the 11 short grounded items (buried/section/opening/transcript/paraphrase/multihop). Score facts found per item and the abstain items' behaviour. Adopt no-tools only if, paired per item across seeds, it wins on more items than it loses and loses consistently on none. Report the prefill saved too (the six schemas are ~1,600 prompt tokens ≈ 5 s at 300 tok/s).

If it wins, the structural trigger is already in this design: a reading round whose notes are all hyphens is the app's own finding that the evidence is short, so tools would be offered on the writing round only then (at the cost of one re-prefill in that case, since tools are rendered into the system prompt). Until measured, tools stay as today.

## 6. Prompt layout hygiene (same PR, no measured claim)

- The citation rule is in the system prompt and again after `Answer:` in the user message; the second copy sits after the answer cue. Keep one copy, before the question.
- Passage [1] renders the document's absolute temp path (`/var/folders/…/Saltings Cottage maintenance log.md`) while later passages render the basename. Tokens and a path leak; same text for all.

## 7. Rubric conflicts in desktop-path-v1 (evals/retrieval/desktop-path-v1.json)

Weighed per item against the replay data (section 2 table):

- **dp-long-lab-notebook — real conflict, and the model is right.** "List every reading and observation" does not cover "Went back to the site … on 2025-01-10 to take a second core"; every run excludes it and lists the five readings. Either rephrase the ask ("everything the notebook records about the Throassmere core") or drop 2025-01-10 from `must_mention`.
- **dp-long-chronicle — scoring form, not question.** The ask is fine; the rubric wants ISO strings and the model writes "December 1887" / "December 1850" in some runs. Add the written-out forms as alternates (`["1887-12-21", "21 December 1887", "December 1887"]`), as "82 days" is already an alternate of "82 calendar days". Keep the ISO form first: the verbatim notes push the answer toward it.
- **dp-long-hall-transcript — no conflict.** "I'm putting £6,497.92 from my ward budget towards the new kitchen, and that is settled" is a commitment; the run that cited it called it one. This item measures the model.
- **dp-long-choir-minutes — real conflict.** The ask is "What did the choir committee *decide* … every decision", but the rubric requires "£6,133.78 received from the retiring collection" (a receipt, not a decision). The thinking replay omitted exactly that fact. "Trustees will next report on 2027-06-19" and "the treasurer closed the old bank account on 2029-09-22" are minuted actions, not resolutions, and are borderline. Fix the ask to "everything the minutes record about the Holdmere fund over the year" or move those facts to `must_not_claim: []`/notes.
- **Mild (the ask's narrowing clause excludes a required fact):** dp-long-conservation-record ("treatments and checks" vs. lining ordered at £9,523.15, returned to the gallery 2034-12-27); dp-long-observing-log ("every observation" vs. a forecast of 61 days and a perihelion date from a circular); dp-long-cookbook (the clause after the colon narrows to use and care; 1875-06-16 is lineage — but the ask opens with "Everything … says about", so it holds). For each, phrase the ask as "everything the <document> records about <subject>", which is what the rubric scores.
- No conflict: household, hall, chronicle, allotment, apiary, canal, family-history.
- Method note: the current shipped run is one seed; on main three "failed" short items pass on both replay seeds. Any pass/fail claim needs pass^k with k independent runs (the 2026-10-12 baseline doc already flags that its pass^3 was three grades of one answer, not three runs).

## 8. Files, tests, eval runs

### Files

| File | Change |
|---|---|
| `crates/lattice-desktop/src/features/conversation/chat/tool_loop/reading.rs` (new) | `READING_INSTRUCTION`, `WRITE_INSTRUCTION`, `CONTINUE_INSTRUCTION`; `parse_notes(text) -> Vec<(u32, Option<String>)>`; `verbatim(note, passage) -> bool` (whitespace-normalised substring); `unaccounted(labels, notes)`; `left_out(noted, cited)`; `read_passages(llm, request, labels, recorder, cancel, clock) -> Result<Option<Notes>>` (reading round + sweep-completeness continuation, modelled on `research_notes::condense`); `append_notes(request, notes)` (assistant replay + `say_last(WRITE_INSTRUCTION)`); `continue_answer(...)` (one coverage continuation) |
| `crates/lattice-desktop/src/features/conversation/chat/tool_loop.rs` | Before the round loop, when `first_round == 0`, no resumed transcript, and `sources` carry labels: `read_passages`, `append_notes`. After an answer with no tool calls and before `emitter.done()`: the coverage loop, appending each continuation to `response_text` and streaming it |
| `crates/lattice-desktop/src/features/conversation/chat/prompting.rs` | `PromptMessageBuilder::build` appends `READING_INSTRUCTION` after `enforce_numeric_citation_format` when the message has labelled items; drop the duplicate citation rule (section 6) |
| `crates/lattice-desktop/src/features/conversation/chat/verification/lexical.rs` | `pub(in crate::features::conversation::chat) fn cited_labels(text: &str) -> BTreeSet<u32>` over the existing extractor |
| `crates/lattice-desktop/src/features/conversation/chat/retrieval/` (passage renderer) | Same document name form for every passage (section 6) |
| `scripts/replay_answer_round.py` (new) | Replays captured answer requests with arms, seeds and the read→write chain; the harness the eval doc cites |
| `evals/retrieval/2026-10-2x-ANSWER-COVERAGE.md` (new) | The runs below, with numbers |

No schema, settings, flag or compat change. `TurnBudget`, `retrieval_depth`, the verifier and the judge are untouched.

### Tests (unit, in the new module and `loop_tests.rs`)

- Parsing: labels in order, hyphen vs text, a label with "none" written out, non-Latin/accented text (German and Spanish lines), lines without a label ignored, cut output keeps complete lines.
- Verbatim: whitespace runs and line breaks normalised; a paraphrase fails; a quote from another passage fails.
- Sweep completeness: missing labels are asked for; a continuation that adds none ends the loop; a cut reading continues from the first unaccounted label.
- Coverage: `left_out` with `[2][3]`, `[2, 3]` and sentence-end citation forms; a note that failed verbatim never appears in `left_out`.
- Request shape: the reading request keeps the tools with `ToolChoice::None`, deterministic sampling, thinking off, the round's reservation; the writing request's input is the reading request's input + assistant notes + the instruction (prefix identity asserted by comparing the leading items); the continuation forbids calls and thinks off.
- Loop (mock LLM, as `loop_tests.rs` does): notes quoting [2],[5],[7] → answer citing [2],[5] → one continuation requested naming [7] → appended and streamed; a continuation citing nothing ends the loop; a resumed research turn with a transcript runs no reading round; a prompt with no labels runs no reading round and sends the same request as today.
- Prompt builder: the instruction is appended only with labelled items (greeting, closed-book-without-evidence and Explorer-without-KB prompts unchanged); the message is priced with it.

### Eval runs that prove it

1. **Before building — replay chain** (an hour, no code): `replay_answer_round.py` on the captured shipped requests, arm `read→write` vs `full+tools`, 2 seeds, 12 long items. Expected: long-item facts 5/6 → 6/6 on most items; this is the go/no-go for the design. Score it against the rubric *after* the section 7 edits (lab-notebook, choir, chronicle date forms), or the two rubric items will count as model misses. Report per item: notes that quoted the fact's passage, notes that hyphened it, answers that cited it.
2. **Shipped path, long items, pass^3 as three independent runs**: `cargo run --example shipped_path_eval -- evals/retrieval/shipped-path-recipe-9b-local.json OUT --only <12 dp-long ids>` three times on the 9B at 16k. Report facts found per item per run, and per-item generation time against the #167 run (`shipped-a-6326b797`).
3. **No regression on short items**: the same recipe over the full desktop-path-v1 set once (abstain, conflict, table, multi-turn and topic-shift items included); the abstain items must still abstain and the dp-mt-* items must not change outcome. Compare with the #167 run; any short item that flips is re-run on two replay seeds before it is called a regression (section 7, method note).
4. **The tools measurement** of section 5, after 1–3, on the replay harness.
5. Record all of it in the eval doc with the commit hash, model hash and recipe, as the baseline doc does.

## 9. Out of scope, noted for later

- The lexical sufficiency heuristic (`retrieval/sufficiency.rs`, `COVERAGE_TOP_N`, term coverage) could be replaced by the reading round's finding, but it drives corrective search *before* the prompt exists; moving it is a separate change.
- The reading notes are quote-level evidence the grounding judge could use directly (a claim whose cited passage's note contains its numbers is supported without a judge call); a later saving of verification time.
- The planner's ~70 s is the largest cost in the turn and is untouched here.
