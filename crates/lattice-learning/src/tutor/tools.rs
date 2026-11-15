//! The tools a tutor turn adds to the chat's. They are not in the process-wide
//! function registry: the course is the conversation's, so the program comes
//! from the turn and never from the arguments.
//!
//! `search_course` and `read_course_source` return passages, which the turn
//! numbers; the others return plain text, cut to the room this call has.

use super::cards::NewCard;
use super::course_turn::CourseTurn;
use super::names::{Card, Choice, Lesson, Source};
use crate::assessment_repository::LearningAssessmentRepository;
use crate::dto::{LearningQuizStatusDto, LearningQuizVerdict};
use crate::recall::study_dto::StudyRating;
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::application::contracts::tools::explorer::{arg_str, arg_u32};
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::application::ports::ToolDefinition;
use lattice_core::application::services::model_contract::shape::Vocabulary;
use lattice_core::application::services::model_contract::tools::Parameters;
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
const TOOL_ASK_CARD: &str = "ask_card";
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
            | TOOL_ASK_CARD
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
        TOOL_ASK_CARD => "Asked a recall card".to_string(),
        TOOL_RECORD_RECALL => "Rated a recall card".to_string(),
        TOOL_SAVE_CARD => "Saved a recall card".to_string(),
        TOOL_SAVE_NOTE => "Saved a note to the Notebook".to_string(),
        other => format!("Running {other}"),
    }
}

impl Vocabulary for StudyRating {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("again", StudyRating::Again),
        ("hard", StudyRating::Hard),
        ("good", StudyRating::Good),
        ("easy", StudyRating::Easy),
    ];
}

fn search_parameters() -> Parameters {
    Parameters::new().text("query", "What to find, in plain words.")
}

fn read_source_parameters() -> Parameters {
    Parameters::new().handle::<Source>(
        "source",
        "The bracketed name of a source, such as [r2], from a passage's label or the outline.",
    )
}

fn card_parameters() -> Parameters {
    Parameters::new().handle::<Card>(
        "card",
        "The bracketed name of a card, such as [c1], from the turn's facts or due_reviews.",
    )
}

fn record_parameters() -> Parameters {
    card_parameters()
        .optional_choice::<StudyRating>(
            "rating",
            "How the learner's answer went. Not for a multiple-choice card: its option decides.",
        )
        .optional_handle::<Choice>(
            "option",
            "For a multiple-choice card: the bracketed name of the option the learner chose, such as [o3], which decides the rating.",
        )
}

fn save_card_parameters() -> Parameters {
    Parameters::new()
        .text("question", "The front of the card: the question to ask.")
        .text("answer", "The back of the card: the answer.")
        .optional_text("explanation", "Why the answer holds. Optional.")
}

fn save_note_parameters() -> Parameters {
    Parameters::new()
        .text("summary", "The learner's own words.")
        .optional_handle::<Lesson>(
            "lesson",
            "The lesson the outline names, by its bracketed name such as [l2]. Omit for the lesson this chat is on.",
        )
}

/// `save_card`'s schema: its parameters, and the list of sources a card cites.
fn save_card_schema() -> Value {
    let mut schema = save_card_parameters().schema().as_json().clone();
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        properties.insert(
            "sources".to_string(),
            json!({ "type": "array", "items": { "type": "string" }, "description": "The bracketed name of each source the card was taught from, such as [r2]." }),
        );
    }
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.push(Value::from("sources"));
    }
    schema
}

pub(super) fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: TOOL_SEARCH_COURSE.to_string(),
            description: "Search the course's sources. Returns passages labelled module › lesson \
                 › source, each under the number to cite it by, with the name of its source to \
                 read it further."
                .to_string(),
            parameters: search_parameters().schema().as_json().clone(),
        },
        ToolDefinition {
            name: TOOL_READ_COURSE_SOURCE.to_string(),
            description: "Read a section of one of the course's sources, as much as this turn \
                 has room for, under the number to cite it by. Says where to read on."
                .to_string(),
            parameters: read_source_parameters().schema().as_json().clone(),
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
                 Fronts only: ask the learner with ask_card, then rate their answer with \
                 record_recall."
                .to_string(),
            parameters: json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        },
        ToolDefinition {
            name: TOOL_ASK_CARD.to_string(),
            description: "Ask the learner one of the course's due cards: records that you asked \
                 it and gives the card to put to them as written. Rate their answer with \
                 record_recall afterwards."
                .to_string(),
            parameters: card_parameters().schema().as_json().clone(),
        },
        ToolDefinition {
            name: TOOL_RECORD_RECALL.to_string(),
            description: "Rate the learner's answer to a card into its review schedule, the same \
                 schedule the Recall tab uses. Only for a card you asked with ask_card: the \
                 turn's facts and due_reviews give its card name. The rating comes from the \
                 learner's answer: again if they missed it, hard if they got it with difficulty, \
                 good if they recalled it, easy if it was effortless. When the answer alone does \
                 not settle it, ask them how it went first."
                .to_string(),
            parameters: record_parameters().schema().as_json().clone(),
        },
        ToolDefinition {
            name: TOOL_SAVE_CARD.to_string(),
            description: "Add a recall card to the course's deck, citing where it comes from. \
                 The card is due at once and shows in the Recall tab."
                .to_string(),
            parameters: save_card_schema(),
        },
        ToolDefinition {
            name: TOOL_SAVE_NOTE.to_string(),
            description: "Save the learner's summary, in their words, to the course Notebook on \
                 a lesson's note."
                .to_string(),
            parameters: save_note_parameters().schema().as_json().clone(),
        },
    ]
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
                let query =
                    self.read_arguments(&search_parameters(), arguments, |args| args.text("query"))?;
                let found = self.search_course(&query, max_chars).await?;
                serde_json::to_value(found)
                    .map_err(|error| AppError::InternalError(error.to_string()))
            }
            TOOL_READ_COURSE_SOURCE => {
                let source = self.read_arguments(&read_source_parameters(), arguments, |args| {
                    args.handle::<Source>("source")
                })?;
                let offset = arg_u32(arguments, "offset").unwrap_or(0) as usize;
                let read = self
                    .read_source(&source.reference, offset, max_chars)
                    .await?
                    .ok_or_else(|| {
                        AppError::NotFound(format!(
                            "The course has no source {}.",
                            source.handle.bracketed()
                        ))
                    })?;
                let mut data = serde_json::to_value(read.passages)
                    .map_err(|error| AppError::InternalError(error.to_string()))?;
                if let (Some(next), Some(object)) = (read.next, data.as_object_mut()) {
                    object.insert(
                        "read_on".to_string(),
                        json!({
                            "tool": TOOL_READ_COURSE_SOURCE,
                            "arguments": { "source": source.handle.bracketed(), "offset": next }
                        }),
                    );
                }
                Ok(data)
            }
            TOOL_COURSE_OUTLINE => Ok(text_result(self.outline().await?, max_chars)),
            TOOL_DUE_REVIEWS => Ok(text_result(self.due_reviews(max_chars).await?, max_chars)),
            TOOL_ASK_CARD => {
                let card = self.read_arguments(&card_parameters(), arguments, |args| {
                    args.handle::<Card>("card")
                })?;
                Ok(Value::String(self.ask_card(&card.reference).await?))
            }
            TOOL_RECORD_RECALL => {
                let (card, rating, option) =
                    self.read_arguments(&record_parameters(), arguments, |args| {
                        let card = args.handle::<Card>("card")?;
                        let rating = args.optional_choice::<StudyRating>("rating")?;
                        let option = args.optional_handle::<Choice>("option")?;
                        Ok((card, rating, option))
                    })?;
                let recorded = self
                    .rate_card(&card.reference, rating, option.map(|option| option.reference))
                    .await?;
                Ok(Value::String(recorded))
            }
            TOOL_SAVE_CARD => {
                let (question, answer, explanation) =
                    self.read_arguments(&save_card_parameters(), arguments, |args| {
                        Ok((
                            args.text("question")?,
                            args.text("answer")?,
                            args.optional_text("explanation"),
                        ))
                    })?;
                let saved = self
                    .save_card(NewCard {
                        question: &question,
                        answer: &answer,
                        explanation: explanation.as_deref().unwrap_or_default(),
                        source_ids: self.read_sources(arguments)?,
                    })
                    .await?;
                Ok(Value::String(saved))
            }
            TOOL_SAVE_NOTE => {
                let (summary, lesson) =
                    self.read_arguments(&save_note_parameters(), arguments, |args| {
                        Ok((
                            args.text("summary")?,
                            args.optional_handle::<Lesson>("lesson")?,
                        ))
                    })?;
                let saved = self
                    .save_note(lesson.as_ref().map(|lesson| lesson.reference.as_str()), &summary)
                    .await?;
                Ok(Value::String(saved))
            }
            other => Err(AppError::InvalidInput(format!(
                "There is no course tool named {other}."
            ))),
        }
    }

    /// The source versions a card cites, from the names the model wrote.
    fn read_sources(&self, arguments: &Value) -> Result<Vec<String>> {
        let written = match arguments.get("sources") {
            Some(Value::Array(items)) => items.clone(),
            Some(one @ Value::String(_)) => vec![one.clone()],
            _ => Vec::new(),
        };
        let parameters = read_source_parameters();
        written
            .iter()
            .filter_map(Value::as_str)
            .map(|name| {
                self.read_arguments(&parameters, &json!({ "source": name }), |args| {
                    args.handle::<Source>("source")
                })
                .map(|source| source.reference)
            })
            .collect()
    }

    async fn outline(&self) -> Result<String> {
        let program = self.program().await?;
        let quizzes: HashMap<String, LearningQuizStatusDto> =
            match LearningAssessmentRepository::new(self.database.clone())
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
                    "  {}.{} {}{state} {}",
                    number + 1,
                    index + 1,
                    lesson.title,
                    self.name(Lesson(lesson.id.clone()))?
                );
            }
        }
        let sources = LearningSourceLibraryRepository::new(self.database.clone())
            .active_versions(&self.program_id)
            .await?;
        if !sources.is_empty() {
            out.push_str("\nSources:\n");
            for source in sources {
                let _ = writeln!(
                    out,
                    "  {} {}",
                    source.title,
                    self.name(Source(source.id.clone()))?
                );
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
            let entry = self.card_entry(&card.id, &card.front, &card.options)?;
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
