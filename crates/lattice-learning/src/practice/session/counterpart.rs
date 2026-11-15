//! The counterpart's reply, for every kind of work: asked for, checked, asked again once with the reason, or
//! refused.

use crate::lessons::structured_call::Asking;
use crate::model_call;
use crate::practice::session::kind::{Kind, Known};
use crate::practice::session::observation::Observation;
use crate::practice::session::policy::{Cue, Policy};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{
    self, ask, Alone, Answered, Refusal, Says, Settled,
};
use lattice_core::application::services::model_contract::shape::{
    Authored, Fields, Form, Union, Value, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    self, Drawn, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::learning::practice::work::{Exercise, PartRef};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::marker::PhantomData;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

/// What the model is asked to reserve for one reply, schema fields included.
const REPLY_OUTPUT_TOKENS: usize = 2048;

/// A reply that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub content: String,
    /// The parts of the learner's work the reply points at.
    pub parts: Vec<PartRef>,
}

/// How a reply is asked for: on a checkpoint the counterpart may stay silent.
pub(crate) trait Mode: Send + Sync + 'static {
    const ID: ContractId;
    const CHECKPOINT: bool;
}

/// A reply the counterpart must give.
pub(crate) struct Ordinary;

impl Mode for Ordinary {
    const ID: ContractId = ContractId::new("practice-counterpart-reply");
    const CHECKPOINT: bool = false;
}

/// A reply the counterpart may decline to give.
pub(crate) struct Checkpointed;

impl Mode for Checkpointed {
    const ID: ContractId = ContractId::new("practice-counterpart-checkpoint");
    const CHECKPOINT: bool = true;
}

pub(crate) struct CounterpartReply<M>(PhantomData<fn() -> M>);

impl<M: Mode> Contract for CounterpartReply<M> {
    const ID: ContractId = M::ID;
    type Input = Input;
}

/// A part of the learner's work the reply may point at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Place {
    part: PartRef,
    name: Told<String>,
}

impl shown::Kind for Place {
    const PREFIX: &'static str = "l";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = PartRef;

    fn reference(&self) -> PartRef {
        self.part.clone()
    }
}

/// A question the exercise already answers, and its answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Fact {
    at: usize,
    question: Told<String>,
    answer: Told<String>,
}

impl shown::Kind for Fact {
    const PREFIX: &'static str = "k";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// A reply the kind's checks refused, and why.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Earlier {
    reply: Told<String>,
    why: Told<String>,
}

/// What one request says about the counterpart's turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Input {
    system: Told<String>,
    observed: Told<String>,
    places: Vec<Place>,
    facts: Vec<Fact>,
    earlier: Option<Earlier>,
}

impl Input {
    fn of(
        kind: &Kind,
        policy: &Policy,
        cue: &Cue,
        observation: &Observation,
    ) -> Result<Self> {
        let (brief, piece) = (&observation.brief, &observation.piece);
        let places = kind
            .parts
            .all(brief, piece)?
            .into_iter()
            .map(|part| {
                let name = Told(kind.parts.name(brief, piece, &part)?);
                Ok(Place { part, name })
            })
            .collect::<Result<Vec<_>>>()?;
        let facts = kind
            .reply_guard
            .known(brief)?
            .into_iter()
            .enumerate()
            .map(|(at, known)| Fact {
                at,
                question: Told(known.question),
                answer: Told(known.answer),
            })
            .collect();
        Ok(Self {
            system: Told(policy.system(cue, observation)),
            observed: Told(observation.render(kind)?),
            places,
            facts,
            earlier: None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Speech {
    Speaks,
    Silent,
}

impl Vocabulary for Speech {
    const WORDS: &'static [(&'static str, Self)] = &[("true", Self::Speaks), ("false", Self::Silent)];
}

/// Whether the reply gives an answer the exercise already has.
pub(crate) enum Answering {
    Nothing,
    YourCall,
    Covered(Picked<Fact>),
}

impl Union for Answering {
    fn forms() -> Vec<(&'static str, Form)> {
        vec![
            ("none", Form::new()),
            ("your_call", Form::new()),
            ("from", Form::new().point::<Fact>("which")),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        match kind {
            "none" => Some(Self::Nothing),
            "your_call" => Some(Self::YourCall),
            "from" => Some(Self::Covered(fields.point::<Fact>("which")?)),
            _ => None,
        }
    }
}

/// What the model wrote in one reply.
pub(crate) struct Said<M> {
    pub(crate) content: String,
    pub(crate) parts: Vec<Picked<Place>>,
    pub(crate) clarification: Answering,
    pub(crate) speech: Speech,
    mode: PhantomData<fn() -> M>,
}

impl<M: Mode> Authored for Said<M> {
    fn form() -> Form {
        let form = Form::new()
            .text("content")
            .cite::<Place>("parts")
            .one_of::<Answering>("clarification");
        if M::CHECKPOINT {
            form.word::<Speech>("speak")
        } else {
            form
        }
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let content = fields.text("content")?;
        let parts = fields.cite::<Place>("parts");
        let clarification = fields.one_of::<Answering>("clarification")?;
        let speech = if M::CHECKPOINT {
            fields.word::<Speech>("speak")?
        } else {
            Speech::Speaks
        };
        Some(Self {
            content,
            parts,
            clarification,
            speech,
            mode: PhantomData,
        })
    }
}

impl<M: Mode> Answered for CounterpartReply<M> {
    type Answer = Value<Said<M>>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let places = showing.show(&input.places)?;
        let facts = showing.show(&input.facts)?;
        let mut closing = Vec::new();
        if !places.is_empty() {
            let mut rows = String::from("Parts of the learner's work you may point at, each under its handle:");
            for (handle, place) in places.iter() {
                let _ = write!(rows, "\n{} {}", handle.bracketed(), place.name.0);
            }
            closing.push(rows);
        }
        if !facts.is_empty() {
            let mut rows = String::from("What the problem already answers, each under its handle:");
            for (handle, fact) in facts.iter() {
                let _ = write!(
                    rows,
                    "\n{} asked \"{}\", answered \"{}\"",
                    handle.bracketed(),
                    fact.question.0,
                    fact.answer.0
                );
            }
            closing.push(rows);
        }
        if let Some(earlier) = &input.earlier {
            closing.push(format!(
                "Your earlier reply was refused.\nYour earlier reply: {}\nWhy it was refused: {}\nWrite the reply again.",
                earlier.reply.0, earlier.why.0
            ));
        }
        let mut says = Says::new(input.system.0.clone(), input.observed.0.clone());
        says.closing = closing.join("\n\n");
        says.output_tokens = REPLY_OUTPUT_TOKENS;
        Ok(says)
    }

    fn accept(_input: &Input, said: &Said<M>) -> std::result::Result<(), Refusal> {
        if said.speech == Speech::Speaks && said.content.trim().is_empty() {
            return Err(Refusal("The reply is blank. Say something.".to_owned()));
        }
        Ok(())
    }
}

/// What the model said, once asked.
struct Heard {
    content: String,
    parts: Vec<PartRef>,
    clarification: Answering,
    silent: bool,
}

async fn heard<M: Mode>(llm: &dyn LLMPort, input: &Input, scope: &mut Scope) -> Result<Heard> {
    let settled = ask::<CounterpartReply<M>>(
        llm,
        &Asking {
            progress: None,
            reasoning: None,
        },
        scope,
        input,
        &model_call::answer_options(),
    )
    .await?;
    match settled {
        Settled::Answered(said) => Ok(Heard {
            content: said.content,
            parts: said.parts.into_iter().map(|part| part.reference).collect(),
            clarification: said.clarification,
            silent: said.speech == Speech::Silent,
        }),
        Settled::Unanswered(why) => Err(ask::failed(&why)),
    }
}

/// The model's words, then the words the app writes for the clarification it pointed at.
fn composed(heard: &Heard, known: &[Known], otherwise: Option<String>) -> String {
    let added = match &heard.clarification {
        Answering::Nothing => None,
        Answering::YourCall => otherwise,
        Answering::Covered(picked) => known
            .get(picked.reference)
            .map(|known| known.answer.clone()),
    };
    match added {
        Some(words) => format!("{}\n\n{words}", heard.content.trim_end()),
        None => heard.content.clone(),
    }
}

/// The reason a refusal gives, or the fault itself.
fn why_of(check: Result<()>) -> Result<Option<String>> {
    match check {
        Ok(()) => Ok(None),
        Err(AppError::InvalidInput(why)) => Ok(Some(why)),
        Err(error) => Err(error),
    }
}

/// Why a check refuses `content`, apart from running anything in it, or None when every check accepts it.
async fn refusal(
    kind: &Kind,
    exercise: &Exercise,
    policy: &Policy,
    cue: &Cue,
    observation: &Observation,
    content: &str,
    cancel: CancellationToken,
) -> Result<Option<String>> {
    let guard = &kind.reply_guard;
    if let Some(why) = why_of(guard.check_words(exercise, content))? {
        return Ok(Some(why));
    }
    if let Some(why) = why_of(policy.check(content, cue, observation))? {
        return Ok(Some(why));
    }
    why_of(guard.check_unsolved(exercise, content, cancel).await)
}

/// The counterpart's checked reply on `cue`, or None when it chose silence (only on a checkpoint).
pub async fn respond(
    llm: &dyn LLMPort,
    kind: &Kind,
    exercise: &Exercise,
    policy: &Policy,
    cue: &Cue,
    observation: &Observation,
    cancel: CancellationToken,
) -> Result<Option<Reply>> {
    let known = kind.reply_guard.known(&observation.brief)?;
    let mut input = Input::of(kind, policy, cue, observation)?;
    let mut scope = Scope::for_question();
    let mut again = false;
    loop {
        let heard = if *cue == Cue::Checkpoint {
            heard::<Checkpointed>(llm, &input, &mut scope).await?
        } else {
            heard::<Ordinary>(llm, &input, &mut scope).await?
        };
        if heard.silent {
            return Ok(None);
        }
        let content = composed(&heard, &known, kind.reply_guard.otherwise());
        let why = match refusal(
            kind,
            exercise,
            policy,
            cue,
            observation,
            &content,
            cancel.clone(),
        )
        .await?
        {
            None => {
                return Ok(Some(Reply {
                    content,
                    parts: heard.parts,
                }))
            }
            Some(why) => why,
        };
        if again {
            return Err(AppError::ModelAnswerUnusable(why));
        }
        input.earlier = Some(Earlier {
            reply: Told(content),
            why: Told(why),
        });
        again = true;
    }
}
