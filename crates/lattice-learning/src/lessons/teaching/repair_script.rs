//! What a scripted model reads in a teaching-repair request: the task, and the labels it shows.
use serde_json::Value;

/// The task a teaching-repair request carries; `None` for any other request.
pub(crate) fn task_of(prompt: &str) -> Option<Value> {
    prompt.rsplit("\n\n").find_map(|chunk| {
        serde_json::from_str::<Value>(chunk)
            .ok()
            .filter(|task| task.get("issuesToFix").is_some() && task.get("instruction").is_none())
    })
}

/// The label that stands before `words` in `marked`, a text with `[label] ` before each sentence.
fn label_before(marked: &str, words: &str) -> Option<String> {
    let close = marked.find(&format!("] {words}"))?;
    let open = marked.get(..close)?.rfind('[')?;
    marked.get(open + 1..close).map(str::to_owned)
}

/// The label of the sentence `sentence` of the text at `pointer` (as `/blocks/0/body`) in the task's candidate.
pub(crate) fn sentence_label(task: &Value, pointer: &str, sentence: &str) -> Option<String> {
    let marked = task.pointer(&format!("/candidate{pointer}"))?.as_str()?;
    label_before(marked, sentence)
}

/// The label of the quotable sentence of source `source_index` that holds `words`.
pub(crate) fn quote_label(task: &Value, source_index: u64, words: &str) -> Option<String> {
    let sources = task.pointer("/originalRequirements/sources")?.as_array()?;
    let source = sources
        .iter()
        .find(|source| source.get("sourceIndex").and_then(Value::as_u64) == Some(source_index))?;
    label_before(source.get("excerpt")?.as_str()?, words)
}

/// The labels of the findings sent, in order, each once.
pub(crate) fn finding_labels(task: &Value) -> Vec<String> {
    let mut labels: Vec<String> = Vec::new();
    for row in task
        .get("issuesToFix")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(label) = row.get("finding").and_then(Value::as_str) {
            if !labels.iter().any(|known| known == label) {
                labels.push(label.to_owned());
            }
        }
    }
    labels
}
