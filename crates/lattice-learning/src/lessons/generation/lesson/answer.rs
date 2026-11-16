//! The lesson writer's answer: what it is shown, what it writes, and the
//! saved draft it becomes.

use crate::assessment::assessment_engine::{option_order, LearningItemFormat};
use crate::dto::LearningSourceDto;
use crate::lessons::rows::Quotes;
use crate::lessons::shape::{self as lesson_shape, Lengths};
use crate::lessons::structured_call::{Asking, CallKind, ReasonFirst};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{
    self, ask, Alone, Answered, Says, Settled,
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

/// Said after the lesson's own instructions: the answer's fields, in words.
const FORM: &str = " Write the answer as one object. ideas lists the lesson's core ideas in teaching order, each with its sections in teaching order: every section has a kind (explanation, worked_example, contrast or check), a title, a body, cites and citations. guidedPractice, independentPractice, reflection and recap are one section each, written in their own fields; the two practice sections also have a rubric of criteria, each with a title, a description and a dimension. questions lists the questions, each with a kind, a prompt, the wrong choices in wrong, then the right choice in right, cites and citations. Every choice, wrong or right, has its option and its reason: a wrong choice's reason says why it fails, the right choice's reason says why it is right. cites names, by their handles, the sentences of the excerpts shown in sources that the section or question rests on: at least one whenever sources are shown, whatever its kind. Each entry of citations has the sentence it cites and, in cites, the handles of the excerpt sentences that state it.";

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
    sources: Vec<Source>,
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
enum Teaching {
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
    fn word(self) -> &'static str {
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
enum Asked {
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
    fn word(self) -> &'static str {
        match self {
            Self::Practice => "practice",
            Self::Quiz => "quiz",
            Self::Test => "test",
        }
    }
}

/// What a rubric criterion judges.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Dimension {
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
    fn word(self) -> &'static str {
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
struct Cited {
    sentence: String,
    cites: Vec<Picked<Quoted>>,
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
struct Part {
    title: String,
    body: String,
    cites: Vec<Picked<Quoted>>,
    cited: Vec<Cited>,
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
struct Section {
    kind: Teaching,
    part: Part,
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
struct Idea {
    sections: Vec<Section>,
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
struct Criterion {
    title: String,
    description: String,
    dimension: Dimension,
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
struct Practice {
    part: Part,
    rubric: Vec<Criterion>,
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

/// A choice of a question and why it fails, or why it is right.
#[derive(Debug, Clone, PartialEq)]
struct Choice {
    option: String,
    reason: String,
}

impl Authored for Choice {
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
struct Question {
    kind: Asked,
    prompt: String,
    right: Choice,
    wrong: Vec<Choice>,
    cites: Vec<Picked<Quoted>>,
    cited: Vec<Cited>,
}

impl Authored for Question {
    fn form() -> Form {
        Form::new()
            .word::<Asked>("kind")
            .text("prompt")
            .counted::<Choice>("wrong", &WRONG)
            .group::<Choice>("right")
            .cite_some::<Quoted>("cites")
            .list::<Cited>("citations")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            kind: fields.word("kind")?,
            prompt: fields.text("prompt")?,
            wrong: fields.list("wrong")?,
            right: fields.group("right")?,
            cites: fields.cite_some("cites")?,
            cited: fields.list("citations")?,
        })
    }
}

/// The lesson as the writer wrote it: its ideas, the sections that close it, its questions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Draft {
    ideas: Vec<Idea>,
    guided: Practice,
    independent: Practice,
    reflection: Part,
    recap: Part,
    questions: Vec<Question>,
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
}

/// The sentences of every source's excerpt a citation may quote.
fn quotes_of(input: &Input) -> Quotes {
    let mut quotes = Quotes::default();
    for (at, source) in input.sources.iter().enumerate() {
        quotes.push(at as u64, &source.excerpt.0);
    }
    quotes
}

/// A part's source and quotation, and the sentences it cites, as a saved draft holds them.
fn cite_into(
    quotes: &Quotes,
    part: &mut Map<String, Value>,
    cites: &[Picked<Quoted>],
    cited: &[Cited],
) {
    let first = cites.iter().find_map(|cite| quotes.get(cite.reference));
    part.insert(
        "sourceIndex".into(),
        first.map_or(Value::Null, |(at, _)| json!(at)),
    );
    part.insert("quote".into(), json!(first.map_or("", |(_, words)| words)));
    let mut entries = Vec::new();
    for sentence in cited {
        for cite in &sentence.cites {
            if let Some((at, words)) = quotes.get(cite.reference) {
                entries.push(json!({
                    "sentence": sentence.sentence, "sourceIndex": at, "quote": words
                }));
            }
        }
    }
    part.insert("citations".into(), Value::Array(entries));
}

fn block(quotes: &Quotes, kind: &str, part: &Part, rubric: Vec<Value>) -> Value {
    let mut block = Map::new();
    block.insert("kind".into(), json!(kind));
    block.insert("title".into(), json!(part.title));
    block.insert("body".into(), json!(part.body));
    block.insert("rubric".into(), Value::Array(rubric));
    cite_into(quotes, &mut block, &part.cites, &part.cited);
    Value::Object(block)
}

fn practice(quotes: &Quotes, kind: &str, practice: &Practice) -> Value {
    let rubric = practice
        .rubric
        .iter()
        .map(|criterion| {
            json!({
                "title": criterion.title, "description": criterion.description,
                "dimension": criterion.dimension.word()
            })
        })
        .collect();
    block(quotes, kind, &practice.part, rubric)
}

fn question(quotes: &Quotes, question: &Question) -> Value {
    let authored: Vec<(&str, &str)> = std::iter::once((question.right.option.as_str(), ""))
        .chain(
            question
                .wrong
                .iter()
                .map(|wrong| (wrong.option.as_str(), wrong.reason.as_str())),
        )
        .collect();
    let order = option_order(
        LearningItemFormat::SingleChoice,
        authored.len(),
        &question.prompt,
    );
    let correct = order.iter().position(|at| *at == 0).unwrap_or(0);
    let laid_out: Vec<&(&str, &str)> = order.iter().filter_map(|at| authored.get(*at)).collect();
    let options: Vec<Value> = laid_out.iter().map(|(option, _)| json!(option)).collect();
    let rationales: Vec<Value> = laid_out.iter().map(|(_, reason)| json!(reason)).collect();
    let mut stored = Map::new();
    stored.insert("kind".into(), json!(question.kind.word()));
    stored.insert("prompt".into(), json!(question.prompt));
    stored.insert("options".into(), Value::Array(options));
    stored.insert("correctIndex".into(), json!(correct));
    stored.insert("rationales".into(), Value::Array(rationales));
    stored.insert("explanation".into(), json!(question.right.reason));
    cite_into(quotes, &mut stored, &question.cites, &question.cited);
    Value::Object(stored)
}

impl Draft {
    /// The lesson as the saved draft later stages read: sections in the order the lesson is laid out, the choices laid out by the assessment engine's seeded order.
    fn stored(&self, input: &Input) -> String {
        let quotes = quotes_of(input);
        let mut blocks = Vec::new();
        for idea in &self.ideas {
            for section in &idea.sections {
                blocks.push(block(
                    &quotes,
                    section.kind.word(),
                    &section.part,
                    Vec::new(),
                ));
            }
        }
        blocks.push(practice(&quotes, "guided_practice", &self.guided));
        blocks.push(practice(&quotes, "independent_practice", &self.independent));
        blocks.push(block(&quotes, "reflection", &self.reflection, Vec::new()));
        blocks.push(block(&quotes, "recap", &self.recap, Vec::new()));
        let questions: Vec<Value> = self
            .questions
            .iter()
            .map(|item| question(&quotes, item))
            .collect();
        json!({"blocks": blocks, "questions": questions}).to_string()
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

/// A lesson answer in the form the writer is asked for, made from a lesson in
/// the form it is saved in, citing the sentences of `prompt`'s sources that
/// hold each quotation. A text that is not a saved lesson is returned as it is.
#[cfg(test)]
pub(crate) fn scripted_lesson(saved: &str, prompt: &str) -> String {
    let Ok(old) = serde_json::from_str::<Value>(saved) else {
        return saved.to_owned();
    };
    let (Some(blocks), Some(questions)) = (
        old.get("blocks").and_then(Value::as_array),
        old.get("questions").and_then(Value::as_array),
    ) else {
        return saved.to_owned();
    };
    let task = prompt
        .rsplit("\n\n")
        .find_map(|chunk| {
            serde_json::from_str::<Value>(chunk)
                .ok()
                .filter(|task| task.get("requirements").is_some())
        })
        .unwrap_or(Value::Null);
    let handles = |index: &Value, quote: &str| -> Vec<Value> {
        let Some(at) = index.as_u64().and_then(|at| usize::try_from(at).ok()) else {
            return Vec::new();
        };
        let rows = task["sources"]
            .get(at)
            .and_then(|source| source["excerpt"].as_str())
            .unwrap_or_default();
        rows.split('[')
            .skip(1)
            .filter_map(|marked| marked.split_once("] "))
            .map(|(handle, words)| (handle, words.trim()))
            .find(|(handle, words)| {
                handle.starts_with('q')
                    && !quote.is_empty()
                    && !words.is_empty()
                    && (words.contains(quote) || quote.contains(words))
            })
            .map(|(handle, _)| vec![json!(handle)])
            .unwrap_or_default()
    };
    let part = |item: &Value| -> Map<String, Value> {
        let mut part = Map::new();
        part.insert("title".into(), item["title"].clone());
        part.insert("body".into(), item["body"].clone());
        part
    };
    let cited_of = |item: &Value| -> Vec<Value> {
        item["citations"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| {
                json!({
                    "sentence": entry["sentence"],
                    "cites": handles(&entry["sourceIndex"], entry["quote"].as_str().unwrap_or_default())
                })
            })
            .collect()
    };
    let with_cites = |mut part: Map<String, Value>, item: &Value| -> Value {
        part.insert(
            "cites".into(),
            Value::Array(handles(
                &item["sourceIndex"],
                item["quote"].as_str().unwrap_or_default(),
            )),
        );
        part.insert("citations".into(), Value::Array(cited_of(item)));
        Value::Object(part)
    };
    let mut ideas: Vec<Vec<Value>> = Vec::new();
    let mut closers: Map<String, Value> = Map::new();
    for item in blocks {
        let kind = item["kind"].as_str().unwrap_or_default();
        let key = match kind {
            "guided_practice" => Some("guidedPractice"),
            "independent_practice" => Some("independentPractice"),
            "reflection" => Some("reflection"),
            "recap" => Some("recap"),
            _ => None,
        };
        match key {
            Some(key) => {
                let mut written = part(item);
                if key.ends_with("Practice") {
                    written.insert("rubric".into(), item["rubric"].clone());
                }
                closers.insert(key.into(), with_cites(written, item));
            }
            None => {
                if kind == "explanation" || ideas.is_empty() {
                    ideas.push(Vec::new());
                }
                let mut written = part(item);
                written.insert("kind".into(), json!(kind));
                if let Some(idea) = ideas.last_mut() {
                    idea.push(with_cites(written, item));
                }
            }
        }
    }
    let written: Vec<Value> = questions
        .iter()
        .map(|item| {
            let options = item["options"].as_array().cloned().unwrap_or_default();
            let right = item["correctIndex"].as_u64().unwrap_or_default();
            let wrong: Vec<Value> = options
                .iter()
                .enumerate()
                .filter(|(at, _)| *at as u64 != right)
                .map(|(at, option)| json!({"option": option, "reason": item["rationales"][at]}))
                .collect();
            let mut question = Map::new();
            question.insert("kind".into(), item["kind"].clone());
            question.insert("prompt".into(), item["prompt"].clone());
            question.insert(
                "right".into(),
                json!({
                    "option": options.get(right as usize).cloned().unwrap_or_default(),
                    "reason": item["explanation"]
                }),
            );
            question.insert("wrong".into(), Value::Array(wrong));
            with_cites(question, item)
        })
        .collect();
    let mut answer = Map::new();
    answer.insert(
        "ideas".into(),
        Value::Array(
            ideas
                .into_iter()
                .map(|sections| json!({"sections": sections}))
                .collect(),
        ),
    );
    answer.extend(closers);
    answer.insert("questions".into(), Value::Array(written));
    Value::Object(answer).to_string()
}

#[cfg(test)]
#[path = "answer_tests.rs"]
mod tests;
