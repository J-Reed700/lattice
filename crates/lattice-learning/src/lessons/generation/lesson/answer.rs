//! The lesson writer's answer: what it is shown, what it writes, and the
//! saved draft it becomes.

use crate::dto::LearningSourceDto;
#[cfg(test)]
use crate::lessons::rows::Quotes;
use crate::lessons::shape::{self as lesson_shape, Lengths};
use crate::lessons::structured_call::{Asking, CallKind, ReasonFirst};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{
    self, ask, Alone, Answered, Refusal, Says, Settled,
};
use lattice_core::application::services::model_contract::shape::{
    self, Authored, Count, Fields, Form, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::units::Quoted;
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

mod draft;
#[cfg(test)]
mod scripted;

use draft::quotes_of;
#[cfg(test)]
use draft::question;
#[cfg(test)]
pub(crate) use scripted::scripted_lesson;

/// Said after the lesson's own instructions: the answer's fields, in words.
const FORM: &str = " Write the answer as one object. ideas lists the lesson's core ideas in teaching order, each with its sections in teaching order: every section has a kind (explanation, worked_example, contrast or check), a title, a body, cites and citations. guidedPractice, independentPractice, reflection and recap are one section each, written in their own fields; the two practice sections also have a rubric of criteria, each with a title, a description and a dimension. questions lists the questions, each with a kind, a prompt, the wrong choices in wrong (each with its option and its reason), then the right choice in right, an explanation, cites and citations. cites names, by their handles, the sentences of the excerpts shown in sources that the section or question rests on: at least one whenever sources are shown, whatever its kind. Each entry of citations has the sentence it cites and, in cites, the handles of the excerpt sentences that state it. The bracketed excerpt handles inside sources' excerpts are for cites and citations only: never write those bracketed handles inside learner-facing text (title, body, prompt, options, explanation, reasons, or rubric).";

const IDEAS: Count = Count("ideas");
const SECTIONS: Count = Count("sections");
const QUESTIONS: Count = Count("questions");
const WRONG: Count = Count("wrong");

/// The lesson writer's call: its reasoning step and its answer.
pub(crate) struct LessonWriter;

impl Contract for LessonWriter {
    const ID: ContractId = ContractId::new("lesson-writer");
    type Input = Input;
}

/// A saved source, shown with the sentences of its excerpt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Source {
    id: String,
    title: Told<String>,
    url: Option<Told<String>>,
    excerpt: Told<String>,
}

impl Kind for Source {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

/// A topic of the lesson and the sources whose excerpts cover it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Topic {
    topic: Told<String>,
    covering: Internal<Vec<String>>,
}

/// What one lesson request says: the lesson's context, its sources and the
/// topics they cover.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Input {
    before: Told<Value>,
    pub(super) sources: Vec<Source>,
    topics: Vec<Topic>,
    after: Told<Value>,
    /// The list lengths the answer is held to. The request's words say them from the same shape policy.
    lengths: Lengths,
}

impl Input {
    /// The request for the authoring `prompt`, whose `sources` are numbered as it numbers them.
    pub(super) fn of(
        prompt: &str,
        sources: &[LearningSourceDto],
        lengths: Lengths,
    ) -> Result<Self> {
        let Value::Object(object) = serde_json::from_str::<Value>(prompt)
            .map_err(|error| AppError::InternalError(error.to_string()))?
        else {
            return Err(AppError::InternalError(
                "The lesson prompt is not an object.".into(),
            ));
        };
        let (mut before, mut after) = (Map::new(), Map::new());
        let mut topics = Vec::new();
        let mut passed = false;
        for (key, value) in object {
            match key.as_str() {
                "sources" => passed = true,
                "evidenceByTopic" => topics = topics_of(&value, sources),
                "requirements" => {}
                _ if passed => {
                    after.insert(key, value);
                }
                _ => {
                    before.insert(key, value);
                }
            }
        }
        Ok(Self {
            before: Told(Value::Object(before)),
            sources: sources
                .iter()
                .map(|source| Source {
                    id: source.id.clone(),
                    title: Told(source.title.clone()),
                    url: source.url.clone().map(Told),
                    excerpt: Told(source.excerpt.clone()),
                })
                .collect(),
            topics,
            after: Told(Value::Object(after)),
            lengths,
        })
    }
}

fn topics_of(entries: &Value, sources: &[LearningSourceDto]) -> Vec<Topic> {
    entries
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| Topic {
            topic: Told(entry["topic"].as_str().unwrap_or_default().to_owned()),
            covering: Internal::of(
                entry["coveringSourceIndexes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_u64)
                    .filter_map(|at| sources.get(usize::try_from(at).ok()?))
                    .map(|source| source.id.clone())
                    .collect(),
            ),
        })
        .collect()
}

/// A section's kind among the teaching ones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Teaching {
    Explanation,
    WorkedExample,
    Contrast,
    Check,
}

impl Vocabulary for Teaching {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("explanation", Self::Explanation),
        ("worked_example", Self::WorkedExample),
        ("contrast", Self::Contrast),
        ("check", Self::Check),
    ];
}

impl Teaching {
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Explanation => "explanation",
            Self::WorkedExample => "worked_example",
            Self::Contrast => "contrast",
            Self::Check => "check",
        }
    }
}

/// The role a question plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Asked {
    Practice,
    Quiz,
    Test,
}

impl Vocabulary for Asked {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("practice", Self::Practice),
        ("quiz", Self::Quiz),
        ("test", Self::Test),
    ];
}

impl Asked {
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Practice => "practice",
            Self::Quiz => "quiz",
            Self::Test => "test",
        }
    }
}

/// What a rubric criterion judges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Dimension {
    Recall,
    Explanation,
    Application,
    Transfer,
}

impl Vocabulary for Dimension {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("recall", Self::Recall),
        ("explanation", Self::Explanation),
        ("application", Self::Application),
        ("transfer", Self::Transfer),
    ];
}

impl Dimension {
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Recall => "recall",
            Self::Explanation => "explanation",
            Self::Application => "application",
            Self::Transfer => "transfer",
        }
    }
}

/// A sentence the writer cites and the excerpt sentences that state it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Cited {
    pub(super) sentence: String,
    pub(super) cites: Vec<Picked<Quoted>>,
}

impl Authored for Cited {
    fn form() -> Form {
        Form::new().text("sentence").cite::<Quoted>("cites")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            sentence: fields.text("sentence")?,
            cites: fields.cite("cites"),
        })
    }
}

/// What every written part of the lesson holds.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Part {
    pub(super) title: String,
    pub(super) body: String,
    pub(super) cites: Vec<Picked<Quoted>>,
    pub(super) cited: Vec<Cited>,
}

fn part_of(fields: &mut Fields<'_, '_>) -> Option<Part> {
    Some(Part {
        title: fields.text("title")?,
        body: fields.text("body")?,
        cites: fields.cite_some("cites")?,
        cited: fields.list("citations")?,
    })
}

/// The reflection and the recap.
impl Authored for Part {
    fn form() -> Form {
        Form::new()
            .text("title")
            .text("body")
            .cite_some::<Quoted>("cites")
            .list::<Cited>("citations")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        part_of(fields)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Section {
    pub(super) kind: Teaching,
    pub(super) part: Part,
}

impl Authored for Section {
    fn form() -> Form {
        Form::new()
            .word::<Teaching>("kind")
            .text("title")
            .text("body")
            .cite_some::<Quoted>("cites")
            .list::<Cited>("citations")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            kind: fields.word("kind")?,
            part: part_of(fields)?,
        })
    }
}

/// One core idea: its sections in teaching order.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Idea {
    pub(super) sections: Vec<Section>,
}

impl Authored for Idea {
    fn form() -> Form {
        Form::new().counted::<Section>("sections", &SECTIONS)
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            sections: fields.list("sections")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Criterion {
    pub(super) title: String,
    pub(super) description: String,
    pub(super) dimension: Dimension,
}

impl Authored for Criterion {
    fn form() -> Form {
        Form::new()
            .text("title")
            .text("description")
            .word::<Dimension>("dimension")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            title: fields.text("title")?,
            description: fields.text("description")?,
            dimension: fields.word("dimension")?,
        })
    }
}

/// The guided and the independent practice.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Practice {
    pub(super) part: Part,
    pub(super) rubric: Vec<Criterion>,
}

impl Authored for Practice {
    fn form() -> Form {
        Form::new()
            .text("title")
            .text("body")
            .list::<Criterion>("rubric")
            .cite_some::<Quoted>("cites")
            .list::<Cited>("citations")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            part: part_of(fields)?,
            rubric: fields.list("rubric")?,
        })
    }
}

/// A choice the question's right one is not, and why it fails.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Wrong {
    pub(super) option: String,
    pub(super) reason: String,
}

impl Authored for Wrong {
    fn form() -> Form {
        Form::new().text("option").text("reason")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            option: fields.text("option")?,
            reason: fields.text("reason")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Question {
    pub(super) kind: Asked,
    pub(super) prompt: String,
    pub(super) right: String,
    pub(super) wrong: Vec<Wrong>,
    pub(super) explanation: String,
    pub(super) cites: Vec<Picked<Quoted>>,
    pub(super) cited: Vec<Cited>,
}

impl Authored for Question {
    fn form() -> Form {
        Form::new()
            .word::<Asked>("kind")
            .text("prompt")
            .counted::<Wrong>("wrong", &WRONG)
            .text("right")
            .text("explanation")
            .cite_some::<Quoted>("cites")
            .list::<Cited>("citations")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            kind: fields.word("kind")?,
            prompt: fields.text("prompt")?,
            wrong: fields.list("wrong")?,
            right: fields.text("right")?,
            explanation: fields.text("explanation")?,
            cites: fields.cite_some("cites")?,
            cited: fields.list("citations")?,
        })
    }
}

/// The lesson as the writer wrote it: its ideas, the sections that close it, its questions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Draft {
    pub(super) ideas: Vec<Idea>,
    pub(super) guided: Practice,
    pub(super) independent: Practice,
    pub(super) reflection: Part,
    pub(super) recap: Part,
    pub(super) questions: Vec<Question>,
}

impl Authored for Draft {
    fn form() -> Form {
        Form::new()
            .counted::<Idea>("ideas", &IDEAS)
            .group::<Practice>("guidedPractice")
            .group::<Practice>("independentPractice")
            .group::<Part>("reflection")
            .group::<Part>("recap")
            .counted::<Question>("questions", &QUESTIONS)
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            ideas: fields.list("ideas")?,
            guided: fields.group("guidedPractice")?,
            independent: fields.group("independentPractice")?,
            reflection: fields.group("reflection")?,
            recap: fields.group("recap")?,
            questions: fields.list("questions")?,
        })
    }
}

impl Answered for LessonWriter {
    type Answer = shape::Value<Draft>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let lengths = input.lengths;
        showing.asks(&IDEAS, lengths.ideas.0..=lengths.ideas.1);
        showing.asks(&SECTIONS, lengths.sections.0..=lengths.sections.1);
        showing.asks(&QUESTIONS, lengths.questions.0..=lengths.questions.1);
        showing.asks(&WRONG, super::WRONG_CHOICES..=super::WRONG_CHOICES);
        let sources = showing.show(&input.sources)?;
        let quotes = quotes_of(input);
        let quoted = showing.show(quotes.all())?;
        let mut shown = Vec::with_capacity(input.sources.len());
        for (at, (handle, source)) in sources.iter().enumerate() {
            shown.push(json!({
                "id": handle.written(), "title": source.title, "url": source.url,
                "excerpt": quotes.marked(at as u64, &source.excerpt.0, &quoted)
            }));
        }
        let topics: Vec<Value> = input
            .topics
            .iter()
            .map(|topic| {
                let covering: Vec<String> = topic
                    .covering
                    .value()
                    .iter()
                    .filter_map(|id| {
                        sources
                            .iter()
                            .find(|(_, source)| &source.id == id)
                            .map(|(handle, _)| handle.written())
                    })
                    .collect();
                json!({"topic": topic.topic, "coveringSources": covering})
            })
            .collect();
        let mut task = Map::new();
        for (key, value) in input.before.0.as_object().into_iter().flatten() {
            task.insert(key.clone(), value.clone());
        }
        task.insert("sources".into(), Value::Array(shown));
        task.insert("evidenceByTopic".into(), Value::Array(topics));
        for (key, value) in input.after.0.as_object().into_iter().flatten() {
            task.insert(key.clone(), value.clone());
        }
        task.insert(
            "requirements".into(),
            super::requirements(super::Answering::Writer),
        );
        let instructions = format!(
            "{}{FORM}",
            super::lesson_system_with(super::writer_grounding())
        );
        let mut says = Says::new(instructions, Value::Object(task).to_string());
        says.output_tokens = crate::generation::LESSON_OUTPUT_RESERVE;
        Ok(says)
    }

    fn accept(input: &Input, draft: &Draft) -> std::result::Result<(), Refusal> {
        if input.sources.is_empty() {
            return Ok(());
        }
        if let Some(handle) = draft.leaked_source_handle() {
            return Err(Refusal(format!(
                "Remove internal source handle {handle} from the learner-facing text; put excerpt handles only in cites and citations."
            )));
        }
        Ok(())
    }
}

/// Asks the writer for the lesson; the draft is returned as it is saved.
pub(super) async fn write(llm: &dyn LLMPort, input: &Input, brief: &str) -> Result<String> {
    let settled = ask::<LessonWriter>(
        llm,
        &Asking {
            progress: None,
            reasoning: Some(ReasonFirst {
                kind: CallKind::Authoring,
                subject: "the lesson",
                brief,
            }),
        },
        &mut Scope::for_question(),
        input,
        &crate::model_call::answer_options(),
    )
    .await?;
    match settled {
        Settled::Answered(draft) => Ok(draft.stored(input)),
        Settled::Unanswered(why) => Err(ask::failed(&why)),
    }
}

/// Whether the writer's request for `prompt` and an answer of `reserve` tokens fit the model's window.
pub(super) async fn fits(
    llm: &dyn LLMPort,
    prompt: &str,
    sources: &[LearningSourceDto],
    reserve: usize,
) -> bool {
    let Ok(says) = request(prompt, sources) else {
        return false;
    };
    crate::generation::fits(llm, &says.instructions, &says.task, reserve).await
}

/// What the writer is sent for `prompt` and its `sources`.
pub(super) fn request(prompt: &str, sources: &[LearningSourceDto]) -> Result<Says> {
    let input = Input::of(prompt, sources, lesson_shape::policy().lengths())?;
    let mut scope = Scope::for_question();
    LessonWriter::says(&input, &mut scope.request())
}

#[cfg(test)]
#[path = "answer_tests.rs"]
mod tests;
