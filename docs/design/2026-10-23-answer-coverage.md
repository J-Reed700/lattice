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

## 10. Reading misses on the merged tree: diagnosis, the second look, and the A/B (2026-10-24)

### 10.1 What the 24 readings show

Three runs × 8 items = 24 reading rounds, 144 fact-bearing passages (6 per item), 158 note lines with text. Every fact was in the prompt every time. Scored: r1 4/8, r2 5/8, r3 5/8 — 10 failures.

**Where each fact-bearing passage ended up** (reading round only, before the writer):

| outcome | passages | what it is |
|---|---|---|
| quoted verbatim under its own label | 121 | the design working |
| hyphen, but the sentence was quoted under label **[1]** | 7 | a *mislabelled* note, not a miss: the document's first bearing sentence (in [2] or [3]) written as [1]'s line, then "[2] -" because the sentence was already used. 8 such notes in all; conservation ×3, apiary ×2, choir ×2, lab ×1 |
| hyphen in a reading that wrote only hyphens | 6 | hall r3: the model opened with `1. [1] -` and wrote 16 numbered hyphen lines. The prompt differed from r1 only by document IDs and the order of decoy chunks [11]–[16] |
| hyphen, sentence quoted nowhere | **10** | the true misses |

**The 10 scored failures:** 8 are true misses (r1 hall, conservation, family; r2 household, hall, choir; r3 conservation ×2 facts, family), 1 is the collapse (hall r3: the writer recovered 5 of 6 from the passages with no notes to go on), 1 is the chronicle form miss. None of the 7 mislabels cost a fact — the writer found them in the prompt — but they cost citations: apiary cited [1] for [2]'s sentence in both runs and the grounding check marked one claim unsupported each time; conservation r2 the same.

**Hypotheses against the data:**

| hypothesis | verdict | evidence |
|---|---|---|
| judges a passage by its opening | refuted as a discriminator | every synthetic passage opens with filler; 121 hits have the same shape as the 10 misses (`## heading`, filler paragraph, the one subject sentence, filler) |
| the fact is late in a long passage | refuted | 47 hits at relative offset 0.91–0.99 (incl. five last sentences); misses at 0.08, 0.10, 0.48, 0.51, 0.57, 0.95, 0.99; mean length of missed passages 948 chars vs 881 for hits |
| a count prior | **supported, in rank form** | miss rate by the passage's rank among the item's bearing passages, in label order (collapse excluded): ranks 1–2: **0/46**; ranks 3–5: 5/69; rank 6 (the last one): **5/23**. Eight of the 10 true misses fell on the 4th–6th bearing passage, after 3–5 quotes had been written. Readings do not stop at a count (choir r1 quoted 19 labels), but the willingness to quote drops as the list grows — the pre-#174 "one bullet each, last ones dropped" prior, now acting line by line |
| instruction wording | untestable here | no wording varied across the runs; the A/B has an arm for it only indirectly (the second-look text) |
| deterministic decoding | half true | per prompt it is deterministic: hall r1 and r2 notes are byte-identical. Across trivially different prompts it is chaotic: reordering six decoy chunks turned hall r3 into a numbered-list collapse; family [7] (same text, same label) was quoted in r2 and hyphenated in r1 and r3; household's `2025-03-19` passage was quoted as [5] and [6], hyphenated as [7]. The decision is a near-tie at one token, and greedy decoding gives it no second chance |
| something else | three things | (i) the first-chunk mislabel above; (ii) the format collapse; (iii) one reading (short6 paraphrase-clock) prefixed every line with the passage heading, so no note was verbatim and the coverage check had nothing to work with (the fact was still found) |

The remaining true miss that the rank prior does not explain is hall [4] — one line by the councillor among fourteen by other speakers, missed identically in both working readings. That is dilution: the single-step lookup over a 10.8k-token prompt does not light up one line in a block of filler.

**What the logs cannot tell apart:** whether a true hyphen is a *lookup* failure (the sentence never got attention at the decision token) or a *judgment* failure (attended, judged not to bear). The reading calls recorded no logprobs. Arm A0 of the A/B records them at zero cost.

### 10.2 The fix

Three parts here, a fourth and the short-question rules in 10.5. The first two are deterministic and cost nothing; the third is one more model call on the cached prompt. `tool_loop.rs` does not change: everything happens inside `reading::read_passages`, whose signature and `Option<Notes>` contract stay as they are (10.5 widens the `Ok(None)` case to "no verified copy"). The one change outside `reading.rs` is the optional `stop` field of 10.5.

**Part 1 — a note belongs to the passage it was copied from (`reading.rs`).**

`verbatim(note, passage)` already decides whether a note is a copy. Generalise it to `locate`: given a note, the set of labels whose passage contains every sentence of it.

- If the labelled passage is in the set → as today.
- If the set is exactly one *other* label → the note is that passage's note: it is recorded under that label, it enters `noted`, and it overrides a hyphen the model wrote there. The label the model wrote keeps its line as a mistaken note (not verbatim, not in `noted`), as a paraphrase does today.
- If the set is empty or has several labels (the filler sentences repeat across these documents) → as today: a mistaken note. Nothing is guessed.

`Notes::text` is rebuilt from `lines` in label order only when a relocation (or a second-look flip, part 3) changed something; otherwise the model's own text is kept byte for byte so the writing round's prefix is unchanged. The turn record's disclosure shows the corrected lines.

Fixes the 8 mislabelled notes and the wrong `[1]` citations they produced. Exact text, language-neutral, no threshold — the same rule as the verbatim check, applied to attribution.

**Part 2 — the ordinal prefix (`parse_line`).** `1. [1] -` is read today as label 1 with the note `[1] -`, which counts as text. After the digits and separator, if the remainder opens with a bracketed number, that is the label and the ordinal was list numbering. Hall r3 then parses as 16 hyphens, which is what it was.

**Part 3 — the second look.** After the sweep, while the reading has an output ceiling left and the clock allows:

- `H` = the labels whose line is a hyphen, in **descending** label order.
- One continuation on the cached prefix: `input + assistant(notes so far) + user(SECOND_LOOK)`, tools kept with `ToolChoice::None`, deterministic, thinking off, ceiling = reservation − notes so far, as every sweep.

  > SECOND_LOOK: "Go through the passages you marked with a hyphen once more, from the last to the first: {labels}. Write one line for each in the same form — its label, then the sentences of that passage that bear on my question copied exactly as written, or a hyphen."

- Lines are accepted only for labels in `H`, and a line counts only if it locates (part 1) to a passage in `H`; a line for a label already quoted is ignored, as the sweep ignores labels it did not ask for.
- Repeat while a pass turned at least one hyphen into a verbatim note. `H` shrinks monotonically, so it ends; in practice one pass, two after a flip.
- The flipped lines go into `lines`/`noted`; `text` is rebuilt (part 1 rule); the writing round replays the merged notes as one assistant message, as today.

*Why reversed.* The measured failure is rank-dependent: the later a bearing passage comes in the list, the likelier its hyphen. The second pass starts a new list in the opposite order, so the passages decided last are decided first and no label is decided after the same predecessors twice. This is a decorrelation of the one-token decision, not a magic number: the set is the hyphen set, read once per pass, and the pass count is bounded by progress, exactly as the sweep and the coverage continuation are.

*Why a continuation and not a fresh reading.* A fresh reading with a different instruction changes the tail of the 10k-token user message; on the hybrid model the server's newest checkpoint before that point is the one before the user message, so it would re-prefill the whole prompt (~35 s on the 9B). A continuation appends after the generated notes and prefills only the instruction.

*Why one line per label and not "list only what you now find".* The walk is the mechanism: emitting `[k]` for each label is what makes the model look at each passage. Lines for hyphens are ~5 tokens each.

*What the second look does not do.* It does not say the first reading missed anything. Telling a 9B "you missed one" produces quotes of filler (choir r1 quoted "the treasurer noted that the reserve is healthy" thirteen times unprompted); the wording allows hyphens again, and only verbatim text counts.

*The collapse.* For hall r3, `H` is every label; the second look is a full reverse re-read on the cached prompt. It is the only remedy that does not cost a re-prefill; whether it breaks the numbered-hyphen pattern is a question for the A/B (hall r3's prompt is in it).

**Writing instruction:** see 10.5 (Q3), which supersedes the first draft of this section; it also carries the chronicle precision clause of 10.3.

**What does not change:** the verbatim rule, the coverage check, `withdraw`, the journal shape (still one notes message closed by WRITE), `tool_loop.rs`. `READING_INSTRUCTION` changes only as arm F of the A/B decides (10.5).

**Cost per grounded turn on the 9B** (measured from the logs' `timings`: prefill 304 tok/s, generation 24.9 tok/s):

| | calls | prefill | generation | wall |
|---|---|---|---|---|
| Parts 1–2 | 0 | 0 | 0 | 0 |
| Second look, first pass | +1 | ~60 tokens (instruction; prefix cached) | ~5 × \|H\| + ~40 × flips: 50–200 tokens for 16–36 passages | **+2–8 s** |
| Second pass (only after a flip) | +1 | ~60 | less than the first | +2–6 s |
| Writing round when the notes changed | 0 | the merged notes again, 200–600 tokens, from the checkpoint the server keeps near the prompt's end | — | +1–2 s |

Against today's reading round (+9–23 s measured) that is roughly a third more reading time, and it is paid on every grounded turn, including single-fact ones (29 hyphen lines ≈ 6 s). The writing-round figure assumes the near-prompt-end checkpoint the 2026-10-16 server notes describe; the A/B reads `timings.cache_n`/`prompt_n` on every call to confirm it. If it is not there, the fallback is to append WRITE after the second look's lines instead of merging (cache-perfect; the writer sees the earlier hyphen and the later quote and the WRITE text covers "every passage you noted").

**The honest limit.** A 9B's one-token judgment over a 10k-token prompt misses about 7 % of bearing passages (10/138 outside the collapse), and the rank pattern says most of that is prior, not capacity. If the A/B shows the second look recovers the rank-driven misses but not the stubborn ones (hall [4], conservation `13 weeks` — 0/3), the remaining fix is structural: the **close read**, which re-presents each hyphenated passage next to the decision —

> CLOSE_READ: "Here again are the passages you marked with a hyphen. Read each on its own and write its line in the same form.\n\n[k] <passage text>\n\n[j] <passage text> …"

— as a continuation on the same prefix, re-presenting as many of `H` as the window's room (`room_left` after prompt and notes, less the reservation) allows, in label order, in further requests when they do not all fit (each later request replaces the previous one's user message, which the server rolls back to its checkpoint before the last user message). Cost on the 9B: the hyphenated text again, ≈ 230 tokens × \|H\| ≈ 5–7k tokens ≈ 17–23 s, plus the lines: **+25–35 s per long turn**, and the same for single-fact turns. It is arm A3 of the A/B and is built only if A2 leaves misses that A3 recovers.

### 10.3 The chronicle form miss

Notes: `[5] 1850-11-18. The wheel at Greassby Mill turned again…` (verbatim). Answer: `**1850**: The wheel at Greassby Mill turned again … [5]`. The label is cited, so the coverage check is satisfied; the grounding check accepts "1850" as supported by a passage that says 1850-11-18. What was lost is precision within one claim.

Should the check compare noted text with the answer? No.

- An exact-text comparison (does the answer contain each noted sentence?) would hold the answer to being a concatenation of quotes. The chronicle answer carried the sentence verbatim and lost the date in its own heading; prose answers paraphrase, merge and translate, and the chronicle's other five dates survived the same treatment.
- Anything looser is a similarity threshold or a digit-run match: lexical, language-fragile, and wrong on every legitimate re-form ("£6,497.92" vs "6497.92", "17.5 per cent" vs "17.5 %", "18 November 1850"). It would fire continuations that repeat facts already stated.
- A model judge ("which of your notes does your answer state incompletely?") is one more call per turn by the same 9B that made the slip.

So the coverage check stays what it is: labels found against labels cited, exact and complete. Precision within a claim is the writer's job and the grounding verifier's domain; the WRITE wording in 10.5 asks for the passage's dates, figures and names as given, and the eval measures it (arm W). If precision loss recurs, the place for it is a verifier verdict ("states less than the passage"), not a lexical gate in the coverage check.

### 10.4 The offline A/B

Runs against llama-server on the saved requests, no app build; after the running eval finishes, on the one 9B server. Harness draft: `scripts/replay_reading_ab.py` (stdlib only; same request plumbing as `scripts/replay_answer_round.py`).

**Data.** The 24 saved reading requests (8 long items × r1–r3: 24 *distinct* prompts, since retrieval order differed per run — that is the replication; the reading itself is deterministic, so seeds add nothing to it) plus short6-r1's six saved readings (single-fact items, 16–34 passages) as the regression gate of 10.5 and the precision control: a second look over 29 hyphens must not manufacture quotes that the coverage check then forces into the answer. Arms A2e and F, the withdrawn baseline write and the stop sequences are defined in 10.5.

**Arms** (per prompt, in this order, so each request extends the previous one's cache):

| arm | what | calls per prompt |
|---|---|---|
| A0 | the saved reading request exactly, with `logprobs` on | 1 |
| A1 | A0's output + relocation + the ordinal parse (offline) | 0 |
| A2 | A1 + the reversed second look as a continuation, repeated while it flips | 1–2 |
| A3 | A1 + the close read of the hyphen set, re-presented, same termination | 1–2 (prefill-heavy) |
| writes | for A1, A2, A3: the writing round from each arm's merged notes, app sampling, seeds 1 and 2, with the coverage continuation | 2 each, plus continuations |
| W | A2's notes, the new WRITE wording, seeds 1 and 2 | 2 |

**Measures.**

1. *Replay fidelity*: A0's notes vs the logged notes, per prompt. 24/24 identical confirms the arm is the log; any difference is server nondeterminism and is reported as such.
2. *Decision margin* (A0 logprobs): at each label's decision token — the first token after `[k] ` — the probability of the chosen token and the best alternative, split by fact-bearing hyphen / fact-bearing quote / other. Near-ties at the misses say "prior, re-rollable"; confident hyphens say "judgment, needs the close read".
3. *Reading recall*: fact-bearing passages noted verbatim under the right label, out of 144, per arm. A2 and A3 are supersets of A1 by construction, so the comparison is paired and one-directional: how many of A1's misses each recovers (McNemar on the 144 pairs; with ~16 misses, recovering ≥ 8 is beyond noise).
4. *Reading precision*: verbatim notes on passages holding no rubric fact, per arm (A1 baseline: 13, all choir r1). Every one of these becomes a forced citation.
5. *Cost*: per call `usage.completion_tokens`, `timings.cache_n`, `prompt_n`, `prompt_ms`, `predicted_ms`. `prompt_n` on the writing round is the direct test of the checkpoint assumption in 10.2.
6. *End to end*: rubric pass per (prompt, seed) — 48 answers per arm — facts missing, continuations fired, labels cited that the passage does not support (the mislabel symptom).

**Decision rule.** Ship parts 1–2 regardless (they are corrections, measured in A1). Ship A2 if it recovers the rank-driven misses with no rise in precision failures on long8 or short6. Build A3 only if A2 leaves misses that A3 recovers, and then as the follow-up to A2 on what A2 left, since A3 costs 4–10× more time. Take the W wording if the chronicle-type precision does not get worse on any item.

**Runtime on the 9B** (one slot, cold prefill 11k tokens ≈ 36 s each): A0 ≈ 24 × 50 s = 20 min; A2 looks ≈ 24 × 8 s; A3 close reads ≈ 24 × 30 s = 12 min; writes 3 arms × 48 × ~12 s ≈ 30 min; W ≈ 10 min; short6 adds ~15 min. About 1.5–2 h, in one process so the cache order holds. Never alongside a compile.

### 10.5 Short questions: the reading round made them worse, and what changes

short6 on the merged tree: 2/6, against 11/12 on the depth replay's `full+tools` arm before #174 (five items 2/2, section-after-clause 1/2). Every fact was in the prompt. What the six readings and answers did:

| item | reading (N passages) | notes replayed as | answer | scored |
|---|---|---|---|---|
| paraphrase-clock | `[7] # Runbook: … > Standing provision When the clock source disagrees…` — right sentence, heading prefixed, so nothing verbatim; same for [10] | notes with 0 verified copies | "The provided context contains no information regarding … The only relevant passage found describes … not general machine disagreement:" then the fact, cited [7] | abstention |
| buried-handbook | `[2] …SIG-26321.` clean | 1 verified | "The only information in the provided text regarding tickets … is found in Passage [2]. It states…" | pass |
| buried-agreement | numbered form `1. [1] -`, [3] right, [4]–[17] the same filler sentence, then `18. [18] -` … `89. [89] -` until **max_tokens (4096, finish=length, ~165 s)** | 13 verified (1 fact, 12 filler) | right, with 12 filler citations | pass |
| section-after-clause | `1. [1] -` … `20. [20] -`, then a paragraph: "The answer is not available … do not include Section 24" (it is in [2]) | 0 verified + the abstention | abstained, citing [2]–[20] | fail |
| opening-agreement | `1. -` … `18. -` (no brackets), then "The provided context does not contain the opening text…" ([1] is "Section 1. Scope and application") | 0 verified + the abstention | abstained, arguing against its own [1] | fail |
| transcript-rollback | `1. [1] -` … `17. [17] -`, nothing else | 0 verified | "The provided context does not contain information about…" then the right quote from [3] | abstention |

Three mechanisms, all already visible in the long runs, now dominant:

1. **The numbered form collapses.** Five of six short readings took the form `n. [n] -` or `n. -`, against one of 24 long readings (hall r3). Every numbered reading either collapsed to all hyphens (4) or ran away past the label set (1); no bracket-form reading ever did either (25 of 25). In `1. -` the hyphen is a bullet, not a verdict: the model is writing an empty numbered list. The first token picks the form, the form picks the outcome.
2. **Notes that found nothing steer the writer.** An all-hyphen reading, replayed as the assistant's own message, is a prior the writer inherits: transcript-rollback opens "does not contain" and quotes the fact in the next sentence; the two readings that appended an abstention paragraph were followed by answers that repeat it. The coverage check cannot help — `noted` is empty, so it never fires — and the sweep never fires either, because every label is accounted for.
3. **The writing instruction makes the writer report on passages.** Three of the five non-abstaining short answers open with meta-talk ("The only information in the provided text regarding…", "The only relevant passage found describes…", "The only explicit mention…"); the pre-#174 answers to the same prompts answer in their first sentence. "Your reading notes above say what each passage states about it" and "Leave out a note that was mistaken" ask the writer to adjudicate notes, and the 9B adjudicates out loud — paraphrase-clock's "a specific arbiter, not general machine disagreement" is that hedge.

**Decisions.**

*Q1 — an all-hyphen reading.* **Withdrawn**, and more generally: **a reading with no verified copy is withdrawn** (`read_passages` returns `Ok(None)` when `noted` is empty after relocation, not only when `lines` is empty; the existing `withdraw` path then runs the writer exactly as before #174). The rule looks only at the reading's output. Rationale: such notes carry nothing the writer or the coverage check can use, and replayed they are a prior with no evidence behind it; a genuinely empty retrieval still ends in abstention by the system prompt's rule, as it did before #174, so nothing is lost; and the reading is then neutral at worst on every grounded turn. It covers the hyphen collapse, the heading-prefixed notes (paraphrase-clock) and hall r3 alike. The reading round is not wrong in principle for these questions — buried-handbook shows it working on one — but it must earn its place with a verified copy. **No second look on such a reading**: the second look is the remedy for the rank prior of a working reading, and a continuation of a collapsed one inherits its form; withdrawal is the guaranteed-neutral path. Arm A2e of the A/B measures whether a continuation can break the collapse; if it does, the rule relaxes to "second look, then withdraw if still empty".

*Q2 — trailing text.* **Stripped.** It is not a short-question quirk: 8 of the 30 readings wrote something after their lines, and in 5 of them (apiary ×3, household ×2 — bracket form, working readings) it is a *complete answer* with bullets and citations, and the citations are shifted by the [1] mislabel (`…67 calendar days… [1]`, `…42.4 per cent… [2]`). Replayed as the assistant's own message, that leak is what the writer then cites — apiary's final answers cite [1] in every run. In paraphrase-clock the leak was a direct, correct answer; the writing round then hedged on the same context, which is the strongest evidence for Q3. So the replayed notes are the note lines and nothing else: `Notes::text` is the model's own text when every line of it parses as a note (byte-identical, cache held), otherwise the lines rendered in canonical `[k] …` form. The leak also costs ~150 generated tokens (~6 s) per reading; the F wording's "write nothing after the last line" is the attempt to stop it at the source.

*Q3 — WRITE wording.* **Yes, it invites hedging**, and it changes:

> WRITE: "Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. Cover every passage your notes quote and cite each with its label. Do not describe the notes or the passages, and say nothing about passages that do not bear on my question."

Whether the hedge comes from the wording or from the notes' presence the logs cannot separate; arm W (same notes, new wording) does. `noted_in` keys on the new text; earlier saved research transcripts then read as unnoted and answer without notes, which is the pre-release rule.

*Part 4 — the runaway.* The reading's only output bound is the answer reservation R, and a numbered reading walks past the last label until it hits it: buried-agreement reached `373. [373] -` at 4096 tokens, ~165 s, once in 30 readings (the cut's partial last line was `3`, which `whole_lines` rightly drops; a harness without that guard reads it as label 3's hyphen and loses the one real note). The bound that exists is the label set: generation should stop at the first label that is not shown. `CompletionRequest` gains `stop: Vec<String>` (llama-server and OpenAI-shaped servers take `stop`, Anthropic `stop_sequences`, Ollama `options.stop`; the one addition outside `reading.rs`), and the reading, sweep and second look send `["[N+1]", "\n{N+1}."]` for N the highest label shown. A legitimate note cut by it (a copied sentence containing that bracket) fails verbatim and is asked for again by the sweep; nothing is lost but one line. Optional if the port change is unwelcome; the cost it removes is real.

*The form itself.* The instruction can say what the line looks like without naming anything in any language:

> READING_INSTRUCTION: "Before answering, go through every numbered passage above in order and write one line for each, beginning with its label in brackets exactly as shown above, then the sentences of that passage that bear on my question, copied exactly as written and in the passage's own language. For a passage that says nothing bearing on my question, write its label and a single hyphen. Account for every label, and write nothing after the last line; the answer comes after."

Two changes: "beginning with its label in brackets exactly as shown above" (against the ordinal form) and "write nothing after the last line" (against the leaked paragraph). It changes the cached prefix for every grounded turn equally and is priced by the budget as today. It is arm F of the A/B, run on all 30 prompts, because a wording change can move the long items too. Assistant prefill (`[1] ` as the start of the reply) would force the form outright, but it is server-specific behaviour and is not used.

**Cost on short turns after this section.** Reading as today (+9–23 s on the 9B; the F wording does not change its size). Second look only when at least one copy was verified: on a single-fact turn that is N−1 hyphen lines, +5–8 s. Withdrawal is free. The runaway stop removes up to R tokens of generation.

**short6 as a regression gate.** Per item and seed, the end-to-end answer must be no worse than the depth replay's `full+tools` arm: no abstention where a fact is in the passages, rubric pass on paraphrase-clock, opening-agreement, transcript-rollback, buried-handbook and buried-agreement (2/2 each), section-after-clause ≥ 1/2; no reading with `finish_reason=length`; no replayed notes without a verified copy. The gate runs with every long8 A/B pass.

**A/B additions.** short6-r1's six saved readings join the 24 long prompts (30). New arms: **A2e** — the second look on the five readings with no verified copy (hall r3 and four short6), to measure whether a continuation breaks the collapse; **F** — the readings again with the new READING_INSTRUCTION (cold prefill, ~50 s each), then A1/A2 and writes on those notes, measuring the numbered-form rate, runaways, mislabels and recall against A0. The withdrawn writing round (notes absent, instruction removed from the message, tools callable) is the baseline write for every prompt whose reading has no verified copy. All readings carry the stop sequences; replay fidelity compares parsed notes for shown labels, not raw text. Recommended order: A2, A2e, W first (~1.5 h with writes); F second (~1 h); A3 only if A2 leaves misses.

### 10.6 Residuals, recorded

- Precision: the reading quotes filler that merely resembles the subject (choir r1: 13 lines), and the coverage check then forces 13 citations into the answer. Correct by the design's rule, verbose for the reader. Not addressed here; the A/B's precision count keeps it visible.
- Heading-prefixed notes (paraphrase-clock): nothing verbatim, check inert, answer still right. A one-reading format deviation; no rule change — any looser match is a threshold.
- The collapse (1 of 24): the second look is the only remedy that fits the cache; if the A/B shows it does not break the pattern, the fallback is that the writer answers from the passages, which it did (5/6).
- The near-prompt-end checkpoint is a server fact from the 2026-10-16 notes, not measured on this build; `prompt_n` in the A/B measures it.

### 10.7 Measured A/B outcomes

The A/B ran on the 30 saved prompts: the 24 long readings (8 items, three runs, 144 passages that hold a rubric fact) and the 6 short ones (one fact each). Every answer ("write") was generated on seeds 1 and 2. Results: `evals/retrieval/results/chat/qwen3.5-9b-local/2026-10-24-reading-misses-ab-{pass1,pass2-f,pass3-a3}.jsonl`, produced by `scripts/replay_reading_ab.py`.

Replaying the saved reading request reproduced the logged notes on 29 of the 30 prompts.

| Arm | Facts copied, long | Facts copied, short | Long writes | Short writes |
|---|---|---|---|---|
| A1: parts 1–2, withdrawal, stripping, stop sequences | 128/144 | 2/6 | 27/48 | 10/12 |
| A2: second look | no line recovered | no line recovered | 26/46 | 4/4 |
| W: A1's notes with the new write wording | 128/144 | 2/6 | 29/48 | 10/12 |
| F: new reading instruction | 135/144 | 3/6 | 27/48 | 10/12 |

Writes in the A1, A2 and F rows use the old write wording. In the A2 row the withdrawn readings are left out, which is why its counts are smaller.

- **F** removed the numbered form (5 of 30 readings in A1, 0 in F) and kept hall r3 from being withdrawn (A1 0/6, F 5/6). But it quoted more passages that hold no rubric fact (32 against 25), and it changed no answer.
- **The close read** (arm A3) ran only on the 15 long readings of the five items that missed facts. One of them, hall r3, was withdrawn and not read again. On the other 14, the close read copied every fact: 84/84, against 74/90 for A1 on the same 15 readings. Answers passed 27/28, against 11/30 for A1. These writes used the old wording.

Not measured: the close read together with the new write wording; the close read on readings that missed nothing; and the close read on short questions. The end-to-end eval of the built tree measures all three.

### 10.8 As built (2026-10-24)

What the app does now, in `tool_loop/reading.rs` and its modules `reading/notes.rs` and `reading/close_read.rs`.

Built:

- **Part 1, attribution.** `notes::locate` finds the passages that hold every sentence of a note. `notes::attribute` puts a note copied from exactly one other passage under that passage's label. The notes are rebuilt in label order (`notes::corrected`) only when a note moved or a line was not kept as written; otherwise the model's text is replayed byte for byte.
- **Part 2, the ordinal prefix.** `1. [1] -` is read as label 1 with a hyphen.
- **Q1.** A reading with no verified copy is withdrawn: `read_passages` returns `Ok(None)` and the answer is asked for as it was before reading existed.
- **Q2.** Text after the note lines is stripped: the writing round gets the note lines only.
- **Stop sequences.** `CompletionRequest::stop` is sent by the llama.cpp, Ollama and cloud adapters. The reading, its sweeps and the close read stop at the first label past the last one shown (`[n+1]`, `\nn+1.`).
- **Close read** (`reading/close_read.rs`). It runs after the reading and its sweeps, when the reading is kept and some notes are hyphens. Each request is a continuation on the reading's prompt: the notes as the model's own message, then `CLOSE_READ` with the hyphenated passages as `[k] text`, a blank line apart, in label order. It is sent as the reading is: tools forbidden, deterministic sampling, thinking off, the reading's output reservation and the same stop sequences. The room for passages is the window less the measured prompt with the notes and the instruction in it, less that reservation. Passages are packed into requests that fit, in label order; each later request replaces the one before, so requests do not stack. A passage larger than the room on its own is not shown again. A line counts only when it copies its own passage's text. The close read repeats while a pass takes a line and ends at the first pass that takes none; the hyphen set shrinks with every pass that continues. A provider error ends it with the notes as they are. Stop and the turn's deadline are errors, as in the reading round. The turn record shows it as the step "Reading the marked passages again", whose disclosure holds the notes the writer is given.
- **Write wording (Q3).** `WRITE_INSTRUCTION` is: "Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. Cover every passage your notes quote and cite each with its label. Do not describe the notes or the passages, and say nothing about passages that do not bear on my question."

Not built:

- **The second look (part 3).** Asking again for the hyphen labels without showing the passages recovered no fact.
- **The new reading instruction (arm F).** It copied more facts but did not change the answers. `READING_INSTRUCTION` is unchanged.

Numbers from the offline A/B on the local Qwen3.5-9B (`scripts/replay_reading_ab.py`):

| measure | result |
|---|---|
| Reading facts copied, parts 1–2 (A1) | 130/150 |
| Reading facts copied, new reading instruction (F) | 138/150; answers unchanged: 27/48 long, 10/12 short |
| Second look | 0 facts recovered |
| Close read, on the 15 long readings that missed facts | every reading 6/6; answers 27/28, against 11/30 without |
| Short questions, readings with no verified copy withdrawn | answers 10/12, against 2/6 with the shipped reading |
