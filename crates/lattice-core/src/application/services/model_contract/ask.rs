//! Asking: one driver, one settlement rule.
//!
//! Three owners, and no decision made twice. A [`Transport`] sends one request
//! and reports how the reply ended; it decides nothing. This module decides
//! everything about an answer: whether it was read, what is unanswered,
//! whether a correction is due, whether to send larger, fewer or again, and
//! how each item settles. A contract's `accept` is the only source of a
//! refusal about content, and it speaks about one statement.
//!
//! `Err` is a fault of sending: an outage, a cancellation. Nothing about an
//! answer's content is an `Err`.

use async_trait::async_trait;

use super::request::GroundedRequest;
use super::shape::{Ended, Found, Listed, PerItem, Shape, Single, Written};
use super::shown::{Kind, Picked, Scope, Showing};
use super::{tagged, Contract, ContractCall};
use crate::application::ports::llm_port::{Finish, LLMPort};
use crate::application::services::grounded_generation::{
    CallOptions, EvidencePassage, EvidenceSelection, GroundedGenerationError, GroundedOutput,
    HistoryItem,
};
use crate::shared::error::{AppError, Result};

#[cfg(test)]
pub(super) use super::shape::{Held, Schema};
#[cfg(test)]
pub(super) use crate::application::services::grounded_generation;
#[cfg(test)]
pub(super) use crate::shared::error::ErrorClass;
#[cfg(test)]
pub(super) use diagnostics::{CUT_OFF, NOT_READ, UNPLACED};
#[cfg(test)]
pub(super) use fitting::fits;

pub use super::shape::{Settled, Why};
pub use diagnostics::failed;
pub use each::{ask_each, Entry};
pub use fitting::room;
pub use transport::{Metered, Plain};

use diagnostics::{alone, reason, told, trace};
use fitting::{correction_for, fitted, larger_if_room, render};

mod diagnostics;
mod each;
mod fitting;
#[cfg(test)]
mod tests;
mod transport;

/// A contract whose answer has a shape. A contract gains this when it moves
/// off the closure form of `answer()`.
pub trait Answered: Contract {
    /// The only definition of this call's schema and reading.
    type Answer: Shape;
    /// What an item unanswered even alone does.
    const ALONE: Alone;

    /// What the request says for `input`. Whatever the answer may point at is
    /// shown through `showing`; context the model only reads is written plainly.
    fn says(input: &Self::Input, showing: &mut Showing<'_>) -> Result<Says>;

    /// Rules on what the model authored. A statement arrives with its
    /// pointers already resolved to the app's own values, so there is no
    /// written id left to compare.
    fn accept(
        _input: &Self::Input,
        _statement: &<Self::Answer as Shape>::Statement,
    ) -> std::result::Result<(), Refusal> {
        Ok(())
    }

    /// [`accept`](Self::accept) for one entry of a per-item answer, given the item the entry is about.
    fn accept_for(
        input: &Self::Input,
        _of: &Picked<<Self::Answer as PerItem>::Item>,
        statement: &<Self::Answer as Shape>::Statement,
    ) -> std::result::Result<(), Refusal>
    where
        Self::Answer: PerItem,
    {
        Self::accept(input, statement)
    }

    /// `input` holding only the opening of its material that `room` tokens
    /// hold, for a contract whose request carries material the window may not
    /// hold whole ([`Packed`](super::packed::Packed)). `None`: it carries none.
    fn within(_input: &Self::Input, _room: usize) -> Option<Self::Input> {
        None
    }
}

/// What an item that stays unanswered alone does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alone {
    /// It stays open, and the feature's own type says what that means.
    Pending,
    /// The answer is the operation's product: without it the operation fails.
    Fails,
}

/// Why `accept` refuses what the model authored, in words the correction tells the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(pub String);

/// What a request says, but for the answer's schema, which the shape supplies.
#[derive(Debug, Clone)]
pub struct Says {
    /// The system message.
    pub instructions: String,
    /// What the model is asked to do, written before the evidence.
    pub task: String,
    /// Written on the line above the evidence. May be empty.
    pub evidence_heading: String,
    pub evidence: Vec<EvidencePassage>,
    pub selection: EvidenceSelection,
    /// Written after the evidence. May be empty.
    pub closing: String,
    /// Sent whole ahead of the task.
    pub history: Vec<HistoryItem>,
    /// The output room the answer is promised. Zero takes the planner's default.
    pub output_tokens: usize,
    pub output_fills_window: bool,
}

impl Says {
    pub fn new(instructions: impl Into<String>, task: impl Into<String>) -> Self {
        Self {
            instructions: instructions.into(),
            task: task.into(),
            evidence_heading: String::new(),
            evidence: Vec::new(),
            selection: EvidenceSelection::All,
            closing: String::new(),
            history: Vec::new(),
            output_tokens: 0,
            output_fills_window: false,
        }
    }
}

/// The sends of a reply and the tokens the model reported for them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub sends: u64,
    pub input: u64,
    pub output: u64,
}

/// A reply and the step it answers.
#[derive(Debug, Clone)]
pub struct Sent {
    /// The step the reply answers: the request, or for a transport that drafts first, its answer step.
    pub request: GroundedRequest,
    pub text: String,
    pub finish: Finish,
    pub usage: Usage,
}

impl Sent {
    /// `output` as the reply to `request`.
    pub fn of(request: GroundedRequest, output: GroundedOutput) -> Self {
        let finish = output.finish();
        let usage = Usage {
            sends: 1,
            input: output.input_tokens,
            output: output.output_tokens,
        };
        Self {
            request,
            text: output.text,
            finish,
            usage,
        }
    }
}

/// One step derived from a reply: the step it answers with more room, or
/// that step's conversation continued by a correction. Only the driver makes
/// one, and a transport sends it as it is.
#[derive(Debug, Clone)]
pub struct Followup(GroundedRequest);

impl Followup {
    pub fn request(&self) -> &GroundedRequest {
        &self.0
    }

    pub fn into_request(self) -> GroundedRequest {
        self.0
    }
}

/// How a request is sent. It never builds a request: it receives one and
/// derives the steps of its protocol through the request's own methods.
#[async_trait]
pub trait Transport: Sync {
    /// Sends `request` by this transport's protocol.
    async fn send(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent>;

    /// Sends `step` as the one request it is: the protocol that led to the
    /// reply it derives from is not run again.
    async fn follow(&self, llm: &dyn LLMPort, step: Followup) -> Result<Sent>;

    /// Told that the answer to `request`, with its follow-ups, left something
    /// unanswered. A transport that keeps work between its steps lets it go
    /// here, so that `request` asked again is not answered from it.
    async fn unanswered(&self, _llm: &dyn LLMPort, _request: &GroundedRequest) -> Result<()> {
        Ok(())
    }
}

/// The error of a send that failed. A request the app built and the planner
/// refuses does not fit the window.
pub fn send_error(error: GroundedGenerationError) -> AppError {
    match error {
        GroundedGenerationError::Budget(error) => AppError::WindowTooSmall(error.to_string()),
        GroundedGenerationError::Model(error) => error,
    }
}

/// Asks about the call's one subject. One larger send if the answer was cut
/// off and the backend has room, then one correction, then it settles.
pub async fn ask<C>(
    llm: &dyn LLMPort,
    transport: &impl Transport,
    scope: &mut Scope,
    input: &C::Input,
    call: &CallOptions,
) -> Result<Settled<<C::Answer as Shape>::Statement>>
where
    C: Answered,
    C::Answer: Single,
{
    let cut = fitted::<C>(llm, scope, input, call).await?;
    let input = cut.as_ref().unwrap_or(input);
    let contract = ContractCall::of::<C>(input)?;
    let tagging = tagged(llm, &contract);
    let llm: &dyn LLMPort = &tagging;
    let (showing, request) = render::<C>(llm, scope, input, call)?;
    let read = |sent: &Sent| match <C::Answer as Single>::read(&written(sent), &showing) {
        Settled::Answered(statement) => Settled::Answered(((), statement)),
        Settled::Unanswered(why) => Settled::Unanswered(why),
    };
    let accept = |_: &(), statement: &Statement<C>| C::accept(input, statement);
    let settled = alone::<C, ()>(llm, transport, request, "", false, read, accept).await?;
    Ok(match settled {
        Settled::Answered(((), statement)) => Settled::Answered(statement),
        Settled::Unanswered(why) => Settled::Unanswered(why),
    })
}

/// Asks for new things with `input`. Each item that closed stands, also in an
/// answer cut off after it; an item `accept` refuses is left out and traced.
/// An answer whose items were all left out, with nothing standing, is unanswered.
/// After an answer that was not read to its end, `rest_for` is given what
/// stands so far and returns the input that asks for the rest, or `None` to stop.
pub async fn ask_new<C>(
    llm: &dyn LLMPort,
    transport: &impl Transport,
    scope: &mut Scope,
    mut input: C::Input,
    rest_for: impl Fn(&[<C::Answer as Shape>::Statement]) -> Option<C::Input> + Send + Sync,
    call: &CallOptions,
) -> Result<Found<<C::Answer as Shape>::Statement>>
where
    C: Answered,
    C::Answer: Listed,
{
    let mut standing: Vec<Statement<C>> = Vec::new();
    let mut left_out = 0;
    loop {
        if let Some(cut) = fitted::<C>(llm, scope, &input, call).await? {
            input = cut;
        }
        let contract = ContractCall::of::<C>(&input)?;
        let tagging = tagged(llm, &contract);
        let llm: &dyn LLMPort = &tagging;
        let (showing, request) = render::<C>(llm, scope, &input, call)?;
        let asked = request.clone();
        let cut_off_round = |request: GroundedRequest| Round {
            sent: Sent {
                request,
                text: String::new(),
                finish: Finish::CutOff,
                usage: Usage::default(),
            },
            rest: Some(Why::CutOff),
            added: 0,
        };
        let mut outcome = match transport.send(llm, request).await {
            Ok(sent) => new_round::<C>(&input, sent, &showing, &mut standing, &mut left_out),
            Err(AppError::AnswerCutOff(_)) => cut_off_round(asked.clone()),
            Err(error) => return Err(error),
        };
        if outcome.added == 0 && outcome.rest.is_some() {
            if outcome.rest == Some(Why::CutOff) {
                if let Some(larger) = larger_if_room(llm, &outcome.sent).await {
                    let larger_asked = larger.request().clone();
                    outcome = match transport.follow(llm, larger).await {
                        Ok(sent) => {
                            new_round::<C>(&input, sent, &showing, &mut standing, &mut left_out)
                        }
                        Err(AppError::AnswerCutOff(_)) => cut_off_round(larger_asked),
                        Err(error) => return Err(error),
                    };
                }
            }
            if outcome.added == 0 && standing.is_empty() {
                let told = outcome.rest.as_ref().and_then(|why| told(why, ""));
                if let Some(told) = told {
                    if let Some(again) = correction_for(llm, &outcome.sent, &told).await {
                        let again_asked = again.request().clone();
                        outcome = match transport.follow(llm, again).await {
                            Ok(sent) => {
                                new_round::<C>(&input, sent, &showing, &mut standing, &mut left_out)
                            }
                            Err(AppError::AnswerCutOff(_)) => cut_off_round(again_asked),
                            Err(error) => return Err(error),
                        };
                    }
                }
            }
        }
        let Round { rest, added, .. } = outcome;
        if rest.is_some() {
            transport.unanswered(llm, &asked).await?;
        }
        if rest.is_none() || added == 0 {
            return found::<C>(standing, rest, left_out);
        }
        match rest_for(&standing) {
            Some(next) => input = next,
            None => return found::<C>(standing, rest, left_out),
        }
    }
}

/// A reply for new things, and what it added.
struct Round {
    sent: Sent,
    rest: Option<Why>,
    added: usize,
}

/// Appends to `standing` every item of the reply `sent` that `accept` takes.
/// A reply read to its end that held items and leaves nothing standing is
/// unanswered: for the first refusal's reason, or as not read.
fn new_round<C>(
    input: &C::Input,
    sent: Sent,
    showing: &Showing<'_>,
    standing: &mut Vec<Statement<C>>,
    left_out: &mut usize,
) -> Round
where
    C: Answered,
    C::Answer: Listed,
{
    let Found {
        items,
        rest,
        left_out: unread,
        unread_why,
    } = <C::Answer as Listed>::read(&written(&sent), showing);
    *left_out += unread;
    let before = standing.len();
    let mut refusal = None;
    for statement in items {
        match C::accept(input, &statement) {
            Ok(()) => standing.push(statement),
            Err(Refusal(why)) => {
                *left_out += 1;
                trace::<C>("", "refused");
                if refusal.is_none() {
                    refusal = Some(why);
                }
            }
        }
    }
    let added = standing.len().saturating_sub(before);
    let rest = match rest {
        None if standing.is_empty() && (unread > 0 || refusal.is_some()) => {
            let unread_why = unread_why.unwrap_or(Why::NotRead);
            Some(refusal.map_or(unread_why, Why::Refused))
        }
        rest => rest,
    };
    Round { sent, rest, added }
}

/// What `ask_new` returns: an outcome with no item whose answer never ended fails when [`Alone::Fails`].
fn found<C: Answered>(
    items: Vec<Statement<C>>,
    rest: Option<Why>,
    left_out: usize,
) -> Result<Found<Statement<C>>> {
    if let Some(why) = &rest {
        if !matches!(why, Why::Refused(_)) {
            trace::<C>("", reason(why));
        }
        if items.is_empty() && C::ALONE == Alone::Fails {
            return Err(failed(why));
        }
    }
    Ok(Found {
        items,
        rest,
        left_out,
        unread_why: None,
    })
}

type Statement<C> = <<C as Answered>::Answer as Shape>::Statement;

fn ended(finish: Finish) -> Ended {
    match finish {
        Finish::Complete => Ended::Complete,
        Finish::CutOff | Finish::Filtered => Ended::CutOff,
        Finish::Declined => Ended::Declined,
    }
}

fn written(sent: &Sent) -> Written<'_> {
    Written {
        text: &sent.text,
        ended: ended(sent.finish),
    }
}

/// The written handle `showing` gives `item`; `None` when it shows none.
fn handle_of<K: Kind>(showing: &Showing<'_>, item: &K) -> Option<String> {
    showing.handle(item).map(|handle| handle.written())
}
