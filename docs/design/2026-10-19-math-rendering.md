# Math in answers and lessons

Status: built, 2026-10-19, branch `feat/chat-math`.

## The problem

Chat answers and lessons could not show math. A model that wanted a formula improvised it in Unicode (`|query's informative terms ∩ terms-in-top-passages| / |query's informative terms|`) and it rendered as plain text. Nothing in the read-only viewer (`src/components/TiptapEditor/TiptapViewer.tsx`, tiptap-markdown over markdown-it with `html: false`) understood TeX, and no prompt asked the model to write it.

## What is parsed

The viewer draws TeX with KaTeX (`katex` and `@tiptap/extension-mathematics` at the installed `@tiptap/core` version).

| Written | Where | Drawn as |
| --- | --- | --- |
| `$…$`, `\(…\)` | inside a line | inline formula |
| `$$…$$` | inside a paragraph | inline formula (kept as `$$` when written back) |
| `$$…$$`, `\[…\]` | starting a line of its own, on that line or across several | displayed formula |

Dollar rules follow Pandoc, so prose keeps its prices:

- an opening `$` has a non-space after it; the closing `$` has a non-space before it and no digit after it, so `$5 and $10` stays text;
- `\$` is a literal dollar, inside or outside a formula;
- a `$` straight after another `$` never opens a formula, so an unmatched `$$` is two literal dollars;
- a formula never holds a backtick, so code spans and fences keep their dollars;
- an unclosed opener stays text, which is what an answer still arriving shows until the closer comes. A displayed formula must close before a blank line.

`\(` and `\[` are read before markdown-it's escape rule, which would otherwise turn them into literal brackets. A `\[…\]` inside a paragraph is not math (markdown writers use `\[1\]` to escape brackets); only a line of its own is.

The rules are written once per language: `src/components/TiptapEditor/extensions/mathSyntax.ts` (markdown-it inline and block rules, registered through each math node's tiptap-markdown `parse.setup`) and `crates/lattice-core/src/shared/markdown_math.rs` (the same rules for the checks).

## Rendering

- `extensions/math.ts` extends the upstream `InlineMath` / `BlockMath` nodes with the delimiter the formula was written with, a tiptap-markdown serializer that writes back exactly what was read, and `renderText` returning `$…$` / `$$…$$`, so `getText()`, ProseMirror's copy and anything else that reads the document's text keep the source.
- KaTeX runs with `throwOnError: true`; the node view catches the error and shows the LaTeX source styled as inline code (`.inline-math-error`, `.block-math-error`). A bad formula never breaks the message. `strict: false` keeps Unicode symbols quiet; `trust` stays off, so a formula cannot load a link or picture.
- KaTeX's stylesheet is imported once by `math.ts`; Vite bundles its fonts as same-origin assets, which the CSP's `font-src 'self' data:` allows.
- A displayed formula scrolls horizontally inside its own frame in narrow panes (`tiptap.css`).
- Text taken from the page rather than the document (a tangent's selected passage, "tangent from this reply") goes through `textWithFormulas` / `selectedTextWithFormulas`, which read each formula as its source. KaTeX draws a formula as MathML and as glyph boxes, so the DOM's own text would read it three times over.
- Message copy, conversation export and continue-in-new-chat already use the stored markdown, so formulas leave as written.

## Citation and claim marks

- Citation chips are drawn only over text nodes. A formula is an atom whose LaTeX is an attribute, so `$a[1]$` never becomes a chip.
- Claim marks match a sentence the verifier cut from the raw markdown against the rendered document on letters and digits. The document side now also reads each atom's `renderText` (a formula's source), all at the formula's one position, so a sentence holding `$\frac{a}{b}$` still matches and its mark covers the formula.

## Guidance

`MATH_GUIDANCE` (in `markdown_math.rs`) is one constant: write notation as LaTeX, inline `$…$`, display `$$…$$` on lines of its own; never `$` math for plain numbers or money; citation markers outside the math. It is appended to every chat system prompt after `DIAGRAM_GUIDANCE` (`chat/prompt_settings.rs`; constant text, so the cached prefix does not move) and listed in the lesson-writing requirements (`learning/lessons/generation/lesson.rs`) and the lesson repair system prompt (`content_verification/repair.rs`).

## Verifier

- `markdown_media::strip_fences_and_images` treats a displayed formula as a fenced block: the grounding check and its judge never see it, like a diagram. An unclosed `$$` is not stripped (unlike an unclosed fence), because the viewer shows it as text.
- In `chat/verification/lexical.rs` an inline formula is part of its sentence: the sentence splitter never stops inside it (`$n!$`), emphasis stripping leaves its `_` and `*`, and its brackets are neither citation markers nor stripped as ones. For token overlap a formula counts only the names written in it (`formulas_as_words`: `$\frac{TP}{TP+FP}$` reads `TP TP FP`), never TeX control words or braces, so `\frac` cannot cost a sentence its support while the prose words still count.
- Lesson claim review extracts claims with a model over lossless text passages (`content_verification/inventory.rs`); it has no sentence splitter of its own and reads LaTeX as written, so nothing changed there.

## Follow-ups

- The note editor (`TiptapEditor`) does not parse math. Re-serialising a note would rewrite a `$` the user typed as prose; math there needs its own decision about what a typed `$` means and an editing UI.
- Synthesis citation renumbering (`conversation/synthesis/prompts.rs::rewrite_citation_ids`) still rewrites a `[n]` inside a formula.
- A `|` inside an inline formula in a GFM table cell still splits the cell, as in other markdown renderers.
