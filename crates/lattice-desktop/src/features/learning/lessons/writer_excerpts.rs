//! The writer's excerpts in the requests that carry its prompt beside the
//! lesson it wrote: the teaching review and the teaching repair.

use crate::features::learning::lessons::structured_call::{
    complete_checked_request, ReasonFirst, MATERIAL_DOES_NOT_FIT,
};
use crate::features::learning::{generation, model_call, outline_progress::OutlineProgress};
use lattice_core::{
    application::{ports::LLMPort, services::grounded_generation::GroundedRequest},
    shared::error::Result,
};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// The writer's excerpts, taken out of its prompt. The writer packed them into
/// the room its own request left, so a request that carries them beside the
/// lesson does not fit a small window whole: it is sent with those that fit,
/// the ones the lesson cites first. They are a partial selection already
/// (`LESSON_REVIEW_EVIDENCE_SCOPE`), and quotations and claims are checked
/// against the saved sources, not this copy.
pub(super) struct WriterExcerpts {
    excerpts: Vec<Value>,
    /// Each excerpt's position and tokens, the cited first.
    order: Vec<(usize, usize)>,
}

impl WriterExcerpts {
    /// The excerpts out of the writer's prompt `context`, the ones `candidate`
    /// cites first. `None` when it carries none.
    pub(super) fn take(llm: &dyn LLMPort, context: &mut Value, candidate: &Value) -> Option<Self> {
        let excerpts = match context.get_mut("sources") {
            Some(Value::Array(sources)) if !sources.is_empty() => std::mem::take(sources),
            _ => return None,
        };
        let cited = cited_sources(candidate);
        let (mut order, uncited): (Vec<_>, Vec<_>) = excerpts
            .iter()
            .enumerate()
            .map(|(position, excerpt)| {
                let is_cited = excerpt
                    .get("sourceIndex")
                    .and_then(Value::as_u64)
                    .is_some_and(|index| cited.contains(&index));
                // The comma that joins it to the one before it.
                (
                    position,
                    llm.count_tokens(&excerpt.to_string()) + 1,
                    is_cited,
                )
            })
            .partition(|(_, _, is_cited)| *is_cited);
        order.extend(uncited);
        Some(Self {
            excerpts,
            order: order
                .into_iter()
                .map(|(position, tokens, _)| (position, tokens))
                .collect(),
        })
    }

    /// Those that fit in `room`, in the planner's estimate tokens, in the
    /// writer's order.
    fn within(&self, mut room: usize) -> Vec<Value> {
        let mut kept = Vec::new();
        for &(position, tokens) in &self.order {
            if tokens <= room {
                room -= tokens;
                kept.push(position);
            }
        }
        kept.sort_unstable();
        kept.into_iter()
            .filter_map(|position| self.excerpts.get(position).cloned())
            .collect()
    }
}

/// The sources the candidate's blocks and questions cite, by index.
fn cited_sources(candidate: &Value) -> BTreeSet<u64> {
    let mut cited = BTreeSet::new();
    for field in ["blocks", "questions"] {
        for item in candidate
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            cited.extend(item.get("sourceIndex").and_then(Value::as_u64));
            for citation in item
                .get("citations")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                cited.extend(citation.get("sourceIndex").and_then(Value::as_u64));
            }
        }
    }
    cited
}

/// A strict-JSON request whose prompt holds the writer's prompt, its excerpts
/// taken out, under `context`.
pub(super) struct Request<'a> {
    /// What the call produces, for logs: "the teaching review".
    pub subject: &'a str,
    pub system: &'a str,
    pub prompt: &'a Map<String, Value>,
    pub context: &'a str,
    pub schema: Value,
    pub output_tokens: usize,
}

impl Request<'_> {
    fn with(&self, excerpts: Option<Vec<Value>>) -> GroundedRequest {
        let mut prompt = self.prompt.clone();
        if let (Some(excerpts), Some(Value::Object(context))) =
            (excerpts, prompt.get_mut(self.context))
        {
            context.insert("sources".into(), Value::Array(excerpts));
        }
        model_call::structured(
            self.system,
            Value::Object(prompt).to_string(),
            self.schema.clone(),
            self.output_tokens,
            "low",
        )
    }
}

/// [`complete_checked_request`] for `request` with as many of `excerpts` as
/// fit beside it ([`generation::send_packed`]): fewer after a size refusal,
/// none when none fits. Only a request that does not fit without any fails,
/// as the Learning model's window being too small.
pub(super) async fn complete<T>(
    llm: &dyn LLMPort,
    request: &Request<'_>,
    excerpts: Option<&WriterExcerpts>,
    progress: Option<&OutlineProgress>,
    reasoning: Option<ReasonFirst<'_>>,
    accept: impl Fn(&str) -> Result<T>,
) -> Result<T> {
    let Some(excerpts) = excerpts else {
        return complete_checked_request(llm, request.with(None), progress, reasoning, accept)
            .await;
    };
    let build = |room: usize| request.with(Some(excerpts.within(room)));
    let room = model_call::source_room(llm, &build(0), usize::MAX);
    let accept = &accept;
    generation::send_packed(
        llm,
        room,
        build,
        |sent| async move {
            complete_checked_request(llm, sent, progress, reasoning, accept).await
        },
        request.subject,
        MATERIAL_DOES_NOT_FIT,
    )
    .await
}
