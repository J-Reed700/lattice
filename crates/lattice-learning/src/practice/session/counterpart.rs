//! The counterpart's reply, for every kind of work: asked for, checked, asked again once with the reason, or
//! refused.

use crate::lessons::structured_call::{complete_checked_request, reask};
use crate::model_call;
use crate::practice::session::kind::Kind;
use crate::practice::session::observation::Observation;
use crate::practice::session::policy::{Cue, Policy};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::{
    Contract, ContractCall, ContractId, PromptInput,
};
use lattice_core::domain::learning::practice::work::{Exercise, PartRef};
use lattice_core::shared::error::{AppError, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tokio_util::sync::CancellationToken;

#[cfg(test)]
mod tests;

struct CounterpartReply;

impl Contract for CounterpartReply {
    const ID: ContractId = ContractId::new("practice-counterpart-reply");
    type Input = PromptInput;
}

/// What the model is asked to reserve for one reply, schema fields included.
const REPLY_OUTPUT_TOKENS: usize = 2048;

/// A reply that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub content: String,
    /// The parts of the learner's work the reply points at.
    pub parts: Vec<PartRef>,
}

#[derive(Deserialize)]
struct Written {
    content: String,
    #[serde(default)]
    speak: Option<bool>,
    #[serde(flatten)]
    fields: Map<String, Value>,
}

fn refuse(reason: impl Into<String>) -> AppError {
    AppError::InvalidInput(reason.into())
}

/// The kind's reply fields read as `T`.
pub fn fields_as<T: DeserializeOwned>(fields: &Map<String, Value>) -> Result<T> {
    crate::generation::parse_json(&Value::Object(fields.clone()).to_string())
}

/// The schema of a reply on `cue`: content, the kind's fields, and on a checkpoint whether to speak.
fn reply_schema(kind: &Kind, observation: &Observation, cue: &Cue) -> Result<Value> {
    let mut properties = Map::new();
    properties.insert("content".to_owned(), json!({"type": "string"}));
    for (name, schema) in kind.reply_guard.fields(&observation.brief)? {
        properties.insert(name, schema);
    }
    if *cue == Cue::Checkpoint {
        properties.insert("speak".to_owned(), json!({"type": "boolean"}));
    }
    let required: Vec<String> = properties.keys().cloned().collect();
    Ok(json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties,
    }))
}

/// Reads and checks a reply, apart from running anything in it. None is silence.
fn accept(
    raw: &str,
    kind: &Kind,
    exercise: &Exercise,
    policy: &Policy,
    cue: &Cue,
    observation: &Observation,
) -> Result<Option<Reply>> {
    let written: Written = crate::generation::parse_json(raw)?;
    if written.speak == Some(false) {
        return if *cue == Cue::Checkpoint {
            Ok(None)
        } else {
            Err(refuse(
                "Only a checkpoint may stay silent. Answer the learner.",
            ))
        };
    }
    let content = written.content;
    if content.trim().is_empty() {
        return Err(refuse("The reply is blank. Say something."));
    }

    let guard = &kind.reply_guard;
    let parts = guard.parts(&observation.brief, &observation.piece, &written.fields)?;

    let internal = exercise
        .cases
        .iter()
        .map(|case| case.id.clone())
        .chain(
            exercise
                .misreadings
                .iter()
                .map(|misreading| misreading.id.clone()),
        )
        .chain(observation.internal_ids());
    for id in internal.filter(|id| !id.is_empty()) {
        if content.contains(&id) {
            return Err(refuse(format!(
                "The reply contains the internal id {id}. Never write an id; describe a case by what it is about."
            )));
        }
    }

    guard.check_words(exercise, &written.fields, &content)?;
    policy.check(&content, cue, observation)?;
    Ok(Some(Reply { content, parts }))
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
    let mut request = model_call::structured(
        &policy.system(cue, observation),
        observation.render(kind)?,
        reply_schema(kind, observation, cue)?,
        REPLY_OUTPUT_TOKENS,
    );
    let call = ContractCall::of::<CounterpartReply>(&PromptInput {
        system: request.instructions.clone(),
        prompt: request.task.clone(),
    })?;
    let mut again = false;
    loop {
        let (raw, reply) =
            complete_checked_request(llm, &call, request.clone(), None, None, |raw| {
                accept(raw, kind, exercise, policy, cue, observation)
                    .map(|reply| (raw.to_owned(), reply))
            })
            .await?;
        let Some(reply) = reply else {
            return Ok(None);
        };
        let why = match kind
            .reply_guard
            .check_unsolved(exercise, &reply.content, cancel.clone())
            .await
        {
            Ok(()) => return Ok(Some(reply)),
            Err(AppError::InvalidInput(why)) => why,
            Err(error) => return Err(error),
        };
        if again {
            return Err(AppError::ModelAnswerUnusable(why));
        }
        request = reask(request, &raw, &why);
        again = true;
    }
}
