//! What the course-plan contracts show of a course: its lessons under handles, and the topics planned so far.

use super::PlannedLesson;
use crate::dto::{LearningBlockKind, LearningLessonDto, LearningModuleDto, LearningProgramDto};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Showing, Told,
};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// What a written lesson taught: its section titles and its recap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Taught {
    pub(super) sections: Vec<Told<String>>,
    pub(super) recap: Vec<Told<String>>,
}

/// A lesson the request asks about, as the model names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Lesson {
    pub(super) id: String,
    pub(super) title: Told<String>,
    pub(super) objective: Told<String>,
    pub(super) minutes: Option<Told<i64>>,
    pub(super) taught: Option<Taught>,
}

impl Kind for Lesson {
    const PREFIX: &'static str = "l";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

/// A lesson of the course the request does not ask about, shown for context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Other(pub(super) Lesson);

impl Kind for Other {
    const PREFIX: &'static str = "o";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.id.clone()
    }
}

/// A module of the outline: the lessons it holds, by their ids.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Module {
    pub(super) title: Told<String>,
    pub(super) milestone: Option<Told<crate::dto::LearningProjectMilestoneDto>>,
    pub(super) lessons: Vec<Internal<String>>,
}

/// The topics planned for a lesson so far.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Held {
    pub(super) lesson: Internal<String>,
    pub(super) ideas: Told<Vec<String>>,
    pub(super) terms: Told<Vec<String>>,
    pub(super) actions: Told<Vec<String>>,
    pub(super) contrast: Told<String>,
}

impl Held {
    pub(super) fn of(topics: &PlannedLesson) -> Self {
        Self {
            lesson: Internal::of(topics.lesson_id.clone()),
            ideas: Told(topics.ideas.clone()),
            terms: Told(topics.terms.clone()),
            actions: Told(topics.actions.clone()),
            contrast: Told(topics.contrast.clone()),
        }
    }
}

/// The lesson of the outline, as the model names it; a written lesson also says what it taught.
pub(super) fn lesson_of(lesson: &LearningLessonDto, detailed: bool) -> Lesson {
    let taught = (detailed && !lesson.blocks.is_empty()).then(|| Taught {
        sections: lesson
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block.kind,
                    LearningBlockKind::Explanation
                        | LearningBlockKind::WorkedExample
                        | LearningBlockKind::Contrast
                ) && !block.title.is_empty()
            })
            .map(|block| Told(block.title.clone()))
            .collect(),
        recap: lesson
            .blocks
            .iter()
            .filter(|block| block.kind == LearningBlockKind::Recap)
            .map(|block| Told(block.body.clone()))
            .collect(),
    });
    Lesson {
        id: lesson.id.clone(),
        title: Told(lesson.title.clone()),
        objective: Told(lesson.objective.clone()),
        minutes: detailed.then_some(Told(lesson.estimated_minutes)),
        taught,
    }
}

/// The modules of `program`, each with the ids of its lessons.
pub(super) fn modules_of(program: &LearningProgramDto) -> Vec<Module> {
    program
        .modules
        .iter()
        .map(|module: &LearningModuleDto| Module {
            title: Told(module.title.clone()),
            milestone: module.project.clone().map(Told),
            lessons: module
                .lessons
                .iter()
                .map(|lesson| Internal::of(lesson.id.clone()))
                .collect(),
        })
        .collect()
}

/// The lessons of a request as it writes them, each under its handle.
pub(super) struct Outline {
    lessons: HashMap<String, Value>,
    others: HashMap<String, Value>,
    handles: HashMap<String, String>,
}

impl Outline {
    /// Shows `asked`, then `others`.
    pub(super) fn show(
        showing: &mut Showing<'_>,
        asked: &[Lesson],
        others: &[Other],
    ) -> Result<Self> {
        let mut outline = Self {
            lessons: HashMap::new(),
            others: HashMap::new(),
            handles: HashMap::new(),
        };
        for (handle, lesson) in showing.show(asked)?.iter() {
            outline.handles.insert(lesson.id.clone(), handle.written());
            outline
                .lessons
                .insert(lesson.id.clone(), lesson_value("lessonId", &handle.written(), lesson));
        }
        for (handle, other) in showing.show(others)?.iter() {
            outline
                .handles
                .insert(other.0.id.clone(), handle.written());
            outline.others.insert(
                other.0.id.clone(),
                lesson_value("otherLessonId", &handle.written(), &other.0),
            );
        }
        Ok(outline)
    }

    /// The handle of the lesson `id`, as written.
    pub(super) fn handle(&self, id: &str) -> Option<&str> {
        self.handles.get(id).map(String::as_str)
    }

    /// The modules, each with its lessons.
    pub(super) fn modules(&self, modules: &[Module]) -> Vec<Value> {
        modules
            .iter()
            .map(|module| {
                let lessons: Vec<&Value> = module
                    .lessons
                    .iter()
                    .filter_map(|id| {
                        self.lessons
                            .get(id.value())
                            .or_else(|| self.others.get(id.value()))
                    })
                    .collect();
                json!({"title": module.title, "milestone": module.milestone, "lessons": lessons})
            })
            .collect()
    }

    /// The topics planned, each under the handle of its lesson.
    pub(super) fn held(&self, held: &[Held]) -> Vec<Value> {
        held.iter()
            .filter_map(|entry| {
                let id = entry.lesson.value();
                let key = if self.lessons.contains_key(id) {
                    "lessonId"
                } else {
                    "otherLessonId"
                };
                let mut value = json!({
                    "ideas": entry.ideas, "terms": entry.terms,
                    "actions": entry.actions, "contrast": entry.contrast
                });
                value
                    .as_object_mut()?
                    .insert(key.into(), json!(self.handle(id)?));
                Some(value)
            })
            .collect()
    }
}

fn lesson_value(key: &str, handle: &str, lesson: &Lesson) -> Value {
    let mut entry = json!({"title": lesson.title, "objective": lesson.objective});
    if let Some(object) = entry.as_object_mut() {
        object.insert(key.into(), json!(handle));
        if let Some(minutes) = &lesson.minutes {
            object.insert("estimatedMinutes".into(), json!(minutes));
        }
        if let Some(taught) = &lesson.taught {
            object.insert(
                "taught".into(),
                json!({"sections": taught.sections, "recap": taught.recap}),
            );
        }
    }
    entry
}
