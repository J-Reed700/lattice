use super::{Cited, Draft, Input, Part, Practice, Question};
use crate::assessment::assessment_engine::{option_order, LearningItemFormat};
use crate::lessons::rows::Quotes;
use lattice_core::application::services::model_contract::shown::Picked;
use lattice_core::application::services::model_contract::units::Quoted;
use serde_json::{json, Map, Value};

/// First `[q<digits>]` source-excerpt handle leaked into learner-facing prose, if any.
pub(super) fn find_leaked_quote_handle(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes.get(i) == Some(&b'[') && bytes.get(i + 1) == Some(&b'q') {
            let mut j = i + 2;
            while j < bytes.len() && bytes.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            if j > i + 2 && bytes.get(j) == Some(&b']') {
                return text.get(i..=j).map(str::to_owned);
            }
        }
        i += 1;
    }
    None
}

impl Draft {
    pub(super) fn leaked_source_handle(&self) -> Option<String> {
        let check_part = |part: &Part| {
            find_leaked_quote_handle(&part.title).or_else(|| find_leaked_quote_handle(&part.body))
        };
        let check_practice = |practice: &Practice| {
            check_part(&practice.part).or_else(|| {
                practice.rubric.iter().find_map(|c| {
                    find_leaked_quote_handle(&c.title)
                        .or_else(|| find_leaked_quote_handle(&c.description))
                })
            })
        };
        self.ideas
            .iter()
            .flat_map(|idea| &idea.sections)
            .find_map(|section| check_part(&section.part))
            .or_else(|| check_practice(&self.guided))
            .or_else(|| check_practice(&self.independent))
            .or_else(|| check_part(&self.reflection))
            .or_else(|| check_part(&self.recap))
            .or_else(|| {
                self.questions.iter().find_map(|q| {
                    find_leaked_quote_handle(&q.prompt)
                        .or_else(|| find_leaked_quote_handle(&q.right))
                        .or_else(|| find_leaked_quote_handle(&q.explanation))
                        .or_else(|| {
                            q.wrong.iter().find_map(|w| {
                                find_leaked_quote_handle(&w.option)
                                    .or_else(|| find_leaked_quote_handle(&w.reason))
                            })
                        })
                })
            })
    }
}

/// The sentences of every source's excerpt a citation may quote.
pub(super) fn quotes_of(input: &Input) -> Quotes {
    let mut quotes = Quotes::default();
    for (at, source) in input.sources.iter().enumerate() {
        quotes.push(at as u64, &source.excerpt.0);
    }
    quotes
}

/// A part's source and quotation, and the sentences it cites, as a saved draft holds them.
pub(super) fn cite_into(
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

pub(super) fn question(quotes: &Quotes, question: &Question) -> Value {
    let authored: Vec<(&str, &str)> = std::iter::once((question.right.as_str(), ""))
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
    stored.insert("explanation".into(), json!(question.explanation));
    cite_into(quotes, &mut stored, &question.cites, &question.cited);
    Value::Object(stored)
}

impl Draft {
    /// The lesson as the saved draft later stages read: sections in the order the lesson is laid out, the choices laid out by the assessment engine's seeded order.
    pub(super) fn stored(&self, input: &Input) -> String {
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
