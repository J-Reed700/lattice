//! Structure-preserving, contextual input shared by imports and rebuilds.
//! Source text remains byte-for-byte citation evidence; metadata is embedded separately.
use crate::features::indexing::engine::extraction::is_code_mime;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::domain::entities::{chunk::Chunk, document::Document};
use lattice_core::domain::value_objects::section_identifier::SectionIdentifier;
use lattice_core::domain::value_objects::source_context::StructureMode;
use lattice_core::shared::error::{AppError, Result};

/// Prepare a document's chunks along its structure spans, each chunk carrying
/// the span's context prefix.
///
/// Page ranges and offsets are UTF-8 bytes, never printed page labels.
pub fn prepare_structured(
    document: Document,
    embedder: &dyn EmbeddingPort,
    pages: &[(usize, usize, usize)],
) -> Result<Document> {
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
        return Ok(document);
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
        for part in embedder.split_text(passage, &prefix)? {
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
    }
    let tags = document.tags().to_vec();
    document.from_parts(chunks, tags)
}

/// Embed a prepared document's chunks: one vector per chunk, in chunk order.
pub async fn embed_prepared_chunks(
    document: &Document,
    embedder: &dyn EmbeddingPort,
) -> Result<Vec<Vec<f32>>> {
    let chunk_texts: Vec<String> = document
        .chunks()
        .iter()
        .map(|chunk| chunk.embedding_text())
        .collect();
    embedder.embed_batch(&chunk_texts).await
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
        let mut has_body = false;
        for line in text[start..end].split_inclusive('\n') {
            if sections && fence.track(line) {
                has_body = true;
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
                    if offset > segment_start && has_body {
                        let breadcrumb = headings
                            .iter()
                            .map(|(_, h)| h.as_str())
                            .collect::<Vec<_>>()
                            .join(" > ");
                        spans.push(Span {
                            start: segment_start,
                            end: offset,
                            page: (page > 0).then_some(page as u32),
                            heading: (!breadcrumb.is_empty()).then_some(breadcrumb),
                        });
                        segment_start = offset;
                        has_body = false;
                    }
                    headings.retain(|(parent, _)| *parent < level);
                    headings.push((level, title));
                    offset += line.len();
                    continue;
                }
            }
            if !line.trim().is_empty() {
                has_body = true;
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

    #[test]
    fn consecutive_headings_before_body_share_one_span() {
        for (text, breadcrumb) in [
            (
                "# Main topic\n## Nested topic\nBody text.",
                "Main topic > Nested topic",
            ),
            (
                "# Überblick\n## Vertragslaufzeit\nDie Laufzeit beträgt zwei Jahre.",
                "Überblick > Vertragslaufzeit",
            ),
        ] {
            let spans = structure_spans(text, &[], true).unwrap();

            assert_eq!(spans.len(), 1, "{text}");
            assert_eq!(&text[spans[0].start..spans[0].end], text);
            assert_eq!(spans[0].heading.as_deref(), Some(breadcrumb));
        }
    }

    #[test]
    fn sibling_sections_with_bodies_remain_separate_spans() {
        for (text, breadcrumbs, bodies) in [
            (
                "# Photosynthesis\n\n## Light reactions\nChlorophyll absorbs light.\n## Calvin cycle\nCarbon fixation follows.",
                ["Photosynthesis > Light reactions", "Photosynthesis > Calvin cycle"],
                ["Chlorophyll absorbs light.", "Carbon fixation follows."],
            ),
            (
                "# Fotosíntesis\n## Fase luminosa\nLa clorofila absorbe la luz.\n## Ciclo de Calvin\nSigue la fijación del carbono.",
                ["Fotosíntesis > Fase luminosa", "Fotosíntesis > Ciclo de Calvin"],
                ["La clorofila absorbe la luz.", "Sigue la fijación del carbono."],
            ),
        ] {
            let spans = structure_spans(text, &[], true).unwrap();

            assert_eq!(spans.len(), 2, "{text}");
            assert_eq!(
                spans
                    .iter()
                    .map(|span| &text[span.start..span.end])
                    .collect::<String>(),
                text
            );
            for ((span, breadcrumb), body) in spans.iter().zip(breadcrumbs).zip(bodies) {
                assert_eq!(span.heading.as_deref(), Some(breadcrumb));
                assert!(text[span.start..span.end].contains(body));
            }
        }
    }
}

#[cfg(test)]
mod live_tests;
