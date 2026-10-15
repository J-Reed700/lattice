use super::*;
use lattice_core::application::ports::embedding_port::EmbeddingTextChunk;
use lattice_core::domain::value_objects::{ChunkingStrategy, FileMetadata};
use lattice_core::shared::types::ValidatedFilePath;
use std::sync::Mutex;

/// Splits at a fixed byte width and records every text handed to the model,
/// so a test can compare what the late-chunking and chunk-first paths embed.
struct RecordingEmbedder {
    late: bool,
    width: usize,
    embedded: Mutex<Vec<String>>,
}

impl RecordingEmbedder {
    fn new(late: bool, width: usize) -> Self {
        Self {
            late,
            width,
            embedded: Mutex::new(Vec::new()),
        }
    }
    fn embedded(&self) -> Vec<String> {
        self.embedded.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl EmbeddingPort for RecordingEmbedder {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let mut out = self.embed_batch(&[text.to_owned()]).await?;
        Ok(out.remove(0))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.embedded.lock().unwrap().extend(texts.iter().cloned());
        Ok(texts.iter().map(|t| vec![t.len() as f32]).collect())
    }

    fn split_text(&self, text: &str, _prefix: &str) -> Result<Vec<EmbeddingTextChunk>> {
        let mut chunks = Vec::new();
        let mut start = 0;
        while start < text.len() {
            let mut end = (start + self.width).min(text.len());
            while !text.is_char_boundary(end) {
                end += 1;
            }
            chunks.push(EmbeddingTextChunk {
                text: text[start..end].to_owned(),
                start,
                end,
                token_count: 1,
            });
            start = end;
        }
        Ok(chunks)
    }

    fn uses_late_chunking(&self) -> bool {
        self.late
    }

    fn dimension(&self) -> usize {
        1
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

fn document(text: &str) -> Document {
    document_named(text, "notes.md", "text/markdown")
}

fn document_named(text: &str, name: &str, mime: &str) -> Document {
    let path = ValidatedFilePath::new(std::path::PathBuf::from(format!(
        "/tmp/lattice-late/{name}"
    )))
    .unwrap();
    let metadata = FileMetadata::new(
        name.to_owned(),
        mime.to_owned(),
        text.len() as i64,
        chrono::Utc::now(),
    )
    .unwrap();
    let checksum =
        lattice_core::application::factories::ChecksumFactory::from_bytes(text.as_bytes()).unwrap();
    Document::from_file(
        path,
        metadata,
        checksum,
        text.to_owned(),
        ChunkingStrategy::FixedSize { size: 512 },
    )
    .unwrap()
}

fn citations(document: &Document) -> Vec<(String, Option<usize>, Option<usize>)> {
    document
        .chunks()
        .iter()
        .map(|c| (c.content().to_owned(), c.start_char(), c.end_char()))
        .collect()
}

const TEXT: &str = "# Alpha\nFirst body line here.\n# Beta\nSecond body line here.\n";

#[test]
fn spans_carry_the_prefix_once_and_address_each_chunk_exactly() {
    let embedder = RecordingEmbedder::new(true, 12);
    let (document, spans) = prepare_structured_with_spans(document(TEXT), &embedder, &[]).unwrap();
    assert!(spans.len() > 1, "headings should open new spans");
    let mut expected_index = 0;
    for span in &spans {
        assert_eq!(span.first_chunk_index, expected_index);
        let prefix_end = span.chunk_ranges.first().unwrap().start;
        assert!(prefix_end > 0, "the context prefix precedes every chunk");
        // The prefix appears once, at the head of the span.
        let prefix = &span.span_text[..prefix_end];
        for (offset, range) in span.chunk_ranges.iter().enumerate() {
            let chunk = document.chunks().get(expected_index + offset).unwrap();
            assert_eq!(&span.span_text[range.clone()], chunk.content());
            assert_eq!(
                format!("{prefix}{}", &span.span_text[range.clone()]),
                chunk.embedding_text()
            );
        }
        // Ranges tile the passage with no gaps and no overlap.
        assert!(span
            .chunk_ranges
            .windows(2)
            .all(|pair| pair[0].end == pair[1].start));
        assert_eq!(span.chunk_ranges.last().unwrap().end, span.span_text.len());
        expected_index += span.chunk_ranges.len();
    }
    assert_eq!(expected_index, document.chunks().len());
}

#[test]
fn a_commented_code_file_is_one_span() {
    let script = "import os\n# install deps\nos.system('pip install x')\n# run it\nmain()\n";
    let embedder = RecordingEmbedder::new(true, 1024);
    let (prepared, spans) = prepare_structured_with_spans(
        document_named(script, "setup.py", "text/x-python"),
        &embedder,
        &[],
    )
    .unwrap();
    assert_eq!(spans.len(), 1);
    assert!(prepared.chunks().iter().all(|c| c.section().is_none()));
    // The same text as markdown would split at each `# ` line.
    let (_, markdown_spans) =
        prepare_structured_with_spans(document(script), &embedder, &[]).unwrap();
    assert!(markdown_spans.len() > 1);
}

#[tokio::test]
async fn late_chunking_falls_back_to_the_identical_per_chunk_inputs() {
    // The default port implementation of `embed_span_chunks` is the
    // chunk-first path, which is exactly what a real embedder does for a
    // span that does not fit its window.
    let chunk_first = RecordingEmbedder::new(false, 12);
    let (plain_document, plain_spans) =
        prepare_structured_with_spans(document(TEXT), &chunk_first, &[]).unwrap();
    let plain_vectors = embed_prepared_chunks(&plain_document, &chunk_first, &plain_spans)
        .await
        .unwrap();

    let late = RecordingEmbedder::new(true, 12);
    let (late_document, late_spans) =
        prepare_structured_with_spans(document(TEXT), &late, &[]).unwrap();
    let late_vectors = embed_prepared_chunks(&late_document, &late, &late_spans)
        .await
        .unwrap();

    assert_eq!(chunk_first.embedded(), late.embedded());
    assert_eq!(plain_vectors, late_vectors);
    assert_eq!(plain_vectors.len(), late_document.chunks().len());
    // Citation evidence is untouched by the strategy.
    assert_eq!(citations(&plain_document), citations(&late_document));
}

/// Splits with the real input policy and answers whole spans, so a test can
/// tell the late path from the per-chunk fallback by which method ran.
struct SpanEmbedder {
    policy: crate::features::embedding::input_policy::InputPolicy,
    spans: Mutex<Vec<usize>>,
    batched: Mutex<Vec<String>>,
}

impl SpanEmbedder {
    fn new(max_tokens: usize) -> Self {
        use tokenizers::{
            models::wordlevel::WordLevel, pre_tokenizers::whitespace::Whitespace, Tokenizer,
        };
        let vocab = [("[UNK]".to_owned(), 0), ("word".to_owned(), 1)]
            .into_iter()
            .collect();
        let mut tokenizer = Tokenizer::new(
            WordLevel::builder()
                .vocab(vocab)
                .unk_token("[UNK]".into())
                .build()
                .unwrap(),
        );
        tokenizer.with_pre_tokenizer(Whitespace);
        Self {
            policy: crate::features::embedding::input_policy::InputPolicy::new(
                tokenizer, max_tokens,
            )
            .unwrap(),
            spans: Mutex::new(Vec::new()),
            batched: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl EmbeddingPort for SpanEmbedder {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let mut out = self.embed_batch(&[text.to_owned()]).await?;
        Ok(out.remove(0))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.batched.lock().unwrap().extend(texts.iter().cloned());
        Ok(texts.iter().map(|t| vec![t.len() as f32]).collect())
    }

    fn split_text(&self, text: &str, prefix: &str) -> Result<Vec<EmbeddingTextChunk>> {
        self.policy.split(text, prefix)
    }

    fn uses_late_chunking(&self) -> bool {
        true
    }

    async fn embed_span_chunks(
        &self,
        span_text: &str,
        chunk_ranges: &[std::ops::Range<usize>],
    ) -> Result<Vec<Vec<f32>>> {
        self.spans.lock().unwrap().push(chunk_ranges.len());
        Ok(chunk_ranges
            .iter()
            .map(|range| vec![span_text[range.clone()].len() as f32])
            .collect())
    }

    fn dimension(&self) -> usize {
        1
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The point of decoupling chunk size from the model window: a span that
/// fits the window now holds several chunks, so late chunking has token
/// states from the rest of the span to pool each chunk against. At the old
/// window-sized chunk size this span was a single chunk, and any span that
/// was not overflowed the window and fell back to chunk-first.
#[tokio::test]
async fn a_span_of_several_chunks_is_pooled_in_one_pass() {
    let embedder = SpanEmbedder::new(2048);
    let text = format!("# Alpha\n{}", "word ".repeat(1400));
    let (document, spans) = prepare_structured_with_spans(document(&text), &embedder, &[]).unwrap();
    assert_eq!(spans.len(), 1, "one heading, one structure span");
    let span = &spans[0];
    assert!(
        span.chunk_ranges.len() >= 3,
        "the retrieval target should cut this span into several chunks, got {}",
        span.chunk_ranges.len()
    );
    assert!(
        embedder.policy.count(&span.span_text).unwrap() <= 2048,
        "the whole span still fits one forward pass, so nothing falls back"
    );

    let vectors = embed_prepared_chunks(&document, &embedder, &spans)
        .await
        .unwrap();
    assert_eq!(vectors.len(), document.chunks().len());
    assert_eq!(
        *embedder.spans.lock().unwrap(),
        vec![span.chunk_ranges.len()],
        "one span call carrying every chunk"
    );
    assert!(
        embedder.batched.lock().unwrap().is_empty(),
        "the per-chunk fallback must not have run"
    );
}

#[tokio::test]
async fn spans_that_do_not_cover_the_chunks_use_the_batch_path() {
    let late = RecordingEmbedder::new(true, 12);
    let (document, spans) = prepare_structured_with_spans(document(TEXT), &late, &[]).unwrap();
    let vectors = embed_prepared_chunks(&document, &late, &[]).await.unwrap();
    assert_eq!(vectors.len(), document.chunks().len());
    assert_eq!(
        late.embedded(),
        document
            .chunks()
            .iter()
            .map(|c| c.embedding_text())
            .collect::<Vec<_>>()
    );
    // A truncated span list is refused the same way.
    let partial = spans.first().cloned().into_iter().collect::<Vec<_>>();
    let vectors = embed_prepared_chunks(&document, &late, &partial)
        .await
        .unwrap();
    assert_eq!(vectors.len(), document.chunks().len());
}

/// A chunk never crosses a page, so marking the pages OCR read marks
/// exactly the chunks cut from them. Marking again from an extraction with
/// no OCR pages clears every mark, which a re-index relies on.
#[test]
fn only_chunks_cut_from_an_ocr_page_are_marked() {
    let pages = [
        "Typed first page.\n",
        "Scanned second page.\n",
        "Typed third page.\n",
    ];
    let text = pages.concat();
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, page) in pages.iter().enumerate() {
        ranges.push((index + 1, start, start + page.len()));
        start += page.len();
    }
    let embedder = RecordingEmbedder::new(false, 8);
    let (mut prepared, _) =
        prepare_structured_with_spans(document(&text), &embedder, &ranges).unwrap();

    prepared.mark_ocr_pages(&[2]);

    assert!(prepared.chunks().iter().any(|c| c.from_ocr()));
    for chunk in prepared.chunks() {
        assert_eq!(
            chunk.from_ocr(),
            chunk.page_number() == Some(2),
            "{:?} on page {:?}",
            chunk.content(),
            chunk.page_number()
        );
    }

    prepared.mark_ocr_pages(&[]);

    assert!(prepared.chunks().iter().all(|c| !c.from_ocr()));
}
