//! Structure-preserving, contextual input shared by imports and rebuilds.
//! Source text remains byte-for-byte citation evidence; metadata is embedded separately.
use crate::application::ports::EmbeddingPort;
use crate::domain::entities::{chunk::Chunk, document::Document};
use crate::domain::value_objects::section_identifier::SectionIdentifier;
use crate::domain::value_objects::source_context::StructureMode;
use crate::shared::error::{AppError, Result};

/// One structure span's embedding input: the span text with its context prefix
/// already prepended once, plus the byte range of each chunk inside it.
///
/// Late chunking embeds `span_text` in a single forward pass and pools each
/// range; the chunk-first path embeds `prefix + span_text[range]` per chunk.
/// Both see the same bytes, and neither changes the chunk content or the
/// `start_char`/`end_char` offsets that citations resolve against.
#[derive(Debug, Clone)]
pub struct SpanEmbeddingGroup {
    pub span_text: String,
    pub chunk_ranges: Vec<std::ops::Range<usize>>,
    /// Index of this span's first chunk in `document.chunks()`.
    pub first_chunk_index: usize,
}

/// Prepare a document's chunks and report how they group into structure spans.
///
/// Page ranges and offsets are UTF-8 bytes, never printed page labels.
///
/// There is deliberately no variant that drops the spans. A caller that
/// prepared chunks here and then embedded them itself would write chunk-first
/// vectors under whatever identity the model reports — including the
/// late-chunking one — and mix two vector spaces in a single generation. Pair
/// this with [`embed_prepared_chunks`], which picks the right path from what
/// the embedder reports.
pub fn prepare_structured_with_spans(
    document: Document,
    embedder: &dyn EmbeddingPort,
    pages: &[(usize, usize, usize)],
) -> Result<(Document, Vec<SpanEmbeddingGroup>)> {
    // Some legacy use cases reconstruct content only from chunks.
    let text = if document.content().is_empty() {
        document
            .chunks()
            .iter()
            .map(|c| c.content())
            .collect::<String>()
    } else {
        document.content().to_owned()
    };
    if text.trim().is_empty() {
        return Ok((document, Vec::new()));
    }
    let by_sections = document
        .source_context()
        .is_none_or(|c| c.group.structure == StructureMode::Sections);
    let spans = structure_spans(&text, pages, by_sections)?;
    let mut chunks = Vec::new();
    let mut groups: Vec<SpanEmbeddingGroup> = Vec::new();
    for span in spans {
        let passage = &text[span.start..span.end];
        if passage.trim().is_empty() {
            continue;
        }
        let mut prefix = context_prefix(&document, span.heading.as_deref());
        while embedder
            .split_text(&prefix, "")?
            .iter()
            .map(|p| p.token_count)
            .sum::<usize>()
            > 64
        {
            prefix = prefix
                .chars()
                .take(prefix.chars().count() * 3 / 4)
                .collect();
        }
        if !prefix.ends_with("\n\n") {
            prefix.push_str("\n\n");
        }
        // The embedder validates the prefix + passage using its real tokenizer.
        let first_chunk_index = chunks.len();
        let mut chunk_ranges = Vec::new();
        for part in embedder.split_text(passage, &prefix)? {
            // Ranges address the passage inside `prefix + passage`, so the
            // prefix is present exactly once and is never pooled into a chunk.
            chunk_ranges.push(prefix.len() + part.start..prefix.len() + part.end);
            let mut chunk = Chunk::new(document.id().clone(), part.text.clone(), chunks.len());
            chunk.set_language(document.language().clone());
            chunk.set_token_count(part.token_count as i32);
            chunk.set_word_count(part.text.split_whitespace().count());
            chunk.set_section(span.heading.clone());
            chunk.set_provenance(
                Some(prefix.clone()),
                Some(span.start + part.start),
                Some(span.start + part.end),
                span.page,
            );
            chunks.push(chunk);
        }
        if !chunk_ranges.is_empty() {
            groups.push(SpanEmbeddingGroup {
                span_text: format!("{prefix}{passage}"),
                chunk_ranges,
                first_chunk_index,
            });
        }
    }
    let tags = document.tags().to_vec();
    Ok((document.from_parts(chunks, tags)?, groups))
}

/// Embed a prepared document's chunks, span by span when the embedder reports
/// late chunking and chunk by chunk otherwise.
///
/// The result is always one vector per chunk, in chunk order. Spans that do not
/// line up with the document's chunks (a legacy document whose chunks were not
/// rebuilt here) fall back to the plain batch path rather than guessing.
pub async fn embed_prepared_chunks(
    document: &Document,
    embedder: &dyn EmbeddingPort,
    spans: &[SpanEmbeddingGroup],
) -> Result<Vec<Vec<f32>>> {
    let chunk_texts: Vec<String> = document
        .chunks()
        .iter()
        .map(|chunk| chunk.embedding_text())
        .collect();
    let aligned = spans
        .iter()
        .try_fold(0usize, |next, span| {
            (span.first_chunk_index == next).then(|| next + span.chunk_ranges.len())
        })
        .is_some_and(|covered| covered == chunk_texts.len());
    if !embedder.uses_late_chunking() || !aligned {
        return embedder.embed_batch(&chunk_texts).await;
    }
    let mut vectors = Vec::with_capacity(chunk_texts.len());
    for span in spans {
        let span_vectors = embedder
            .embed_span_chunks(&span.span_text, &span.chunk_ranges)
            .await?;
        if span_vectors.len() != span.chunk_ranges.len() {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Late chunking returned {} vectors for {} chunks",
                    span_vectors.len(),
                    span.chunk_ranges.len()
                ),
            });
        }
        vectors.extend(span_vectors);
    }
    Ok(vectors)
}

fn short(text: &str, count: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(count)
        .collect()
}

fn context_prefix(document: &Document, heading: Option<&str>) -> String {
    // Deliberately compact for small embedding windows. Never include arbitrary
    // generated summaries or treat a user's description as source evidence.
    let mut parts = Vec::new();
    if let Some(heading) = heading {
        parts.push(short(heading.rsplit(" > ").next().unwrap_or(heading), 100));
    }
    if let Some(context) = document.source_context() {
        parts.push(short(&context.group.title, 64));
        if let Some(edition) = &context.group.edition {
            parts.push(short(edition, 24));
        }
    }
    parts.push(short(document.file_name(), 48));
    if let Some(description) = document
        .source_context()
        .and_then(|c| c.group.description.as_deref())
    {
        parts.push(short(description, 64));
    }
    format!("{}\n\n", parts.join(" | "))
}

#[derive(Debug)]
struct Span {
    start: usize,
    end: usize,
    page: Option<u32>,
    heading: Option<String>,
}

fn structure_spans(
    text: &str,
    pages: &[(usize, usize, usize)],
    sections: bool,
) -> Result<Vec<Span>> {
    let ranges: Vec<_> = if pages.is_empty() {
        vec![(0, 0, text.len())]
    } else {
        pages.to_vec()
    };
    let mut covered = 0;
    let mut spans = Vec::new();
    let mut headings: Vec<(usize, String)> = Vec::new();
    for (page, start, end) in ranges {
        if start != covered
            || end < start
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(end)
            || end > text.len()
        {
            return Err(AppError::InvalidData(
                "Extracted page ranges do not cover the source text correctly".into(),
            ));
        }
        covered = end;
        let mut segment_start = start;
        let mut offset = start;
        for line in text[start..end].split_inclusive('\n') {
            if sections {
                if let Some((level, title)) = heading(line) {
                    if level == 1
                        && headings
                            .first()
                            .is_some_and(|(l, h)| *l == level && *h == title)
                    {
                        offset += line.len();
                        continue;
                    }
                    let breadcrumb = headings
                        .iter()
                        .map(|(_, h)| h.as_str())
                        .collect::<Vec<_>>()
                        .join(" > ");
                    if offset > segment_start {
                        spans.push(Span {
                            start: segment_start,
                            end: offset,
                            page: (page > 0).then_some(page as u32),
                            heading: (!breadcrumb.is_empty()).then_some(breadcrumb),
                        });
                    }
                    headings.retain(|(parent, _)| *parent < level);
                    headings.push((level, title));
                    segment_start = offset;
                }
            }
            offset += line.len();
        }
        let breadcrumb = headings
            .iter()
            .map(|(_, h)| h.as_str())
            .collect::<Vec<_>>()
            .join(" > ");
        if segment_start < end {
            spans.push(Span {
                start: segment_start,
                end,
                page: (page > 0).then_some(page as u32),
                heading: (!breadcrumb.is_empty()).then_some(breadcrumb),
            });
        }
    }
    if covered != text.len() {
        return Err(AppError::InvalidData(
            "Extracted pages omit part of the source text".into(),
        ));
    }
    Ok(spans)
}

/// Conservative structural headings. Ordinary prose and numbered list sentences
/// are not promoted into headings. A heading's text always comes from the source.
fn heading(line: &str) -> Option<(usize, String)> {
    let line = line.trim();
    if line.is_empty() || line.chars().count() > 140 {
        return None;
    }
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
        return Some((hashes, line[hashes..].trim().to_owned()));
    }
    let lower = line.to_lowercase();
    if ["chapter ", "part ", "volume ", "book "]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return Some((1, line.to_owned()));
    }
    if ["introduction", "foreword", "preface", "table of contents"].contains(&lower.as_str()) {
        return Some((1, line.to_owned()));
    }
    let numbered = line
        .strip_prefix("Section ")
        .or_else(|| line.strip_prefix("SECTION "))
        .or_else(|| line.strip_prefix("section "))
        .unwrap_or(line)
        .trim_start_matches('§')
        .trim_start();
    let mut words = numbered.split_whitespace();
    let identifier = words.next()?.trim_start_matches('§');
    // Section identifiers (101, 706.07, 1.2), not "1." list bullets.
    let identifier = SectionIdentifier::parse(identifier)?;
    if words.clone().count() >= 1
        && words.count() <= 16
        && !line.ends_with(['.', ';', ','])
        && !line.contains("....")
    {
        return Some((identifier.depth, line.to_owned()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hierarchy_preserves_unicode_bytes_and_page_boundaries() {
        let text = "Chapter 1 Foundations\nÉvidence\n101 Access\nFirst page.\n101.1 Exceptions\nSecond page.";
        let boundary = text.find("101.1").unwrap();
        let spans =
            structure_spans(text, &[(1, 0, boundary), (2, boundary, text.len())], true).unwrap();
        assert_eq!(
            spans
                .iter()
                .map(|s| &text[s.start..s.end])
                .collect::<String>(),
            text
        );
        assert!(spans.iter().any(|s| s.page == Some(2)
            && s.heading.as_deref()
                == Some("Chapter 1 Foundations > 101 Access > 101.1 Exceptions")));
        assert!(spans
            .iter()
            .all(|s| s.end <= boundary || s.start >= boundary));
    }
    #[test]
    fn page_mode_and_invalid_extraction() {
        let text = "# One\nText\n## Two\nMore";
        assert_eq!(structure_spans(text, &[], false).unwrap().len(), 1);
        assert!(structure_spans(text, &[(1, 1, text.len())], true).is_err());
        assert!(heading("1. Do this first.").is_none());
        assert!(heading("706.07 Rejection on Prior Art").is_some());
    }

    #[test]
    fn numbered_subsections_keep_their_parent_and_sibling_boundaries() {
        let text = "Chapter 700 Examination\n706 Rejection\n706.07 Actions\n706.07(a) First action\nFirst evidence.\n706.07(a)(1) Exception\nException evidence.\n§ 706.07(b) Second action\nSecond evidence.";
        let spans = structure_spans(text, &[], true).unwrap();
        assert_eq!(
            spans
                .iter()
                .map(|s| &text[s.start..s.end])
                .collect::<String>(),
            text
        );
        assert_eq!(spans.last().unwrap().heading.as_deref(), Some("Chapter 700 Examination > 706 Rejection > 706.07 Actions > § 706.07(b) Second action"));
        assert!(spans.iter().any(|s| s.heading.as_deref() == Some("Chapter 700 Examination > 706 Rejection > 706.07 Actions > 706.07(a) First action > 706.07(a)(1) Exception")));
        assert!(heading("706..07 Broken heading").is_none());
    }
}

#[cfg(test)]
mod late_chunking_tests {
    use super::*;
    use crate::application::ports::embedding_port::EmbeddingTextChunk;
    use crate::domain::value_objects::{ChunkingStrategy, FileMetadata};
    use crate::shared::domain_types::ValidatedFilePath;
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
        let path =
            ValidatedFilePath::new(std::path::PathBuf::from("/tmp/lattice-late/notes.md")).unwrap();
        let metadata = FileMetadata::new(
            "notes.md".to_owned(),
            "text/markdown".to_owned(),
            text.len() as i64,
            chrono::Utc::now(),
        )
        .unwrap();
        let checksum =
            crate::application::factories::ChecksumFactory::from_bytes(text.as_bytes()).unwrap();
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
        let (document, spans) =
            prepare_structured_with_spans(document(TEXT), &embedder, &[]).unwrap();
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
        let (document, spans) =
            prepare_structured_with_spans(document(&text), &embedder, &[]).unwrap();
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
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use crate::application::ports::ContentExtractionPort;
    use crate::features::embedding::candle_service::CandleEmbeddingService;
    use crate::infrastructure::adapters::content_extraction_adapter::ContentExtractionAdapter;

    #[tokio::test]
    #[ignore = "requires LATTICE_STRUCTURE_MODEL and LATTICE_STRUCTURE_PDF (read-only)"]
    async fn real_pdf_context_and_vectors_keep_source_pages() -> anyhow::Result<()> {
        let model =
            CandleEmbeddingService::open_unregistered(std::env::var("LATTICE_STRUCTURE_MODEL")?)?;
        let path = std::path::PathBuf::from(std::env::var("LATTICE_STRUCTURE_PDF")?);
        let extracted = ContentExtractionAdapter::new()
            .extract_content(&path)
            .await?;
        let metadata = crate::application::factories::FileMetadataFactory::from_path(&path)?;
        let checksum = crate::application::factories::ChecksumFactory::from_path(&path)?;
        let mut doc = Document::from_file(
            crate::shared::domain_types::ValidatedFilePath::new(path)?,
            metadata,
            checksum,
            extracted.text.clone(),
            crate::domain::value_objects::ChunkingStrategy::default(),
        )?;
        doc.set_source_context(Some(
            crate::domain::value_objects::source_context::SourceContext {
                group: crate::domain::value_objects::source_context::SourceGroup {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: "Manual of Patent Examining Procedure".into(),
                    edition: Some("Rev. 01.2024".into()),
                    description: None,
                    ordered: true,
                    structure: StructureMode::Sections,
                },
                position: 0,
            },
        ));
        let (doc, _spans) = prepare_structured_with_spans(doc, &model, &extracted.page_ranges)?;
        assert!(!doc.chunks().is_empty());
        let mut covered = vec![false; extracted.text.len()];
        for chunk in doc.chunks() {
            let (start, end) = (chunk.start_char().unwrap(), chunk.end_char().unwrap());
            assert_eq!(&extracted.text[start..end], chunk.content());
            assert!(extracted
                .page_ranges
                .iter()
                .any(|(page, a, b)| Some(*page as u32) == chunk.page_number()
                    && start >= *a
                    && end <= *b));
            assert!(chunk.token_count() <= 256 && chunk.token_count() > 0);
            assert!(chunk.embedding_text().contains("Manual of Patent"));
            covered[start..end].fill(true);
        }
        assert!(extracted
            .text
            .char_indices()
            .all(|(offset, ch)| ch.is_whitespace() || covered[offset]));
        let inputs: Vec<_> = doc
            .chunks()
            .iter()
            .map(|chunk| chunk.embedding_text())
            .collect();
        let started = std::time::Instant::now();
        let vectors = model.embed_batch(&inputs).await?;
        assert_eq!(vectors.len(), inputs.len());
        assert!(vectors
            .iter()
            .all(|v| v.len() == 384 && v.iter().all(|n| n.is_finite())));
        println!(
            "Structured PDF smoke: {} pages, {} contextual passages, {} vectors; embedding {:?}",
            extracted.page_ranges.len(),
            inputs.len(),
            vectors.len(),
            started.elapsed()
        );
        Ok(())
    }
}
