# Citations that say how: one anchor, one relation, one card

Date: 2026-11-26. Status: design with measurements, nothing shipped. Code read at `origin/main` `7ada090e3`, with #654 (`498c96bed`, cite only what a check named) assumed merged first. Data read read-only from the test app's database: one prepared course of 19 lessons, 419 attribution findings. Replays ran the course's findings through both judge forms on `claude-haiku-5-5`, on the local `Qwen3.5-9B-Q4_K_M` and on `Qwen3.8-27B-Q5_K_S`; each row names the model its server reported. The replay scripts are `evals/grounding/claim-classes/replay_anchors.py` and `compare_anchors.py`.

## 0. The design in one paragraph

A citation chip today says only that a sentence has a source. It cannot say how the sentence uses that source, and it rarely shows where. The lesson stores the passage the judge was shown as the supporting quote, so the reader marks a whole 3.4 KB chunk, and every web citation's location reads "Location unknown". The part-wise judge writes each part in its own words and ties it to a whole passage, so a "stated" part can be the judge's rewording, which is how a general sentence gets credited with a specific the lesson never wrote. This design makes the judge point instead. Each part is the claim's own words, copied and checked by code, and it names the exact source sentences that state it or that it follows from. From those pointers, code derives a relation for the sentence: quoted, paraphrased, summarized or inferred. It also derives byte anchors in the saved source, so the reader marks the sentences and nothing else. One relation vocabulary, one anchor type and one card serve every surface that shows a citation. Lessons come first, then chat and Explorer, then the rest.

## 1. What is there now

### 1.1 How a lesson's chip is made

At read time, `content_verification/cited_text.rs` places a chip at the end of each statement's lesson text (`citation_point`). `stands_on` decides which passages the chip opens:

- **Common:** none.
- **Reasoning:** every non-run passage.
- **Sourced:** the passages of its stated parts, else the passage holding its supporting quote.

`number` numbers the passages down the lesson, and `pieces` cuts each section body into `ReplyPieceDto::Text` and `ReplyPieceDto::Reference`, the pieces chat answers use.

What a chip carries is a passage number and nothing else:

- `ReplyReferenceDto { written, kind, number, lines, name }`.
- `CheckedPart { part, kind, source_id }` records which source version a part rests on, not where in it. Two passages of one version cannot be told apart.
- The supporting quote of a part-wise finding is `texts.get(at)`, the whole labelled passage the judge was shown (`evidence_checks/bridge.rs`, `cited`).

### 1.2 What the reader does with it

- `fromLearningEvidencePassage` puts the passage text in the reader's `content` and `locator`. A web passage's saved page goes in `webSnapshot`, trimmed.
- `WebArticleView` then guesses which sentences are cited. It uses `useCitedSentences`, then `findQuoteSpan` (12 or more folded characters), then `findSourcedPassages`, a lexical score of 0.5 or more over 4 or more shared tokens.
- `CitationRail` prints `formatSourceLocation(source) ?? 'Location unknown'`. Only the PDF viewer reports a location, so every web citation reads "Location unknown".

### 1.3 Every other surface decides alone

Outside lessons and chat, the frontend numbers citations itself, by position. Examples:
- the course outline's evidence: `indexOf(citation) + 1` in `OutlineEvidence`
- saved-source citations: `index + 1` in `SavedSourceCitations` and `useSavedSourceReader`
- journal insights: a map of its own in `InsightMessage`

There are two quote rules:

- lessons: `generation.rs`, `MIN_QUOTE_CHARS = 25` and `MIN_QUOTE_WORDS = 5`
- chat verification: `quotes.rs`, its own `MIN_QUOTE_WORDS = 5`

No surface says how a sentence uses its source.

### 1.4 Measured

- **Passage size:** a lesson passage is 3,361 bytes at the median (p90 3,549). That is what a chip highlights today.
- **One lesson's 24 chips, read by hand:**
  - 19 were right.
  - 3 were stretched: the judge's rewritten part credited a passage with a specific the lesson sentence does not state.
  - 2 opened a passage the check had rejected. #654 removes this class.
- **How lessons use sources:** of 356 cited lesson sentences, 280 share under 30% of their words with a passage they cite, and 5 share 90% or more. Lessons paraphrase and summarize; they rarely quote. A chip that cannot say which is hiding the common case.

## 2. The model

### 2.1 Anchor

```rust
pub struct Anchor {
    pub source_version_id: String,
    pub start_byte: usize,
    pub end_byte: usize,
}
```

An anchor is a span of one source version's saved text, in UTF-8 bytes.

**Why bytes:**
- Every lesson passage already is `full_text[start_byte..end_byte]` of its version. That held for 603 of 603 passages in the course.
- Rust slices `str` by byte.
- A byte offset means the same thing in every language that reads the text.

The frontend converts to UTF-16 once (§6.2).

On the wire an anchor travels with `exact`, the words at the span. This follows the W3C Web Annotation model, which pairs a `TextPositionSelector` with a `TextQuoteSelector`. A reader holding a different text detects the mismatch instead of marking the wrong words.

An anchor covers whole sentences of its passage, as `flowed_sentences` cuts it. The one exception is a quotation, which covers exactly the quoted words.

### 2.2 Relation

How a sentence uses a source, strongest first:

| Relation | Card label | Meaning | Comes from |
|---|---|---|---|
| `quotes` | Quoted from | The sentence is the source's own words | Code finds the lesson text in a checked passage (`same_text::quotation`), or the provenance gate settled the statement's quotation (label `attributed`) |
| `restates` | Paraphrased from | One source sentence says all of it, in other words | One anchor is named by every stated part |
| `summarizes` | Summarized from | Several source sentences together say it | The stated parts rest on different sentences |
| `infers` | Inferred from | The source's sentences entail it; none says it | A part the judge marked `follows` |
| `agrees` | Consistent with | The lesson's own reasoning, checked for consistency with the source | Reasoning class |
| `backs` | Supported by | Checked and supported, but the check named no sentence | A finding from before this design; a strict-judge finding |
| `cites` | Cited (not checked) | The writer cited it; nothing checked the sentence against it | Unverified answers, notes, cards and compare tables (§7) |
| `contradicts` | Contradicted by | The source says otherwise | The review screen only |

The first four are the distinctions academic writing teaches: quotation, paraphrase, summary and inference. The rest keep the card honest where no such distinction was checked. A Bluebook signal works the same way: no signal for direct support, *See* for support by inference, *Contra* for contradiction. CiTO names the relations a citation can carry (`cito:agreesWith`, `cito:usesConclusionsFrom`, `cito:disagreesWith`). We use only the ones a check established.

### 2.3 The part rules

The relation of a statement the part-wise judge read is a pure function of its parts. It lives in `lattice_core::domain::citation` so every caller derives it the same way:

```rust
pub enum Grounds { Stated, Follows }

pub fn relation_of(parts: &[(Grounds, &[Anchor])]) -> Option<(Relation, Vec<Anchor>)>
```

First match wins:

1. **No part has an anchor:** none.
2. **Any `Follows` part with an anchor:** `Infers`. Anchors: every anchor of every part, in source order, each once. A statement is only as strongly grounded as its weakest grounded part.
3. **One anchor that every `Stated` part with anchors names:** `Restates`. Anchors: those common anchors, in source order.
4. **Otherwise:** `Summarizes`. Anchors: every anchor of every part, in source order, each once.

**Why rule 3 is a cover rule and not a count:**
- A count rule calls a statement a summary when its parts name more than one distinct sentence.
- A judge lists every sentence that mentions a part's name, so most extra sentences are repetition, not synthesis.
- On the Haiku replay the count rule called 152 of 263 statements summaries. The cover rule called 71.
- Read by hand, the cover rule's restatements are restatements: the sentence every part names is the one that says the whole thing.

### 2.4 A lesson statement's citation

`lattice-learning`'s `content_verification/support.rs` replaces `stands_on` with one function. It is the only place a lesson statement's citation is decided:

```rust
pub(super) struct Cited<'f> {
    pub(super) relation: Relation,
    /// The statement's own lesson text is drawn as a quotation.
    pub(super) quoted: bool,
    /// The passages the chips open, in the order the anchors come, each with its
    /// anchors; an empty list opens the passage whole.
    pub(super) on: Vec<(&'f EvidencePassage, Vec<Anchor>)>,
}

pub(super) fn cited(finding: &Finding) -> Option<Cited<'_>>
```

First match wins:

1. **Not supported, or class `Common`:** none. An unsupported statement publishes as not verified and gets no chip; only the review screen shows a contradiction.
2. **Class `Reasoning`:** `Agrees`, on every non-run passage, whole.
3. **The lesson text is a quotation of a non-run evidence passage:** `Quotes`, `quoted: true`. The lesson text is the finding's `quote`, and `same_text::quotation` decides (§2.5). Anchor: the span found.
4. **Label `Attributed`:** `Quotes`, `quoted: false`, because the lesson already marks its quotation. Anchors: the quotation parts' anchors when the finding records them (§3.4). Otherwise, for a finding from before this design, the supporting quote's span in the passage that holds it.
5. **Parts with anchors:** `relation_of` over the `Stated` and `Follows` parts. Each anchor is grouped under the evidence passage that contains it.
6. **A supporting quote found in a non-run passage, shorter than that passage:** `Backs`. Anchors: the whole sentences over the quote (`same_text::sentences_over`).
7. **Stated parts naming sources, or a supporting quote equal to its whole passage:** `Backs`, on those passages, whole. This is #654's rule, kept for findings from before this design.
8. **Otherwise:** none. A statement never falls back to the first passage it was compared with.

### 2.5 One quote rule

`lattice_core::shared::text::same_text::quotation(text, quote) -> Option<(usize, usize)>` returns where `text` holds `quote`. The quote must be long enough to stand as a quotation, at least 25 characters and 5 words, and found word for word apart from spacing, typography, emphasis marks and letter case.

It is the one rule for three callers:
- the generator's own quotations (`validate_excerpt_quote`)
- the provenance gate's settled quotations (`quote_claims`)
- rule 3

`MIN_QUOTE_CHARS` and `MIN_QUOTE_WORDS` move with it. Chat's copy in `verification/quotes.rs` folds into it in phase 2.

### 2.6 Invariants

1. **Inside the checked passages.** Every anchor lies inside a non-run passage the statement was checked against.
2. **Code finds every anchor.** The judge names sentences by handle, and the handle resolves to a span `flowed_sentences` cut. A quotation span is found by `same_text`.
3. **The model's words are the claim's.** Code refuses a part whose words are not in the claim, so a stated part cannot carry the judge's own specifics.
4. **Facts are stored; the relation is derived at read time.** A finding stores its parts, their kinds and their anchors; `cited` runs when the lesson is read. Changing a rule never needs a re-check.
5. **Chip if and only if `cited` is some.** A chip's number, relation and anchors come from one call.
6. **Anchors or nothing.** The reader marks an anchored citation only where the anchor's words are. It never guesses lexically (§6.3).

## 3. The judge, `lesson-evidence` v23

### 3.1 What it is shown

Each passage is shown as its label line (title, site, date), then its text with a handle before each sentence: `[s1] … [s2] …`. Numbering runs on across passages, so a handle names one sentence of one passage. Sentences are cut by `flowed_sentences`, which keeps a hard-wrapped paragraph's lines together.

The handles are a `Kind` with prefix `s`, minted by the request's `Showing`. Their reference is `SentenceAt { passage, start, end }`, byte offsets in that passage's text. Passages are no longer handles.

### 3.2 What it answers

```json
{"parts": [{"words": "<the claim's words for the part>", "kind": "<kind>", "from": ["s3", "s7"]}], "reason": "<one sentence>"}
```

The kinds are `stated`, `follows`, `common`, `own`, `specific_missing` and `contradicted`.

- `follows` is new: no passage says the part, but what the passages say directly entails it.
- `from` is required in the schema for every part and lists sentence handles.
- Code reads `from` for `stated`, `follows` and `contradicted`, and ignores it for the other kinds.

The prompt is the replay's prompt of record: `BRIDGE_SYSTEM` in `replay_anchors.py`, with the own-figures sentence taken from `own_figures!()`.

### 3.3 What code refuses, with one correction each

- **No parts:** "Name at least one part of the claim in parts."
- **A part whose `words` are not in the claim** (`same_text::find` over the claim as shown): "The words \"…\" are not in the claim. Give each part as the words of the claim that say it, copied exactly."
- **A `stated`, `follows` or `contradicted` part whose `from` names no shown sentence:** "The part \"…\" is marked stated but names no sentence. List in from the id of every sentence that says it, or give the part the kind it has." For `follows` the sentence says "that it follows from"; for `contradicted`, "that contradicts it".

**What the parts add up to:**
- A contradicted part makes the claim contradicted.
- Otherwise a `specific_missing` part makes it unsupported.
- Otherwise a `stated` or `follows` part makes it supported, and with neither it is all common.
- `guarded` still turns a common part that names a specific into `specific_missing`. A `follows` part is not guarded: the sentences it names are its grounds.

**Answer room:** the part-wise answer takes `ANSWER_ROOM_TOKENS`, the room every durable check keeps (§8.3), and `PARTS_ANSWER_TOKENS` goes.

### 3.4 What a finding stores

`CheckedPart` gains `anchors: Vec<Anchor>`. The lesson checker turns each `SentenceAt` into an anchor by adding the passage's `start_byte`. Each `stated` or `follows` part records its anchors, as does each `contradicted` part, for the review screen.

`quote_claims::settle` records one stated part per quotation part, with the quoted words and the anchor of the span it found. `source_id` stays readable for findings from before v23, and new findings leave it empty. The supporting quote keeps its meaning: the provenance gate, the pass logic and the report read it.

`POLICY` becomes `lesson-evidence` v23. A check's receipt key includes the policy, so no v22 judgment is reused under v23. A v22 report stays readable through rules 4, 6 and 7 of §2.4. A lesson still being prepared is checked again at publication under v23. Edits are unaffected.

## 4. Read time

`learner_view` calls `cited` once per finding. `number` numbers the passages `cited` puts the statement on, in reading order, as it does now. Each chip carries the statement's relation and the anchors that fall in its own passage. A statement on two passages gets two chips, each with its own sentences; 8 of the 394 supported statements on the Haiku replay rest on two sources.

When `quoted` is true and the chip is placed, the statement's lesson text becomes a `Quoted` piece, followed by its chip.

## 5. Contracts

In `application/contracts/replies.rs`:

```rust
#[serde(rename_all = "snake_case")]
pub enum RelationDto { Quotes, Restates, Summarizes, Infers, Agrees, Backs, Cites, Contradicts }

#[serde(rename_all = "camelCase")]
pub struct ByteSpanDto { pub start: usize, pub end: usize }

#[serde(rename_all = "camelCase")]
pub struct AnchorDto {
    pub exact: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<ByteSpanDto>,
}

#[serde(rename_all = "camelCase")]
pub struct SupportDto { pub relation: RelationDto, pub anchors: Vec<AnchorDto> }

pub struct ReplyReferenceDto {
    /* written, kind, number, lines, name as now */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<SupportDto>,
}

pub enum ReplyPieceDto {
    Text { text: String },
    Reference { reference: ReplyReferenceDto },
    Quoted { text: String },
}
```

In `lesson_evidence.rs`:

- `LearningClaimPartKindDto::Follows`.
- `LearningClaimPartDto.anchors: Vec<AnchorDto>`, the sentences a part rests on, for the review screen.
- `LearningClaimEvidenceDto.relation: Option<RelationDto>`, none when the statement cites nothing.

`span` is optional for a caller that knows only the words, such as a chat answer's evidence quote in a chunk whose offset in its document is unknown. Every lesson anchor has one.

## 6. The frontend

### 6.1 One module

`src/features/reading/citations/` holds everything a citation looks like, for every surface:

- **`relation.ts`:** the only place relation words live: the card label and the phrase the accessible name uses ("Source 2, paraphrased from The Quillet Guide").
- **`anchors.ts`:** `utf16Span` and `placeAnchors` (§6.2).
- **`occurrences.ts`:** the anchored citation of each chip drawn from one reply's pieces, keyed as the chip's click is keyed.
- **`headingTrail.ts`:** the headings above an offset of an article, outermost first, from `articleBlocks`' heading levels.
- **`CitationCard.tsx`:** the one card. Top to bottom:
  1. the relation label in small capitals
  2. the source's title and site
  3. the anchored sentences, with an ellipsis between sentences that are not adjacent, or the passage clamped when there are no anchors
  4. the heading trail
  5. "Open source"

  A quotation's sentences are set in quotation marks. `CitationHoverCard` becomes this card.

### 6.2 Placing an anchor

`placeAnchors(text, anchors, base)` reads `text`, which starts at byte `base` of the saved version. Per anchor, in order:

1. Convert `span` to UTF-16 offsets. If the slice there equals `exact`, mark it.
2. Else mark the copy of `exact` nearest the expected offset.
3. Else mark nothing. The reader says: "The cited sentence is not in the saved text of this source."

The reader's article text must be the saved text exactly, so the trims of the snapshot text in `sourceAdapter.ts` go: `fromLearningEvidencePassage` and `fromLearningSourceVersion`.

### 6.3 The reader

`SourceWithMetadata.anchoredBy?: AnchoredOccurrence[]`, where `AnchoredOccurrence` is `{ occurrence, relation, anchors }`. When a source carries it:

- **What `WebArticleView` marks:** the placed anchors, the clicked occurrence's as the active mark. It does not call `useCitedSentences`, `findQuoteSpan` or `findSourcedPassages`. An occurrence with no anchors marks its passage as today.
- **`CitationRail`:** shows the occurrence's anchored sentences as its excerpt and the heading trail as its location. It steps over the lesson's cited occurrences only. "Location unknown" goes: with no trail the line is left out.

Everything without `anchoredBy` reads as it does now, until its phase.

### 6.4 Quoted text

A `Quoted` piece is written into the marked text between `` and ``, the next two private-use characters after the reference marks. `ANY_MARK` widens to `[-]`, so `withoutMarks` drops them from selections and copies. `citationMarks` draws the pair as opening and closing quotation-mark widgets with class `cite-quote`. `writtenOf` gives the text in curly quotes, so copy and export keep the quotation.

## 7. Phases

1. **Lessons and the shared core (this PR).**
   - the judge (§3)
   - storage (§3.4)
   - `cited` and the chips (§2.4, §4)
   - the contracts (§5)
   - the module and the reader (§6)
   - the lesson surfaces: `CitedSection`, `CitationSourcesRow`, `LessonEvidencePanel`, `LessonClaimHoverCard` (a `follows` part reads "follows from the source"), and the review screen's `ClaimEvidence` and `ReviewSourceReader`
2. **Chat, Explorer, journal insights and references.**
   - A verified sentence whose check matched a quote, or whose method was verbatim, `quotes`.
   - One the verifier supported `backs`, with its evidence quote as an anchor (`span` none unless the chunk's offset is known).
   - Every other cited sentence `cites`.
   - The pointing judge comes to chat only after a chat eval shows it holds there.
   - Chat's quote rule folds into `same_text::quotation`.
3. **Saved-source citations, the course outline, flashcards, journal notes and compare.**
   - A saved-source citation whose words are found `quotes`.
   - Notes, cards and compare tables `cite`.
   - Every surface takes its numbers from the backend, and the frontend's own numbering goes.

## 8. Measurement

### 8.1 The two judge forms on the same findings

419 findings the saved run had judged supported, replayed through the current form and the pointing form. All arms ran at deterministic sampling.

| Model | Form | Supported | Unsupported | Contradicted | All common | Corrections |
|---|---|---|---|---|---|---|
| claude-haiku-5-5 | current | 402 | 11 | 4 | 2 | 0 |
| claude-haiku-5-5 | pointing | 394 | 15 | 6 | 4 | 1 |
| Qwen3.5-9B-Q4_K_M | current | see §8.4 | | | | |
| Qwen3.5-9B-Q4_K_M | pointing | see §8.4 | | | | |
| Qwen3.8-27B-Q5_K_S | current | see §8.4 | | | | |
| Qwen3.8-27B-Q5_K_S | pointing | see §8.4 | | | | |

**Haiku, current to pointing:**
- 385 supported in both.
- 11 supported → unsupported, 4 → contradicted, 2 → all common.
- 7 unsupported → supported, 2 contradicted → supported.

**How far apart the forms are:** they disagree on 28 of 419 findings. The same form run twice disagrees on 17 of 419: the current arm against the saved verdicts, which the same model wrote.

**Read by hand:**
- The pointing form no longer rescues a general sentence by rewriting it into a specific.
- It is too strict on about five reasoning clauses.
- It reaches too far on a few.
- Only 3 of the 9 findings that became supported rest on a `follows` part.

The pointing form is the one whose support can be shown.

### 8.2 What the pointers look like (Haiku, pointing)

- **Part kinds:** stated 752, follows 151, common 47, specific_missing 17, contradicted 6, own 4.
- **Sentences per stated or follows part:** 1 for 542 parts, 2 for 298, 3 for 43, 4 or more for 20. A chip marks one or two sentences, not a 3.4 KB chunk.
- **Sources per supported statement:** one for 386, two for 8.
- **Supported through `follows` alone:** 22.
- **Relations of the 394 supported statements:** quotes 6, restates 192, summarizes 71, infers 125.

### 8.3 Answer size

The pointing form writes more than the current form: Haiku's output mean rose from 377 to 442 tokens with adaptive thinking on. On the local models, which run without thinking, the most a pointing answer wrote is in §8.4. No answer was cut off at 1,200 tokens, the room the replay gave every model. So the part-wise judge takes the room every durable check keeps, `ANSWER_ROOM_TOKENS`, instead of a constant of its own.

### 8.4 Local models

Filled from `compare_anchors.py` when the runs end: outcome counts, moves, part kinds, relation split, output-token maximum, and a hand read of every moved statement.

## 9. Tests

**Domain:**
- Each rule of `relation_of`, with its tie cases:
  - a follows part among stated parts
  - two stated parts naming one common sentence among others
  - parts naming disjoint sentences
  - no anchors
- `quotation` at the length limits.

**Judge:**
- Sentences are numbered across passages, and a hard-wrapped paragraph is one sentence.
- Each refusal and its correction.
- `follows` counts as support; `guarded` leaves `follows` alone.
- The answer room.
- The request snapshot `claim-parts`.

**Checker:**
- A `SentenceAt` becomes the version's byte span.
- A settled quotation records its spans.
- A v22 receipt is not restored under v23.

**Read time:**
- Every rule of `cited`, including v22 findings.
- Two chips for a statement on two passages, each with its own anchors.
- The `Quoted` piece and its chip.
- No chip for unsupported, common or contradicted statements.

**Frontend:**
- `placeAnchors`' three tiers over text with multi-byte characters before the anchor.
- The rail stepping over cited occurrences only.
- No lexical marking when a source has `anchoredBy`.
- The heading trail.
- The card for each relation.
- Quote marks drawn and dropped from selections.
- The lesson surfaces end to end on the mock backend.

## 10. Alternatives not taken

- **Ask the model for the relation.** A citation-function classifier (Teufel, Siddharthan and Tidhar, 2006) asks what a citation does. Its label is a judgement nobody can check. Here the judge decides the facts it is good at (which sentences state each part, which only entail it), and the relation is a function of those facts, so code can test it and change it without a re-check.
- **Atomic facts.** FActScore (Min et al., EMNLP 2023) splits a text into atomic facts written by the model. Those facts are paraphrases of the text, which is the failure this design removes. Here each part is the claim's own words, and attribution is to a span, in the sense of AIS (Rashkin et al., *Computational Linguistics* 2023).
- **Passages as the cited unit.** ALCE (Gao et al., EMNLP 2023) scores citation precision by whether each cited unit is needed. A 3.4 KB passage is never precisely needed. A sentence is.
- **Count rule for summaries.** Measured in §2.3.
- **Character offsets.** Code points or UTF-16 offsets would tie stored data to one runtime's string model. Bytes are what the backend slices, and the frontend converts once.
- **Storing the relation.** It would fix today's rules into every report. The stored facts are the parts and anchors; the relation is read from them.
- **Lexical highlighting as a fallback for anchored citations.** A wrong mark is worse than an honest notice that the sentence is not in the saved text.

## 11. Out of scope

- Re-checking published lessons: a v22 lesson reads with `backs` and `quotes` chips until it is prepared again.
- Page-level anchors inside PDFs: the PDF viewer keeps `findPassagePage` until PDFs keep a text layer with byte offsets.
- A pointing judge for chat answers before a chat eval exists.
- Synthesis across sources shown as one claim with several relations: a statement takes the relation of its weakest grounded part.
