use serde_json::{json, Map, Value};

/// A lesson answer in the form the writer is asked for, made from a lesson in
/// the form it is saved in, citing the sentences of `prompt`'s sources that
/// hold each quotation. A text that is not a saved lesson is returned as it is.
pub(crate) fn scripted_lesson(saved: &str, prompt: &str) -> String {
    let Ok(old) = serde_json::from_str::<Value>(saved) else {
        return saved.to_owned();
    };
    let (Some(blocks), Some(questions)) = (
        old.get("blocks").and_then(Value::as_array),
        old.get("questions").and_then(Value::as_array),
    ) else {
        return saved.to_owned();
    };
    let task = prompt
        .rsplit("\n\n")
        .find_map(|chunk| {
            serde_json::from_str::<Value>(chunk)
                .ok()
                .filter(|task| task.get("requirements").is_some())
        })
        .unwrap_or(Value::Null);
    let handles = |index: &Value, quote: &str| -> Vec<Value> {
        let Some(at) = index.as_u64().and_then(|at| usize::try_from(at).ok()) else {
            return Vec::new();
        };
        let rows = task["sources"]
            .get(at)
            .and_then(|source| source["excerpt"].as_str())
            .unwrap_or_default();
        rows.split('[')
            .skip(1)
            .filter_map(|marked| marked.split_once("] "))
            .map(|(handle, words)| (handle, words.trim()))
            .find(|(handle, words)| {
                handle.starts_with('q')
                    && !quote.is_empty()
                    && !words.is_empty()
                    && (words.contains(quote) || quote.contains(words))
            })
            .map(|(handle, _)| vec![json!(handle)])
            .unwrap_or_default()
    };
    let part = |item: &Value| -> Map<String, Value> {
        let mut part = Map::new();
        part.insert("title".into(), item["title"].clone());
        part.insert("body".into(), item["body"].clone());
        part
    };
    let cited_of = |item: &Value| -> Vec<Value> {
        item["citations"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| {
                json!({
                    "sentence": entry["sentence"],
                    "cites": handles(&entry["sourceIndex"], entry["quote"].as_str().unwrap_or_default())
                })
            })
            .collect()
    };
    let with_cites = |mut part: Map<String, Value>, item: &Value| -> Value {
        part.insert(
            "cites".into(),
            Value::Array(handles(
                &item["sourceIndex"],
                item["quote"].as_str().unwrap_or_default(),
            )),
        );
        part.insert("citations".into(), Value::Array(cited_of(item)));
        Value::Object(part)
    };
    let mut ideas: Vec<Vec<Value>> = Vec::new();
    let mut closers: Map<String, Value> = Map::new();
    for item in blocks {
        let kind = item["kind"].as_str().unwrap_or_default();
        let key = match kind {
            "guided_practice" => Some("guidedPractice"),
            "independent_practice" => Some("independentPractice"),
            "reflection" => Some("reflection"),
            "recap" => Some("recap"),
            _ => None,
        };
        match key {
            Some(key) => {
                let mut written = part(item);
                if key.ends_with("Practice") {
                    written.insert("rubric".into(), item["rubric"].clone());
                }
                closers.insert(key.into(), with_cites(written, item));
            }
            None => {
                if kind == "explanation" || ideas.is_empty() {
                    ideas.push(Vec::new());
                }
                let mut written = part(item);
                written.insert("kind".into(), json!(kind));
                if let Some(idea) = ideas.last_mut() {
                    idea.push(with_cites(written, item));
                }
            }
        }
    }
    let written: Vec<Value> = questions
        .iter()
        .map(|item| {
            let options = item["options"].as_array().cloned().unwrap_or_default();
            let right = item["correctIndex"].as_u64().unwrap_or_default();
            let wrong: Vec<Value> = options
                .iter()
                .enumerate()
                .filter(|(at, _)| *at as u64 != right)
                .map(|(at, option)| json!({"option": option, "reason": item["rationales"][at]}))
                .collect();
            let mut question = Map::new();
            question.insert("kind".into(), item["kind"].clone());
            question.insert("prompt".into(), item["prompt"].clone());
            question.insert(
                "right".into(),
                options.get(right as usize).cloned().unwrap_or_default(),
            );
            question.insert("wrong".into(), Value::Array(wrong));
            question.insert("explanation".into(), item["explanation"].clone());
            with_cites(question, item)
        })
        .collect();
    let mut answer = Map::new();
    answer.insert(
        "ideas".into(),
        Value::Array(
            ideas
                .into_iter()
                .map(|sections| json!({"sections": sections}))
                .collect(),
        ),
    );
    answer.extend(closers);
    answer.insert("questions".into(), Value::Array(written));
    Value::Object(answer).to_string()
}
