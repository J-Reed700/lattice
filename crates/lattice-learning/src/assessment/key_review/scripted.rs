//! Scripted replies to the blind solver and the item judge, for tests that run a quiz review.
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::services::model_contract::test_support::ScriptedModel;
use lattice_core::application::services::model_contract::Contract;
use lattice_core::shared::error::AppError;
use serde_json::{json, Value};

/// The task a call was first asked, also when the request is a correction (the task is then in the history) or a restatement (the task is followed by the reason).
fn asked(request: &CompletionRequest) -> Value {
    request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { role, content } if role == "user" => Some(content.as_str()),
            _ => None,
        })
        .find_map(|content| {
            serde_json::Deserializer::from_str(content)
                .into_iter::<Value>()
                .next()?
                .ok()
                .filter(Value::is_object)
        })
        .unwrap_or_default()
}

/// Answers every call of `C` with the JSON `reply` makes of the call's task and the number of calls of `C` before it.
pub(crate) fn answering<C: Contract>(
    model: ScriptedModel,
    reply: impl Fn(&Value, usize) -> Value + Send + Sync + 'static,
) -> ScriptedModel {
    model.on_request::<C>(move |sent, nth| {
        let task = asked(&sent.request);
        let text = reply(&task, nth).to_string();
        Box::pin(std::future::ready(Ok::<_, AppError>(
            CompletionResponse::from_text(text),
        )))
    })
}

/// A solve of every choice question of `task`: the options `chosen` gives by their place are marked yes.
pub(crate) fn solved(task: &Value, chosen: impl Fn(&Value) -> Vec<usize>) -> Value {
    let entries: Vec<Value> = task["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|question| {
            let picked = chosen(question);
            let options: Vec<Value> = question["options"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(at, option)| {
                    let mark = if picked.contains(&at) { "yes" } else { "no" };
                    json!({"of": option["id"], "correct": mark})
                })
                .collect();
            json!({
                "of": question["id"],
                "reason": "Torn stamps are set aside.",
                "options": options
            })
        })
        .collect();
    json!({"entries": entries})
}

/// A solve of every written question of `task` with `answer`.
pub(crate) fn solved_text(task: &Value, answer: &str) -> Value {
    let entries: Vec<Value> = task["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|question| {
            json!({
                "of": question["id"],
                "reason": "Torn stamps are set aside.",
                "answer": answer
            })
        })
        .collect();
    json!({"entries": entries})
}

/// A solve of questions `q1` onward, one for each of `options`, that marks the option `chosen` (from zero) of each yes.
pub(crate) fn solved_each(options: &[usize], chosen: usize) -> Value {
    let entries: Vec<Value> = options
        .iter()
        .enumerate()
        .map(|(question, count)| {
            let marks: Vec<Value> = (0..*count)
                .map(|at| {
                    let mark = if at == chosen { "yes" } else { "no" };
                    json!({"of": format!("o{}", at + 1), "correct": mark})
                })
                .collect();
            json!({
                "of": format!("q{}", question + 1),
                "reason": "The fixture's key.",
                "options": marks
            })
        })
        .collect();
    json!({"entries": entries})
}

/// A solve of `questions` questions of `options` options each.
pub(crate) fn solved_static(questions: usize, options: usize, chosen: usize) -> Value {
    solved_each(&vec![options; questions], chosen)
}
