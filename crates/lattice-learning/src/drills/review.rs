//! What the review view's commands do, apart from Tauri: a course's drills, starting a drill, handing a review in,
//! reading an attempt and asking for its grade again (docs/design/2026-11-04-code-review-drills.md section 1 and
//! docs/design/2026-11-07-drills-in-the-course.md section 8). The jobs they queue are returned for the caller to tell
//! the job runtime of once they are committed.

use super::attempts::{self, AttemptMode, AttemptStatus, Submission};
use super::bank::{self, NextStatus, Shelf};
use super::dto::{
    CodeReviewFileDto, CodeReviewFindingDto, CodeReviewResultDto, CodeReviewScoreDto,
    CourseDrillAttemptDto, CourseDrillsDto, DrillLessonDto, DrillMaterialDto, DrillModuleDto,
    StartedCodeReviewDto,
};
use super::lesson::prepared_bodies_in;
use super::reveal::{learner_drill, lesson_of, result_of};
use super::schedules;
use super::spec::{check_language, check_request};
use crate::persistence::{db, now};
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::domain::learning::drills::{language_of_code, Material, LANGUAGES};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::HashMap;

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
    pool: &SqlitePool,
    asked: &CourseDrill,
    mode: AttemptMode,
    lines: u32,
    issues: u32,
) -> Result<Started> {
    let count = usize::try_from(issues).unwrap_or(usize::MAX);
    check_request(&asked.material, lines, count)?;
    if !course_exists(pool, &asked.program_id).await? {
        return Err(not_found());
    }
    if let Some(lesson) = asked.lesson_id.as_deref() {
        let mut connection = pool.acquire().await.map_err(db)?;
        if !schedules::drills_lesson_in(&mut connection, &asked.program_id, lesson).await? {
            return Err(AppError::InvalidInput(
                "The lesson is not one the course drills.".into(),
            ));
        }
    }
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

/// A course's review drills as its Practice view reads them, with code drills offered in `language` or, when none is
/// asked for, in the one the course suggests.
pub async fn course_drills(
    pool: &SqlitePool,
    program_id: &str,
    language: Option<&str>,
) -> Result<CourseDrillsDto> {
    if !course_exists(pool, program_id).await? {
        return Err(not_found());
    }
    let language = match language {
        Some(asked) => {
            check_language(asked)?;
            asked.to_owned()
        }
        None => {
            let suggested = {
                let mut connection = pool.acquire().await.map_err(db)?;
                suggested_language_in(&mut connection, program_id).await?
            };
            match suggested {
                Some(suggested) => suggested,
                None => LANGUAGES
                    .first()
                    .map(|first| (*first).to_owned())
                    .ok_or_else(|| {
                        AppError::InternalError("No language is in the catalogue.".into())
                    })?,
            }
        }
    };
    let with_sources = has_sources(pool, program_id).await?;
    let code = Material::code(language.clone());
    let at = now();
    let mut transaction = pool.begin().await.map_err(db)?;
    let standings = schedules::lesson_states_in(&mut transaction, program_id, &code, None).await?;
    let mut fits = Vec::with_capacity(standings.len());
    for standing in &standings {
        fits.push(
            schedules::lesson_fits_in(
                &mut transaction,
                program_id,
                &standing.lesson_id,
                &Material::Passage,
            )
            .await?,
        );
    }
    let titles: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT l.id,l.title,m.id,m.title FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=?",
    )
    .bind(program_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(db)?;
    let next_code = bank::next_up_in(
        &mut transaction,
        Shelf {
            program_id,
            material: &code,
        },
        at,
    )
    .await?;
    let next_passage = bank::next_up_in(
        &mut transaction,
        Shelf {
            program_id,
            material: &Material::Passage,
        },
        at,
    )
    .await?;
    let recent = attempts::recent_in(&mut transaction, program_id, RECENT_ATTEMPTS).await?;
    transaction.commit().await.map_err(db)?;

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

/// Whether the course is there.
async fn course_exists(pool: &SqlitePool, program_id: &str) -> Result<bool> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM learning_programs WHERE id=?)")
        .bind(program_id)
        .fetch_one(pool)
        .await
        .map_err(db)
}

/// Whether the course has a source with text, which a passage drill's passage is taken from.
async fn has_sources(pool: &SqlitePool, program_id: &str) -> Result<bool> {
    let sources = LearningSourceLibraryRepository::new(pool.clone())
        .verification_sources(program_id)
        .await?;
    Ok(sources
        .iter()
        .any(|source| !source.excerpt.trim().is_empty()))
}

/// The language a code drill of the course is suggested in: the one its latest code drill was taken in, which keeps the
/// learner's choice; otherwise the catalogue language most of the fenced code in its prepared lessons is written in;
/// otherwise the one the learner's latest code drill of any course was taken in.
async fn suggested_language_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<String>> {
    let profile_language: Option<String> =
        sqlx::query_scalar("SELECT language FROM learning_practice_profiles WHERE program_id=?")
            .bind(program_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
    if let Some(language) = profile_language.filter(|language| check_language(language).is_ok()) {
        return Ok(Some(language));
    }
    let in_course: Option<String> = sqlx::query_scalar(
        "SELECT d.language FROM learning_drill_attempts a JOIN learning_drills d ON d.id=a.drill_id WHERE d.program_id=? AND d.material='code' ORDER BY a.started_at DESC,a.rowid DESC LIMIT 1",
    )
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    if let Some(language) = in_course.filter(|language| check_language(language).is_ok()) {
        return Ok(Some(language));
    }
    let bodies = prepared_bodies_in(connection, program_id).await?;
    let texts: Vec<&str> = bodies.iter().map(String::as_str).collect();
    if let Some(language) = language_of_code(&texts) {
        return Ok(Some(language.to_owned()));
    }
    let anywhere: Option<String> = sqlx::query_scalar(
        "SELECT d.language FROM learning_drill_attempts a JOIN learning_drills d ON d.id=a.drill_id WHERE d.material='code' ORDER BY a.started_at DESC,a.rowid DESC LIMIT 1",
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    Ok(anywhere.filter(|language| check_language(language).is_ok()))
}

/// Hands in an open attempt: the learner's findings and, when they wrote one, their fix, with the time they spent as the
/// client measured it. The grading job is queued.
pub async fn submit(
    pool: &SqlitePool,
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
pub async fn result(pool: &SqlitePool, attempt_id: &str) -> Result<CodeReviewResultDto> {
    let (attempt, stored) = attempts::get(pool, attempt_id).await?;
    result_of(&attempt, &stored)
}

/// Asks for the grade of an attempt that could not be graded again. The grading job is queued.
pub async fn regrade(pool: &SqlitePool, attempt_id: &str) -> Result<Acted> {
    let queued = attempts::regrade(pool, attempt_id).await?;
    acted(pool, &queued.attempt.id, queued.job).await
}

async fn acted(pool: &SqlitePool, attempt_id: &str, job: Option<String>) -> Result<Acted> {
    Ok(Acted {
        dto: result(pool, attempt_id).await?,
        job,
    })
}
