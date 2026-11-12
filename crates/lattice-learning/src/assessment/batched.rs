//! Questions put to a model in as many requests as its window needs, shared
//! by the blind solver and the graders.
use crate::assessment_generation::text_call;
use crate::lessons::structured_call::complete_checked_request;
use crate::model_call;
use lattice_core::application::services::model_contract::{Contract, PromptInput};
use lattice_core::{
    application::ports::LLMPort,
    application::services::grounded_generation::GroundedRequest,
    shared::error::{smaller_parts_may_succeed, AppError, Result},
};

/// What a batch does with an item whose request fails even when it goes alone.
pub(crate) enum Alone<'a> {
    /// The whole call fails as unusable, with this text.
    Fails(&'a str),
    /// The item is left without an answer and the rest go on.
    Pending,
}

/// One answer per item, in item order, `None` for an item left pending.
/// Items go in as many requests as the window needs: `request_for` builds the
/// request for a slice of item indices and `read` takes the answers it can
/// read for those indices out of a reply, or refuses the reply. The
/// transport asks a single item cut off at its output reservation again with
/// twice the room, and a refused reply once more, saying why. A request that
/// still fails, or is cut off while holding several items, is asked for again
/// with half the items; an item that fails alone is settled by `alone`.
pub(crate) async fn answers<C: Contract<Input = PromptInput>, T: Send>(
    llm: &dyn LLMPort,
    count: usize,
    request_for: impl Fn(&[usize]) -> GroundedRequest,
    read: impl Fn(&str, &[usize]) -> Result<Vec<(usize, T)>> + Send + Sync,
    window: &str,
    alone: Alone<'_>,
) -> Result<Vec<Option<T>>> {
    let mut answered: Vec<Option<T>> = (0..count).map(|_| None).collect();
    let mut start = 0;
    // The most items a request may hold: every item, until a request fails.
    let mut most = count;
    while start < count {
        let indices_for = |size: usize| -> Vec<usize> { (start..count).take(size).collect() };
        let mut size = (count - start).min(most);
        while size > 1 && !model_call::fits(llm, &request_for(indices_for(size).as_slice())).await {
            size /= 2;
        }
        let indices = indices_for(size);
        let request = request_for(indices.as_slice());
        if size == 1 && !model_call::fits(llm, &request).await {
            return Err(model_call::window_too_small(llm, window));
        }
        let call = text_call::<C>(&request)?.splitting(size > 1);
        let sent = complete_checked_request(llm, &call, request, None, None, |text| {
            read(text, indices.as_slice())
        })
        .await;
        match sent {
            Ok(read_now) => {
                for (index, answer) in read_now {
                    if let Some(slot) = answered.get_mut(index).filter(|slot| slot.is_none()) {
                        if indices.contains(&index) {
                            *slot = Some(answer);
                        }
                    }
                }
                start += size;
            }
            Err(error)
                if smaller_parts_may_succeed(&error)
                    || matches!(error, AppError::ModelAnswerUnusable(_)) =>
            {
                if size > 1 {
                    most = size / 2;
                    continue;
                }
                match alone {
                    Alone::Fails(unusable) => {
                        return Err(AppError::ModelAnswerUnusable(unusable.into()))
                    }
                    Alone::Pending => {
                        tracing::warn!(
                            item = start,
                            %error,
                            window,
                            "An item's answer was unusable alone; it stays pending"
                        );
                        start += 1;
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(answered)
}
