//! Fitting a request to the model's context window, choosing its schema,
//! and building follow-ups when a reply is cut off or refused.

use super::super::request::GroundedRequest;
use super::super::shape::{Held, Schema, Shape};
use super::super::shown::{Scope, Showing};
use super::{Answered, Followup, Says, Sent};
use crate::application::ports::llm_port::LLMPort;
use crate::application::services::grounded_generation::{self, CallOptions};
use crate::shared::error::Result;

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

/// The schema the backend holds the model to: with the lengths the request sets, else with enumerated
/// pointers, else with plain ones.
fn schema_for<C: Answered>(llm: &dyn LLMPort, showing: &Showing<'_>) -> Option<Schema> {
    let counts = <C::Answer as Shape>::schema(showing, Held::Counts)?;
    if llm.enforces(&counts) {
        return Some(counts);
    }
    let enums = <C::Answer as Shape>::schema(showing, Held::Enums)?;
    if enums != counts && llm.enforces(&enums) {
        return Some(enums);
    }
    <C::Answer as Shape>::schema(showing, Held::Plain)
}

/// What `input` shows on a fresh request of `scope`, and the request that says it.
pub(super) fn render<'s, C: Answered>(
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
pub(super) async fn fitted<C: Answered>(
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
pub(super) async fn fits(llm: &dyn LLMPort, request: &GroundedRequest) -> bool {
    grounded_generation::measure(llm, request)
        .await
        .is_ok_and(|measured| measured.fits())
}

/// The step `sent` answers with twice the output room, when the backend has the room
/// and the planner's cap leaves the doubled room larger than the room it had.
pub(super) async fn larger_if_room(llm: &dyn LLMPort, sent: &Sent) -> Option<Followup> {
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
pub(super) async fn correction_for(llm: &dyn LLMPort, sent: &Sent, told: &str) -> Option<Followup> {
    let corrected = sent.request.corrected(&sent.text, told);
    if fits(llm, &corrected).await {
        return Some(Followup(corrected));
    }
    let restated = sent.request.restated(told);
    fits(llm, &restated).await.then_some(Followup(restated))
}
