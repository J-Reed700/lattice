# One text segmentation layer

Design for the split brain in how the app cuts text. Repo read at `fa702bb6` (main) and `c66215de` (lesson/design).

## 1. What is there now

Every feature that cuts text wrote its own cutter. Twenty of them, with fourteen definitions of where a sentence ends:

| Consumer | Code | Unit and sentence rule | Measure | Packing |
|---|---|---|---|---|
| Library import and reindex (production) | `ChunkingStrategy::chunk` (`lattice-core/domain/value_objects/chunking_strategy.rs:96`) over `ChunkingService::chunk_by_sentences` (`domain/services/chunking_service.rs:69`) | `.!?\n` before whitespace or an uppercase letter, 13 abbreviations | words, though the field is `max_tokens` | greedy, no overlap, sentences re-joined with spaces: no byte offsets, line breaks lost |
| Library (retrieval eval `--mode production`) | `SemanticChunker` (`features/indexing/engine/chunker.rs`) | `.!?\n` before whitespace, last 50 tokens | tokens | 800 / 120 overlap |
| Embedding input fit | `input_policy::cut_candidates` (`features/embedding/input_policy.rs:195`) | paragraph > `.!?…` + whitespace, `。！？` > whitespace; never in a fence or table | tokens | partition, floors ½ ½ ¾ |
| Course index | `span_chunks` (`infrastructure/passage_index/chunking/spans.rs`) | `.!?…` + closers + whitespace, `。！？`, `\n` | chars | windows, step |
| Explorer and lab files | `lines::chunk_text` (`passage_index/chunking/lines.rs`) | lines; before a top-level item > after a blank line | lines and chars | streaming partition, back half |
| Parts of a long text | `split_into_parts` (`shared/text/mod.rs:300`) | `\n` > space, last fifth | chars | partition |
| Summary evidence | `passages_from_text` (`grounded_generation/mod.rs:52`) | `\n\n` > whitespace | chars | partition, trimmed |
| Memory extraction | `segment_message` (`conversation/memory/segment.rs:75`) | `\n\n` > `\n` > `. ` `.\n` `! ` `? ` | bytes | exact tiling, floor ¼ |
| Claim inventory | `inventory::passages` (`content_verification/inventory.rs:68`) | none: `\n` after 600 chars, else hard at 1,400 | chars | tiling |
| Claim ledger, cut, confine, citations, lesson rules, protected, interview evidence | `shared::text::sentences` | `.!?` before whitespace, `\n` | n/a | n/a |
| Chat verification | `lexical::split_sentences` | `.!?` before a non-alphanumeric, `\n`; inline math kept whole | n/a | n/a |
| Teaching quotes | `quotes::sentence_spans` | `.!?` before whitespace, `\n` | n/a | n/a |
| Outline linking | `spans::sentence_ends` | `.!?` + closers before whitespace | n/a | n/a |
| Reading notes, search snippets | `unicode_segmentation::split_sentence_bounds` | UAX #29 | n/a | n/a |
| Study recall | `split_inclusive(['.','!','?','\n'])` | every `.`, so `3.14` splits | n/a | n/a |
| HyDE | `split(&['.','!','?'])` | every `.` | n/a | n/a |
| Claim review | `service::whole_sentence` | `ends_with` checks | n/a | n/a |
| Excerpts | `build_excerpt` (core) and `SnippetExtractor` (search) | ±80 chars for `.?!\n` | chars | one window |
| Tutor `read_course_source` | `chars().skip().take()` | none: cuts mid-word | chars | one window |
| Read aloud | `split_phonemes` (`speech/engine/service.rs:127`) | `.!?…` > `;:—` > `,` > space | chars | partition |

Two things in this table are defects, not just duplication:

- **The lesson's claim inventory and its ledger disagree on what a sentence is.** The inventory hands the extractor 600–1,400-character chunks, and a claim's quote is its whole chunk. The ledger, the cut, confinement and citations all work in `shared::text::sentences`. When the code cuts a claim's sentence on its last attempt, the chunk-wide quote still relocates into what is left. The claim then survives in the inventory, re-matches its old verdict and ends queued, with "Needs your review" and no attempts left. That was the 2026-11-02 acceptance run.
- **The library is measured with a chunker it does not use.** The retrieval eval's `--mode production` (`examples/retrieval_eval/production.rs:6`) and its README (§"Each fixture document is chunked by `SemanticChunker` at the production 800/120") chunk with `SemanticChunker`. The shipped import path chunks with `ChunkingStrategy`, which packs 800 *words*, scales to 1,600 for long documents (`adapt_for_content`) and has no overlap. No production code constructs `SemanticChunker`. `2026-10-12-SHIPPED-PATH-BASELINE.md` also records the importer as "semantic, 800 tokens".

## 2. Design

Three concerns are mixed together in every cutter above, and each one gets one owner:

- **Where text may be cut**, and how good each place is. This is a property of the text: its sentences, lines, paragraphs, sections and protected spans.
- **How big a piece is**: bytes, characters, words or tokens. This is a property of the consumer's budget.
- **How pieces are chosen** under the budget: exact tiling, or overlapping windows. This is a property of the consumer's contract.

The layer lives in `lattice-core/src/shared/text/segmentation/` and has no dependencies beyond `unicode-segmentation` (already in the lockfile, 1.13).

```rust
pub type Span = std::ops::Range<usize>;            // bytes, both ends on char boundaries

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Strength { Char, Word, Clause, Sentence, Line, Paragraph, Section }

pub struct Break { pub at: usize, pub strength: Strength }

/// Where `text` may be cut. Every break is a char boundary strictly inside
/// the text, ascending, one per offset (its strongest kind). Char breaks
/// are implied at every char boundary and never listed.
pub trait Boundaries { fn breaks(&self, text: &str) -> Vec<Break>; }

/// How big a piece is.
pub trait Measure {
    /// The furthest char boundary `end >= start` whose piece `start..end`
    /// measures at most `max`.
    fn reach(&self, text: &str, start: usize, max: usize) -> usize;
}

pub struct Rung { pub at_least: Strength, pub floor: f64 }   // floor: share of `max` the piece must reach
pub enum Overflow { Cut, Extend }
pub struct Plan<'a> { pub max: usize, pub ladder: &'a [Rung], pub overflow: Overflow }

pub fn partition(text, &impl Boundaries, &impl Measure, &Plan) -> Vec<Span>;          // exact tiling
pub fn windows(text, &impl Boundaries, &impl Measure, &Plan, step: usize) -> Vec<Span>; // overlapping, trimmed
pub fn units(text, &impl Boundaries, at_least: Strength) -> Vec<Span>;                 // cut at every break that strong
pub fn sentences(text) -> Vec<(usize, usize)>;   // units(text, &Prose, Sentence), trimmed; today's signature
```

### 2a. Where a break sits

A break at `at` divides `text[..at]` from `text[at..]`. The whitespace between two pieces stays with the piece before it: a sentence break comes after the terminator, its closers and the following whitespace. UAX #29 assigns whitespace the same way, and the memory segments and the embedding fit already do. So a partition is an exact tiling: its pieces concatenated are the text. Consumers that show pieces (sentences, windows) trim them.

Strength comes from the whitespace before the break: two or more `\n` make a Paragraph, one makes a Line, none leaves the kind of break that was found there. A break before a Markdown heading line (`#{1,6} ` after a line break) is a Section.

### 2b. `Prose`: the one rule for running text

`Prose` implements `Boundaries` as **UAX #29 sentence boundaries**, read with `unicode_segmentation::split_sentence_bound_indices`, with **CLDR's English sentence-break suppressions** applied the way ICU's filtered break iterator applies them. On top of that come four Markdown tailorings:

1. **Suppressions.** No sentence break after a token on CLDR's list (`common/segments/en.xml`, 151 entries such as `Mr.`, `Mrs.`, `Prof.`, `U.S.`, `Ph.D.`, `a.m.`, `vs.`, `Mt.`) when the token starts a word. The list is the CLDR file verbatim, with its source and fetch date, in `segmentation/suppressions.rs`. Nobody curates it by hand.
2. **Emphasis and code closers belong to their sentence.** In `**Bold.** Next`, UAX #29 breaks between `.` and `**`. The break moves past the closing run of `*`, `_`, `~` or a backtick and the whitespace after it. If the closing run is followed directly by a word character, there is no break.
3. **Protected spans.** There is no sentence or clause break inside inline code, inline math (`shared::markdown_math::inline_math_spans`, today used only by chat verification) or a link destination. In a fenced code block or a table, each line end is a Line break (a blank line inside a fence is a Paragraph break) and nothing inside a line is stronger than Word. In this rule, a line of code or a table row is a sentence.
4. **List markers.** A line-initial ordered-list marker (`1.`, `2)`, `1.)`) ends nothing.

Other breaks:
- **Line.** A line break ends a sentence, as it does in every rule above. UAX #29 rule SB4 agrees. A heading or a list item is a sentence of its own.
- **Word.** At the end of every whitespace run.
- **Clause.** After `,` `;` `:` followed by whitespace, and after `—`, outside protected spans.

**Why a rule-based segmenter, and this one.** The consumers need deterministic, offline, byte-exact boundaries that run in the hot loops of the claim ledger and the indexer. UAX #29 plus CLDR suppressions is what ICU, the browsers and Android ship. pySBD (Sadvilkar & Neumann, NLP-OSS 2020) is the best-documented rule-based segmenter, and its "Golden Rules" (inherited from pragmatic_segmenter) are the standard English test set; they become our boundary table (§3). Neural segmenters, SaT (Frohmann et al., EMNLP 2024, wtpsplit), are the frontier for unpunctuated and noisy text. They would add a model call to every segmentation, and the text here is punctuated Markdown and extracted documents. They stay out of this layer. A transcript consumer could add a `Boundaries` implementation backed by one without touching any packer.

### 2c. `Code`: the rule for source files

There is a Section break before a top-level item (`fn`, `class`, `export`, a doc comment or attribute that leads into one, or a Markdown heading) when the previous line doesn't open one too. There is a Paragraph break after a blank line, a Line break at each line end, and a Word break at whitespace. That is `lines.rs`'s `cut_before`, lifted out unchanged.

### 2d. Measures

`Bytes`, `Chars` and `Words` live in core. A word is a maximal run of non-whitespace, and `reach` stops before the next word's first byte. `Tokens` lives in lattice-desktop beside the tokenizer. It is built per text from one encoding, `Tokens::new(&tokenizer, text)`, and `reach` binary-searches the token end offsets, floored to a char boundary. A token count taken from the whole text's encoding can differ by a token or two at a cut, so the embedding fit keeps its exact re-count of each piece.

### 2e. Packing

**`partition`**: from `start`, take `reach = measure.reach(text, start, max)`. For each rung in order, take the latest break in `(start, reach]` of at least the rung's strength whose piece reaches `floor × max`. The floor test is `at > measure.reach(text, start, ceil(floor × max) − 1)`, so no piece is measured twice. The first rung that finds a break decides the cut. If no rung finds one:
- `Cut` cuts at `reach`, or one character past `start` when nothing fits, so the loop always makes progress.
- `Extend` runs on to the next break at least as strong as the last rung, so a piece over budget is one whole unit and never half of one.

**`windows`**: each window ends as `partition` would end it. The next window starts at the latest ladder break in `(start, reach(start, step)]`. If the sentence holding the step point began at `start`, the first such break after the step point is used instead. If there is no break at all, the window starts at the step point. That is `span_chunks`'s rule, ladder `[Sentence]`.

### 2f. Alignment: what became of a text after an edit

The lesson pipeline had three answers to the question "what happened to this sentence between two versions of a field":

| Code | What it does |
|---|---|
| The inventory's `locations::anchor` | maps a character interval through one common-prefix/common-suffix diff |
| The ledger's `successor::changed_region` | a byte prefix/suffix diff, widened to sentences |
| `repair/confine.rs::aligned` | an LCS over the two sentence lists |

The first one is how the stuck claims stuck. A whole-passage quote whose sentence was cut anchors into whatever is left of the passage. When two edits land in one field, its single diff spans both, and a deleted sentence can map onto its rewritten neighbour.

The layer owns one alignment instead:

```rust
pub fn common_subsequence<T: PartialEq>(a: &[T], b: &[T]) -> Vec<(usize, usize)>;   // the LCS `confine` ran: matched index pairs
pub fn align(old: &str, new: &str) -> Alignment;   // `common_subsequence` over the `sentences` of each side
pub enum Fate { Kept(Span), Rewritten(Span), Removed }
impl Alignment { pub fn fate(&self, old: Span) -> Fate; pub fn hunks(&self) -> &[(Span, Span)]; }
```

- A sentence matched by the LCS is **Kept**, at its new place.
- Unmatched old sentences between two matches (or before the first, or after the last) form a **hunk** with the unmatched new sentences between the same matches. When both sides hold as many, they pair one for one, each pair its own hunk, as `confine` reads them. A hunk whose new side is empty means the sentences were **Removed**. Otherwise they were **Rewritten**, into the hunk's new text. A removal's new span, or an insertion's old span, is empty before the next sentence of that text (at its end when none follows).
- The fate of a span clamps it to the old sentences it overlaps, so the whitespace around them is nothing. It is Removed when none of them survived. Otherwise its new span runs from the first surviving image to the last, keeping the offset where it starts or ends inside a Kept sentence, so a fragment of a kept sentence maps to the same fragment. It is Kept when every sentence it overlaps was Kept and the new text is the old text, which an insertion between two kept sentences breaks, and Rewritten otherwise.

`confine.rs` keeps its hunk logic and calls `common_subsequence`, the LCS moved out of it unchanged. The inventory's relocation uses `fate`, and `anchor` is deleted. The ledger's `changed_region` becomes the alignment's hunks in phase 3, behind the ledger suites.

### 2g. Invariants, tested for every `Boundaries` × `Measure` × `Plan`

- **Partition.** Pieces are non-empty, contiguous and on char boundaries, and they concatenate to the text. No piece is over budget, except under `Extend`, where an over-budget piece holds no break of the last rung's strength.
- **Windows.** Starts strictly increase, every non-whitespace byte is in some window, and neighbouring windows overlap only by whole units.
- **Breaks.** Breaks are ascending, unique, strictly inside the text and on char boundaries. `units(text, b, s)` tiles the text.

## 3. The boundary table

`segmentation/golden.rs` holds two kinds of rows. Each row lists the sentences we produce, and a row that misses the golden answer says so.
- **The English Golden Rules**, each with its golden answer.
- **Our own cases**: Markdown emphasis, inline code and math, fences, tables, list markers, CJK `。！？`, closing quotes and brackets, `3.14`, `file.rs`, CRLF, headings.

The test asserts that the number of misses never grows.

Two kinds of known misses:
- **Deliberate.** A line break ends a sentence, while the Golden Rules join PDF-wrapped lines. Our extractors already unwrap PDF lines.
- **CLDR's choices.** `I live in the U.S. How about you?` reads as one sentence because `U.S.` is suppressed. `Dr.` and `St.` are not on the list, so they break.

## 4. Consumers become policies

Each consumer keeps a `const` plan next to its code, so what it wants stays readable where it is used:

| Consumer | Plan |
|---|---|
| Claim inventory | `partition(Prose, Chars, max 1,400, [Paragraph ≥ 0.43, Sentence 0], Extend)`. A claim's quote is narrowed at extraction to the sentence `claim_sentence` finds, the ledger's own locator, falling back to its passage. Revision drops a claim whose sentence is gone from the section. |
| Memory segments | `partition(Prose, Bytes, 6 KiB, [Paragraph ¼, Line ¼, Sentence ¼], Cut)`, overlap context unchanged |
| Embedding fit | `partition(Prose, Tokens, limit, [Paragraph ½, Sentence ½, Word ¾], Cut)`, then the exact re-count |
| `split_into_parts` | `partition(Prose, Chars, max, [Line 0.8, Word 0.8], Cut)` |
| `passages_from_text` | `partition(Prose, Chars, max, [Paragraph 0, Word 0], Cut)`, returned as spans of the caller's text |
| Course index | `windows(Prose, Chars, CHUNK_CHARS, [Sentence 0], Cut, STEP_CHARS)` |
| Tutor `read_course_source` | the window from `offset` cut by `partition(Prose, Chars, room, [Sentence ½, Word ¾], Cut)` |
| Excerpts (`build_excerpt`, `SnippetExtractor`) | one excerpt: the window around the match, both ends on the strongest break within 80 characters |
| Read aloud | its own `Boundaries` over phoneme strings (`.!?…` Sentence, `;:—,` Clause, space Word), `partition(Chars, 510, [Sentence 0, Clause 0, Word 0], Cut)` |
| Every sentence list in §1 | `sentences`; the private rules are deleted |
| Library import and reindex | one library chunker: `ChunkingStrategy::chunk` becomes `partition(Prose, Words, …, [Section ½, Paragraph ½, Sentence 0], Extend)`, returning spans of the extracted text. `ChunkingService` and `SemanticChunker` are deleted, and the retrieval eval calls the production chunker. |
| Explorer and lab files | `Code` boundaries with a two-cap measure (lines, chars), partitioned over the streaming buffer |

## 5. Phases

Every phase is test-first. Before a consumer moves, its current output is recorded over a corpus: the boundary table, its own fixtures, `docs/**/*.md` and the synthetic library v3 documents. The diff after the move is the full list of behaviour changes, and each change is either a fix or gets reverted.

1. **The layer** (`text/segmentation`, core only): types, `Prose`, `Code`, `Bytes`/`Chars`/`Words`, `partition`/`windows`/`units`, `sentences` re-pointed, the boundary table and the invariants as property tests (proptest). The ledger, cut and confine suites must stay green on the new `sentences`.
2. **The lesson pipeline** (on lesson/design):
   - `align` in core; `confine.rs` calls it.
   - The claim inventory on `partition`.
   - Every claim quote, extracted or revised, narrowed by one `claim_quote(passage, statement)`: `claim_sentence`, or the passage when no single sentence holds the statement.
   - Relocation by `fate`: a Kept quote moves, a Rewritten one becomes its new text for the model's revision to restate, and a Removed one takes its claim out of the inventory. An old passage-wide quote is narrowed first, so a cut inside it decides the claim's fate.
   - The red test is `a_claim_cut_on_its_last_attempt_ends_removed_though_the_inventory_revision_keeps_its_statement` (`claim_ledger_tests.rs`). Then comes the 27B acceptance run.
3. **Sentence lists and small chunkers** (main): chat verification, teaching quotes, outline linking, notes, snippets, study recall, HyDE, claim review, memory segments, `split_into_parts`, `passages_from_text`, the tutor read, excerpts, read aloud, the course index. These behaviour changes stay inside each consumer's own tests. The course index runs its fixtures.
4. **The library chunker and the code chunker**: they change what is indexed. The library move ships behind a before/after retrieval eval on the shipped path, at 9B local. There is no explorer eval, so the code chunker's move must reproduce its current chunks byte for byte over every source file in this repo. Documents already indexed keep their chunks until they are re-indexed.
