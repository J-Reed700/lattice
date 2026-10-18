# Fix: OCR text is now marked

Branch `fix/ocrmark` (from the integration branch at 0f19c85a). Uncommitted.

Follow-up from track H. Nothing recorded which chunks came from OCR. The model and the reader could not tell that a passage was read off a scanned page and may hold recognition errors.

## Decision: a flag per chunk

The flag is per chunk, not per document. The pipeline already knows each chunk's page. `embedding_input::structure_spans` never lets a span cross a page range, and every chunk is cut from one span. So a chunk's `page_number` says exactly which page it came from. A mixed PDF marks only the chunks cut from its scanned pages; its text-layer pages carry no mark. A per-document flag would have marked every passage of a 400-page manual for one scanned insert.

## What changed

### Schema

- New migration `crates/lattice-desktop/migrations/20261013170000_chunk_ocr_flag.sql`: `text_chunks.from_ocr BOOLEAN NOT NULL DEFAULT 0 CHECK (from_ocr IN (0, 1))`. The init migration is untouched.

### Extraction

- `engine/extraction/types.rs:27` `ExtractedContent.ocr_pages: Vec<u32>`, the one-based pages OCR read. Empty for every non-PDF extractor (one line each in `csv.rs`, `docx.rs`, `html.rs`, `odt.rs`, `pptx.rs`, `rtf.rs`, `text.rs`, `xlsx.rs`).
- `engine/extraction/pdf.rs:82` `PageText.from_ocr`. Set at `pdf.rs:350` when OCR returns text for a page. Collected into `ocr_pages` at `pdf.rs:138`. A page OCR failed on, or that no provider read, is not marked.
- `application/ports/content_extraction_port.rs:23` `ExtractedContentData.ocr_pages`, filled by `content_extraction_adapter.rs` (empty for audio).

### Write path

- `domain/entities/chunk.rs:74` `Chunk.from_ocr` with `from_ocr()` and `set_from_ocr()`. Defaults to false.
- `domain/entities/document.rs:451` `Document::mark_ocr_pages(pages)`. It marks each chunk whose page is in `pages` and clears every other chunk. A chunk with no page is never marked.
- Import: `indexing/use_cases/index_file/prepare.rs:199` marks right after `prepare_structured_with_spans`.
- Re-index: `indexing/use_cases/reindex_document.rs:94` `extract` now also returns the OCR pages; `reindex_document.rs:256` marks from this extraction alone. The new chunk set replaces the old one, and the mark is computed fresh, so a page that has since gained a text layer loses its mark.
- Persistence: `mappers/chunk_mapper.rs:58` `ChunkModel.from_ocr` (both directions). The column is written and upserted in `repositories/chunk/ops.rs` (`save`, `save_batch_optimized`), `repositories/chunk_repository.rs` (`save`, `save_batch`) and `repositories/document/ops/parts.rs` (`save_chunks`, the import and re-index path).

### Read path

- Every chunk SELECT that builds a `ChunkModel` now reads `from_ocr`: `chunk/ops.rs` (4), `chunk_repository.rs` (4), `document/ops/parts.rs:35`.
- `document/ops/parts.rs` `fetch_chunks_for_document` did not read `page_number` either. Every document aggregate save rewrites the chunks from the loaded aggregate, so a load-then-save dropped the page. It now reads `page_number` and `from_ocr`. This is a small adjacent fix; without it any aggregate save would also wipe the OCR mark.
- `SourceDto.from_ocr` (`qa/dto.rs:146`, `#[serde(default)]`). Filled from the stored chunk in `retrieval/source_citations.rs:311`. Every other `SourceDto` literal sets `false` (web results, attachments, tool sources, tests).
- Search results for the prompt: `CorpusPassage.from_ocr` (`application/contracts/search.rs:48`) is read by the three passage queries in `conversation/repository/workspace/retrieval.rs` (openings, section lookup, neighbours). `retrieval_locations` (`retrieval.rs:403`) now returns `(section, page, from_ocr)`; the port signature changed in `chat/ports.rs` and `repository/chat_port.rs`. `retrieval/corpus_plan.rs:767` and `corpus_plan.rs:796` put `"ocr": true` in the hit's metadata, beside `section` and `pageNumber`. The key is left out for an ordinary passage.

### Prompt

- `chat/prompting.rs:482` `kb_passage`: an OCR passage gets one extra header line, before `Content:`:
  `Text: OCR of a scanned page; odd characters may be recognition errors`.
  Nothing changes for any other passage. The budget prices passages through the same function, so the line is paid for.

### UI

- `src/lib/bindings.ts:7718` `SourceDto.fromOcr?: boolean` (hand-edited; see "Not verified").
- `src/types/conversation.ts:40` and `:92` `SourceWithMetadata.fromOcr` and its zod field. `chat/controller/messageMetadata.ts:61` carries it through `normalizeSource`. `journal/model/journalCitationSources.ts:24` carries it into journal citations.
- `chat/components/EvidenceMargin.tsx:111`: the note's meta line reads, for example, `PDF p. 3 · OCR text`. Muted, same line as the page, no new element.

### Tests

- `pdf_tests.rs:420` `only_the_pages_ocr_read_are_marked_as_ocr_text`: scan between two text pages; with OCR `ocr_pages == [2]`; without a provider the page is in `needs_ocr` and `ocr_pages` is empty. Extended `a_failed_page_does_not_stop_the_next_one` (`:517`, the failed page is not marked, the read one is) and `a_text_pdf_never_asks_for_ocr` (`:556`, no OCR pages).
- `embedding_input.rs:748` `only_chunks_cut_from_an_ocr_page_are_marked`: three page ranges, small chunks; only page 2's chunks are marked; marking again with no pages clears them all.
- `chunk_repository.rs:806` `an_ocr_chunk_keeps_its_mark_through_the_database`: real migrations; the mark and page survive `save_batch` plus `find_by_ids`, and `save_chunks` plus `fetch_chunks_for_document`.
- `workspace/retrieval.rs:782` `a_hit_location_carries_the_ocr_mark`.
- `prompting.rs:626` `only_a_passage_read_by_ocr_is_labelled_as_ocr_text`: the phrase is in the header of the flagged passage only.
- `EvidenceMargin.test.tsx:54` `marks a passage read off a scanned page as OCR text, and only that one`.
- Hand-built `text_chunks` fixtures in `corpus_plan/tests.rs` and `retrieval/tests/web_fallback.rs` gained the column; their positional inserts now name their columns.

## Verified

Run from `crates/lattice-desktop` with `CARGO_TARGET_DIR=/Users/josh/Code/lattice-imm-target2`, except the frontend and script checks.

- `cargo check --lib`: finished, no warnings shown.
- `cargo test --lib -- indexing qa:: conversation::chat::prompting conversation::chat::retrieval chunk_repository conversation::repository::workspace domain::entities`: first run 460 passed, 7 failed (the hand-built fixtures above lacked the column). After the fixture fix: 467 passed, 0 failed, 3 ignored.
- `cargo fmt`; then `rustfmt --check --edition 2021` on every touched `.rs` file: clean. `cargo fmt` changed only files this branch touches.
- `bash scripts/check-rust-layer-boundaries.sh`: clean. `bash scripts/check-repository-barrier.sh`: clean. `python3 scripts/check-tauri-command-inventory.py`: clean.
- `npx tsc --noEmit -p .`: exit 0.
- `npx vitest run` on `EvidenceMargin.test.tsx`, `journalCitationSources.test.ts`, `api.commandRoute.test.ts`, `conversationReasoning.test.ts`: 4 files, 19 tests passed.

## Not verified

The shared target broke partway through. From about 17:29, every cargo command fails in the `lattice-desktop` build script with `failed to run tauri-build: failed to read plugin permissions`. Cause: the build-script outputs `debug/build/tauri-00b5f5a29c8e568b` and `debug/build/tauri-0c4148f3981e549e` in `lattice-imm-target2` were copied from the old `lattice-temp/target`, and their permission listings point at that directory, which no longer exists. My rules forbid `cargo clean` and editing that directory, so I stopped. A likely fix for the owner: `cargo clean -p tauri` with that target dir, which reruns tauri's build script. So:

- `cargo clippy --lib` did not run.
- `export_bindings` did not run. I added the `fromOcr?: boolean` field to `bindings.ts` by hand, in the format the generator uses for this struct. specta marks a `#[serde(default)]` field optional, and `--check` will confirm or correct it.
- The last Rust edit (dropping `skip_serializing_if` from `SourceDto.from_ocr`, so the generated field is optional rather than required) was not recompiled. It removes one serde attribute argument.
- The wider `conversation`, `daily_notes` and `infrastructure::persistence` test modules beyond the filters above did not run. They compiled in the passing run.
- Nothing ran live: no real scan was imported, re-indexed or cited.

## Risks

- Only `kb_passage` prints the OCR line. Passages that reach the model by other routes do not: a carried attachment, the follow-up document, prior evidence, and the tool loop's search results. They still carry `fromOcr` on their `SourceDto` where it came from the chunk repository.
- Only the evidence margin shows the marker. The footnote popover, the citation rail and the source reader do not. `SourceChunkExcerptDto` has no flag.
- `SourceDto` JSON now always includes `fromOcr`, so each persisted source grows by one field.
- An existing database needs a re-index for old OCR chunks to be marked. That is in line with the no-legacy rule.

## Suggested commit

```
Mark chunks read by OCR and say so to the model and the reader

A passage OCR read off a scanned page looked the same as one taken
from a text layer, so a misread character could pass as a fact. The
PDF extractor now reports which pages OCR read, and the chunks cut
from those pages carry a new text_chunks.from_ocr flag. The flag is
per chunk because a chunk never crosses a page, so a mixed PDF marks
only its scanned pages. Re-indexing recomputes it. The passage header
in the prompt names OCR text, SourceDto carries the flag, and the
evidence margin shows "OCR text" beside the page.
```
