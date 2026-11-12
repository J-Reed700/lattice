//! Questions put to a model in as many requests as its window needs, shared
//! by the blind solver.
use crate::model_call;
use lattice_core::application::services::model_contract::ContractCall;
use lattice_core::{
    application::ports::{llm_port::Finish, LLMPort},
    application::services::grounded_generation::GroundedRequest,
    shared::error::{AppError, Result},
};
use std::collections::BTreeMap;

/// One answer per item, in item order. Items go in as many requests as the
/// window needs: `request_for` builds the request for a slice of item
/// indices and `read` takes the answers it can read for those indices out of
/// the parsed reply. An answer cut off at the output reservation is asked
/// for again with half the items; an incomplete one is asked for again once,
/// then the call fails with `unusable` rather than approve anything.
pub(crate) async fn answers<T>(
    llm: &dyn LLMPort,
    call: &ContractCall,
    count: usize,
    request_for: impl Fn(&[usize]) -> GroundedRequest,
    read: impl Fn(&str) -> Result<Vec<(usize, T)>>,
    window: &str,
    unusable: &str,
) -> Result<Vec<T>> {
    let unusable = || AppError::ModelAnswerUnusable(unusable.into());
    let mut answered: Vec<Option<T>> = (0..count).map(|_| None).collect();
    let mut start = 0;
    // The most items one request may hold once an answer was cut off at the
    // output reservation, which is an estimate.
    let mut room = usize::MAX;
    'chunks: while start < count {
        let mut size = (count - start).min(room);
        let indices_for = |size: usize| -> Vec<usize> { (start..count).take(size).collect() };
        while size > 1 && !model_call::fits(llm, &request_for(indices_for(size).as_slice())).await {
            size = size.div_ceil(2);
        }
        let indices = indices_for(size);
        let mut read_so_far: BTreeMap<usize, T> = BTreeMap::new();
        for attempt in 0..2 {
            let request = request_for(indices.as_slice());
            let response = model_call::send(
                llm,
                call,
                request,
                None,
                model_call::TooLarge::Window(window),
            )
            .await?;
            if Finish::of(&response.finish_reason) == Finish::CutOff {
                if size == 1 {
                    return Err(unusable());
                }
                room = size.div_ceil(2);
                continue 'chunks;
            }
            if let Ok(answers) = read(&response.text) {
                for (index, answer) in answers {
                    if indices.contains(&index) && !read_so_far.contains_key(&index) {
                        read_so_far.insert(index, answer);
                    }
                }
            }
            if indices.iter().all(|i| read_so_far.contains_key(i)) {
                break;
            }
            tracing::warn!(attempt, window, "The answer was incomplete; asking again");
        }
        if !indices.iter().all(|i| read_so_far.contains_key(i)) {
            return Err(unusable());
        }
        for (index, answer) in read_so_far {
            if let Some(slot) = answered.get_mut(index) {
                *slot = Some(answer);
            }
        }
        start += size;
    }
    answered
        .into_iter()
        .map(|answer| answer.ok_or_else(unusable))
        .collect()
}
