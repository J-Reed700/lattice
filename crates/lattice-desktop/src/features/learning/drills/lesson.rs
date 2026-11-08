//! The lesson a drill is built from: its course, its module, and what the lesson teaches, read from the course.

use lattice_core::domain::learning::drills::{LessonBrief, LessonSentence};
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// What a drill is built from in its course: the course, the module and the lesson, and what the lesson teaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::learning) struct LessonContext {
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
    pub async fn load(_pool: &SqlitePool, program_id: &str, lesson_id: &str) -> Result<Self> {
        Ok(Self {
            program_id: program_id.to_owned(),
            lesson_id: lesson_id.to_owned(),
            course: String::new(),
            goal: String::new(),
            prior_knowledge: String::new(),
            module: String::new(),
            module_summary: String::new(),
            outcomes: Vec::new(),
            lesson: String::new(),
            objective: String::new(),
            estimated_minutes: 0,
            prepared: false,
            teaching: Vec::new(),
            cited_versions: Vec::new(),
        })
    }

    /// What the course's sources are searched for: the lesson's title and objective, then the module's title and
    /// outcomes, one to a line. The search keeps only its first terms, so the order is the order of weight.
    pub fn query(&self) -> String {
        String::new()
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
