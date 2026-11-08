//! Which passage a drill explains (docs/design/2026-11-07-drills-in-the-course.md section 4): one the course's sources
//! hold for the drill's lesson, found by searching them for the lesson's title, objective and module, widened to whole
//! paragraphs and sized to hold at least as many sentences as the explanation, so that each sentence of it has
//! something of its own to say. The best-ranked passage no drill of the course was built on comes first; once every
//! one was, the one whose latest drill is the oldest. A prepared lesson draws only on the sources its blocks cite.

use crate::drills::build::SourcePassage;
use crate::drills::lesson::LessonContext;
use crate::drills::repository;
use crate::dto::LearningSourceDto;
use crate::persistence::db;
use crate::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::shared::error::Result;
use lattice_core::shared::text::segmentation::{sentences, units, Prose, Span, Strength};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// The passage the next drill of the lesson of `context` explains, sized for an explanation of `wanted` sentences:
/// the best-ranked passage for the lesson that no drill of the course was built on, or, once every one was, the one
/// whose latest drill is the oldest. The ranking is read a page at a time, deeper only while every passage of it was
/// drilled. `None` when the lesson's sources hold nothing for it.
pub(super) async fn pick(
    pool: &SqlitePool,
    context: &LessonContext,
    wanted: u32,
) -> Result<Option<SourcePassage>> {
    let owners = owners(pool, &context.program_id).await?;
    let owner = |version: &str| -> String {
        owners
            .get(version)
            .cloned()
            .unwrap_or_else(|| version.to_owned())
    };
    let cited: Vec<String> = context
        .cited_versions
        .iter()
        .map(|version| owner(version))
        .collect();
    let sources: Vec<LearningSourceDto> = LearningSourceLibraryRepository::new(pool.clone())
        .verification_sources(&context.program_id)
        .await?
        .into_iter()
        .filter(|source| !source.excerpt.trim().is_empty())
        .filter(|source| !context.prepared || cited.contains(&owner(&source.id)))
        .collect();
    if sources.is_empty() {
        return Ok(None);
    }
    let drilled = repository::drilled_passages(pool, &context.program_id).await?;
    let wanted = usize::try_from(wanted).unwrap_or(usize::MAX);
    let collection = ReferenceCollection::lexical(&sources)?;
    let query = context.query();
    let mut split: HashMap<&str, Vec<Span>> = HashMap::new();
    let mut depth = FIRST_PAGE;
    loop {
        let hits = collection.retrieve(&query, depth).await?;
        let best = best_on_page(&hits, &sources, &mut split, &drilled, wanted);
        let unused = best.as_ref().is_some_and(|(key, _)| key.0.is_none());
        if unused || hits.len() < depth {
            return Ok(best.map(|(_, passage)| passage));
        }
        depth = depth.saturating_mul(2);
    }
}

/// How many passages of the ranking the first page holds; each further page doubles it.
const FIRST_PAGE: usize = 16;

/// A candidate passage's place in the ranking: when a drill last used it (`None` for never), then its hit's rank and
/// its paragraph's index, the least of which is the best.
type Key = (Option<usize>, (usize, usize));

/// The best candidate among the paragraphs the ranked `hits` overlap, or the first one no drill used, which is the
/// best of the whole ranking. The paragraphs of each source are kept in `split` across pages, so each source is split
/// once per pick.
fn best_on_page<'a>(
    hits: &[ReferencePassage],
    sources: &'a [LearningSourceDto],
    split: &mut HashMap<&'a str, Vec<Span>>,
    drilled: &[SourcePassage],
    wanted: usize,
) -> Option<(Key, SourcePassage)> {
    let mut best: Option<(Key, SourcePassage)> = None;
    for (rank, hit) in hits.iter().enumerate() {
        let Some(source) = sources.iter().find(|source| source.id == hit.source_id) else {
            continue;
        };
        let places: &[Span] = split
            .entry(source.id.as_str())
            .or_insert_with(|| units(&source.excerpt, &Prose, Strength::Paragraph));
        for (index, paragraph) in places.iter().enumerate() {
            if paragraph.end <= hit.start_byte || hit.end_byte <= paragraph.start {
                continue;
            }
            let Some((start, end)) = sized(
                &source.excerpt,
                places,
                paragraph.start,
                paragraph.end,
                wanted,
            ) else {
                continue;
            };
            let Some(text) = source.excerpt.get(start..end) else {
                continue;
            };
            let latest_use = drilled
                .iter()
                .enumerate()
                .filter(|(_, passage)| {
                    passage.version_id == source.id
                        && passage.start_byte < end
                        && start < passage.end_byte
                })
                .map(|(position, _)| position)
                .max();
            let key = (latest_use, (rank, index));
            if best.as_ref().is_some_and(|(held, _)| *held <= key) {
                continue;
            }
            let found = (
                key,
                SourcePassage {
                    version_id: source.id.clone(),
                    start_byte: start,
                    end_byte: end,
                    title: source.title.clone(),
                    text: text.to_owned(),
                },
            );
            if latest_use.is_none() {
                return Some(found);
            }
            best = Some(found);
        }
    }
    best
}

/// For each version of the course's sources, the source it is a version of.
async fn owners(pool: &SqlitePool, program_id: &str) -> Result<HashMap<String, String>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id,source_id FROM learning_source_versions WHERE program_id=?")
            .bind(program_id)
            .fetch_all(pool)
            .await
            .map_err(db)?;
    Ok(rows.into_iter().collect())
}

/// The passage of `text`, whose paragraphs are `paragraphs`, around `start..end`: the whole paragraphs it touches,
/// then the paragraphs after them and, at the end of the text, before them, until the passage holds `wanted`
/// sentences or the whole text; trimmed of the space at its ends. `None` when there is nothing but space there.
fn sized(
    text: &str,
    paragraphs: &[Span],
    start: usize,
    end: usize,
    wanted: usize,
) -> Option<(usize, usize)> {
    let mut first = paragraphs
        .iter()
        .position(|paragraph| paragraph.end > start)?;
    let mut last = paragraphs
        .iter()
        .rposition(|paragraph| paragraph.start < end.max(start.saturating_add(1)))?
        .max(first);
    let span = |first: usize, last: usize| -> Option<(usize, usize)> {
        Some((paragraphs.get(first)?.start, paragraphs.get(last)?.end))
    };
    loop {
        let (from, to) = span(first, last)?;
        if sentences(text.get(from..to)?).len() >= wanted {
            break;
        }
        if last + 1 < paragraphs.len() {
            last += 1;
        } else if first > 0 {
            first -= 1;
        } else {
            break;
        }
    }
    let (from, to) = span(first, last)?;
    let piece = text.get(from..to)?;
    let trimmed = piece.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lead = piece.len() - piece.trim_start().len();
    Some((from + lead, from + lead + trimmed.len()))
}
