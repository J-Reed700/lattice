# Spot the errors: review drills on your own material, not only code

Date: 2026-11-05. Status: design. The direction was signed off on 2026-11-05 ("yea sounds good to me"); the calls in §8 are
still open. It builds on the code review drills (`2026-11-04-code-review-drills.md`), branch `learning/spot-the-errors`,
which is stacked on `learning/code-review-drills`.

## 0. Why, and what carries over

The code review drill trains one move: read something you didn't write, find what is wrong, say why it matters, and
lead with the worst of it. Code is only one material that move applies to. Finding planted errors in an
explanation of something you are studying is the same task, and it has its own evidence: studying erroneous examples
improves delayed learning over correct examples alone (Große & Renkl, 2007; McLaren, Adams & Mayer, 2016), and texts
that name and refute a misconception change belief more than texts that only state the correct account (Tippett,
2010). It is also retrieval practice: you can only catch "the study found a 40% drop" if you remember it was 14%.

Of the code drill, two parts are code-specific and everything else is not:

| Part | Code drill | Passage drill |
|---|---|---|
| The material | model-written code and a hidden suite | a passage from one of the learner's course sources, and an explanation of it the model writes and the checker proves sentence by sentence |
| The proof of a planted error | a killing test (or behaviour kept + judge) | the source check: the clean sentence is supported by the source with an exact quote, the planted one is contradicted or unsupported by it |
| Key, spans, findings, `Assessment`, `validate`, `score`, ratings, the schedule, the judge's answer contract, attempts, the reveal's issue cards | shared | shared |

So the design is one drill pipeline with the material as a Strategy, not a second feature.

## 1. What the learner does

They pick **what to review**: code (a language, as today) or a passage (one of their courses, or any course). For a
passage drill they get a short explanation of something from that course's sources, written the way a classmate's
study note or an answer in an exam would be, with 1–4 planted errors. They select sentences (click a sentence's
number; shift-click for a run), comment on what is wrong and why it matters, optionally give a severity, optionally
rewrite the sentences. Submit. The reveal is the code drill's reveal with two changes: each issue shows **what the
source says**, the exact quote that refutes the planted sentence, and the contrast pane is "the explanation you
reviewed · the explanation without the errors", with the source passage under it.

Modes stay the same: practice and interview (timed). Rapid mode applies later (single planted sentence, name the error
type).

## 2. The representation: a passage is a file with one sentence per line

Everything downstream of the material speaks `DrillFile { path, text }` and `LineSpan { path, start, end }` over whole
lines. A passage drill's text is written **one sentence per line**, split with the core segmenter
(`shared::text::segmentation::sentences`), and its path is a readable name (`explanation.md`). Then, unchanged:

- spans are sentence runs; the overlap hint, `numbered()` listings in the judge's task, `check_key`, `apply`/`replace`
  edits, `line_diff` for the fix, and the gutter picking in the UI all work as they do for code;
- the judge sees numbered sentences, which is a better unit than wrapped lines anyway.

Paragraphs are kept as a blank line between sentence lines; the passage view (§6) renders sentence lines inline, so
the learner reads prose, not a list.

## 3. The core (lattice-core `domain::learning::drills`)

- **`Material`**: `Code { language }` | `Passage`. `Drill.language` becomes `Drill.material`; `DrillSpec.language`
  likewise. Serde keeps reading a stored `language` as `Code { language }` so existing rows load.
- **Catalogue per material.** `Family` and `Pattern` are unchanged in shape; `patterns_for(material)` replaces
  `patterns_for(language)`. The passage families and patterns (each with a definition the planter and the judge read):
  - *Fidelity*: misquoted figure, wrong unit or magnitude, swapped entities, wrong attribution, wrong date or order.
  - *Scope*: overgeneralisation (some → all), dropped qualifier or hedge, shifted population or condition.
  - *Reasoning*: correlation stated as cause, reversed relation or direction, unsupported mechanism, non sequitur.
  - *Definition*: wrong definition, conflated terms.
  - *Consistency*: contradicts another sentence of the explanation.
  The catalogue is data, as it is for code; adding a pattern is a row.
- **`Proof::Sourced { quote, planted: SourceVerdict, reason }`**, beside `Killed` and `Judged`. `quote` is the exact
  source text that supports the clean sentence; `planted` is `Contradicted` or `Unsupported`. A `Consistency` pattern
  is proven with `Judged` against the explanation itself (no source needed).
- **Severity** is assigned by the planter per pattern and checked by rule: a contradicted figure, entity, direction or
  definition is at least major; an unsupported addition (scope, mechanism) is at least minor. The weights (4/3/2/1)
  and every score are unchanged.

## 4. The pipeline: one Template Method, the material as a Strategy

`DrillBuild` keeps its checkpointed stage order: clean → clean review → plant → prove → blind review → store. Each
stage body asks a `DrillMaterial` for the material-specific part:

```rust
#[async_trait]
pub trait DrillMaterial: Send + Sync {
    fn material(&self) -> Material;
    /// The clean files and whatever the proof needs later (code: the suite; passage: the source passage).
    async fn clean(&self, spec: &DrillSpec, ctx: &StageCtx<'_>) -> Result<CleanMaterial>;
    /// Repairs the clean material until it has no issue of its own (code: a reviewer + the suite; passage: every
    /// sentence supported by the source, unsupported ones rewritten from it or dropped).
    async fn clean_review(&self, clean: &CleanMaterial, ctx: &StageCtx<'_>) -> Result<Baseline>;
    fn plant_wording(&self) -> PlantWording;               // system text, the patterns, what a verification is
    fn proof_strategy(&self, mutation: &Mutation) -> Box<dyn ProofStrategy + '_>;
    fn fix_strategy(&self, issue: &KeyIssue) -> Box<dyn FixStrategy + '_>;
    fn reviewer_wording(&self) -> ReviewerWording;         // blind review and describer system text
    fn judge_wording(&self) -> JudgeWording;               // the grader's nouns: "code"/"lines" vs "explanation"/"sentences"
}
```

- **`CodeMaterial`** is today's code moved behind the trait, with no behaviour change: the same prompts, the
  `CodeRunner` adapter, `KillingTest`/`BehaviourKept`/`WeakenedTests`/`Judged`. Its tests stay green unchanged; that
  is the check on the refactor.
- **`PassageMaterial`**:
  - *clean*: picks a passage (§5), then asks the model for an explanation of it in the learner's register (spec
    `lines` = sentences). Output is split one sentence per line.
  - *clean review*: the source check on every sentence. Supported sentences stay; an unsupported or contradicted one
    is rewritten from the source once and re-checked, else dropped. The baseline is an explanation every sentence of
    which the source supports, the same guarantee a clean suite gives code.
  - *plant*: the code planter's contract (exact edits, one pattern each, consequence and "how to say it"), with prose
    wording and the passage patterns. The consequence is what a reader who believed it would get wrong.
  - *prove*: `SourceChecked`: the clean sentences under the edit are `Supported` by the source with an exact quote,
    and the planted sentences are `Contradicted` or `Unsupported`. Anything else drops the mutation, as an unproven
    code defect is dropped. `Consistency` patterns go to `Judged`.
  - *fix*: `SourceCheckedFix`: the learner's rewrite of the issue's sentences is fixed when the source supports it,
    partly when it removes the error but states something unsupported, not fixed otherwise.
  - *blind review*: a fresh reviewer reads the planted explanation **with the source passage**, so a real unplanted
    error (one the clean review missed) can become a `FoundInReview` issue, as in code.
- **`SourceCheck`** is an Adapter over main's `ClaimChecker` in `CheckPolicy::Strict`, using `check(claim, &ClaimEvidence
  { text: passage, quote: None })` so the quote is an exact substring of the source. Strict already turns a
  `Supported`/`Contradicted` without a valid quote into `Unusable` and a low-confidence verdict into `Unsupported`;
  an `Unusable` or `Failed` judgment is an outage for the stage, never a proof. It runs on the Learning model, like
  every drill call (the verifier role can be swapped in later through the same adapter).

## 5. The material: which passage

- **From a course's sources**, through `ReferenceCollection` (`retrieve`, passages widened to paragraph edges, exact
  substrings of a version that never changes). A passage is keyed `(source version id, start_byte, end_byte)`, so the
  reveal can open it in the source reader and a regraded attempt reads the same text.
- **Which one**: the least-drilled source of the chosen course, then a passage that has not been drilled, sized to
  the spec. A topic the learner names narrows `retrieve` to it.
- **A library document** joins the same path through `capture_document` (it makes a fixed version with byte offsets),
  so the drill never depends on chunk ids that re-indexing replaces. Later step; courses come first.

The bank keeps one passage drill ready per course the learner has drilled, as it keeps one per language.

## 6. Persistence, IPC and the UI

- **Tables**: a migration adds `material TEXT NOT NULL DEFAULT 'code'` and `source_json` (the passage key and its
  text) to `learning_drills`; `language` becomes nullable for passages. Attempts are material-free already.
- **IPC**: `start_code_review_drill` gains a `material` argument (`{ kind: 'code', language } | { kind: 'passage',
  programId, topic? }`) and is renamed `start_review_drill`; submit, result and regrade keep their shape (they are by
  attempt id). `CodeReviewDrillDto` gains `material` and, for a passage, the source's title. The DTOs keep leaking no
  key: the source passage is in the reveal only.
- **UI**: the Studio section becomes "Review drills" with the two kinds. `DrillChoose` gets "What to review" (code /
  a passage from a course) and, for a passage, the course select instead of the language. `DrillReview` takes a
  `MaterialPane`: `CodePane` for code, `PassagePane` for prose (sentence numbers as the gutter, text wrapped, same
  `onSelectLines` contract, a plain editor for the rewrite). The reveal swaps `CodeContrast` for `PassageContrast`
  and shows each issue's source quote. The copy says "in the code" only for code.

## 7. Measured before it ships

The same bar as code (§6 of the code drill design), read by hand: of 20 passage drills on Haiku from two of the
learner's courses, ≥95% of planted errors are real errors against the source, and every clean explanation is
faithful; the judge's grades agree with hand labels on ≥0.9 of issues on a fixture set built like the code one.
`live_claim_pairs_eval` already measures the source check on labelled pairs; the planted/clean pairs from these 20
drills are added to it.

## 8. Build order and open calls

1. **Core** (pure): `Material`, the passage catalogue, `Proof::Sourced`, `check_key` and the schedule over `Material`,
   old rows still read. Tests first.
2. **The seam** (no behaviour change): `DrillMaterial` and `CodeMaterial`; `DrillBuild`, the grader's wording and fix
   grading go through it. The existing drill tests are the check.
3. **`PassageMaterial`**: passage picking, clean + clean review, plant wording, `SourceCheck`, `SourceChecked` proof
   and fix, blind review with the source, the migration, the bank per course.
4. **IPC and UI**: `start_review_drill`, the chooser, `PassagePane`, the reveal with source quotes.
5. **Measure** (§7), then the Tutor tool takes a material.

Calls for Josh (the defaults are what gets built unless he says otherwise):
- The drill is an **explanation written from the source** with planted errors (default), not the source passage itself
  with errors in it; the second turns the drill into spot-the-difference from memory of the exact wording.
- **Courses first**, library documents later (default), since course sources already have fixed versions and offsets.
- The reveal shows **the source passage** (default); the drill itself does not, so the learner reviews from memory, as
  in an exam. An "open book" practice option can show it during the drill.
