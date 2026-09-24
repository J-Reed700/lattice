# Citable web evidence: one numbering, acquisition as a type, quotes over guesses

Status: design specification, not implemented. Prepared against the working tree on
2026-09-22. Complements `docs/design/2026-09-21-persistent-tool-evidence.md`, which owns
the durable ledger; this document owns what a single turn produces for it to store.

An earlier draft of this document proposed recording the byte span of page text that
entered the prompt, and claimed that this removed the reader's guesswork. It does not.
`render_page_body` puts the whole page in, so that span is the whole document. §1 D-4 and
§7 phase D carry the corrected approach; the error is left described here because the
reasoning that produced it is easy to repeat.

## 1. Problem

Three defects observed in one deep-research turn (conversation `44a57cbf`, 2026-09-22):

1. Clicking `[2]` on a sentence about lettuce opened the reader on the broccoli section.
2. `[4]` cited a page the site refused with HTTP 403. Nothing distinguishes it from the
   pages that were read, so the reader opens it, fails, and shows an error.
3. Every stored snippet begins with the title again and a bare URL; some carry
   `2026-01-07T00:00:00.0000000` before the first word of prose.

Symptom 1 was fixed at commit `2a49d7ff` inside the matcher. The others are not reader
bugs. **A turn does not record what the model was shown for a given citation number**,
the number itself is derived twice by different code, and the UI reconstructs provenance
afterwards from word overlap.

### Verified defects

**D-1 — the citation numbering is derived twice, and can diverge.**
`build_web_source_citations` (`chat/retrieval/source_citations.rs:259`) skips a result
when its URL is empty or already seen:

```rust
if url.is_empty() || !seen_urls.insert(url) { continue; }
```

`assign_citation_ids` (same file, line 25) then numbers the survivors `1..n` in order.
The prompt is numbered independently in `chat/retrieval/pipeline.rs:1138`, which
enumerates **all** results including the skipped ones:

```rust
.enumerate().map(|(i, result)| format!("[{}] {}\nURL: {}\nSnippet: {}...", i + 1, ...))
```

For results `[A, B, B, C]` the model is told C is `[4]` while the source list holds three
entries and C is `[3]`. Every citation after a skip resolves to the wrong page, and
`parseCitations` (`src/utils/citations.ts`) drops any `[n]` above the source count, so
the last citation silently disappears from the rendered answer. The two sites agree only
because they usually walk the same vector in the same order. Verified by reading both
paths; not yet observed firing, and it did not fire in the turn above, whose ten URLs
were distinct.

**D-2 — whether a page was read is computed and then discarded.**
`pipeline.rs:1129` builds the source list from every result. Eight lines later it fetches
the pages and knows exactly which succeeded:

```rust
let (pages, page_memory) = self.fetch_page_texts(&output.results).await;
let body = render_page_body(pages.get(i).and_then(Option::as_ref));
```

`pages.get(i)` never reaches the UI. Nothing downstream can tell a page that was read
from one that only ever had a caption, which is the whole of symptom 2.

**D-3 — the caption is an unverified scrape.**
`web.rs:926` selects `.result__snippet, .result-snippet, .result__body, .b_caption p,
.b_snippet` and takes `.text()`, which collects every descendant — including the `cite`
element and the publish-date span. There is **no recorded HTML fixture for any engine in
the repository**, so which selector produced a given stored string cannot be determined
without a live request. A cleanup pass over the output was drafted and dropped: it
sanitized a parser instead of fixing one.

**D-4 — the only non-inferred intra-document signal is optional, and is usually off.**
`src/components/Chat/reader/sourcedPassages.ts` locates the part of a page a cited
sentence matches by weighted word overlap. This cannot be replaced by recording what
entered the prompt, because what enters the prompt is the entire page
(`render_page_body`, `pipeline.rs:1262`, *"this is the whole page and there is nothing
more to fetch"*). **Which paragraph backs a sentence is an inference in any design that
does not obtain attribution from the model.**

One non-inferred signal already exists: `ClaimVerdict.evidenceQuote`, the text a
verification judge matched. `WebArticleView` already prefers it over wording and scores
it 1.0 (`findQuoteSpan`). In the turn above it was empty for all 32 claims, because
verification ran without a judge (`judgeUsed: false`, 30 of 32 claims unsupported). The
defect is therefore not that the reader guesses — it is that **the recorded alternative
is absent in practice**, so guessing is the only path the reader ever takes.

## 2. Verified integration points

| Component | Current behaviour | Responsibility under this design |
|---|---|---|
| `features/web/services/web.rs` `parse_search_results` | One selector list across DuckDuckGo, Bing and fallbacks; `.text()` over all descendants | Split per engine behind `SearchPort`; each adapter owns its markup and a recorded fixture |
| `features/web/services/web.rs` `absorb_provider_page` | Dedups by canonical URL across providers and pages | Stays; the one place a `SearchHit` enters the domain |
| `chat/retrieval/source_citations.rs` `build_web_source_citations` | Builds `SourceDto` from results, skipping duplicates | Becomes a projection of the ledger |
| `chat/retrieval/source_citations.rs` `assign_citation_ids` | Numbers surviving sources `1..n` | Deleted; the ledger carries the number |
| `chat/retrieval/pipeline.rs` ~1129–1170 | Builds sources, fetches pages, renders context with its own numbering | Builds one ledger; prompt and sources become projections |
| `chat/fetch_memory.rs`, `page_cache` | Turn-local page and refusal memo | Supplies `PageReader`; a cached refusal stays a refusal |
| `features/qa/dto.rs` `SourceDto` | Flat struct shared by vault retrieval, study, journal, export, TS bindings | **Unchanged.** See §4 |
| `chat/persistence.rs` message metadata | Already carries `sources` and `verification` | Carries a parallel `webEvidence` array keyed by citation number |
| verification / claim verdicts | `evidenceQuote` optional; judge often not run | Emits a quote per supported (sentence, citation) pair |
| `reader/WebArticleView.tsx` | Fetches at render time, guesses the span | Branches on evidence kind; prefers recorded quotes |
| `reader/sourcedPassages.ts` | Primary provenance mechanism | Explicit fallback when no quote was recorded |

## 3. Design

### 3.1 The unit

```rust
/// One numbered thing the model was shown, and how it was obtained.
pub struct CitedEvidence {
    /// Assigned once, by the ledger. Never recomputed anywhere.
    pub number: CitationNumber,
    pub id: EvidenceId,
    pub source: SourceRef,          // normalized url, host, title
    pub acquisition: Acquisition,
}

pub enum Acquisition {
    /// The page was fetched and its text put in the prompt.
    Page {
        digest: ContentDigest,      // of the archived body, with extractor version
        word_count: usize,
        clipped_at: Option<usize>,  // what the model actually saw, if not all of it
        fetched_at: DateTime<Utc>,
    },
    /// The engine's description of the page. The page itself was never read.
    Caption { text: String, engine: EngineId, captured_at: DateTime<Utc> },
}
```

A source that could not be read is not a `CitedEvidence`, and has no constructor that
yields one:

```rust
pub struct UnusedSource { pub url: Url, pub reason: Unusable }
pub enum Unusable { Refused { status: u16 }, Empty, OverBudget }
```

This is the load-bearing decision. "Cite a page nobody read" stops being something four
call sites must remember to check and becomes a state the types cannot express. D-2 is
fixed by construction, not by a boolean a later caller forgets.

`Acquisition::Page` deliberately carries **no span**. The model was shown the document;
recording "span = the document" would dress a tautology up as provenance. `clipped_at`
is recorded because it is the one case where the model saw less than the archive holds,
and the reader must not highlight past it.

`EvidenceId` is content-addressed over `(normalized_url, content digest, extractor
version)`. The extractor version is included so that re-extracting the same page under a
new pipeline produces a new id rather than silently colliding with the old text — the
representation-version concern from §7 of the 2026-09-21 spec.

### 3.2 The ledger

```rust
pub struct EvidenceLedger {
    cited: Vec<CitedEvidence>,     // ordered; index + 1 == number
    unused: Vec<UnusedSource>,
}

impl EvidenceLedger {
    pub fn render_context(&self) -> ContextBlock;   // the only numbering that exists
    pub fn sources(&self) -> Vec<SourceDto>;        // projection
    pub fn evidence(&self) -> Vec<WebEvidenceDto>;  // sidecar, keyed by number
    pub fn resolve(&self, n: CitationNumber) -> Option<&CitedEvidence>;
}
```

Prompt text and source list become projections of one ordered vector, so D-1 becomes
unrepresentable: there is no second place where a number is computed. The two sites share
an algorithm today — both know "position in this vector after these skips" — and will
share a name instead.

### 3.3 Ports, and why assembly is two-phase

```rust
trait SearchPort { async fn search(&self, q: &Query) -> Result<Vec<SearchHit>>; }
trait PageReader { async fn read(&self, u: &Url) -> Result<PageDocument, Refusal>; }
```

Assembly cannot be a single pure function: the budget decides which pages are worth
fetching, and fetching is what reveals which pages exist. It splits in two, with the
network between them:

```rust
fn select(hits: &[SearchHit], budget: &Budget) -> Vec<Candidate>;          // pure
// ... PageReader runs over the candidates here ...
fn assemble(hits: Vec<SearchHit>, pages: PageSet, budget: &Budget) -> EvidenceLedger;  // pure
```

Both halves are pure — no clock, no network, no database — which is where all four
defects live and is what makes them testable without a live request. `SearchHit` carries
a `Caption`, never engine-shaped data: each engine adapter is an anti-corruption layer
between hostile, changing markup and the domain. Scrapers are where golden-file tests
earn their keep, and a recorded fixture per engine is what makes D-3 a verified fix
rather than another guess.

## 4. What the frontend receives, and what it does not touch

`SourceDto` is shared by vault retrieval, study, journal, export and the generated TS
bindings. Making it a discriminated union would touch every one of them for the benefit
of one source kind. It stays as it is.

Web evidence rides alongside it in the assistant message's metadata — the same place
`sources` and `verification` already live — keyed by citation number:

```ts
type WebEvidenceDto =
  | { kind: 'page'; number: number; url: string; title: string;
      digest: string; wordCount: number; clippedAt: number | null; fetchedAt: string }
  | { kind: 'caption'; number: number; url: string; title: string;
      text: string; engine: string; capturedAt: string };
```

The reader branches on `kind` rather than on a fetch that fails at render time:

- `page` — show the archived body; mark passages as §5 allows, never past `clippedAt`.
- `caption` — show the caption under a plain label: the search result's summary, this
  page was not read. **No error state, because no page was ever promised.**

`[4]` in the 2026-09-22 turn renders as a caption card. It stops looking broken because
the app stops claiming something it never had.

## 5. Where a highlight may come from

In priority order, and the reader says which one it used:

1. **A recorded quote.** The verifier's `evidenceQuote` for this (sentence, citation),
   located with `findQuoteSpan`. Exact, non-inferred, and the only one of the three that
   is a record rather than a reconstruction.
2. **Wording overlap.** `findSourcedPassages`, with the section rules from `2a49d7ff`.
   Explicitly labelled a wording match, as it already is.
3. **Nothing.** Stated plainly, as it already is.

The work D-4 asks for is at level 1, in the verifier, not in the matcher: emit a quote
per supported (sentence, citation) pair and persist it. That converts the common case
from reconstruction to record and leaves the matcher as the honest fallback it was
written to be.

## 6. Invariants

1. A citation number is assigned once, by the ledger, and never recomputed.
2. Prompt block and source list are projections of the same ordered ledger.
3. A source whose page was not read cannot carry `Acquisition::Page`.
4. An `UnusedSource` receives no number and appears in no source list.
5. A highlight is never drawn past `clipped_at`: the model did not see that text.
6. A `Caption` is what the engine's description element held — never the title, the cite
   line, or the publish date.
7. A recorded quote is only ever reported as recorded; a wording match is only ever
   reported as a wording match.
8. Archived page text is historical. §7 of the 2026-09-21 spec owns freshness.

## 7. Phases

- **A — adapters and fixtures.** Split `parse_search_results` per engine behind
  `SearchPort`; record one HTML fixture per engine, each including a cite line and a
  publish date; assert `SearchHit`. Fixes D-3 at the parser. No downstream change.
- **B — the ledger.** Introduce `EvidenceLedger`; make prompt and `SourceDto` list
  projections; delete `assign_citation_ids` and the positional `web-result-{idx+1}`.
  Fixes D-1. Self-contained and the highest value per line changed.
- **C — acquisition reaches the UI.** Emit `WebEvidenceDto` into message metadata; branch
  the reader on `kind`. Fixes D-2 and symptom 2. `SourceDto` untouched.
- **D — quotes from the verifier.** Emit and persist an evidence quote per supported
  (sentence, citation) pair; the reader prefers it and says so. Addresses D-4 as far as it
  can honestly be addressed. Superseded the span work from the first draft.

Schema follows the single-migration policy; the database is deletable pre-release, so no
compatibility shim is owed to turns recorded under the old shape. A turn without evidence
renders through the matcher exactly as today.

## 8. Tests

- Golden fixture per engine adapter: recorded HTML in, `SearchHit` out, with a cite line
  and a publish date present in the input and absent from the caption.
- Property test on `assemble`: for any hit list containing duplicate and empty URLs,
  every `[n]` in the rendered context resolves to exactly one source whose number is `n`.
  D-1 as an executable law.
- A refused page produces an `UnusedSource` and no numbered entry — asserted by the type,
  and by a test that the refusal never reaches the source list.
- A clipped page does not highlight past `clipped_at`.
- Reader: `caption` evidence renders no error and no highlight affordance; a recorded
  quote is labelled as recorded and a wording match as a wording match.

## 9. What this removes

- `assign_citation_ids`, and the second numbering in `pipeline.rs`.
- The reader's error state for a page that was never fetched.
- Word overlap as the *only* path to a highlight. It stays as the labelled fallback, with
  the section rules from `2a49d7ff`, which remain correct for that job.
