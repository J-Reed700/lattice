//! What the review view's commands do, apart from Tauri: a course's drills, starting a drill, handing a review in,
//! reading an attempt and asking for its grade again (docs/design/2026-11-04-code-review-drills.md section 1 and
//! docs/design/2026-11-07-drills-in-the-course.md section 8). The jobs they queue are returned for the caller to tell
//! the job runtime of once they are committed.

use super::attempts::{self, Attempt, AttemptMode, AttemptStatus, Submission};
use super::bank::{self, NextStatus, NextUp, Shelf};
use super::dto::{
    CodeReviewFileDto, CodeReviewFindingDto, CodeReviewResultDto, CodeReviewScoreDto,
    CourseDrillAttemptDto, CourseDrillsDto, DrillLessonDto, DrillMaterialDto, DrillModuleDto,
    StartedCodeReviewDto,
};
use super::lesson::repository::prepared_bodies_in;
use super::repository::StoredDrill;
use super::reveal::{learner_drill, lesson_of, result_of};
use super::schedules;
use super::spec::{check_language, check_request};
use crate::persistence::{db, now};
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::domain::learning::drills::{
    language_of_code, LessonStanding, Material, LANGUAGES,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{DatabaseHandle, ReadTx, Transactions};
use std::collections::HashMap;

mod repository;

/// The most of a course's attempts its drills view lists.
const RECENT_ATTEMPTS: u32 = 10;

/// What starting a drill came to, and the jobs to tell the job runtime of.
#[derive(Debug, Clone, PartialEq)]
pub struct Started {
    pub dto: StartedCodeReviewDto,
    pub jobs: Vec<String>,
}

/// An attempt as it stands after the learner acted on it, and the job to tell the job runtime of, if one was queued.
#[derive(Debug, Clone, PartialEq)]
pub struct Acted {
    pub dto: CodeReviewResultDto,
    pub job: Option<String>,
}

/// A drill a learner asks for: of a course, on a material, built from the lesson they name or, when they name none,
/// from any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CourseDrill {
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub material: Material,
}

/// Starts a drill on `asked`: opens an attempt on the oldest ready drill of its shelf built from the lesson asked
/// for, or from any lesson when none is, which can queue the build of the next. When none is ready, the build under
/// way is found or one is queued, of the lesson asked for or of the one next up, and the learner is told it is under
/// way; `lines` (lines of code, or sentences) and `issues` size it, its patterns the ones the schedule offers. A
/// request of no size, a lesson the course does not drill, and a course with no lesson to drill on the material are
/// refused, and nothing is taken, as is a course that is not there and, for a passage with none ready, a course whose
/// sources hold no text.
pub async fn start(
    pool: &impl DatabaseHandle,
    asked: &CourseDrill,
    mode: AttemptMode,
    lines: u32,
    issues: u32,
) -> Result<Started> {
    let count = usize::try_from(issues).unwrap_or(usize::MAX);
    check_request(&asked.material, lines, count)?;
    check_asked(pool, asked).await?;
    let shelf = Shelf {
        program_id: &asked.program_id,
        material: &asked.material,
    };
    if let Some(begun) = attempts::start(pool, shelf, asked.lesson_id.as_deref(), mode).await? {
        return Ok(Started {
            dto: StartedCodeReviewDto::Started {
                attempt_id: begun.attempt.id.clone(),
                mode: begun.attempt.mode,
                started_at: begun.attempt.started_at,
                drill: learner_drill(&begun.drill),
            },
            jobs: begun.next_build.into_iter().collect(),
        });
    }
    if asked.material == Material::Passage && !has_sources(pool, &asked.program_id).await? {
        return Err(AppError::InvalidInput(
            "The course has no source a passage can be taken from.".into(),
        ));
    }
    let job = bank::ensure_build(pool, shelf, asked.lesson_id.as_deref(), lines, count).await?;
    Ok(Started {
        dto: StartedCodeReviewDto::Building {
            job_id: job.clone(),
        },
        jobs: vec![job],
    })
}

/// Refuses a course that is not there and a lesson the course does not drill.
async fn check_asked(pool: &impl DatabaseHandle, asked: &CourseDrill) -> Result<()> {
    let mut tx = pool.begin_read().await.map_err(db)?;
    if !repository::course_exists_in(&mut tx, &asked.program_id).await? {
        return Err(not_found());
    }
    if let Some(lesson) = asked.lesson_id.as_deref() {
        if !schedules::repository::drills_lesson_in(&mut tx, &asked.program_id, lesson).await? {
            return Err(AppError::InvalidInput(
                "The lesson is not one the course drills.".into(),
            ));
        }
    }
    tx.commit().await.map_err(db)?;
    Ok(())
}

/// What a course's drills view reads of the database, in one snapshot.
struct CourseRead {
    language: String,
    standings: Vec<LessonStanding>,
    fits: Vec<bool>,
    titles: Vec<(String, String, String, String)>,
    next_code: Option<NextUp>,
    next_passage: Option<NextUp>,
    recent: Vec<(Attempt, StoredDrill)>,
}

/// Reads the course's drills state in one read unit; the course not being there is refused.
async fn read_course(
    pool: &impl DatabaseHandle,
    program_id: &str,
    language: Option<&str>,
    at: i64,
) -> Result<CourseRead> {
    let mut tx = pool.begin_read().await.map_err(db)?;
    if !repository::course_exists_in(&mut tx, program_id).await? {
        return Err(not_found());
    }
    let language = match language {
        Some(asked) => {
            check_language(asked)?;
            asked.to_owned()
        }
        None => match suggested_language_in(&mut tx, program_id).await? {
            Some(suggested) => suggested,
            None => LANGUAGES
                .first()
                .map(|first| (*first).to_owned())
                .ok_or_else(|| {
                    AppError::InternalError("No language is in the catalogue.".into())
                })?,
        },
    };
    let code = Material::code(language.clone());
    let standings = schedules::lesson_states_in(&mut tx, program_id, &code, None).await?;
    let mut fits = Vec::with_capacity(standings.len());
    for standing in &standings {
        fits.push(
            schedules::lesson_fits_in(&mut tx, program_id, &standing.lesson_id, &Material::Passage)
                .await?,
        );
    }
    let titles = repository::lesson_titles_in(&mut tx, program_id).await?;
    let next_code = bank::next_up_in(
        &mut tx,
        Shelf {
            program_id,
            material: &code,
        },
        at,
    )
    .await?;
    let next_passage = bank::next_up_in(
        &mut tx,
        Shelf {
            program_id,
            material: &Material::Passage,
        },
        at,
    )
    .await?;
    let recent = attempts::recent_in(&mut tx, program_id, RECENT_ATTEMPTS).await?;
    tx.commit().await.map_err(db)?;
    Ok(CourseRead {
        language,
        standings,
        fits,
        titles,
        next_code,
        next_passage,
        recent,
    })
}

/// A course's review drills as its Practice view reads them, with code drills offered in `language` or, when none is
/// asked for, in the one the course suggests.
pub async fn course_drills(
    pool: &impl DatabaseHandle,
    program_id: &str,
    language: Option<&str>,
) -> Result<CourseDrillsDto> {
    let at = now();
    let CourseRead {
        language,
        standings,
        fits,
        titles,
        next_code,
        next_passage,
        recent,
    } = read_course(pool, program_id, language, at).await?;
    let with_sources = has_sources(pool, program_id).await?;

    let titles: HashMap<String, (String, String, String)> = titles
        .into_iter()
        .map(|(lesson_id, title, module_id, module_title)| {
            (lesson_id, (title, module_id, module_title))
        })
        .collect();
    let mut modules: Vec<DrillModuleDto> = Vec::new();
    for (standing, fits_passage) in standings.iter().zip(fits) {
        let Some((title, module_id, module_title)) = titles.get(&standing.lesson_id) else {
            continue;
        };
        let lesson = DrillLessonDto {
            lesson_id: standing.lesson_id.clone(),
            title: title.clone(),
            standing: standing.standing(at).into(),
            due_at: standing.due_at,
            fits_code: standing.fits,
            fits_passage,
            current: standing.current,
            completed: standing.completed,
        };
        let same_module = modules
            .last()
            .is_some_and(|module| module.module_id == *module_id);
        match modules.last_mut() {
            Some(module) if same_module => module.lessons.push(lesson),
            _ => modules.push(DrillModuleDto {
                module_id: module_id.clone(),
                title: module_title.clone(),
                lessons: vec![lesson],
            }),
        }
    }
    Ok(CourseDrillsDto {
        language,
        next_code: next_code.map(Into::into),
        next_passage: next_passage
            .filter(|next| with_sources || next.status != NextStatus::ToBuild)
            .map(Into::into),
        has_sources: with_sources,
        modules,
        recent: recent
            .iter()
            .map(|(attempt, stored)| CourseDrillAttemptDto {
                attempt_id: attempt.id.clone(),
                status: attempt.status,
                mode: attempt.mode,
                started_at: attempt.started_at,
                title: stored.drill.title.clone(),
                material: DrillMaterialDto::from(&stored.drill.material),
                lesson: lesson_of(stored),
                score: match attempt.status {
                    AttemptStatus::Graded => attempt.score.as_ref().map(CodeReviewScoreDto::from),
                    _ => None,
                },
            })
            .collect(),
    })
}

fn not_found() -> AppError {
    AppError::NotFound("Learning program not found.".into())
}

/// Whether the course has a source with text, which a passage drill's passage is taken from.
async fn has_sources(pool: &impl DatabaseHandle, program_id: &str) -> Result<bool> {
    let sources = LearningSourceLibraryRepository::new(pool.database())
        .verification_sources(program_id)
        .await?;
    Ok(sources
        .iter()
        .any(|source| !source.excerpt.trim().is_empty()))
}

/// The language a code drill of the course is suggested in: the one its latest code drill was taken in, which keeps the
/// learner's choice; otherwise the catalogue language most of the fenced code in its prepared lessons is written in;
/// otherwise the one the learner's latest code drill of any course was taken in.
async fn suggested_language_in(tx: &mut ReadTx, program_id: &str) -> Result<Option<String>> {
    let in_course = repository::course_language_in(tx, program_id).await?;
    if let Some(language) = in_course.filter(|language| check_language(language).is_ok()) {
        return Ok(Some(language));
    }
    let bodies = prepared_bodies_in(tx, program_id).await?;
    let texts: Vec<&str> = bodies.iter().map(String::as_str).collect();
    if let Some(language) = language_of_code(&texts) {
        return Ok(Some(language.to_owned()));
    }
    let anywhere = repository::latest_language_in(tx).await?;
    Ok(anywhere.filter(|language| check_language(language).is_ok()))
}

/// Hands in an open attempt: the learner's findings and, when they wrote one, their fix, with the time they spent as the
/// client measured it. The grading job is queued.
pub async fn submit(
    pool: &impl DatabaseHandle,
    attempt_id: &str,
    findings: Vec<CodeReviewFindingDto>,
    fix_files: Option<Vec<CodeReviewFileDto>>,
    time_spent_ms: u64,
) -> Result<Acted> {
    let time_spent_ms = i64::try_from(time_spent_ms)
        .map_err(|_| AppError::InvalidInput("The time spent is not a time.".into()))?;
    let queued = attempts::submit(
        pool,
        attempt_id,
        Submission {
            findings: findings.into_iter().map(Into::into).collect(),
            fix_files: fix_files.map(|files| files.into_iter().map(Into::into).collect()),
            time_spent_ms,
        },
    )
    .await?;
    acted(pool, &queued.attempt.id, queued.job).await
}

/// The attempt: where it stands and, once it is graded, the reveal.
pub async fn result(pool: &impl DatabaseHandle, attempt_id: &str) -> Result<CodeReviewResultDto> {
    let (attempt, stored) = attempts::get(pool, attempt_id).await?;
    result_of(&attempt, &stored)
}

/// Asks for the grade of an attempt that could not be graded again. The grading job is queued.
pub async fn regrade(pool: &impl DatabaseHandle, attempt_id: &str) -> Result<Acted> {
    let queued = attempts::regrade(pool, attempt_id).await?;
    acted(pool, &queued.attempt.id, queued.job).await
}

async fn acted(pool: &impl DatabaseHandle, attempt_id: &str, job: Option<String>) -> Result<Acted> {
    Ok(Acted {
        dto: result(pool, attempt_id).await?,
        job,
    })
}
