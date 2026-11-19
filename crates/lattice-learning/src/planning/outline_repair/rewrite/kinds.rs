//! What the module rewrite shows and what it answers with.
use crate::dto::{
    GenerateLearningProgramRequestDto, LearningCourseDepth, LearningProjectMilestoneDto,
};
use crate::generation::{GeneratedEvidenceText, GeneratedModule, GeneratedOutlineLesson};
use lattice_core::application::services::model_contract::edit::Consistent;
use lattice_core::application::services::model_contract::shape::{Authored, Fields, Form};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Told,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where a text of the module under repair sits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) enum Place {
    Title,
    Summary,
    Outcome(usize),
    ProjectTitle,
    ProjectBrief,
    Deliverable(usize),
    Criterion(usize),
    LessonTitle(usize),
    LessonObjective(usize),
}

/// One text of the module under repair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Row {
    pub place: Place,
    pub text: Told<String>,
}

impl Row {
    pub(super) fn of(place: Place, text: &str) -> Self {
        Self {
            place,
            text: Told(text.to_owned()),
        }
    }
}

impl Kind for Row {
    const PREFIX: &'static str = "t";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = Place;

    fn reference(&self) -> Place {
        self.place.clone()
    }
}

/// The citation an item holds: recorded, never shown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Citation {
    pub source_index: Internal<Option<usize>>,
    pub quote: Internal<String>,
}

impl Citation {
    fn of(source_index: Option<usize>, quote: &str) -> Self {
        Self {
            source_index: Internal::of(source_index),
            quote: Internal::of(quote.to_owned()),
        }
    }

    /// No citation.
    pub(super) fn none() -> Self {
        Self::of(None, "")
    }
}

/// A lesson of the module under repair, by its position in the saved module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Lesson {
    pub position: usize,
    pub title: Row,
    pub objective: Row,
    pub minutes: Told<i64>,
    pub citation: Citation,
}

impl Kind for Lesson {
    const PREFIX: &'static str = "l";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.position
    }
}

/// A module of the course, by its position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Entry {
    pub position: usize,
    pub title: Told<String>,
    pub outcomes: Vec<Told<String>>,
    pub project: Told<LearningProjectMilestoneDto>,
    pub lessons: Vec<Told<String>>,
}

impl Entry {
    fn of(position: usize, module: &GeneratedModule) -> Self {
        Self {
            position,
            title: Told(module.title.clone()),
            outcomes: module
                .outcomes
                .iter()
                .map(|outcome| Told(outcome.text.clone()))
                .collect(),
            project: Told(module.project.clone()),
            lessons: module
                .lessons
                .iter()
                .map(|lesson| Told(lesson.title.clone()))
                .collect(),
        }
    }
}

impl Kind for Entry {
    const PREFIX: &'static str = "m";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.position
    }
}

/// The module under repair, as rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Module {
    pub title: Row,
    pub summary: Row,
    pub summary_citation: Citation,
    pub outcomes: Vec<(Row, Citation)>,
    pub project_title: Row,
    pub project_brief: Row,
    pub deliverables: Vec<Row>,
    pub criteria: Vec<Row>,
    pub prerequisites: Vec<usize>,
    pub lessons: Vec<Lesson>,
}

impl Module {
    /// The saved module as rows.
    pub(super) fn of(saved: &GeneratedModule) -> Self {
        let numbered = |place: fn(usize) -> Place, texts: &[String]| -> Vec<Row> {
            texts
                .iter()
                .enumerate()
                .map(|(at, text)| Row::of(place(at), text))
                .collect()
        };
        Self {
            title: Row::of(Place::Title, &saved.title),
            summary: Row::of(Place::Summary, &saved.summary.text),
            summary_citation: Citation::of(saved.summary.source_index, &saved.summary.quote),
            outcomes: saved
                .outcomes
                .iter()
                .enumerate()
                .map(|(at, outcome)| {
                    (
                        Row::of(Place::Outcome(at), &outcome.text),
                        Citation::of(outcome.source_index, &outcome.quote),
                    )
                })
                .collect(),
            project_title: Row::of(Place::ProjectTitle, &saved.project.title),
            project_brief: Row::of(Place::ProjectBrief, &saved.project.brief),
            deliverables: numbered(Place::Deliverable, &saved.project.deliverables),
            criteria: numbered(Place::Criterion, &saved.project.success_criteria),
            prerequisites: saved.prerequisite_indices.clone(),
            lessons: saved
                .lessons
                .iter()
                .enumerate()
                .map(|(position, lesson)| Lesson {
                    position,
                    title: Row::of(Place::LessonTitle(position), &lesson.title),
                    objective: Row::of(Place::LessonObjective(position), &lesson.objective),
                    minutes: Told(lesson.estimated_minutes),
                    citation: Citation::of(lesson.source_index, &lesson.quote),
                })
                .collect(),
        }
    }

    /// The module as it is saved.
    pub(super) fn generated(&self) -> GeneratedModule {
        let text = |row: &Row| row.text.0.clone();
        let cited = |row: &Row, citation: &Citation| GeneratedEvidenceText {
            text: text(row),
            source_index: *citation.source_index.value(),
            quote: citation.quote.value().clone(),
        };
        GeneratedModule {
            prerequisite_indices: self.prerequisites.clone(),
            project: LearningProjectMilestoneDto {
                title: text(&self.project_title),
                brief: text(&self.project_brief),
                deliverables: self.deliverables.iter().map(text).collect(),
                success_criteria: self.criteria.iter().map(text).collect(),
            },
            title: text(&self.title),
            summary: cited(&self.summary, &self.summary_citation),
            outcomes: self
                .outcomes
                .iter()
                .map(|(row, citation)| cited(row, citation))
                .collect(),
            lessons: self
                .lessons
                .iter()
                .map(|lesson| GeneratedOutlineLesson {
                    title: text(&lesson.title),
                    objective: text(&lesson.objective),
                    estimated_minutes: lesson.minutes.0,
                    source_index: *lesson.citation.source_index.value(),
                    quote: lesson.citation.quote.value().clone(),
                })
                .collect(),
        }
    }

    /// Every text of the module, in the order it is read.
    pub(super) fn rows(&self) -> Vec<Row> {
        let mut rows = vec![self.title.clone(), self.summary.clone()];
        rows.extend(self.outcomes.iter().map(|(row, _)| row.clone()));
        rows.push(self.project_title.clone());
        rows.push(self.project_brief.clone());
        rows.extend(self.deliverables.iter().cloned());
        rows.extend(self.criteria.iter().cloned());
        for lesson in &self.lessons {
            rows.push(lesson.title.clone());
            rows.push(lesson.objective.clone());
        }
        rows
    }
}

/// What a finding concerns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) enum About {
    Module,
    Row(Place),
    Lesson(usize),
}

/// The passage a finding says contradicts the text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Passage {
    pub source_index: Told<Option<usize>>,
    pub quote: Told<String>,
}

/// One finding to apply to the module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Finding {
    pub(super) about: About,
    pub(super) kind: Told<String>,
    pub(super) claim: Told<String>,
    pub(super) reason: Told<String>,
    pub(super) instruction: Told<String>,
    pub(super) contradicting: Option<Passage>,
}

impl Finding {
    /// The finding raised at `path`, with the passage that contradicts it, if one does.
    pub(crate) fn new(
        path: &str,
        kind: &str,
        claim: &str,
        reason: &str,
        instruction: &str,
        passage: Option<(Option<usize>, String)>,
    ) -> Self {
        let mut parts = path.strip_prefix("/modules/").unwrap_or("").split('/');
        let about = match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(_), Some("summary"), None, None) => About::Row(Place::Summary),
            (Some(_), Some("outcomes"), Some(at), None) => at
                .parse()
                .map_or(About::Module, |at| About::Row(Place::Outcome(at))),
            (Some(_), Some("lessons"), Some(at), None) => {
                at.parse().map_or(About::Module, About::Lesson)
            }
            _ => About::Module,
        };
        Self {
            about,
            kind: Told(kind.to_owned()),
            claim: Told(claim.to_owned()),
            reason: Told(reason.to_owned()),
            instruction: Told(instruction.to_owned()),
            contradicting: passage.map(|(source_index, quote)| Passage {
                source_index: Told(source_index),
                quote: Told(quote),
            }),
        }
    }

    /// The claim the finding names.
    pub(super) fn claim(&self) -> &str {
        &self.claim.0
    }
}

/// What the model is asked about one module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Asked {
    pub goal: Told<String>,
    pub prior_knowledge: Told<String>,
    pub minutes_per_session: Told<i64>,
    pub index: Internal<usize>,
    pub depth: Internal<Option<LearningCourseDepth>>,
    pub most_lessons: Told<usize>,
    pub module: Module,
    pub sequence: Vec<Entry>,
    pub findings: Vec<Finding>,
    pub sources: Told<Value>,
}

impl Asked {
    /// The question about module `index` of `candidate`.
    pub(super) fn of(
        request: &GenerateLearningProgramRequestDto,
        candidate: &Value,
        index: usize,
        findings: &[Finding],
        sources: Vec<Value>,
    ) -> Result<Self> {
        let modules: Vec<GeneratedModule> =
            serde_json::from_value(candidate.get("modules").cloned().unwrap_or(Value::Null))
                .map_err(|error| {
                    AppError::InvalidInput(format!("The saved outline is unreadable: {error}"))
                })?;
        let saved = modules.get(index).ok_or_else(|| {
            AppError::InvalidInput("A review referred to an unavailable module.".into())
        })?;
        let module = Module::of(saved);
        let findings = findings
            .iter()
            .map(|finding| {
                let about = match &finding.about {
                    About::Row(Place::Outcome(at)) if *at >= module.outcomes.len() => About::Module,
                    About::Lesson(at) if *at >= module.lessons.len() => About::Module,
                    other => other.clone(),
                };
                Finding {
                    about,
                    ..finding.clone()
                }
            })
            .collect();
        Ok(Self {
            goal: Told(request.goal.clone()),
            prior_knowledge: Told(request.prior_knowledge.clone()),
            minutes_per_session: Told(request.minutes_per_session),
            index: Internal::of(index),
            depth: Internal::of(request.course_depth),
            most_lessons: Told(LearningCourseDepth::bounds(request.course_depth).3),
            module,
            sequence: modules
                .iter()
                .enumerate()
                .map(|(position, module)| Entry::of(position, module))
                .collect(),
            findings,
            sources: Told(Value::Array(sources)),
        })
    }
}

/// A lesson the model adds.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct NewLesson {
    pub after: Option<Picked<Lesson>>,
    pub title: String,
    pub objective: String,
    pub minutes: u32,
}

impl Authored for NewLesson {
    fn form() -> Form {
        Form::new()
            .point_or_none::<Lesson>("after")
            .text("title")
            .text("objective")
            .number("minutes")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            after: fields.point_or_none("after")?,
            title: fields.text("title")?,
            objective: fields.text("objective")?,
            minutes: fields.number("minutes")?,
        })
    }
}

/// A new estimate for a lesson.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Estimate {
    pub lesson: Picked<Lesson>,
    pub minutes: u32,
}

impl Authored for Estimate {
    fn form() -> Form {
        Form::new().point::<Lesson>("lesson").number("minutes")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            lesson: fields.point("lesson")?,
            minutes: fields.number("minutes")?,
        })
    }
}

/// The earlier modules this module starts or stops building on.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Prerequisites {
    pub add: Vec<Picked<Entry>>,
    pub remove: Vec<Picked<Entry>>,
}

impl Authored for Prerequisites {
    fn form() -> Form {
        Form::new().cite::<Entry>("add").cite::<Entry>("remove")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            add: fields.cite("add"),
            remove: fields.cite("remove"),
        })
    }
}

/// The changes the model makes to the module.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Revision {
    pub text: Consistent<Row, String>,
    pub new_lessons: Vec<NewLesson>,
    pub minutes: Vec<Estimate>,
    pub prerequisites: Prerequisites,
}

impl Authored for Revision {
    fn form() -> Form {
        Form::new()
            .change::<Row, String>("text")
            .list::<NewLesson>("newLessons")
            .list::<Estimate>("minutes")
            .group::<Prerequisites>("prerequisites")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.change("text")?,
            new_lessons: fields.list("newLessons")?,
            minutes: fields.list("minutes")?,
            prerequisites: fields.group("prerequisites")?,
        })
    }
}
