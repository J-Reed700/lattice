//! The lesson a drill is built from: its course, its module, and what the lesson teaches, read from the course.

use crate::persistence::{db, decode};
use lattice_core::domain::learning::drills::{LessonBrief, LessonSentence};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{SqliteConnection, SqlitePool};

/// The kinds of block that teach: the ones a prepared lesson's evidence is cut from.
const TEACHING: [&str; 4] = ["explanation", "worked_example", "contrast", "recap"];

/// What a drill is built from in its course: the course, the module and the lesson, and what the lesson teaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LessonContext {
    pub program_id: String,
    pub lesson_id: String,
    pub course: String,
    pub goal: String,
    pub prior_knowledge: String,
    pub module: String,
    pub module_summary: String,
    pub outcomes: Vec<String>,
    pub lesson: String,
    pub objective: String,
    pub estimated_minutes: u32,
    /// Whether the lesson is prepared (`preparation = 'ready'`).
    pub prepared: bool,
    /// The bodies of a prepared lesson's teaching blocks (explanation, worked example, contrast, recap), in block
    /// order. Empty for a lesson that is not prepared.
    pub teaching: Vec<String>,
    /// The source versions the lesson's blocks cite (every kind of block), each once, in block order.
    pub cited_versions: Vec<String>,
}

impl LessonContext {
    /// The lesson `lesson_id` of the course `program_id`. An error when the lesson is not one of the course's.
    pub async fn load(pool: &SqlitePool, program_id: &str, lesson_id: &str) -> Result<Self> {
        type Row = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            String,
        );
        let row: Option<Row> = sqlx::query_as(
            "SELECT p.title,p.goal,p.prior_knowledge,m.title,m.summary,m.outcomes_json,l.title,l.objective,l.estimated_minutes,l.preparation FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id JOIN learning_programs p ON p.id=l.program_id WHERE l.id=? AND l.program_id=?",
        )
        .bind(lesson_id)
        .bind(program_id)
        .fetch_optional(pool)
        .await
        .map_err(db)?;
        let Some((
            course,
            goal,
            prior_knowledge,
            module,
            module_summary,
            outcomes_json,
            lesson,
            objective,
            minutes,
            preparation,
        )) = row
        else {
            return Err(AppError::InvalidInput(
                "The lesson is not one of the course's.".into(),
            ));
        };
        let profile_details: Option<String> =
            sqlx::query_scalar("SELECT details FROM learning_practice_profiles WHERE program_id=?")
                .bind(program_id)
                .fetch_optional(pool)
                .await
                .map_err(db)?;
        let goal = match profile_details.filter(|d| !d.trim().is_empty()) {
            Some(details) => format!("{goal} ({details})"),
            None => goal,
        };
        let blocks: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT kind,body,source_ids_json FROM learning_blocks WHERE lesson_id=? ORDER BY ordinal",
        )
        .bind(lesson_id)
        .fetch_all(pool)
        .await
        .map_err(db)?;
        let prepared = preparation == "ready";
        let mut teaching = Vec::new();
        let mut cited_versions: Vec<String> = Vec::new();
        for (kind, body, source_ids) in blocks {
            if prepared && TEACHING.contains(&kind.as_str()) {
                teaching.push(body);
            }
            for id in decode::<Vec<String>>(&source_ids)? {
                if !cited_versions.contains(&id) {
                    cited_versions.push(id);
                }
            }
        }
        Ok(Self {
            program_id: program_id.to_owned(),
            lesson_id: lesson_id.to_owned(),
            course,
            goal,
            prior_knowledge,
            module,
            module_summary,
            outcomes: decode(&outcomes_json)?,
            lesson,
            objective,
            estimated_minutes: u32::try_from(minutes).unwrap_or(0),
            prepared,
            teaching,
            cited_versions,
        })
    }

    /// What the course's sources are searched for: the lesson's title and objective, then the module's title and
    /// outcomes, one to a line. The search keeps only its first terms, so the order is the order of weight.
    pub fn query(&self) -> String {
        let mut lines = vec![
            self.lesson.as_str(),
            self.objective.as_str(),
            self.module.as_str(),
        ];
        lines.extend(self.outcomes.iter().map(String::as_str));
        lines.join("\n")
    }

    /// The brief of this lesson with `evidence`.
    pub fn brief(&self, evidence: Vec<LessonSentence>) -> LessonBrief {
        LessonBrief {
            lesson_id: self.lesson_id.clone(),
            course: self.course.clone(),
            goal: self.goal.clone(),
            prior_knowledge: self.prior_knowledge.clone(),
            module: self.module.clone(),
            module_summary: self.module_summary.clone(),
            outcomes: self.outcomes.clone(),
            lesson: self.lesson.clone(),
            objective: self.objective.clone(),
            evidence,
        }
    }
}

/// The bodies of every block of the course's prepared lessons, other than replaced or skipped ones.
pub(crate) async fn prepared_bodies_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT b.body FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE l.program_id=? AND l.preparation='ready' AND l.curriculum_state NOT IN ('replaced','skipped') ORDER BY l.id,b.ordinal",
    )
    .bind(program_id)
    .fetch_all(connection)
    .await
    .map_err(db)
}
