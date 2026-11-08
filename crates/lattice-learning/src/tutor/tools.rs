//! The tools a tutor turn adds to the chat's. They are not in the process-wide
//! function registry: the course is the conversation's, so the program comes
//! from the turn and never from the arguments.
//!
//! `search_course` and `read_course_source` return passages, which the turn
//! numbers; the others return plain text, cut to the room this call has.

use super::cards::NewCard;
use super::course_turn::CourseTurn;
use crate::assessment_repository::LearningAssessmentRepository;
use crate::dto::{LearningQuizStatusDto, LearningQuizVerdict};
use crate::recall::study_dto::StudyRating;
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::application::contracts::sources::COURSE_SOURCE_PREFIX;
use lattice_core::application::contracts::tools::explorer::{arg_str, arg_u32};
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::ToolDefinition;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::safe_truncate;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Write as _;
use tracing::warn;

const TOOL_SEARCH_COURSE: &str = "search_course";
const TOOL_READ_COURSE_SOURCE: &str = "read_course_source";
const TOOL_COURSE_OUTLINE: &str = "course_outline";
const TOOL_DUE_REVIEWS: &str = "due_reviews";
const TOOL_RECORD_RECALL: &str = "record_recall";
const TOOL_SAVE_CARD: &str = "save_card";
const TOOL_SAVE_NOTE: &str = "save_note";

pub(super) fn is_course_tool(name: &str) -> bool {
    matches!(
        name,
        TOOL_SEARCH_COURSE
            | TOOL_READ_COURSE_SOURCE
            | TOOL_COURSE_OUTLINE
            | TOOL_DUE_REVIEWS
            | TOOL_RECORD_RECALL
            | TOOL_SAVE_CARD
            | TOOL_SAVE_NOTE
    )
}

/// The tools that write run alone, in the order the model wrote them.
pub(super) fn only_reads(name: &str) -> bool {
    matches!(
        name,
        TOOL_SEARCH_COURSE | TOOL_READ_COURSE_SOURCE | TOOL_COURSE_OUTLINE | TOOL_DUE_REVIEWS
    )
}

pub(super) fn activity_label(tool: &str, arguments: &Value) -> String {
    match tool {
        TOOL_SEARCH_COURSE => match arg_str(arguments, "query") {
            Some(query) => format!("Searched the course for \"{query}\""),
            None => "Searched the course".to_string(),
        },
        TOOL_READ_COURSE_SOURCE => "Read a course source".to_string(),
        TOOL_COURSE_OUTLINE => "Read the course outline".to_string(),
        TOOL_DUE_REVIEWS => "Checked the cards due".to_string(),
        TOOL_RECORD_RECALL => "Rated a recall card".to_string(),
        TOOL_SAVE_CARD => "Saved a recall card".to_string(),
        TOOL_SAVE_NOTE => "Saved a note to the Notebook".to_string(),
        other => format!("Running {other}"),
    }
}

pub(super) fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: TOOL_SEARCH_COURSE.to_string(),
            description: "Search the course's sources. Returns passages labelled module › lesson \
                 › source, each under the number to cite it by, with the source_id to read it \
                 further."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to find, in plain words." }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: TOOL_READ_COURSE_SOURCE.to_string(),
            description: "Read a section of one of the course's sources, as much as this turn \
                 has room for, under the number to cite it by. Says where to read on."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source_id": { "type": "string", "description": "The source_id a passage or the outline names." },
                    "offset": { "type": "integer", "description": "The character to start from. Omit to start at the beginning." }
                },
                "required": ["source_id"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: TOOL_COURSE_OUTLINE.to_string(),
            description: "The course's modules and lessons, which the learner finished, their \
                 quiz results, and the sources."
                .to_string(),
            parameters: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: TOOL_DUE_REVIEWS.to_string(),
            description: "This course's recall cards that are due, mixed across its modules. \
                 Fronts only: ask the learner, then rate their answer with record_recall."
                .to_string(),
            parameters: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: TOOL_RECORD_RECALL.to_string(),
            description: "Rate the learner's answer to a card into its review schedule, the same \
                 schedule the Recall tab uses. Only for a card you asked: the turn's facts and \
                 due_reviews give its card_id. The rating comes from the learner's answer: again \
                 if they missed it, hard if they got it with difficulty, good if they recalled \
                 it, easy if it was effortless. When the answer alone does not settle it, ask \
                 them how it went first."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "card_id": { "type": "string" },
                    "rating": { "type": "string", "enum": ["again", "hard", "good", "easy"] },
                    "selected_option": { "type": "integer", "description": "For a multiple-choice card: the number of the option the learner chose, which decides the rating." }
                },
                "required": ["card_id", "rating"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: TOOL_SAVE_CARD.to_string(),
            description: "Add a recall card to the course's deck, citing where it comes from. \
                 The card is due at once and shows in the Recall tab."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string" },
                    "answer": { "type": "string" },
                    "explanation": { "type": "string", "description": "Why the answer holds. Optional." },
                    "source_ids": { "type": "array", "items": { "type": "string" }, "description": "The source_id of each passage the card was taught from." }
                },
                "required": ["question", "answer", "source_ids"],
                "additionalProperties": false
            }),
        },
        ToolDefinition {
            name: TOOL_SAVE_NOTE.to_string(),
            description: "Save the learner's summary, in their words, to the course Notebook on \
                 a lesson's note."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "summary": { "type": "string", "description": "The learner's own words." },
                    "lesson_id": { "type": "string", "description": "The lesson the outline names. Omit for the lesson this chat is on." }
                },
                "required": ["summary"],
                "additionalProperties": false
            }),
        },
    ]
}

fn required<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arg_str(arguments, key)
        .ok_or_else(|| AppError::InvalidInput(format!("Pass {key}: it is required.")))
}

fn version_id(text: &str) -> &str {
    text.strip_prefix(COURSE_SOURCE_PREFIX).unwrap_or(text)
}

fn rating(arguments: &Value) -> Result<StudyRating> {
    match required(arguments, "rating")?.to_lowercase().as_str() {
        "again" => Ok(StudyRating::Again),
        "hard" => Ok(StudyRating::Hard),
        "good" => Ok(StudyRating::Good),
        "easy" => Ok(StudyRating::Easy),
        _ => Err(AppError::InvalidInput(
            "rating is one of again, hard, good or easy.".into(),
        )),
    }
}

fn text_result(text: String, max_chars: usize) -> Value {
    Value::String(safe_truncate(&text, max_chars))
}

fn quiz_line(quiz: &LearningQuizStatusDto) -> Option<String> {
    let score = quiz.best.as_ref().map_or_else(String::new, |best| {
        format!(", best {:.0}%", best.score * 100.0)
    });
    match quiz.verdict {
        LearningQuizVerdict::NotTaken => None,
        LearningQuizVerdict::Pending => Some("quiz awaiting its grade".to_string()),
        LearningQuizVerdict::Passed => Some(format!("quiz passed{score}")),
        LearningQuizVerdict::NotPassed => Some(format!("quiz not passed{score}")),
    }
}

impl CourseTurn {
    pub(super) async fn run_tool(&self, call: FunctionCall, max_chars: usize) -> FunctionResult {
        match self.dispatch(&call, max_chars).await {
            Ok(data) => FunctionResult::success(data),
            Err(error) => FunctionResult::error("course_error", error.to_string()),
        }
    }

    async fn dispatch(&self, call: &FunctionCall, max_chars: usize) -> Result<Value> {
        let arguments = &call.arguments;
        match call.name.as_str() {
            TOOL_SEARCH_COURSE => {
                let found = self
                    .search_course(required(arguments, "query")?, max_chars)
                    .await?;
                serde_json::to_value(found)
                    .map_err(|error| AppError::InternalError(error.to_string()))
            }
            TOOL_READ_COURSE_SOURCE => {
                let source = required(arguments, "source_id")?;
                let offset = arg_u32(arguments, "offset").unwrap_or(0) as usize;
                let read = self
                    .read_source(version_id(source), offset, max_chars)
                    .await?
                    .ok_or_else(|| {
                        AppError::NotFound(format!("The course has no source {source}."))
                    })?;
                serde_json::to_value(read)
                    .map_err(|error| AppError::InternalError(error.to_string()))
            }
            TOOL_COURSE_OUTLINE => Ok(text_result(self.outline().await?, max_chars)),
            TOOL_DUE_REVIEWS => Ok(text_result(self.due_reviews(max_chars).await?, max_chars)),
            TOOL_RECORD_RECALL => {
                let selected = arg_u32(arguments, "selected_option").map(|index| index as usize);
                let recorded = self
                    .rate_card(
                        required(arguments, "card_id")?,
                        rating(arguments)?,
                        selected,
                    )
                    .await?;
                Ok(Value::String(recorded))
            }
            TOOL_SAVE_CARD => {
                let source_ids = arguments
                    .get("source_ids")
                    .and_then(Value::as_array)
                    .map(|ids| {
                        ids.iter()
                            .filter_map(Value::as_str)
                            .map(|id| version_id(id.trim()).to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                let saved = self
                    .save_card(NewCard {
                        question: required(arguments, "question")?,
                        answer: required(arguments, "answer")?,
                        explanation: arg_str(arguments, "explanation").unwrap_or_default(),
                        source_ids,
                    })
                    .await?;
                Ok(Value::String(saved))
            }
            TOOL_SAVE_NOTE => {
                let saved = self
                    .save_note(
                        arg_str(arguments, "lesson_id"),
                        required(arguments, "summary")?,
                    )
                    .await?;
                Ok(Value::String(saved))
            }
            other => Err(AppError::InvalidInput(format!(
                "There is no course tool named {other}."
            ))),
        }
    }

    async fn outline(&self) -> Result<String> {
        let program = self.program().await?;
        let quizzes: HashMap<String, LearningQuizStatusDto> =
            match LearningAssessmentRepository::new(self.pool.clone())
                .overview(&self.program_id)
                .await
            {
                Ok(overview) => overview
                    .modules
                    .into_iter()
                    .flat_map(|module| module.lessons)
                    .map(|lesson| (lesson.lesson_id, lesson.quiz))
                    .collect(),
                Err(error) => {
                    warn!(%error, "The outline goes without quiz results");
                    HashMap::new()
                }
            };
        let mut out = format!("{}\n", program.summary.title);
        for (number, module) in program.modules.iter().enumerate() {
            let _ = writeln!(out, "\nModule {}: {}", number + 1, module.title);
            for (index, lesson) in module.lessons.iter().enumerate() {
                let finished = lesson.completed.then_some("finished".to_string());
                let quiz = quizzes.get(&lesson.id).and_then(quiz_line);
                let state = [finished, quiz]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(", ");
                let state = if state.is_empty() {
                    String::new()
                } else {
                    format!(" [{state}]")
                };
                let _ = writeln!(
                    out,
                    "  {}.{} {}{state} (lesson_id: {})",
                    number + 1,
                    index + 1,
                    lesson.title,
                    lesson.id
                );
            }
        }
        let sources = LearningSourceLibraryRepository::new(self.pool.clone())
            .active_versions(&self.program_id)
            .await?;
        if !sources.is_empty() {
            out.push_str("\nSources:\n");
            for source in sources {
                let _ = writeln!(out, "  {} (source_id: {})", source.title, source.id);
            }
        }
        Ok(out)
    }

    /// As many due cards as `max_chars` shows, saying how many are left.
    async fn due_reviews(&self, max_chars: usize) -> Result<String> {
        let cards = self.due_cards().await?;
        if cards.is_empty() {
            return Ok("No cards are due.".to_string());
        }
        let total = cards.len();
        let mut out = format!("{total} due:\n");
        let closing = 120;
        let mut shown = 0;
        for card in &cards {
            let entry = card.entry();
            if out.chars().count() + entry.chars().count() + closing > max_chars {
                break;
            }
            out.push_str(&entry);
            shown += 1;
        }
        if shown < total {
            let _ = write!(
                out,
                "[{} more did not fit. Call due_reviews again after rating these.]",
                total - shown
            );
        }
        Ok(out)
    }
}
