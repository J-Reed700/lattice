# Track H — OCR for scanned PDFs

Scanned PDF pages are now read by PaddleOCR-VL 1.6 in its own `llama-server` (branch `llm/ocr-vlm`, uncommitted). Live check on the bundled v0.6.0 binary: 0.6% character error rate on prose, about 5 s per page idle. Tests: 247 passed, 0 failed; `cargo check --all-targets`, the barrier check and the ratchet are clean.

## What changed

- **New `features/ocr/`:**
  - `model.rs` finds the downloaded model.
  - `render.rs` renders a PDF page to PNG with CoreGraphics at 150 dpi; PNG/JPEG files pass through.
  - `server.rs` starts the server on the first page, handles one page at a time, stops it after 60 s idle or at app shutdown, and replaces a crashed one.
  - `launcher.rs` runs the bundled `llama-server` on loopback, waits for `/health`, and logs to `$TMPDIR`. The key goes in the environment, not the command line.
  - `client.rs` sends the page with the prompt `OCR:` and a 180 s timeout.
  - `recognizer.rs` (`VisionOcr`, the `OcrPort` implementation), `di.rs`.
- **`ocr_port.rs`:** new `OcrError::Unavailable`.
- **`curated.rs`, OCR section only:** a `CuratedOcrModel` table. One entry, PaddleOCR-VL 1.6 from the official PaddlePaddle repo: weights 935,769,056 B (sha256 `f3ae46ec…`), projector 881,770,560 B (`204d757d…`). It replaces two old entries that listed no files. The download system already handles two-file entries; unchanged.
- **`pdf.rs` and its tests:** if OCR cannot run, the indexer asks once per document, not once per page. The "no text" error now says whether the model is missing, OCR could not run, or OCR found nothing.
- **Wiring:** `modules.rs` builds the real port instead of `NoopOcr`; one-line pass-throughs in `indexing/di.rs` and `content_extraction_adapter.rs` (outside my list) give import and re-index OCR.
- **Cargo:** macOS-only `objc2-core-graphics`, `objc2-core-foundation`, `png`, all already in Cargo.lock.

## Live check

Test files: page 2 of *Attention Is All You Need* with the text layer removed, and a three-page scan / text / scan PDF.

- **Indexer path:** only the mixed PDF's scanned pages (1 and 3) went to OCR; page 2 kept its text layer.
- **Accuracy:** the prose page scored 0.6% character error rate. The table page scored 6.7%, mostly ordering: the model reads rows, pdftotext reads columns, and formulas come back as LaTeX. Sample error: "gated recurrent Neural Networks" where the original reads "gated recurrent [7] neural networks".
- **Speed and memory:** load 0.8 s warm, 5.3 s cold; 5–5.3 s per page idle, about 10 s under other agents' builds; peak RSS 2.1 GB. Server stopped afterwards.

## Not verified

- The in-app catalog download and the import UI.
- Windows and Linux: PDF rendering there returns `Unavailable`, so scanned pages stay unindexed. `hayro` is the pure-Rust option.
- JBIG2/CCITT scans, rotated pages, and running alongside the chat model.

## Risks

- The 96.33 OmniDocBench score is for PaddleOCR's pipeline, which crops layout regions first; Lattice sends whole pages, so expect lower accuracy.
- Nothing marks chunks as OCR-derived; no column exists and I did not add a migration for one.
- The indexer does not accept image files. Enabling them would OCR every photo in an indexed folder; your call.
- A crash mid-OCR leaves the server running until the next launch, when the existing orphan reaper kills it.

## Suggested commit

```
Read scanned PDF pages with PaddleOCR-VL in its own llama-server

Scanned pages were left unindexed (NoopOcr). They now go to the 0.9B
PaddleOCR-VL 1.6 catalog model in a separate llama-server, because
--mmproj cannot share the chat server with MTP. The server starts on the
first scanned page and stops after a minute idle or at shutdown, so a
library without scans never loads it.
```
