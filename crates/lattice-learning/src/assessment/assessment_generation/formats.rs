//! What each format's item holds as the model writes it, and turning it into a
//! checked bank item (section 5.2).
use super::*;
use crate::dto::LearningPracticeEvidenceDimension;
use crate::generation::{long_enough_quote, MIN_QUOTE_CHARS, MIN_QUOTE_WORDS};
use lattice_core::application::services::model_contract::shape::{
    Authored, Fields, Form, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::Picked;
use lattice_core::application::services::model_contract::units::{Grain, Unit, Units};
use std::marker::PhantomData;
use std::ops::RangeInclusive;

/// How many ordered values a writer is asked for.
pub(super) const ORDERING_ASKED: RangeInclusive<usize> = 3..=8;

/// One fill-in blank as the key keeps it.
#[derive(Debug, Default)]
pub(super) struct RawBlank {
    pub(super) accepted_answers: Vec<String>,
}

/// One matching pair as the key keeps it.
#[derive(Debug, Default)]
pub(super) struct RawPair {
    pub(super) left: String,
    pub(super) right: String,
}

/// An item as the app keeps it between the model's statement and the bank:
/// positions and keys are made from what was written.
#[derive(Debug, Default)]
pub(crate) struct RawItem {
    pub(super) prompt: String,
    pub(super) options: Vec<String>,
    /// The positions of the options marked right.
    pub(super) right: Vec<usize>,
    pub(super) distractor_rationales: Vec<String>,
    pub(super) is_true: bool,
    pub(super) accepted_answers: Vec<String>,
    pub(super) is_numeric: bool,
    pub(super) numeric_value: f64,
    pub(super) tolerance: f64,
    pub(super) units: Vec<String>,
    pub(super) blanks: Vec<RawBlank>,
    pub(super) pairs: Vec<RawPair>,
    pub(super) extra_right: String,
    pub(super) ordered_values: Vec<String>,
    pub(super) rubric: Vec<crate::teaching::GeneratedCriterion>,
    pub(super) explanation: String,
    /// The sentences of the passages the item rests on.
    pub(super) cited: Vec<Picked<Unit>>,
    pub(super) difficulty: u8,
}

/// Whether an option is the right answer or a distractor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Right,
    Wrong,
}

impl Vocabulary for Mark {
    const WORDS: &'static [(&'static str, Self)] =
        &[("right", Self::Right), ("wrong", Self::Wrong)];
}

/// A truth value, written as a word.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Truth {
    True,
    False,
}

impl Vocabulary for Truth {
    const WORDS: &'static [(&'static str, Self)] = &[("true", Self::True), ("false", Self::False)];
}

/// The kind of performance a rubric criterion looks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Dimension {
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
    fn evidence(self) -> LearningPracticeEvidenceDimension {
        match self {
            Self::Recall => LearningPracticeEvidenceDimension::Recall,
            Self::Explanation => LearningPracticeEvidenceDimension::Explanation,
            Self::Application => LearningPracticeEvidenceDimension::Application,
            Self::Transfer => LearningPracticeEvidenceDimension::Transfer,
        }
    }
}

/// One option of a choice question: its words, whether it is right, and its own rationale.
pub(crate) struct Choice {
    text: String,
    verdict: Mark,
    reason: String,
}

impl Authored for Choice {
    fn form() -> Form {
        Form::new()
            .text("text")
            .word::<Mark>("verdict")
            .text("reason")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.text("text")?,
            verdict: fields.word("verdict")?,
            reason: fields.text("reason")?,
        })
    }
}

/// One written phrase of a list: an accepted answer, a unit, an ordered value.
pub(crate) struct Phrase {
    text: String,
}

impl Authored for Phrase {
    fn form() -> Form {
        Form::new().text("text")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.text("text")?,
        })
    }
}

/// One blank of a cloze item, with every spelling it accepts.
pub(crate) struct Blank {
    accepted: Vec<Phrase>,
}

impl Authored for Blank {
    fn form() -> Form {
        Form::new().list::<Phrase>("acceptedAnswers")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            accepted: fields.list("acceptedAnswers")?,
        })
    }
}

/// One matching pair.
pub(crate) struct Pair {
    left: String,
    right: String,
}

impl Authored for Pair {
    fn form() -> Form {
        Form::new().text("left").text("right")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            left: fields.text("left")?,
            right: fields.text("right")?,
        })
    }
}

/// One rubric criterion.
pub(crate) struct Criterion {
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

fn texts(phrases: Vec<Phrase>) -> Vec<String> {
    phrases.into_iter().map(|phrase| phrase.text).collect()
}

fn read_options(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
    for (at, option) in fields.list::<Choice>("options")?.into_iter().enumerate() {
        if option.verdict == Mark::Right {
            raw.right.push(at);
        }
        raw.options.push(option.text);
        raw.distractor_rationales.push(option.reason);
    }
    Some(())
}

const CHOICE_FIELDS: &str =
    "options (a list; each option has text, verdict as the word right or wrong, and rationale)";

/// What the items of one format hold between their prompt and their explanation.
pub(crate) trait Body: 'static {
    /// The fields in words, for the instructions.
    const FIELDS: &'static str;

    fn fields(form: Form) -> Form;

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()>;
}

/// The options of a single-choice question.
pub(crate) struct ChoiceBody;

impl Body for ChoiceBody {
    const FIELDS: &'static str = CHOICE_FIELDS;

    fn fields(form: Form) -> Form {
        form.list::<Choice>("options")
    }

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
        read_options(fields, raw)
    }
}

/// The truth value of a true/false statement.
pub(crate) struct TruthBody;

impl Body for TruthBody {
    const FIELDS: &'static str = "isTrue (the word true or false)";

    fn fields(form: Form) -> Form {
        form.word::<Truth>("isTrue")
    }

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
        raw.is_true = fields.word::<Truth>("isTrue")? == Truth::True;
        Some(())
    }
}

/// The options of a multi-select question.
pub(crate) struct SelectBody;

impl Body for SelectBody {
    const FIELDS: &'static str = CHOICE_FIELDS;

    fn fields(form: Form) -> Form {
        form.list::<Choice>("options")
    }

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
        read_options(fields, raw)
    }
}

/// The accepted answers of a short-answer question.
pub(crate) struct ShortBody;

impl Body for ShortBody {
    const FIELDS: &'static str = "acceptedAnswers (a list of objects with text), isNumeric (the word true or false), numericValue and tolerance (numbers written as text), units (a list of objects with text)";

    fn fields(form: Form) -> Form {
        form.list::<Phrase>("acceptedAnswers")
            .word::<Truth>("isNumeric")
            .text("numericValue")
            .text("tolerance")
            .list::<Phrase>("units")
    }

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
        raw.accepted_answers = texts(fields.list("acceptedAnswers")?);
        raw.is_numeric = fields.word::<Truth>("isNumeric")? == Truth::True;
        let value = fields.text("numericValue")?;
        let tolerance = fields.text("tolerance")?;
        raw.units = texts(fields.list("units")?);
        if raw.is_numeric {
            raw.numeric_value = value.trim().parse().ok()?;
            raw.tolerance = tolerance.trim().parse().ok()?;
        }
        Some(())
    }
}

/// The blanks of a cloze question.
pub(crate) struct ClozeBody;

impl Body for ClozeBody {
    const FIELDS: &'static str =
        "blanks (a list; each blank has acceptedAnswers, a list of objects with text)";

    fn fields(form: Form) -> Form {
        form.list::<Blank>("blanks")
    }

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
        raw.blanks = fields
            .list::<Blank>("blanks")?
            .into_iter()
            .map(|blank| RawBlank {
                accepted_answers: texts(blank.accepted),
            })
            .collect();
        Some(())
    }
}

/// The pairs of a matching question.
pub(crate) struct PairsBody;

impl Body for PairsBody {
    const FIELDS: &'static str =
        "pairs (a list of objects with left and right), extraRight (text, empty for none)";

    fn fields(form: Form) -> Form {
        form.list::<Pair>("pairs").text("extraRight")
    }

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
        raw.pairs = fields
            .list::<Pair>("pairs")?
            .into_iter()
            .map(|pair| RawPair {
                left: pair.left,
                right: pair.right,
            })
            .collect();
        raw.extra_right = fields.text("extraRight")?;
        Some(())
    }
}

/// The values of an ordering question, in their correct order.
pub(crate) struct OrderBody;

impl Body for OrderBody {
    const FIELDS: &'static str =
        "orderedValues (a list of objects with text, in their correct order)";

    fn fields(form: Form) -> Form {
        form.list::<Phrase>("orderedValues")
    }

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
        raw.ordered_values = texts(fields.list("orderedValues")?);
        Some(())
    }
}

/// The criteria of an explanation task.
pub(crate) struct RubricBody;

impl Body for RubricBody {
    const FIELDS: &'static str = "rubric (a list of criteria, each with title, description and dimension as the word recall, explanation, application or transfer)";

    fn fields(form: Form) -> Form {
        form.list::<Criterion>("rubric")
    }

    fn read(fields: &mut Fields<'_, '_>, raw: &mut RawItem) -> Option<()> {
        raw.rubric = fields
            .list::<Criterion>("rubric")?
            .into_iter()
            .map(|criterion| crate::teaching::GeneratedCriterion {
                title: criterion.title,
                description: criterion.description,
                dimension: criterion.dimension.evidence(),
            })
            .collect();
        Some(())
    }
}

/// Who decides an item's difficulty.
pub(crate) trait Role: 'static {
    /// Whether the model writes the difficulty; otherwise the app keeps the item's own.
    const WRITES_DIFFICULTY: bool;
}

/// An item written for a need.
pub(crate) struct Fresh;

impl Role for Fresh {
    const WRITES_DIFFICULTY: bool = true;
}

/// An item written in the place of one that was refused.
pub(crate) struct Replacement;

impl Role for Replacement {
    const WRITES_DIFFICULTY: bool = false;
}

/// One item as the model writes it: its prompt, the fields of its format, its
/// explanation, the sentences it rests on and, for a fresh item, its difficulty.
pub(crate) struct Item<B, R>(RawItem, PhantomData<fn() -> (B, R)>);

impl<B, R> Item<B, R> {
    pub(super) fn prompt(&self) -> &str {
        self.0.prompt.trim()
    }

    pub(super) fn into_raw(self) -> RawItem {
        self.0
    }
}

impl<B: Body, R: Role> Authored for Item<B, R> {
    fn form() -> Form {
        let form = B::fields(Form::new().text("prompt"))
            .text("explanation")
            .cite::<Unit>("quote");
        if R::WRITES_DIFFICULTY {
            form.text("difficulty")
        } else {
            form
        }
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let mut raw = RawItem {
            prompt: fields.text("prompt")?,
            ..RawItem::default()
        };
        B::read(fields, &mut raw)?;
        raw.explanation = fields.text("explanation")?;
        raw.cited = fields.cite::<Unit>("quote");
        if R::WRITES_DIFFICULTY {
            raw.difficulty = fields.text("difficulty")?.trim().parse().ok()?;
        }
        Some(Self(raw, PhantomData))
    }
}

/// An item the checks refused: what it said, and every fault, in words its rewrite is told.
#[derive(Debug)]
pub(super) struct Unbuilt {
    pub(super) prompt: String,
    pub(super) options: Vec<String>,
    pub(super) faults: Vec<String>,
    pub(super) difficulty: u8,
}

/// The passages the cited sentences lie in, each once in the order first cited,
/// and what the sentences say, read from the app's own copy of the passages; a
/// fault when they name no sentence of any.
fn quoted<'a>(
    cited: &[Picked<Unit>],
    passages: &[&'a MaterialPassage],
) -> std::result::Result<(Vec<&'a MaterialPassage>, String), String> {
    let sentences: Vec<(&'a MaterialPassage, Units)> = passages
        .iter()
        .map(|passage| (*passage, Units::of(&passage.text, Grain::Sentence)))
        .collect();
    let mut homes: Vec<&'a MaterialPassage> = Vec::new();
    let mut said: Vec<String> = Vec::new();
    for cite in cited {
        let found = sentences.iter().find_map(|(passage, units)| {
            units
                .text_of(cite.reference)
                .map(|text| (*passage, text.to_owned()))
        });
        let Some((passage, text)) = found else {
            continue;
        };
        if !homes.iter().any(|held| std::ptr::eq(*held, passage)) {
            homes.push(passage);
        }
        said.push(text);
    }
    if homes.is_empty() {
        return Err("The item's quote cites no sentence of the supplied passages.".into());
    }
    Ok((homes, said.join(" ")))
}

/// Turn one model item into a validated candidate, or say every fault it has.
pub(super) fn to_candidate(
    raw: RawItem,
    need: &AuthoringNeed,
    passages: &[&MaterialPassage],
) -> std::result::Result<AuthoredItem, Unbuilt> {
    let prompt = raw.prompt.trim().to_string();
    let said = raw.options.clone();
    let difficulty = raw.difficulty;
    let refuse = |options: Vec<String>, faults: Vec<String>| Unbuilt {
        prompt: prompt.clone(),
        options,
        faults,
        difficulty,
    };
    let (cited_passages, quote) = match quoted(&raw.cited, passages) {
        Ok(found) => found,
        Err(fault) => return Err(refuse(said, vec![fault])),
    };
    let quote = quote.trim().to_string();
    let mut faults = Vec::new();
    if !long_enough_quote(&quote) {
        faults.push(format!("The item's quote is shorter than {MIN_QUOTE_CHARS} characters or {MIN_QUOTE_WORDS} words."));
    }
    let mut candidate = match candidate_of(raw, need, &cited_passages) {
        Ok(candidate) => candidate,
        Err(fault) => {
            faults.push(fault);
            return Err(refuse(said, faults));
        }
    };
    faults.extend(engine::checks::item_faults(&candidate));
    if let (true, Err(error)) = (faults.is_empty(), engine::validate_candidate(&candidate)) {
        faults.push(error.to_string());
    }
    if !faults.is_empty() {
        return Err(refuse(candidate.options, faults));
    }
    // Code, not the model, decides where the key sits.
    engine::shuffle_authored(&mut candidate);
    Ok(AuthoredItem {
        candidate: candidate.into(),
        source_quote: quote,
        lesson_id: need.lesson_id.clone(),
    })
}

/// The bank candidate a model item makes, unchecked; a fault when its key or rubric cannot be built.
fn candidate_of(
    raw: RawItem,
    need: &AuthoringNeed,
    passages: &[&MaterialPassage],
) -> std::result::Result<LearningAssessmentCandidate, String> {
    let format = need.requirement.format;
    let mut options = vec![];
    let mut left = vec![];
    let mut rationales = vec![];
    let mut rubric = vec![];
    let key = match format {
        LearningItemFormat::SingleChoice => {
            options = raw.options;
            rationales = raw.distractor_rationales;
            match raw.right.as_slice() {
                [only] => LearningAssessmentKey::Choice(*only),
                _ => {
                    return Err(
                        "A single-choice question has exactly one option marked right.".into(),
                    )
                }
            }
        }
        LearningItemFormat::TrueFalse => {
            options = engine::TRUE_FALSE.map(String::from).to_vec();
            LearningAssessmentKey::Choice(if raw.is_true { 0 } else { 1 })
        }
        LearningItemFormat::MultiSelect => {
            options = raw.options;
            rationales = raw.distractor_rationales;
            LearningAssessmentKey::Choices(raw.right.into_iter().collect::<BTreeSet<_>>())
        }
        LearningItemFormat::ShortAnswer => LearningAssessmentKey::Text(LearningTextKey {
            variants: raw.accepted_answers,
            numeric: raw.is_numeric.then(|| LearningNumericKey {
                value: raw.numeric_value,
                tolerance: raw.tolerance.max(0.0),
                unit_variants: raw.units,
            }),
        }),
        LearningItemFormat::Cloze => LearningAssessmentKey::Cloze(
            raw.blanks
                .into_iter()
                .map(|b| LearningTextKey {
                    variants: b.accepted_answers,
                    numeric: None,
                })
                .collect(),
        ),
        LearningItemFormat::Matching => {
            left = raw
                .pairs
                .iter()
                .map(|p| p.left.trim().to_string())
                .collect();
            options = raw
                .pairs
                .iter()
                .map(|p| p.right.trim().to_string())
                .collect();
            if !raw.extra_right.trim().is_empty() {
                options.push(raw.extra_right.trim().to_string());
            }
            LearningAssessmentKey::Matching((0..raw.pairs.len()).collect())
        }
        LearningItemFormat::Ordering => {
            options = raw.ordered_values.clone();
            LearningAssessmentKey::Ordering(raw.ordered_values)
        }
        LearningItemFormat::Explanation | LearningItemFormat::Artifact => {
            rubric = crate::teaching::rubric(raw.rubric, true)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|c| LearningRubricCriterion {
                    id: c.id,
                    title: c.title,
                    description: c.description,
                    max_points: c.max_points as u32,
                })
                .collect();
            LearningAssessmentKey::Rubric
        }
    };
    let difficulty = raw.difficulty.clamp(
        need.requirement.difficulty_min,
        need.requirement.difficulty_max,
    );
    Ok(LearningAssessmentCandidate {
        id: uuid::Uuid::new_v4().to_string(),
        outcome_ids: vec![need.requirement.outcome_id.clone()],
        format,
        difficulty,
        prompt: raw.prompt.trim().to_string(),
        options,
        left,
        source_version_ids: {
            let mut ids: Vec<String> = Vec::new();
            for id in passages.iter().flat_map(|p| p.source_version_ids.iter()) {
                if uuid::Uuid::parse_str(id).is_ok() && !ids.contains(id) {
                    ids.push(id.clone());
                }
            }
            ids
        },
        rubric,
        key,
        explanation: raw.explanation.trim().to_string(),
        distractor_rationales: rationales
            .into_iter()
            .map(|r| r.trim().to_string())
            .collect(),
    })
}
