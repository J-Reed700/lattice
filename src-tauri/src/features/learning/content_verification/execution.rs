use super::super::embedded_runtime::{
    execute_builtin_lab, BuiltinLabExecutionSpec, LearningBuiltinRuntime,
};
use super::super::lab_runtime::{
    LearningLabExecutionResult, LearningLabFile, LearningLabLimits, LearningLabRunStatus,
};
use super::*;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize)]
pub(super) struct Observation {
    pub(super) id: String,
    pub(super) unit: Option<usize>,
    runtime: String,
    code_sha256: String,
    code: String,
    result: LearningLabExecutionResult,
}
impl Observation {
    pub(super) fn unavailable(&self) -> bool {
        matches!(
            self.result.status,
            LearningLabRunStatus::RuntimeUnavailable | LearningLabRunStatus::Cancelled
        )
    }
    pub(super) fn passed(&self) -> bool {
        self.result.status == LearningLabRunStatus::Passed
            && self.result.exit_code == Some(0)
            && !self.result.output_truncated
    }
    pub(super) fn evidence(&self) -> String {
        format!("Execution on {} for the exact code below:\n{}\nStatus: {:?}; stdout (JSON string preserving whitespace): {}\nstderr: {}", self.runtime, self.code, self.result.status, json!(self.result.stdout), json!(self.result.stderr))
    }
}
async fn run(
    id: String,
    runtime: LearningBuiltinRuntime,
    code: String,
    unit: Option<usize>,
) -> Result<Observation> {
    let cancellation = CancellationToken::new();
    // Dropping verification (timeout, cancelled preparation) stops its guest.
    let _guard = cancellation.clone().drop_guard();
    let result = execute_builtin_lab(
        BuiltinLabExecutionSpec {
            run_id: uuid::Uuid::new_v4().to_string(),
            runtime,
            files: vec![LearningLabFile {
                path: runtime.entrypoint().into(),
                content: code.clone(),
            }],
            read_only_paths: vec![runtime.entrypoint().into()],
            limits: LearningLabLimits {
                timeout_seconds: 5,
                output_bytes: 8192,
                ..Default::default()
            },
        },
        cancellation,
    )
    .await?;
    Ok(Observation {
        id,
        unit,
        runtime: runtime.version().into(),
        code_sha256: digest(&code),
        code,
        result,
    })
}

/// Recognize fenced examples without rewriting the rendered code. Unlabeled
/// or incomplete fences block verification; other languages are disclosed.
fn examples(body: &str) -> Result<Vec<(LearningBuiltinRuntime, String)>> {
    let mut result = Vec::new();
    let mut fence: Option<(char, usize, String, String)> = None;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if let Some((marker, width, language, code)) = fence.as_mut() {
            if trimmed.chars().take_while(|c| c == marker).count() >= *width
                && trimmed.trim().chars().all(|c| c == *marker)
            {
                let runtime = match language.as_str() {
                    "python" | "py" => Some(LearningBuiltinRuntime::Python),
                    "javascript" | "js" => Some(LearningBuiltinRuntime::Javascript),
                    "text" | "output" | "csv" | "json" => None,
                    "" => {
                        return Err(invalid(
                            "Label worked-example code fences with their language.",
                        ))
                    }
                    _ => None, // Evidence-checked; explicitly reported as not executed.
                };
                if let Some(runtime) = runtime {
                    result.push((runtime, code.clone()));
                }
                fence = None;
            } else {
                code.push_str(line);
            }
        } else if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker = trimmed.chars().next().unwrap_or('`');
            let width = trimmed.chars().take_while(|c| *c == marker).count();
            let language = trimmed
                .get(width..)
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            fence = Some((marker, width, language, String::new()));
        }
    }
    if fence.is_some() {
        return Err(invalid("A worked example contains an unclosed code fence."));
    }
    Ok(result)
}

pub(super) async fn observe(candidate: &Value) -> Result<Vec<Observation>> {
    let mut observations = Vec::new();
    for (index, block) in candidate["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if block["kind"].as_str() != Some("worked_example") {
            continue;
        }
        for (example_index, (runtime, code)) in
            examples(block["body"].as_str().unwrap_or_default())?
                .into_iter()
                .enumerate()
        {
            if observations.len() >= 12 {
                return Err(invalid(
                    "A lesson contains too many runnable examples for verification.",
                ));
            }
            observations.push(
                run(
                    format!("block-{index}-example-{example_index}"),
                    runtime,
                    code,
                    Some(index),
                )
                .await?,
            );
        }
    }
    Ok(observations)
}

/// Unsupported languages remain visible limitations, never execution passes.
pub(super) fn unexecuted_languages(candidate: &Value) -> Vec<String> {
    let mut languages = std::collections::BTreeSet::new();
    if let Some(blocks) = candidate.get("blocks").and_then(Value::as_array) {
        for block in blocks {
            if block["kind"] != "worked_example" {
                continue;
            }
            let mut open = false;
            for line in block["body"].as_str().unwrap_or_default().lines() {
                let line = line.trim_start();
                if line.starts_with("```") || line.starts_with("~~~") {
                    if !open {
                        let language = line
                            .trim_start_matches(['`', '~'])
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_lowercase();
                        if !matches!(
                            language.as_str(),
                            "" | "python"
                                | "py"
                                | "javascript"
                                | "js"
                                | "text"
                                | "output"
                                | "csv"
                                | "json"
                        ) {
                            languages.insert(language);
                        }
                    }
                    open = !open;
                }
            }
        }
    }
    languages.into_iter().collect()
}
