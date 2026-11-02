//! The course a tutor chat is bound to, opened for one turn.

use super::binding::conversation_course;
use super::cards::due_count;
use super::facts::replies_since_check;
use super::prompt::{self, TurnFacts, TUTOR_POLICY};
use super::tools;
use crate::features::learning::dto::{LearningLessonDto, LearningModuleDto, LearningProgramDto};
use crate::features::learning::persistence::now;
use crate::features::learning::references::course_index::CourseIndexes;
use crate::features::learning::repository::LearningRepository;
use async_trait::async_trait;
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::turn_source::{
    SourcePassage, TurnSourcePolicy, TurnSourcePort,
};
use lattice_core::application::ports::ToolDefinition;
use lattice_core::shared::error::Result;
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::OnceCell;
use tracing::warn;

/// Called after the tutor writes to a note, so the vault's mirror picks it up.
pub type NoteWritten = Arc<dyn Fn() + Send + Sync>;

pub struct CourseTurn {
    pub(super) pool: SqlitePool,
    pub(super) courses: Option<Arc<CourseIndexes>>,
    pub(super) conversation_id: String,
    pub(super) program_id: String,
    pub(super) lesson_id: Option<String>,
    pub(super) note_written: NoteWritten,
    program: OnceCell<LearningProgramDto>,
}

impl std::fmt::Debug for CourseTurn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CourseTurn")
            .field("conversation_id", &self.conversation_id)
            .field("program_id", &self.program_id)
            .field("lesson_id", &self.lesson_id)
            .finish()
    }
}

impl CourseTurn {
    /// `None` for a chat bound to no course. One whose binding cannot be read
    /// also gives `None`: the turn then runs as a plain chat rather than failing.
    pub async fn resolve(
        pool: &SqlitePool,
        courses: Option<Arc<CourseIndexes>>,
        conversation_id: &str,
        note_written: NoteWritten,
    ) -> Option<Self> {
        let binding = match conversation_course(pool, conversation_id).await {
            Ok(binding) => binding?,
            Err(error) => {
                warn!(%error, conversation_id, "Could not read the conversation's course");
                return None;
            }
        };
        Some(Self {
            pool: pool.clone(),
            courses,
            conversation_id: conversation_id.to_string(),
            program_id: binding.program_id,
            lesson_id: binding.lesson_id,
            note_written,
            program: OnceCell::new(),
        })
    }

    /// The course, read once a turn.
    pub(super) async fn program(&self) -> Result<&LearningProgramDto> {
        self.program
            .get_or_try_init(|| async {
                LearningRepository::new(self.pool.clone())
                    .get(&self.program_id)
                    .await
            })
            .await
    }

    /// The lesson this turn is about: the one the chat was opened from, else
    /// the learner's current one, else the first not finished.
    pub(super) async fn working_lesson(&self) -> Result<Option<String>> {
        if self.lesson_id.is_some() {
            return Ok(self.lesson_id.clone());
        }
        let program = self.program().await?;
        Ok(current_lesson(program).map(|(_, lesson)| lesson.id.clone()))
    }

    async fn facts(&self) -> TurnFacts {
        let program = match self.program().await {
            Ok(program) => program,
            Err(error) => {
                warn!(%error, "Could not read the tutor's course");
                return TurnFacts::default();
            }
        };
        let lessons = || program.modules.iter().flat_map(|module| &module.lessons);
        let cards_due = due_count(&self.pool, &self.program_id, now())
            .await
            .inspect_err(|error| warn!(%error, "Could not count the course's due cards"))
            .ok();
        let replies_since_check = replies_since_check(&self.pool, &self.conversation_id)
            .await
            .inspect_err(|error| warn!(%error, "Could not read the tutor's last check"))
            .ok();
        TurnFacts {
            title: program.summary.title.clone(),
            goal: program.summary.goal.clone(),
            lessons_finished: lessons().filter(|lesson| lesson.completed).count(),
            lessons: lessons().count(),
            current_lesson: current_lesson(program).map(|(module, lesson)| place(module, lesson)),
            focus: self.lesson_id.as_deref().and_then(|id| {
                program.modules.iter().find_map(|module| {
                    let lesson = module.lessons.iter().find(|lesson| lesson.id == id)?;
                    Some((place(module, lesson), lesson.objective.clone()))
                })
            }),
            cards_due,
            replies_since_check,
        }
    }
}

/// Where a lesson sits in its course, as a passage's label names it.
pub(super) fn place(module: &LearningModuleDto, lesson: &LearningLessonDto) -> String {
    format!("{} › {}", module.title, lesson.title)
}

fn current_lesson(
    program: &LearningProgramDto,
) -> Option<(&LearningModuleDto, &LearningLessonDto)> {
    let lessons = || {
        program
            .modules
            .iter()
            .flat_map(|module| module.lessons.iter().map(move |lesson| (module, lesson)))
    };
    let current = program.summary.current_lesson_id.as_deref();
    lessons()
        .find(|(_, lesson)| Some(lesson.id.as_str()) == current)
        .or_else(|| lessons().find(|(_, lesson)| !lesson.completed))
}

#[async_trait]
impl TurnSourcePort for CourseTurn {
    fn policy(&self) -> TurnSourcePolicy {
        TUTOR_POLICY
    }

    fn owns_tool(&self, tool: &str) -> bool {
        tools::is_course_tool(tool)
    }

    fn reads_only(&self, tool: &str) -> bool {
        tools::only_reads(tool)
    }

    fn activity_label(&self, tool: &str, arguments: &Value) -> String {
        tools::activity_label(tool, arguments)
    }

    fn add_tools(&self, turn_tools: &mut Vec<ToolDefinition>) {
        for definition in tools::definitions() {
            if !turn_tools.iter().any(|tool| tool.name == definition.name) {
                turn_tools.push(definition);
            }
        }
    }

    async fn context_block(
        &self,
        max_chars: usize,
        tools_offered: bool,
        _question: Option<&str>,
    ) -> String {
        prompt::render_context(&self.facts().await, tools_offered, max_chars)
    }

    async fn passages(&self, question: &str, max_chars: usize) -> Vec<SourcePassage> {
        self.first_passages(question, max_chars)
            .await
            .unwrap_or_else(|error| {
                warn!(%error, "Course search failed; the turn runs without passages");
                Vec::new()
            })
    }

    async fn execute_tool(&self, call: FunctionCall, max_chars: usize) -> Result<FunctionResult> {
        Ok(self.run_tool(call, max_chars).await)
    }
}
