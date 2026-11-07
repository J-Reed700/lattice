//! Which passage a drill explains (docs/design/2026-11-05-spot-the-errors-drills.md section 5): one from the course's
//! source drilled least, that no drill of the course was built on, widened to whole paragraphs and sized to hold at
//! least as many sentences as the explanation, so that each sentence of it has something of its own to say. A topic
//! narrows the choice to the passages the course's sources hold about it, best first.

use crate::drills::build::SourcePassage;
use crate::drills::repository;
use crate::dto::LearningSourceDto;
use crate::persistence::db;
use crate::reference_collection::ReferenceCollection;
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::segmentation::{sentences, units, Prose, Strength};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// A place in a source a passage may start from.
struct Candidate<'a> {
    source: &'a LearningSourceDto,
    /// How many drills of the course were built on a passage of the same source.
    uses: usize,
    /// The place's rank: the search's for a topic, the source's place in the course and then the text's otherwise.
    order: (usize, usize),
    start: usize,
    end: usize,
}

/// The passage the next drill of the course `program_id` explains, about `topic` when one is given, sized for an
/// explanation of `wanted` sentences. An error when the course has no source, or no passage of its sources (about
/// the topic) is left that no drill was built on.
pub(super) async fn pick(
    pool: &SqlitePool,
    program_id: &str,
    topic: Option<&str>,
    wanted: u32,
) -> Result<SourcePassage> {
    let sources: Vec<LearningSourceDto> = LearningSourceLibraryRepository::new(pool.clone())
        .verification_sources(program_id)
        .await?
        .into_iter()
        .filter(|source| !source.excerpt.trim().is_empty())
        .collect();
    if sources.is_empty() {
        return Err(AppError::InvalidInput(
            "The course has no source to take a passage from.".into(),
        ));
    }
    let drilled = repository::drilled_passages(pool, program_id).await?;
    let owners = owners(pool, program_id).await?;
    let owner = |version: &str| -> String {
        owners
            .get(version)
            .cloned()
            .unwrap_or_else(|| version.to_owned())
    };
    let uses = |source: &LearningSourceDto| {
        let owned = owner(&source.id);
        drilled
            .iter()
            .filter(|passage| owner(&passage.version_id) == owned)
            .count()
    };
    let mut candidates: Vec<Candidate<'_>> = Vec::new();
    match topic {
        Some(topic) => {
            let collection = ReferenceCollection::lexical(&sources)?;
            let hits = collection.retrieve_within(topic, |_| true).await?;
            for (rank, hit) in hits.iter().enumerate() {
                if let Some(source) = sources.iter().find(|source| source.id == hit.source_id) {
                    candidates.push(Candidate {
                        source,
                        uses: uses(source),
                        order: (rank, 0),
                        start: hit.start_byte,
                        end: hit.end_byte,
                    });
                }
            }
        }
        None => {
            for (position, source) in sources.iter().enumerate() {
                let used = uses(source);
                for (index, paragraph) in units(&source.excerpt, &Prose, Strength::Paragraph)
                    .into_iter()
                    .enumerate()
                {
                    candidates.push(Candidate {
                        source,
                        uses: used,
                        order: (position, index),
                        start: paragraph.start,
                        end: paragraph.end,
                    });
                }
            }
        }
    }
    candidates.sort_by_key(|candidate| (candidate.uses, candidate.order));
    let wanted = usize::try_from(wanted).unwrap_or(usize::MAX);
    for candidate in candidates {
        let text = &candidate.source.excerpt;
        let Some((start, end)) = sized(text, candidate.start, candidate.end, wanted) else {
            continue;
        };
        let taken = drilled.iter().any(|passage| {
            passage.version_id == candidate.source.id
                && passage.start_byte < end
                && start < passage.end_byte
        });
        if taken {
            continue;
        }
        let Some(passage) = text.get(start..end) else {
            continue;
        };
        return Ok(SourcePassage {
            version_id: candidate.source.id.clone(),
            start_byte: start,
            end_byte: end,
            title: candidate.source.title.clone(),
            text: passage.to_owned(),
        });
    }
    Err(AppError::InvalidInput(match topic {
        Some(topic) => {
            format!("No passage of the course's sources about {topic} is left to drill.")
        }
        None => "No passage of the course's sources is left to drill.".to_owned(),
    }))
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

/// The passage of `text` around `start..end`: the whole paragraphs it touches, then the paragraphs after them and,
/// at the end of the text, before them, until the passage holds `wanted` sentences or the whole text; trimmed of the
/// space at its ends. `None` when there is nothing but space there.
fn sized(text: &str, start: usize, end: usize, wanted: usize) -> Option<(usize, usize)> {
    let paragraphs = units(text, &Prose, Strength::Paragraph);
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
