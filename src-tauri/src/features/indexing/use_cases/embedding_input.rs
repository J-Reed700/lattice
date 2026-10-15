//! Structure-preserving, contextual input shared by imports and rebuilds.
//! Source text remains byte-for-byte citation evidence; metadata is embedded separately.
use crate::features::indexing::engine::extraction::is_code_mime;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::entities::{chunk::Chunk, document::Document};
use lattice_core::domain::value_objects::section_identifier::SectionIdentifier;
use lattice_core::domain::value_objects::source_context::StructureMode;
use lattice_core::shared::error::{AppError, Result};

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
    // In source code a `# ` line is a comment, so a code file is never split
    // into sections: one commented script would otherwise become many tiny
    // spans, each with a comment for a breadcrumb.
    let by_sections = !is_code_mime(document.mime_type())
        && document
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
    let mut fence = CodeFence::default();
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
            if sections && fence.track(line) {
                offset += line.len();
                continue;
            }
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

/// Markdown fenced code blocks (``` or ~~~). A `# ` line inside one is code,
/// not a heading. The fence state carries across page ranges, as a code block
/// can run over a page break.
#[derive(Default)]
struct CodeFence {
    open: Option<(char, usize)>,
}

impl CodeFence {
    /// Whether `line` is a fence line or inside a fenced block.
    fn track(&mut self, line: &str) -> bool {
        let trimmed = line.trim_start_matches(' ');
        // A fence may be indented at most three spaces.
        let indented = line.len() - trimmed.len() > 3;
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        let run = marker.map_or(0, |m| trimmed.chars().take_while(|c| *c == m).count());
        match (self.open, marker) {
            (Some((open, len)), Some(m))
                if !indented
                    && m == open
                    && run >= len
                    && trimmed[run * m.len_utf8()..].trim().is_empty() =>
            {
                self.open = None;
                true
            }
            (Some(_), _) => true,
            (None, Some(m)) if !indented && run >= 3 => {
                self.open = Some((m, run));
                true
            }
            (None, _) => false,
        }
    }
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
    fn hash_lines_inside_a_markdown_fence_are_not_headings() {
        let text = "# Setup\nRun this:\n```sh\n# not a heading\npip install x\n```\nDone.\n~~~~\n## also code\n~~~\n~~~~\n## Usage\nCall it.\n";
        let spans = structure_spans(text, &[], true).unwrap();
        let headings: Vec<_> = spans.iter().map(|s| s.heading.as_deref()).collect();
        assert_eq!(headings, vec![Some("Setup"), Some("Setup > Usage")]);
        assert!(text[spans[0].start..spans[0].end].contains("# not a heading\npip install x"));
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
mod late_chunking_tests;

#[cfg(test)]
mod live_tests;
