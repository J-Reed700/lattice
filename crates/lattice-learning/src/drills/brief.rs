//! The evidence a lesson brief carries: a prepared lesson's own teaching, or what the course's sources say of an
//! outline lesson, as much as the request's room and the lesson's planned minutes allow.

use super::lesson::LessonContext;
use crate::dto::LearningSourceDto;
use crate::model_call;
use crate::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::{evidence_units, LessonBrief, LessonSentence};
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// The mean silent reading rate of adults for non-fiction, in words a minute (Brysbaert, 2019): an outline lesson's
/// evidence is as much as its planned minutes take to read.
const READING_WORDS_PER_MINUTE: usize = 238;

/// The brief of the lesson of `context`, with as much evidence as `room` tokens hold, counted by `llm` over the
/// rendered brief. A prepared lesson's evidence is a prefix of the units of its teaching. An outline lesson's is the
/// units of the course's source passages for the lesson, best first, as many passages as its planned minutes take to
/// read, and always the first. An error when the brief without evidence, or its first unit, does not fit.
pub(crate) async fn lesson_brief(
    llm: &dyn LLMPort,
    pool: &SqlitePool,
    context: &LessonContext,
    room: usize,
) -> Result<LessonBrief> {
    let fits = |evidence: &[LessonSentence]| {
        llm.count_tokens(&context.brief(evidence.to_vec()).render()) <= room
    };
    if !fits(&[]) {
        return Err(model_call::window_too_small(
            llm,
            "The lesson's brief does not fit",
        ));
    }
    let evidence = if context.prepared {
        let units: Vec<LessonSentence> = context
            .teaching
            .iter()
            .flat_map(|body| evidence_units(body))
            .map(|text| LessonSentence { text, source: None })
            .collect();
        let any = !units.is_empty();
        let mut taken: Vec<LessonSentence> = Vec::new();
        for unit in units {
            taken.push(unit);
            if !fits(&taken) {
                taken.pop();
                break;
            }
        }
        if any && taken.is_empty() {
            return Err(model_call::window_too_small(
                llm,
                "The lesson's first sentence does not fit",
            ));
        }
        taken
    } else {
        outline_evidence(llm, pool, context, &fits).await?
    };
    Ok(context.brief(evidence))
}

/// The units of the course's source passages for an outline lesson, best first, as many passages as the planned
/// minutes take to read and `fits` accepts, and always the first.
async fn outline_evidence(
    llm: &dyn LLMPort,
    pool: &SqlitePool,
    context: &LessonContext,
    fits: &impl Fn(&[LessonSentence]) -> bool,
) -> Result<Vec<LessonSentence>> {
    let sources: Vec<LearningSourceDto> = LearningSourceLibraryRepository::new(pool.clone())
        .verification_sources(&context.program_id)
        .await?
        .into_iter()
        .filter(|source| !source.excerpt.trim().is_empty())
        .collect();
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let collection = ReferenceCollection::lexical(&sources)?;
    let query = context.query();
    let reading = usize::try_from(context.estimated_minutes)
        .unwrap_or(usize::MAX)
        .saturating_mul(READING_WORDS_PER_MINUTE);
    let units_of = |passages: &[ReferencePassage]| -> Vec<LessonSentence> {
        passages
            .iter()
            .flat_map(|passage| {
                let title = sources
                    .iter()
                    .find(|source| source.id == passage.source_id)
                    .map(|source| source.title.clone());
                evidence_units(&passage.text)
                    .into_iter()
                    .map(move |text| LessonSentence {
                        text,
                        source: title.clone(),
                    })
            })
            .collect()
    };
    let passages = collection
        .retrieve_within(&query, |passages| {
            let words: usize = passages
                .iter()
                .map(|passage| passage.text.split_whitespace().count())
                .sum();
            (passages.len() <= 1 || words <= reading) && fits(&units_of(passages))
        })
        .await?;
    if passages.is_empty() {
        if collection.retrieve(&query, 1).await?.is_empty() {
            return Ok(Vec::new());
        }
        return Err(model_call::window_too_small(
            llm,
            "The first passage the course's sources hold for the lesson does not fit",
        ));
    }
    Ok(units_of(&passages))
}
