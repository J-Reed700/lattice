# Close read: precision diagnosis and fix design (2026-10-24)

Scope: the close read built from §10 of docs/design/2026-10-23-answer-coverage.md (worktree /Users/josh/Code/lattice-coverage, fix/reading-misses, 7503dcc8). Sources: short6-r1 and long8-r1 call logs, the first six items of long8-r2, and the A/B's A3 records (`2026-10-24-reading-misses-ab-pass3-a3.jsonl`). Analysis scripts: `ana/close_calls.py`, `ana/passes.py` beside this file.

## 0. Summary

The close read copies whole passages for one reason and single filler sentences for the same reason: its instruction has no criterion. It says "write its line in the same form", and the form is decided by the first passage in the request and copied down the list. The fix is the reading's own criterion re-said after the passages (C1), one showing per marked passage instead of repeated passes (P1), request room derived from what a reply may copy instead of the answer's reservation (R1), a coverage check that counts a passage as covered when the answer states its note (K1a) and quotes only what the window can hold a restatement of (K1b), and one clause in the write instruction for a sentence several passages state (W1). No counts, thresholds or language inspection anywhere; the one open question (does C1 keep the recall?) is what the offline A/B measures. dp-section-after-clause is a different failure and this fix must not touch it.

## 1. Diagnosis

### 1.1 What the close read did, every reading it ran on

Readings close-read in the end-to-end runs: 16 (short6-r1: paraphrase-clock, handbook-ticket, agreement-meeting; long8-r1: 7, hall withdrawn; long8-r2 so far: 6). Lines the close read took, by kind (F = holds a rubric fact; W = the whole passage body; S = one sentence):

| reading | hyphens shown | taken | facts | false, whole | false, sentence | requests | close-read s |
|---|---|---|---|---|---|---|---|
| paraphrase-clock (short, 1 fact) | 29 | 16 | 0 | **15** | 1 ([22] "…the clock restarts") | 4 | 179 (93 + 71 on the two copying requests) |
| handbook-ticket (short) | 17 | 0 | 0 | 0 | 0 | 9 | 53 |
| agreement-meeting (short) | 3 | 0 | 0 | 0 | 0 | 1 | 4 |
| household, chronicle, lab, family, apiary (long r1, missed nothing) | 26, 19, 30, 29, 29 | 0 | 0 | 0 | 0 | 6, 1, 8, 5, 6 | 34, 19, 34, 34, 33 |
| choir r1 (missed [7]) | 27 | 11 | 1 ([7]) | 0 | **10** ("The treasurer noted that the reserve is healthy but should not be run down." and its decorated variants) | 17 | 93 |
| conservation r1 (missed [2], [6]) | 28 | 1 | 1 ([6]) | 0 | 0 | 6 | 61 |
| long r2: household, chronicle, lab, choir, conservation (missed nothing) | — | 0 | 0 | 0 | 0 | 5, 1, 8, 9, 3 | 35, 23, 46, 42, 30 |
| hall r2 (missed [4]) | 12 | 1 | 1 ([4]) | 0 | 0 | 8 | 65 |

The A/B's A3 arm, re-read from its records (15 readings, 9 with the calls logged): facts recovered 15/15; false notes added **+15**, all in choir r2, the same "reserve is healthy" sentence (A1 false quotes 13 → A3 28). §10.7 reported the recall and not this.

So across 25 close reads with data: whole-passage copying happened in 1 (paraphrase-clock); sentence-level false notes in 3 (choir r1 10, choir r2 15, paraphrase-clock 1); 21 took nothing wrong. When it goes wrong it goes wrong in bulk (10–16 lines), because of the mechanism below.

### 1.2 Why whole passages: the request's first passage sets the form

paraphrase-clock, question "What does the system do when the machines cannot agree on what time it is?", reading: [4] and [9] quoted, 29 hyphens. Close read, one pass in four parts:

| part | first passage shown | reply |
|---|---|---|
| [1 2 3 5 6 7 8 10 11] | [1] "Reading notes: Fabrisse on distributed clock discipline" — opinion prose, no bearing sentence | all 9 copied whole (minus the markdown heading), 1,385 tokens, 93 s; [10] field-tablet runbook and [11] facilities handbook included |
| [12 … 17] | [12] "Runbook: clock skew on ingest … Escalation" | all 6 copied whole, 1,214 tokens, 71 s |
| [18 … 26] | [18] Tessivar agreement, Section 5 | 8 hyphens; [22] one sentence containing "the clock restarts" |
| [27 … 31] | [27] Tessivar agreement, Section 32 | 5 hyphens |

Same prompt, same notes beside every part; the only thing that differs is the passages, and the form of the whole reply follows its first line. A passage whose heading names the question's subject and which has no single sentence to pick gets its whole body as "the sentences that bear"; the next eight lines take that shape. This is the format momentum §10.1 already recorded for the reading round (the numbered-list collapse), now in copy form.

The instruction as built: "Here again are the passages you marked with a hyphen. Read each on its own and write its line in the same form." Three things are missing that the reading instruction has: the criterion ("the sentences of that passage that bear on my question"), the hyphen as an outcome ("for a passage that says nothing bearing on my question, write its label and a single hyphen"), and placement after the text to be judged. "In the same form" points at the shape of the lines above — label then text — and "you marked with a hyphen" frames the hyphen as the thing to revise. The guess in the brief is confirmed; the refinement is that the wording does not make the model copy on its own, it leaves the first passage of each request to decide for all of them.

The sentence-level false notes are the same gap at smaller scale: "the reserve is healthy" is near the subject (a fund), "the clock restarts" has a word of the question; the reading, with the criterion in view, wrote hyphens for every one of them; the close read, without it, flipped them.

Being verbatim, every one of these lines passes `locate` and becomes a note. The notes then carry into the writer's prompt: paraphrase-clock's notes grew from 284 to about 2,800 tokens and the write prompt to 13,589 of 16,384.

### 1.3 The repeated passes bought nothing

The close read loops "while a pass takes a line", re-showing every remaining hyphen passage. On a re-show (a passage shown in an earlier pass and hyphenated again) it took: 0 facts and 3 false notes (choir r1 [8], [12], [20]) in the end-to-end runs; 0 facts in the 9 A/B readings with logged calls (every recovered fact came on the passage's first showing; the second pass took nothing in 13 of 14 readings and more false notes in choir r2). Cost: choir r1 11 of 17 requests (54 s of 93), conservation r1 3 of 6 (27 s of 61), hall r2 4 of 8 (39 s of 65); 81 s of long8-r1's 308 s of close reading.

### 1.4 The requests are small because the answer's reservation is subtracted

`room_for_passages` = window − measured(prompt + notes + instruction) − `max_output_tokens`, and `max_output_tokens` is 4,096 on every one of these requests. With prompts of 11–12k that leaves 500–1,700 tokens of passages per request (parts of 2–7 passages), so a reading of 30 hyphens costs 5–9 requests of ~5 s each. The reply to a close read is lines copied from the passages shown, so its size is bounded by those passages, not by the answer.

### 1.5 Why the answer repeated one sentence three times and ended cut short

Correction to the brief: passages [4] and [22] do **not** hold the same sentence. [22] (People handbook, "Escalation path > Common mistake") holds nothing about clocks disagreeing; its note was the false positive "A request routed through the wrong queue is not refused; it is re-routed, and the clock restarts." The writer, told to "cover every passage your notes quote and cite each with its label", covered [22] by writing [4]'s sentence a second time with [22]'s label — an unsupported claim (verification on the item: 3 supported, 2 unsupported). That is repetition one.

Then the coverage continuation: left out = noted − cited = the 13 whole-passage notes. `continue_instruction` quoted all 13 (12,878 characters, ~2,750 tokens) into a window that had 2,795 free; the request went out with 39 tokens of room (`room == 0` is the only guard), the model began the answer again from the top, was cut at 39 tokens, and the fragment plus the cut-short notice were appended. Repetition two, and the notice. The root is upstream (false notes); the continuation's own defects are that it does not size its quotes to the room and does not ask whether the reply could hold what it asks for.

### 1.6 Citation noise

agreement-meeting: the reading quoted "The parties shall attempt to resolve a dispute at contract-manager level before any escalation." under 14 labels — the agreement repeats it in every clause, and every one of those notes is a true copy of its own passage — so the writer cited all 14. choir r1: 11 citations for the one filler sentence. The first is a real property of documents (boilerplate recurs) and needs a write rule, not a reading rule; the second disappears with the false notes.

## 2. The fix

Everything stays inside `tool_loop/reading.rs`, `reading/close_read.rs`, `reading/notes.rs`; `read_passages`'s contract (`Option<Notes>`) is unchanged.

### 2.1 C1 — the close-read instruction re-says the criterion, after the passages

```
Here again, each on its own, are the passages you marked with a hyphen.

{passages}

Write one line for each of these passages, as before: its label, then the sentences of that passage that bear on my question, copied exactly as written and in the passage's own language — or its label and a single hyphen when it says nothing bearing on my question, however near its subject. Write only these lines.
```

What each part does: "each on its own" keeps the per-passage decision; "as before" ties the line to the reading's lines; the criterion and the hyphen are the reading instruction's own words, so the model is held to the same test it passed in the reading; "however near its subject" names the trap (a heading about clocks, a sentence about a reserve) without naming any word; "Write only these lines" closes it the way READING_INSTRUCTION closes the message. It is placed after the passages so the last thing before generation is the rule, not the first passage's text.

Position variant for the A/B, C2: the same sentences before the passages (the built position), to learn whether placement or wording carries the effect. Only C1 is proposed for the build; C2 ships only if it measures better.

### 2.2 P1 — each marked passage is shown again once

`close_read` makes one pass: the hyphens as they stand after the reading and its sweeps, packed into parts in label order, each part one request beside the notes the reading produced. A line that copies its own passage replaces the hyphen; that is the end. The bound is the label set (each marked label once), as §10 intended for every loop; the "while a pass takes a line" repeat rested on the idea that grown notes change later verdicts, and 1.3 refutes it. Remove the `loop`, keep `taken`/`as_kept`/`tokens` for the one update.

### 2.3 R1 — request room from what a reply may copy

A close-read reply can hold at most the lines for the labels shown and copies of their passages. So for a request:

- `free` = window − measured(prompt + notes + instruction with no passages) — `room_left` of the bare request, no reservation subtracted;
- a part takes passages in label order while `tokens(part) + tokens(part) + lines(part) ≤ free`, where `lines(part)` is the measured size of `[label] -\n` for its labels (a label-set quantity; measure it once per label alongside `sizes`);
- the request's `max_output_tokens` = `free − tokens(part)`, capped by the reading's reservation as `within` caps everything else.

A passage that cannot fit with its own copy is not shown again (as now). Effect on these prompts: 2–3 parts instead of 5–9, and a copy-mode reply (should one still happen) fits its request instead of being cut. The cost that remains is the re-processing of the hyphen passages themselves (~5–6k prompt tokens on a 32-passage reading, ~10 s idle, ~20 s on the loaded machine); nothing can take that away without choosing which passages to re-show, and any such choice is a criterion the app may not apply.

### 2.4 K1 — the coverage continuation

(a) **Covered means stated.** `left_out(noted, cited)` becomes `left_out(notes, answer)`: a noted passage is covered when the answer cites its label, **or** when the answer's text (citation marks removed, whitespace normalized — the same normalization `locate` uses) contains every sentence of its note. Exact text, no threshold, any language. A passage whose noted sentence the answer already states under another label is therefore covered; it is not asked for again. This is what the brief asks about: the answer has the passage's content, and which of several identical passages carries the citation is not a coverage gap. The same test makes 14 boilerplate notes covered by one written sentence.

(b) **Quote what the reply can restate.** `continue_instruction` quotes, in label order, the longest prefix of the left-out notes for which `tokens(instruction with those quotes)` fits the room **and** leaves at least `tokens(quotes)` of room after it — the model is asked to write those passages' content, so the reply must have room for at least the quotes themselves. If not even the first fits, the continuation is skipped with the answer as it stands. The loop otherwise stays as built (ends when a continuation adds nothing; the set shrinks); notes that did not fit are asked for next time round if room remains. This replaces `room == 0`, which let a 39-token request through.

### 2.5 W1 — the write instruction, one clause

```
Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. Cover every passage your notes quote and cite each with its label; a sentence that several of your notes quote, write once, citing one of them. Do not describe the notes or the passages, and say nothing about passages that do not bear on my question.
```

With K1(a) the other labels count as covered. "Citing one of them" rather than all: a citation is where the reader verifies the sentence, and one place suffices; fourteen chips for one sentence is noise in the margin and in the grounding pass.

### 2.6 Things considered and not proposed

- **Telling a whole copy of a non-bearing passage from a bearing one.** There is no deterministic test. A copy that equals the whole passage is legitimate for a one-sentence passage ([4] here is exactly that: heading plus one sentence), and "more than n sentences" is a threshold. The app verifies only that a line is copied from its passage; whether it bears is the model's judgment, and the fix gives the model the criterion (C1) and makes a wrong judgment survivable (R1, K1). Do not add a detector.
- **Restating the question in the close read.** The criterion says "my question" and the question stands a few thousand tokens back; the reading round judged these same passages correctly with the criterion in view, so the criterion is the missing piece, not the question. Repeating the question would also copy pasted material back into the prompt. Hold in reserve as an arm only if C1/C2 both lose recall.
- **Re-showing only passages from documents the reading quoted from.** A label-set bound, but it assumes the bearing passages share a document with the ones found — true of the synthetic long set by construction and not of real collections. Rejected.
- **Skipping the close read on "short" questions.** Nothing may inspect the question; the close read's cost on a one-fact question is bounded by R1 and P1 and nothing else.
- **A reservation for the close read's reply other than the copy bound.** The reading's 4,096 is the answer's; the close read's reply is bounded by what it is shown.

## 3. The offline A/B

Replay, no app build. One scratch copy of `scripts/replay_reading_ab.py` (do not edit the committed one until the result is in).

### 3.1 Prompts

- The 30 saved readings the A/B used (long8 r1/r2/r3 × 8 items + short6), loaded as the script loads them.
- The new calls: `short6-r1.jsonl.calls.jsonl`, `long8-r1.jsonl.calls.jsonl`, and `long8-r2/r3` when they finish. The loader needs two changes: the saved write request is recognised by the new ending (`"do not bear on my question."`) as well as the old, and the run name comes from the file stem as now (`short6-r1`, `long8-r1`), which collides with the old A/B's keys — prefix the new ones (`e2e-short6-r1`).
- A reading is close-read only when, after parts 1–2 (attribution, ordinal parse), it has a verified copy — exactly the built rule. Expected: ~29 of the old 30, 16 of the new 17 (hall r1 withdrawn). Every one runs the close read, not only those that missed facts: the precision cost lives in the readings that missed nothing.

### 3.2 Arms

Stage 1, the close read (reading metrics only, no writes):

| arm | instruction | passes | room | status |
|---|---|---|---|---|
| C0R0 | as built | repeat | reservation subtracted | the e2e logs already hold it for the 16 new readings and the A3 records for 15 old ones; run it only on the 14 old readings A3 skipped (full recall) so every arm covers every prompt |
| C1R1 | §2.1, after the passages | one | copy bound | proposed |
| C2R1 | §2.1 sentences before the passages | one | copy bound | position control |
| C1R0 | §2.1 after | one | reservation subtracted | run only if C1R1 loses recall against C0R0, to separate wording from room |

Pass P1 for the new arms is "one pass" in the loop; record whether pass 0 took a line, so the cost a repeat would have added is known without paying it.

Stage 2, the write (on the notes of the arm Stage 1 picks; seeds 1 and 2 as before):

| arm | write wording | coverage | quotes |
|---|---|---|---|
| W0K0 | WRITE as built | cited labels | all, `room == 0` guard |
| W1K1 | §2.5 | cited or stated (§2.4a) | packed to the room with the restatement bound (§2.4b) |

### 3.3 Measures

Per reading and arm: facts copied after the close read (`recall`/`of`); non-fact lines accepted (`false_quotes` after − before the close read), split whole / sentence (whole = the note equals the passage's normalized text or the text after its first heading line — a report split, not a decision rule); requests, new prompt tokens, output tokens, seconds; lines taken per part by position in the part (first vs later), to see whether C1 breaks the momentum. Per write and arm: rubric pass (`missing` empty and no abstention); answer characters; citations, distinct citations, citations of non-fact passages; repeated sentences (a sentence of >30 characters, citations stripped, appearing twice); continuations sent, their prompt tokens and room, and whether any finished `length` (the cut-short case); seconds.

Totals reported on the same prompt set for every arm, with the subsets "missed facts" and "missed nothing" shown separately, and short6 separately from the long items.

### 3.4 Decision rule (lexicographic, no thresholds)

Stage 1: an arm qualifies if it copies, on the readings that missed facts, every fact C0R0 copies (the recall the close read bought: 15/15 in the A/B, 3/3 in the e2e). Among qualifying arms, take the one with the fewest non-fact lines accepted over all readings; ties go to fewer seconds. If no arm qualifies, take the one that copies the most facts, and report the recall lost beside the precision gained — that is Josh's call, not the rule's. If C1R1 does not qualify but C1R0 does, the room change cost recall and ships separately.

Stage 2: W1K1 ships if it passes at least as many writes as W0K0 and has no more answers with a repeated sentence and none with a `length`-finished continuation. Citation counts and length are reported, not decided on.

### 3.5 Prototype pieces for the scratch copy

Replace `close_read` and add three helpers; nothing else in the script's flow changes except the loader's write-ending check and the arm wiring.

```python
CLOSE_C1 = ("Here again, each on its own, are the passages you marked with a hyphen.\n\n{passages}\n\n"
            "Write one line for each of these passages, as before: its label, then the sentences of that passage "
            "that bear on my question, copied exactly as written and in the passage's own language — or its label and "
            "a single hyphen when it says nothing bearing on my question, however near its subject. Write only these lines.")
CLOSE_C2 = ("Here again, each on its own, are the passages you marked with a hyphen. Write one line for each, as before: "
            "its label, then the sentences of that passage that bear on my question, copied exactly as written and in the "
            "passage's own language — or its label and a single hyphen when it says nothing bearing on my question, "
            "however near its subject. Write only these lines.\n\n{passages}")
WRITE_W1 = ("Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. "
            "Cover every passage your notes quote and cite each with its label; a sentence that several of your notes quote, "
            "write once, citing one of them. Do not describe the notes or the passages, and say nothing about passages that "
            "do not bear on my question.")

def close_read_once(read_req, lines, text, texts, log, prompt_tokens, instruction, copy_bound, reserve):
    """One showing per hyphen passage. copy_bound: a part fits when 2*passages + its label lines <= free;
    otherwise the built rule (free - reserve). Returns lines, text, taken-by-part for the position measure."""
    hyph = sorted(l for l, n in lines.items() if n is None)
    free = n_ctx() - prompt_tokens - tokens(text) - tokens(instruction.format(passages=""))
    room = free if copy_bound else free - reserve
    parts, cur, used = [], [], 0
    for l in hyph:
        t = tokens(f"[{l}] {texts[l][1]}\n\n")
        cost = 2 * t + tokens(f"[{l}] -\n") if copy_bound else t
        if cost > room:
            continue
        if used + cost > room:
            parts.append(cur); cur, used = [], 0
        cur.append(l); used += cost
    if cur:
        parts.append(cur)
    taken = []
    for part in parts:
        block = "\n\n".join(f"[{l}] {texts[l][1]}" for l in part)
        shown_tokens = tokens(block + "\n\n")
        max_out = min(reserve, free - shown_tokens) if copy_bound else reserve
        r = call(read_req, stop=stops_for(texts), max_tokens=max_out, messages=read_req["messages"] + [
            {"role": "assistant", "content": text},
            {"role": "user", "content": instruction.format(passages=block)}])
        log.append(("close_read", r))
        got = parse_notes(whole_lines(r["content"], r["finish"]))
        for i, l in enumerate(part):
            n = got.get(l)
            if n and l in locate(n, texts):
                lines[l] = n
                taken.append({"label": l, "position": i, "part": len(part), "whole": norm(n) == texts[l][1] or norm(n) == norm(texts[l][1].split("\n", 1)[-1])})
    return lines, (render(lines) if taken else text), taken

def covered(answer, label, note):
    """K1a: cited, or every sentence of the note is stated in the answer."""
    if label in cited(answer):
        return True
    body = norm(re.sub(r"\[\d+(?:\s*,\s*\d+)*\]", "", answer))
    sents = [norm(s).strip('"“”„«»') for s in sentences(note)]
    return all(s and s in body for s in sents)

def packed_quotes(left, lines, room_after):
    """K1b: the longest label-ordered prefix of left whose instruction fits the room and leaves room for the quotes."""
    out = []
    for l in left:
        q = out + [f'[{l}] "{lines[l]}"']
        instr = CONTINUE.format(quotes=" ".join(q))
        if room_after(instr) < tokens(" ".join(q)):
            break
        out = q
    return out
```

`write_round` for W1K1: compute `left = [l for l in noted if not covered(answer, l, lines[l])]`, quote `packed_quotes(left, lines, room_after)` where `room_after(instr)` measures the request with that instruction appended and returns `n_ctx() − prompt_tokens`; skip the continuation when the pack is empty; loop as the script already does. The loader: `last.endswith("Leave out a note that was mistaken.") or last.endswith("do not bear on my question.")`.

Running order on one slot: C1R1 then C2R1 over all prompts (~45 readings × ~3 requests × ~5 s ≈ 15 min each when the machine is idle; more under load), then C0R0 on the 14 skipped readings, then Stage 2. Nothing in this document was run; the 9B is busy and must not be started a second time.

## 4. dp-section-after-clause

Leave it alone, and make sure the fix does not reach it:

- Its reading wrote 25 hyphens and a prose paragraph; `attribute` found no verified copy; the reading was withdrawn under 10.5 Q1 and the close read never ran. That path (no verified copy → withdrawn → the old request) is what took short questions from 2/6 to 10/12 and must stay.
- The failure is the model's, on a structural question: it looked at [2], wrote that Section 24 "ends at 24.111" and answered from Brackleton's 24.111, although "The Reackgate step obliges the parties to hold an escalation meeting within 31 weeks…" stands in [2] as the paragraph after 24.111. The reading's criterion ("sentences that bear on my question") is a poor fit for "what does the paragraph after clause X say" — nothing in the bearing sentence echoes the question — and the item failed before #174 too. Verification marks the answer 1 contradicted, 3 unsupported; that is a grounding matter, not a reading one.
- Running the close read on withdrawn readings would re-show every passage of every all-hyphen reading (a second full reading, ~10k tokens in parts) for an uncertain gain; not part of this fix. If someone wants to know, two probe calls in the replay (C1 on [2] alone; C1 on the part that holds [2]) answer it as a diagnostic, not an arm.
- Not touched either: transcript-rollback's 3 unsupported claims (reading withdrawn, old path).

## 5. Build notes

- `close_read.rs`: `CLOSE_READ_INSTRUCTION` → C1 text; `close_read` loses its `loop` (one pass; keep the `step`, `taken`, `as_kept`, `tokens` update and the failure handling); `room_for_passages` drops the reservation and returns `free`; `pack` takes `(label, passage_tokens, line_tokens)` and the copy-bound test; `ask` sets `max_output_tokens = within(reservation, free − shown)`. `measured` gains the `[label] -\n` size per label.
- `reading.rs`: `WRITE_INSTRUCTION` → W1; `left_out(notes, answer)` with the stated test (reuse `notes::sentences` and `normalize_whitespace`; strip citation marks with the same form `cited_labels` reads); `quotes` → the packed prefix with the restatement bound, measured through `room_left`; `add_what_was_left_out` skips when the pack is empty and logs it.
- `noted_in` matches on `WRITE_INSTRUCTION`, so saved research turns written under the old wording stop matching; pre-release, no shim (memory: no legacy support).
- Tests to add in `reading/tests.rs`: one pass only (a second showing is never requested); copy-bound packing (a part's passages + their copy + lines ≤ free; a passage that cannot fit with its copy is left out); `left_out` covered-by-statement (identical boilerplate under 14 labels, one cited → none left; a paraphrase → still left); packed quotes stop before the room is exhausted and skip when none fits; the C1/W1 texts in the instruction-shape tests.
- Design doc: add §10.9 with 1.1–1.6 and the A/B result; correct §10.7's omission (the close read's precision was not measured) and the brief's [4]/[22] claim.
