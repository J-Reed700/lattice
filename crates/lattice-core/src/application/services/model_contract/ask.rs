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

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;

use super::request::GroundedRequest;
use super::shape::{Ended, Found, Listed, PerItem, Pointers, Rows, Schema, Shape, Single, Written};
use super::shown::{Kind, Picked, Scope, Showing};
use super::{tagged, Contract, ContractCall};
use crate::application::ports::llm_port::{Finish, LLMPort};
use crate::application::services::grounded_generation::{
    self, CallOptions, EvidencePassage, EvidenceSelection, GroundedGenerationError, GroundedOutput,
    HistoryItem,
};
use crate::shared::error::{AppError, ErrorClass, Result};

pub use super::shape::{Settled, Why};

#[cfg(test)]
mod tests;

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

/// A request sent as it is.
pub struct Plain;

#[async_trait]
impl Transport for Plain {
    async fn send(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent> {
        let output = grounded_generation::generate(llm, request.clone())
            .await
            .map_err(send_error)?;
        Ok(Sent::of(request, output))
    }

    async fn follow(&self, llm: &dyn LLMPort, step: Followup) -> Result<Sent> {
        self.send(llm, step.0).await
    }
}

/// A transport that adds up the usage of every reply it passes through.
pub struct Metered<'a, T: Transport> {
    inner: &'a T,
    used: Mutex<Usage>,
}

impl<'a, T: Transport> Metered<'a, T> {
    pub fn new(inner: &'a T) -> Self {
        Self {
            inner,
            used: Mutex::new(Usage::default()),
        }
    }

    /// What the replies passed through so far used.
    pub fn used(&self) -> Usage {
        *self.used.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn counted(&self, sent: Sent) -> Sent {
        let mut used = self.used.lock().unwrap_or_else(PoisonError::into_inner);
        used.sends = used.sends.saturating_add(sent.usage.sends);
        used.input = used.input.saturating_add(sent.usage.input);
        used.output = used.output.saturating_add(sent.usage.output);
        sent
    }
}

#[async_trait]
impl<T: Transport> Transport for Metered<'_, T> {
    async fn send(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent> {
        let sent = self.inner.send(llm, request).await?;
        Ok(self.counted(sent))
    }

    async fn follow(&self, llm: &dyn LLMPort, step: Followup) -> Result<Sent> {
        let sent = self.inner.follow(llm, step).await?;
        Ok(self.counted(sent))
    }

    async fn unanswered(&self, llm: &dyn LLMPort, request: &GroundedRequest) -> Result<()> {
        self.inner.unanswered(llm, request).await
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
        let sent = transport.send(llm, request).await?;
        let mut outcome = new_round::<C>(&input, sent, &showing, &mut standing, &mut left_out);
        if outcome.added == 0 && outcome.rest.is_some() {
            if outcome.rest == Some(Why::CutOff) {
                if let Some(larger) = larger_if_room(llm, &outcome.sent).await {
                    let sent = transport.follow(llm, larger).await?;
                    outcome = new_round::<C>(&input, sent, &showing, &mut standing, &mut left_out);
                }
            }
            if outcome.added == 0 && standing.is_empty() {
                let told = outcome.rest.as_ref().and_then(|why| told(why, ""));
                if let Some(told) = told {
                    if let Some(again) = correction_for(llm, &outcome.sent, &told).await {
                        let sent = transport.follow(llm, again).await?;
                        outcome =
                            new_round::<C>(&input, sent, &showing, &mut standing, &mut left_out);
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

/// One item of an [`ask_each`] and how it settled.
pub type Entry<C> = (
    Picked<<<C as Answered>::Answer as PerItem>::Item>,
    <<C as Answered>::Answer as Shape>::Statement,
);

/// Asks about `items` one by one, in as few requests as the backend holds.
/// Returns one settlement per item, in the order of `items`.
///
/// `input_for` builds the contract's input for the items still open and is
/// given the entries settled so far; a contract whose entries constrain each
/// other shows the settled ones as facts. Nothing is applied here: whatever is
/// done with an answer, the feature does once the question has settled.
pub async fn ask_each<C>(
    llm: &dyn LLMPort,
    transport: &impl Transport,
    scope: &mut Scope,
    items: &[<C::Answer as PerItem>::Item],
    input_for: impl Fn(&[&<C::Answer as PerItem>::Item], &[Entry<C>]) -> C::Input + Send + Sync,
    call: &CallOptions,
) -> Result<Vec<Settled<<C::Answer as Shape>::Statement>>>
where
    C: Answered,
    C::Answer: PerItem,
{
    let mut entries: Vec<Entry<C>> = Vec::new();
    let mut owners: Vec<usize> = Vec::new();
    let mut slots: Vec<Option<Settled<Statement<C>>>> = items.iter().map(|_| None).collect();
    let mut queue: VecDeque<Vec<usize>> = VecDeque::new();
    if !items.is_empty() {
        queue.push_back((0..items.len()).collect());
    }
    while let Some(batch) = queue.pop_front() {
        let open: Vec<&<C::Answer as PerItem>::Item> =
            batch.iter().filter_map(|&index| items.get(index)).collect();
        let input = input_for(&open, &entries);
        if let [only] = batch.as_slice() {
            if let Some(item) = items.get(*only) {
                let at = (*only, item);
                let settled = ask_alone::<C>(llm, transport, scope, input, call, at, None).await?;
                record::<C>(&mut slots, &mut owners, &mut entries, *only, settled);
            }
            continue;
        }
        let input = match fitted::<C>(llm, scope, &input, call).await? {
            Some(cut) => cut,
            None => input,
        };
        let contract = ContractCall::of::<C>(&input)?;
        let tagging = tagged(llm, &contract);
        let batch_llm: &dyn LLMPort = &tagging;
        let mut accepted: Vec<(usize, Entry<C>)> = Vec::new();
        let mut refused: Vec<(usize, String)> = Vec::new();
        let mut complete = false;
        let mut split = false;
        {
            let (showing, request) = render::<C>(batch_llm, scope, &input, call)?;
            let shown = batch
                .iter()
                .filter(|&&index| {
                    items
                        .get(index)
                        .is_some_and(|item| handle_of(&showing, item).is_some())
                })
                .count();
            let sent = if shown == 0 || !fits(batch_llm, &request).await {
                None
            } else {
                let asked = request.clone();
                match transport.send(batch_llm, request).await {
                    Ok(sent) => Some((asked, sent)),
                    Err(error) if ErrorClass::of(&error) == ErrorClass::DoesNotFit => None,
                    Err(error) => return Err(error),
                }
            };
            if let Some((asked, sent)) = sent {
                complete = sent.finish == Finish::Complete;
                for (picked, settled) in <C::Answer as PerItem>::read(&written(&sent), &showing) {
                    let found = batch.iter().copied().find(|&index| {
                        items
                            .get(index)
                            .is_some_and(|item| item.reference() == picked.reference)
                    });
                    let Some(index) = found else { continue };
                    if accepted.iter().any(|(taken, _)| *taken == index)
                        || refused.iter().any(|(taken, _)| *taken == index)
                    {
                        continue;
                    }
                    if let Settled::Answered(statement) = settled {
                        match C::accept_for(&input, &picked, &statement) {
                            Ok(()) => accepted.push((index, (picked, statement))),
                            Err(Refusal(why)) => {
                                trace::<C>(&picked.handle.written(), "refused");
                                refused.push((index, why));
                            }
                        }
                    }
                }
                if accepted.len() < shown {
                    transport.unanswered(batch_llm, &asked).await?;
                }
            } else {
                split = true;
            }
        }
        if split {
            if let Some((first, second)) = batch.split_at_checked(batch.len() / 2) {
                queue.push_front(second.to_vec());
                queue.push_front(first.to_vec());
            }
            continue;
        }
        let settled_here = accepted.len().saturating_add(refused.len());
        let rest: Vec<usize> = batch
            .iter()
            .copied()
            .filter(|index| {
                !accepted.iter().any(|(taken, _)| taken == index)
                    && !refused.iter().any(|(taken, _)| taken == index)
                    && slots.get(*index).is_some_and(Option::is_none)
            })
            .collect();
        for (index, entry) in accepted {
            owners.push(index);
            entries.push(entry);
        }
        for (index, why) in refused {
            let Some(item) = items.get(index) else {
                continue;
            };
            let alone_input = input_for(&[item], &entries);
            let at = (index, item);
            let settled =
                ask_alone::<C>(llm, transport, scope, alone_input, call, at, Some(why)).await?;
            record::<C>(&mut slots, &mut owners, &mut entries, index, settled);
        }
        if rest.is_empty() {
            continue;
        }
        if complete && settled_here > 0 {
            queue.push_front(rest);
        } else {
            let size = (batch.len() / 2).max(1);
            let chunks: Vec<Vec<usize>> = rest.chunks(size).map(<[usize]>::to_vec).collect();
            for chunk in chunks.into_iter().rev() {
                queue.push_front(chunk);
            }
        }
    }
    for (index, (_, statement)) in owners.into_iter().zip(entries) {
        if let Some(slot) = slots.get_mut(index) {
            *slot = Some(Settled::Answered(statement));
        }
    }
    Ok(slots
        .into_iter()
        .map(|slot| slot.unwrap_or(Settled::Unanswered(Why::Missing)))
        .collect())
}

/// The error an operation fails with when the answer that is its product
/// stayed unanswered ([`Alone::Fails`]).
pub fn failed(why: &Why) -> AppError {
    match why {
        Why::CutOff => AppError::AnswerCutOff("The answer was cut off before it ended.".into()),
        Why::NotRead => AppError::ModelAnswerUnusable("The answer could not be read.".into()),
        Why::Missing => {
            AppError::ModelAnswerUnusable("The answer said nothing about what was asked.".into())
        }
        Why::Unplaced => AppError::ModelAnswerUnusable(
            "The answer pointed at something that was not shown.".into(),
        ),
        Why::Contradicted(what) => AppError::ModelAnswerUnusable(what.clone()),
        Why::Declined => AppError::ModelAnswerUnusable("The model declined to answer.".into()),
        Why::Refused(reason) => AppError::ModelAnswerUnusable(reason.clone()),
        Why::NoRoom => AppError::WindowTooSmall(
            "What was asked about, with the material it needs, does not fit in this model's context window".into(),
        ),
    }
}

type Statement<C> = <<C as Answered>::Answer as Shape>::Statement;

const NOT_READ: &str = "Your answer could not be read. Answer again in the form asked for.";
const CUT_OFF: &str = "Your answer was cut off before it ended. Answer again, more briefly.";
const UNPLACED: &str = "Your answer pointed at something that is not shown. Point only at what is shown, by the name it is shown under.";

/// What a correction tells the model about `why`; `None` when none is due.
fn told(why: &Why, handle: &str) -> Option<String> {
    match why {
        Why::NotRead => Some(NOT_READ.to_owned()),
        Why::CutOff => Some(CUT_OFF.to_owned()),
        Why::Missing if handle.is_empty() => Some(NOT_READ.to_owned()),
        Why::Missing => Some(format!(
            "Your answer said nothing about {handle}. Answer for it."
        )),
        Why::Unplaced => Some(UNPLACED.to_owned()),
        Why::Contradicted(what) => Some(format!("{what} Answer again and say one thing.")),
        Why::Refused(reason) => Some(format!("{reason} Answer again.")),
        Why::Declined | Why::NoRoom => None,
    }
}

/// The fixed word a trace event names `why` by.
fn reason(why: &Why) -> &'static str {
    match why {
        Why::NotRead => "not_read",
        Why::CutOff => "cut_off",
        Why::Missing => "missing",
        Why::Unplaced => "unplaced",
        Why::Contradicted(_) => "contradicted",
        Why::Declined => "declined",
        Why::Refused(_) => "refused",
        Why::NoRoom => "no_room",
    }
}

fn trace<C: Contract>(handle: &str, reason: &'static str) {
    tracing::info!(
        target: "lattice::answers",
        contract = C::ID.as_str(),
        handle = handle,
        reason = reason,
    );
}

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

/// The one place a request is built from what a contract says.
fn request_of(says: Says, schema: Option<Schema>, call: &CallOptions) -> GroundedRequest {
    let mut request = GroundedRequest::of(says.instructions, says.task);
    request.evidence_heading = says.evidence_heading;
    request.evidence = says.evidence;
    request.selection = says.selection;
    request.closing = says.closing;
    request.history = says.history;
    request.output_tokens = says.output_tokens;
    request.output_fills_window = says.output_fills_window;
    request.schema = schema;
    request.call = call.clone();
    request
}

/// The schema with enumerated pointers where the backend enforces it, else with plain ones.
fn schema_for<C: Answered>(llm: &dyn LLMPort, showing: &Showing<'_>) -> Option<Schema> {
    match <C::Answer as Shape>::schema(showing, Pointers::Enum) {
        Some(schema) if llm.enforces(&schema) => Some(schema),
        Some(_) => <C::Answer as Shape>::schema(showing, Pointers::Plain),
        None => None,
    }
}

/// What `input` shows on a fresh request of `scope`, and the request that says it.
fn render<'s, C: Answered>(
    llm: &dyn LLMPort,
    scope: &'s mut Scope,
    input: &C::Input,
    call: &CallOptions,
) -> Result<(Showing<'s>, GroundedRequest)> {
    let mut showing = scope.request();
    let says = C::says(input, &mut showing)?;
    let schema = schema_for::<C>(llm, &showing);
    Ok((showing, request_of(says, schema, call)))
}

/// The tokens the request for `input` leaves for material, by the planner's
/// estimate: for a feature that sizes a search before it asks. Zero when the
/// request itself does not fit.
pub fn room<C: Answered>(llm: &dyn LLMPort, input: &C::Input, call: &CallOptions) -> Result<usize> {
    let mut scope = Scope::for_question();
    let (_, request) = render::<C>(llm, &mut scope, input, call)?;
    Ok(grounded_generation::evidence_room(llm, &at_its_largest(&request)).unwrap_or(0))
}

/// `request` as its largest follow-up sends it: the larger send of
/// [`larger_if_room`], whose doubled reservation also covers the correction,
/// which carries the answer. Material is sized and measured against this.
fn at_its_largest(request: &GroundedRequest) -> GroundedRequest {
    request.larger(2)
}

/// `input` cut to the material its request holds, by the backend's own count
/// where it has one. `None` when the contract packs nothing.
async fn fitted<C: Answered>(
    llm: &dyn LLMPort,
    scope: &mut Scope,
    input: &C::Input,
    call: &CallOptions,
) -> Result<Option<C::Input>> {
    let Some(bare) = C::within(input, 0) else {
        return Ok(None);
    };
    let (_, skeleton) = render::<C>(llm, scope, &bare, call)?;
    let skeleton = at_its_largest(&skeleton);
    let beside = match grounded_generation::estimate(llm, &skeleton) {
        Ok(measured) if measured.estimated < measured.allowed => measured.estimated,
        _ => return Ok(Some(bare)),
    };
    let first = grounded_generation::evidence_room(llm, &skeleton).unwrap_or(0);
    let mut room = first;
    while room > 0 {
        let Some(cut) = C::within(input, room) else {
            break;
        };
        let (_, request) = render::<C>(llm, scope, &cut, call)?;
        room = match grounded_generation::measure(llm, &at_its_largest(&request)).await {
            Ok(measured) if measured.fits() => {
                if room < first {
                    tracing::info!(
                        contract = C::ID.as_str(),
                        room = first,
                        shrunk_to = room,
                        exact = measured.exact,
                        "model_contract.fit"
                    );
                }
                return Ok(Some(cut));
            }
            Ok(measured) => grounded_generation::shrunk(room, beside, &measured),
            Err(_) => room / 2,
        };
    }
    tracing::info!(
        contract = C::ID.as_str(),
        room = first,
        shrunk_to = 0,
        "model_contract.fit"
    );
    Ok(Some(bare))
}

/// Whether `request` fits the window. A request the planner refuses does not.
async fn fits(llm: &dyn LLMPort, request: &GroundedRequest) -> bool {
    grounded_generation::measure(llm, request)
        .await
        .is_ok_and(|measured| measured.fits())
}

/// The step `sent` answers with twice the output room, when the backend has the room
/// and the planner's cap leaves the doubled room larger than the room it had.
async fn larger_if_room(llm: &dyn LLMPort, sent: &Sent) -> Option<Followup> {
    if sent.request.output_fills_window() {
        return None;
    }
    let larger = at_its_largest(&sent.request);
    let grows = matches!(
        (
            grounded_generation::output_room(llm, &larger),
            grounded_generation::output_room(llm, &sent.request),
        ),
        (Ok(doubled), Ok(had)) if doubled > had
    );
    (grows && fits(llm, &larger).await).then_some(Followup(larger))
}

/// The step that asks again after `sent`, quoting `told`; restated when the reply does not fit.
async fn correction_for(llm: &dyn LLMPort, sent: &Sent, told: &str) -> Option<Followup> {
    let corrected = sent.request.corrected(&sent.text, told);
    if fits(llm, &corrected).await {
        return Some(Followup(corrected));
    }
    let restated = sent.request.restated(told);
    fits(llm, &restated).await.then_some(Followup(restated))
}

/// `sent` read, and the statement `accept` took, if it was answered.
fn settle<C: Answered, P>(
    sent: &Sent,
    handle: &str,
    read: &impl Fn(&Sent) -> Settled<(P, Statement<C>)>,
    accept: &impl Fn(&P, &Statement<C>) -> std::result::Result<(), Refusal>,
) -> Settled<(P, Statement<C>)> {
    match read(sent) {
        Settled::Answered((tag, statement)) => match accept(&tag, &statement) {
            Ok(()) => Settled::Answered((tag, statement)),
            Err(Refusal(why)) => {
                trace::<C>(handle, "refused");
                Settled::Unanswered(Why::Refused(why))
            }
        },
        unanswered => unanswered,
    }
}

/// One subject or one item alone: send, one larger send after a cut-off,
/// one correction (unless `corrected`), then settle by [`Answered::ALONE`].
/// A request the backend refuses for its size is unanswered for want of room.
async fn alone<C: Answered, P>(
    llm: &dyn LLMPort,
    transport: &impl Transport,
    request: GroundedRequest,
    handle: &str,
    corrected: bool,
    read: impl Fn(&Sent) -> Settled<(P, Statement<C>)>,
    accept: impl Fn(&P, &Statement<C>) -> std::result::Result<(), Refusal>,
) -> Result<Settled<(P, Statement<C>)>> {
    let asked = request.clone();
    let mut sent = match transport.send(llm, request).await {
        Ok(sent) => sent,
        Err(error)
            if C::ALONE == Alone::Pending && ErrorClass::of(&error) == ErrorClass::DoesNotFit =>
        {
            trace::<C>(handle, reason(&Why::NoRoom));
            return Ok(Settled::Unanswered(Why::NoRoom));
        }
        Err(error) => return Err(error),
    };
    let mut settled = settle::<C, P>(&sent, handle, &read, &accept);
    if matches!(settled, Settled::Unanswered(Why::CutOff)) {
        if let Some(larger) = larger_if_room(llm, &sent).await {
            sent = transport.follow(llm, larger).await?;
            settled = settle::<C, P>(&sent, handle, &read, &accept);
        }
    }
    if !corrected {
        let due = match &settled {
            Settled::Unanswered(why) => told(why, handle),
            Settled::Answered(_) => None,
        };
        if let Some(due) = due {
            if let Some(again) = correction_for(llm, &sent, &due).await {
                let reply = transport.follow(llm, again).await?;
                settled = settle::<C, P>(&reply, handle, &read, &accept);
            }
        }
    }
    if let Settled::Unanswered(why) = &settled {
        transport.unanswered(llm, &asked).await?;
        if !matches!(why, Why::Refused(_)) {
            trace::<C>(handle, reason(why));
        }
        if C::ALONE == Alone::Fails {
            return Err(failed(why));
        }
    }
    Ok(settled)
}

/// The written handle `showing` gives `item`; `None` when it shows none.
fn handle_of<K: Kind>(showing: &Showing<'_>, item: &K) -> Option<String> {
    showing.handle(item).map(|handle| handle.written())
}

/// What a reply said about the item `reference` names.
fn lone<K: Kind, S>(reference: &K::Ref, rows: Rows<K, S>) -> Settled<(Picked<K>, S)> {
    match rows
        .into_iter()
        .find(|(picked, _)| picked.reference == *reference)
    {
        Some((picked, Settled::Answered(statement))) => Settled::Answered((picked, statement)),
        Some((_, Settled::Unanswered(why))) => Settled::Unanswered(why),
        None => Settled::Unanswered(Why::Missing),
    }
}

/// Files what `settled` says of the item at `index` of an `ask_each`.
fn record<C>(
    slots: &mut [Option<Settled<Statement<C>>>],
    owners: &mut Vec<usize>,
    entries: &mut Vec<Entry<C>>,
    index: usize,
    settled: Settled<Entry<C>>,
) where
    C: Answered,
    C::Answer: PerItem,
{
    match settled {
        Settled::Answered(entry) => {
            owners.push(index);
            entries.push(entry);
        }
        Settled::Unanswered(why) => {
            if let Some(slot) = slots.get_mut(index) {
                *slot = Some(Settled::Unanswered(why));
            }
        }
    }
}

/// `at`, an item of an `ask_each` and its place in the items, asked alone
/// about `input`. A request that does not show the item is not sent. After a
/// refusal, the request restates why and the item gets no further correction.
async fn ask_alone<C>(
    llm: &dyn LLMPort,
    transport: &impl Transport,
    scope: &mut Scope,
    input: C::Input,
    call: &CallOptions,
    at: (usize, &<C::Answer as PerItem>::Item),
    refusal: Option<String>,
) -> Result<Settled<Entry<C>>>
where
    C: Answered,
    C::Answer: PerItem,
{
    let (index, item) = at;
    let input = match fitted::<C>(llm, scope, &input, call).await? {
        Some(cut) => cut,
        None => input,
    };
    let contract = ContractCall::of::<C>(&input)?;
    let tagging = tagged(llm, &contract);
    let llm: &dyn LLMPort = &tagging;
    let (showing, request) = render::<C>(llm, scope, &input, call)?;
    let Some(handle) = handle_of(&showing, item) else {
        trace::<C>(&index.to_string(), reason(&Why::NoRoom));
        if C::ALONE == Alone::Fails {
            return Err(failed(&Why::NoRoom));
        }
        return Ok(Settled::Unanswered(Why::NoRoom));
    };
    let corrected = refusal.is_some();
    let request = match refusal.and_then(|why| told(&Why::Refused(why), &handle)) {
        Some(told) => request.restated(&told),
        None => request,
    };
    let reference = item.reference();
    let read = |sent: &Sent| {
        lone(
            &reference,
            <C::Answer as PerItem>::read(&written(sent), &showing),
        )
    };
    let accept = |picked: &Picked<<C::Answer as PerItem>::Item>, statement: &Statement<C>| {
        C::accept_for(&input, picked, statement)
    };
    alone::<C, _>(llm, transport, request, &handle, corrected, read, accept).await
}
