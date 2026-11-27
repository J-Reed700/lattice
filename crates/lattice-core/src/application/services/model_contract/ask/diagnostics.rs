//! Failure mapping, correction prompts, trace diagnostics, and single-item settlement.

use super::super::request::GroundedRequest;
use super::super::shape::{PerItem, Rows, Settled, Why};
use super::super::shown::{Kind, Picked, Scope};
use super::super::{tagged, Contract, ContractCall};
use super::fitting::{correction_for, fitted, larger_if_room, render};
use super::{handle_of, written, Alone, Answered, Entry, Refusal, Sent, Statement, Transport};
use crate::application::ports::llm_port::LLMPort;
use crate::application::services::grounded_generation::CallOptions;
use crate::shared::error::{AppError, ErrorClass, Result};

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

pub(super) const NOT_READ: &str =
    "Your answer could not be read. Answer again in the form asked for.";
pub(super) const CUT_OFF: &str =
    "Your answer was cut off before it ended. Answer again, more briefly.";
pub(super) const UNPLACED: &str = "Your answer pointed at something that is not shown. Point only at what is shown, by the name it is shown under.";

/// What a correction tells the model about `why`; `None` when none is due.
pub(super) fn told(why: &Why, handle: &str) -> Option<String> {
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
pub(super) fn reason(why: &Why) -> &'static str {
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

pub(super) fn trace<C: Contract>(handle: &str, reason: &'static str) {
    tracing::info!(
        target: "lattice::answers",
        contract = C::ID.as_str(),
        handle = handle,
        reason = reason,
    );
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

/// A send that failed: an unusable or oversized request is recorded as
/// unanswered; one the backend refuses for its size leaves a pending ask
/// unanswered for want of room, and every other error stands.
async fn failed_send<C: Answered, P>(
    llm: &dyn LLMPort,
    transport: &impl Transport,
    asked: &GroundedRequest,
    handle: &str,
    error: AppError,
) -> Result<Settled<(P, Statement<C>)>> {
    let class = ErrorClass::of(&error);
    if matches!(class, ErrorClass::Unusable | ErrorClass::DoesNotFit) {
        transport.unanswered(llm, asked).await?;
    }
    if C::ALONE == Alone::Pending && class == ErrorClass::DoesNotFit {
        trace::<C>(handle, reason(&Why::NoRoom));
        return Ok(Settled::Unanswered(Why::NoRoom));
    }
    Err(error)
}

/// One subject or one item alone: send, one larger send after a cut-off,
/// one correction (unless `corrected`), then settle by [`Answered::ALONE`].
/// A request the backend refuses for its size is unanswered for want of room.
pub(super) async fn alone<C: Answered, P>(
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
        Err(error) => return failed_send::<C, P>(llm, transport, &asked, handle, error).await,
    };
    let mut settled = settle::<C, P>(&sent, handle, &read, &accept);
    if matches!(settled, Settled::Unanswered(Why::CutOff)) {
        if let Some(larger) = larger_if_room(llm, &sent).await {
            sent = match transport.follow(llm, larger).await {
                Ok(sent) => sent,
                Err(error) => {
                    return failed_send::<C, P>(llm, transport, &asked, handle, error).await
                }
            };
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
                let reply = match transport.follow(llm, again).await {
                    Ok(reply) => reply,
                    Err(error) => {
                        return failed_send::<C, P>(llm, transport, &asked, handle, error).await
                    }
                };
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

/// `at`, an item of an `ask_each` and its place in the items, asked alone
/// about `input`. A request that does not show the item is not sent. After a
/// refusal, the request restates why and the item gets no further correction.
pub(super) async fn ask_alone<C>(
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
