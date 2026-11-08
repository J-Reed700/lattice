//! The course's sources as the turn reads them: passages found by the course
//! index, and stretches of one source, each labelled module › lesson › source
//! and shaped as a citation the turn can number.

use super::course_turn::{place, CourseTurn};
use crate::features::learning::dto::{LearningProgramDto, LearningSourceDto};
use crate::features::learning::persistence::db;
use crate::features::learning::reference_collection::{
    ReferenceCollection, ReferencePassage, Scope,
};
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use lattice_core::application::contracts::sources::{SourceDto, COURSE_SOURCE_PREFIX};
use lattice_core::application::ports::turn_source::{SourcePassage, SourcePassages};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::build_excerpt;
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use tokio_util::sync::CancellationToken;

/// About what a passage takes beyond its text: the label it is shown under.
const LABEL_CHARS: usize = 200;

/// Room a read keeps around its text for the label and the note that closes it.
const READ_FRAMING_CHARS: usize = 600;

/// A passage's share of the citation card's preview, as a library passage's.
const EXCERPT_CHARS: usize = 480;

/// The lesson each source version is first cited in, as module › lesson.
fn lessons_citing(program: &LearningProgramDto) -> HashMap<&str, String> {
    let mut places = HashMap::new();
    for module in &program.modules {
        for lesson in &module.lessons {
            for block in &lesson.blocks {
                for source in &block.source_ids {
                    places
                        .entry(source.as_str())
                        .or_insert_with(|| place(module, lesson));
                }
            }
        }
    }
    places
}

fn source_passage(
    place: Option<&str>,
    title: &str,
    version_id: &str,
    span: (usize, usize),
    text: &str,
    score: f32,
) -> SourcePassage {
    let label = place.map_or_else(
        || format!("{title} (source_id: {version_id})"),
        |place| format!("{place} › {title} (source_id: {version_id})"),
    );
    SourcePassage {
        label,
        source: SourceDto {
            page_number: None,
            from_ocr: false,
            document_id: format!("{COURSE_SOURCE_PREFIX}{version_id}"),
            chunk_id: format!("{COURSE_SOURCE_PREFIX}{version_id}:{}-{}", span.0, span.1),
            content: text.to_string(),
            score,
            path: None,
            position: None,
            file_name: title.to_string(),
            file_path: String::new(),
            mime_type: "text/plain".to_string(),
            category: "Course source".to_string(),
            file_size_bytes: text.len() as i64,
            modified_at: chrono::Utc::now().to_rfc3339(),
            excerpt: Some(build_excerpt(text, &[], EXCERPT_CHARS)),
            highlights: None,
            section: place.map(str::to_string),
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: None,
            web_snapshot: None,
        },
    }
}

impl CourseTurn {
    /// `query`'s best passages over the sources `scope` keeps, as many as
    /// `room` characters show.
    async fn search(
        &self,
        query: &str,
        scope: &Scope,
        sources: &[LearningSourceDto],
        room: usize,
    ) -> Result<Vec<SourcePassage>> {
        let program = self.program().await?;
        let course = match &self.courses {
            Some(courses) => courses
                .search(&self.program_id, CancellationToken::new())
                .await
                .inspect_err(|error| tracing::warn!(%error, "Course index could not be opened"))
                .ok(),
            None => None,
        };
        let collection = ReferenceCollection::searchable_now(course.as_ref(), sources).await?;
        let fits = |passages: &[ReferencePassage]| {
            passages
                .iter()
                .map(|passage| passage.text.chars().count() + LABEL_CHARS)
                .sum::<usize>()
                <= room
        };
        let found = collection.retrieve_in(query, scope, fits).await?;
        let places = lessons_citing(program);
        let titles: HashMap<&str, &str> = sources
            .iter()
            .map(|source| (source.id.as_str(), source.title.as_str()))
            .collect();
        Ok(found
            .iter()
            .map(|passage| {
                source_passage(
                    places.get(passage.source_id.as_str()).map(String::as_str),
                    titles
                        .get(passage.source_id.as_str())
                        .copied()
                        .unwrap_or_default(),
                    &passage.source_id,
                    (passage.start_byte, passage.end_byte),
                    &passage.text,
                    passage.score as f32,
                )
            })
            .collect())
    }

    async fn sources(&self) -> Result<Vec<LearningSourceDto>> {
        LearningSourceLibraryRepository::new(self.pool.clone())
            .verification_sources(&self.program_id)
            .await
    }

    /// What the course holds for the turn's message, before generation. A chat
    /// opened from a lesson reads that lesson's sources first, and the whole
    /// course when they hold nothing for the message.
    pub(super) async fn first_passages(
        &self,
        question: &str,
        room: usize,
    ) -> Result<Vec<SourcePassage>> {
        let sources = self.sources().await?;
        if sources.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(cited) = self.focused_sources().await? {
            let focused = self
                .search(question, &Scope::Keys(cited), &sources, room)
                .await?;
            if !focused.is_empty() {
                return Ok(focused);
            }
        }
        self.search(question, &Scope::All, &sources, room).await
    }

    /// The source versions the chat's lesson cites, when it has a lesson that
    /// cites any.
    async fn focused_sources(&self) -> Result<Option<HashSet<String>>> {
        let Some(lesson_id) = self.lesson_id.as_deref() else {
            return Ok(None);
        };
        let program = self.program().await?;
        let cited: HashSet<String> = program
            .modules
            .iter()
            .flat_map(|module| &module.lessons)
            .filter(|lesson| lesson.id == lesson_id)
            .flat_map(|lesson| &lesson.blocks)
            .flat_map(|block| block.source_ids.iter().cloned())
            .collect();
        Ok((!cited.is_empty()).then_some(cited))
    }

    /// The `search_course` tool: the whole course, as much as `max_chars` shows.
    pub(super) async fn search_course(
        &self,
        query: &str,
        max_chars: usize,
    ) -> Result<SourcePassages> {
        let sources = self.sources().await?;
        if sources.is_empty() {
            return Ok(SourcePassages {
                passages: Vec::new(),
                note: Some(
                    "This course has no sources: it was written from general knowledge, so there is nothing to cite."
                        .to_string(),
                ),
            });
        }
        let passages = self.search(query, &Scope::All, &sources, max_chars).await?;
        let note = passages.is_empty().then(|| {
            "No passage of the course's sources matched. Try other words, or say the course does not cover it."
                .to_string()
        });
        Ok(SourcePassages { passages, note })
    }

    /// The `read_course_source` tool: one stretch of a source from character
    /// `offset`, as much as `max_chars` shows, saying where the next starts.
    pub(super) async fn read_source(
        &self,
        version_id: &str,
        offset: usize,
        max_chars: usize,
    ) -> Result<Option<SourcePassages>> {
        let row = sqlx::query(
            "SELECT title, full_text FROM learning_source_versions WHERE program_id=? AND id=?",
        )
        .bind(&self.program_id)
        .bind(version_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let title: String = row.get("title");
        let text: String = row.get("full_text");
        let total = text.chars().count();
        let start = offset.min(total);
        let room = max_chars.saturating_sub(READ_FRAMING_CHARS).max(1);
        let shown: String = text.chars().skip(start).take(room).collect();
        let end = start + shown.chars().count();
        if shown.trim().is_empty() {
            return Ok(Some(SourcePassages {
                passages: Vec::new(),
                note: Some(format!(
                    "Nothing at character {offset}: this source is {total} characters long."
                )),
            }));
        }
        let note = if end < total {
            format!(
                "Characters {start} to {end} of {total}. Call read_course_source with this source_id and offset={end} to read on."
            )
        } else {
            format!("Characters {start} to {end} of {total}: the end of this source.")
        };
        let program = self.program().await?;
        let places = lessons_citing(program);
        Ok(Some(SourcePassages {
            passages: vec![source_passage(
                places.get(version_id).map(String::as_str),
                &title,
                version_id,
                (start, end),
                &shown,
                1.0,
            )],
            note: Some(note),
        }))
    }
}
