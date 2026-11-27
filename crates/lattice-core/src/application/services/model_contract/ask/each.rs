//! Batching and per-item settlement for [`PerItem`] questions.

use std::collections::VecDeque;

use super::super::shape::{PerItem, Settled, Shape, Why};
use super::super::shown::{Kind, Picked, Scope};
use super::super::{tagged, ContractCall};
use super::diagnostics::{ask_alone, trace};
use super::fitting::{fits, fitted, render};
use super::{handle_of, written, Answered, Refusal, Statement, Transport};
use crate::application::ports::llm_port::{Finish, LLMPort};
use crate::application::services::grounded_generation::CallOptions;
use crate::shared::error::{ErrorClass, Result};

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
