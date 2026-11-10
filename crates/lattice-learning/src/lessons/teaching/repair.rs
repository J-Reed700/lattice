//! The repair call: the model's answer to a review's findings.
use crate::copied_wording::Wording;
use crate::lessons::text_edits;
use crate::lessons::writer_excerpts::{self, WriterExcerpts};
use crate::outline_progress::OutlineProgress;
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::{json, Map, Value};

pub(super) struct Request<'a> {
    pub system: &'a str,
    pub schema: &'a Value,
    pub authoring: &'a Value,
    pub candidate: &'a Value,
    pub quote_checks: &'a [Value],
    /// `issuesToFix` for the model.
    pub issues: Value,
    /// The repair may give edits; otherwise it rewrites the material whole.
    pub edits: bool,
    pub wording: &'a Wording,
    pub output_tokens: usize,
}

pub(super) enum Repaired {
    /// The patch, not yet applied (edits form).
    Patch(String),
    /// The whole material (rewrite form).
    Whole(String),
}

/// The candidate a repair's answer makes.
pub(super) fn apply(previous: &Value, repaired: Repaired, wording: &Wording) -> Result<String> {
    match repaired {
        Repaired::Patch(raw) => text_edits::apply_in(previous, &raw, &Value::Null, wording)
            .map(|value| value.to_string()),
        Repaired::Whole(text) => Ok(text),
    }
}

const REPAIR_POLICY: &str = "teaching-repair-v1";

/// One way of asking for a repair: what it sends, and what it makes of the answer.
trait Form: Sync {
    fn system(&self, request: &Request<'_>) -> String;
    fn schema(&self, request: &Request<'_>) -> Value;
    /// The whole prompt, `requirements` standing for `originalRequirements`.
    fn prompt(&self, request: &Request<'_>, requirements: Value) -> Map<String, Value>;
    /// Whether the form can use this raw answer.
    fn accept(&self, request: &Request<'_>, raw: &str) -> Result<()>;
    fn repaired(&self, raw: String) -> Repaired;
}

struct Edits;
struct Rewrite;

fn parts(request: &Request<'_>, requirements: Value) -> [(&'static str, Value); 4] {
    [
        ("originalRequirements", requirements),
        ("candidate", request.candidate.clone()),
        ("sourceQuoteChecks", json!(request.quote_checks)),
        ("issuesToFix", request.issues.clone()),
    ]
}

impl Form for Edits {
    fn system(&self, request: &Request<'_>) -> String {
        format!("You are editing a rejected draft. Correct only the established teaching defects. A defect that something is not taught is corrected by adding it: before is an existing sentence beside the place, after is that sentence followed by the added text. The learner sees only the lesson: added or changed text teaches in the lesson's own words and never mentions the course plan, the review, the sources' excerpts or these instructions. {}", text_edits::instructions(request.schema))
    }

    fn schema(&self, request: &Request<'_>) -> Value {
        text_edits::schema_for(request.candidate, request.schema)
    }

    fn prompt(&self, request: &Request<'_>, requirements: Value) -> Map<String, Value> {
        parts(request, requirements)
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn accept(&self, request: &Request<'_>, raw: &str) -> Result<()> {
        text_edits::apply_in(request.candidate, raw, &Value::Null, request.wording).map(|_| ())
    }

    fn repaired(&self, raw: String) -> Repaired {
        Repaired::Patch(raw)
    }
}

impl Form for Rewrite {
    fn system(&self, request: &Request<'_>) -> String {
        format!("{}\nYou are editing a rejected draft. Apply every listed correction to the actual content. Do not reproduce the draft unchanged.", request.system)
    }

    fn schema(&self, request: &Request<'_>) -> Value {
        request.schema.clone()
    }

    fn prompt(&self, request: &Request<'_>, requirements: Value) -> Map<String, Value> {
        [("task", json!("Repair the teaching material"))]
            .into_iter()
            .chain(parts(request, requirements))
            .chain([("instruction", json!("Give the complete corrected material. Change every defective passage, exercise, key or explanation identified above. An unchanged draft is not a repair. Preserve valid content. Do not mention this internal review, the course plan or the sources' excerpts in the learner-facing material."))])
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn accept(&self, _request: &Request<'_>, _raw: &str) -> Result<()> {
        Ok(())
    }

    fn repaired(&self, raw: String) -> Repaired {
        Repaired::Whole(raw)
    }
}

/// Asks the model for the repair, unless the job already holds its answer
/// (or its refusal) for these inputs.
pub(super) async fn ask(
    llm: &dyn LLMPort,
    request: &Request<'_>,
    progress: Option<&OutlineProgress>,
) -> Result<Repaired> {
    let form: &dyn Form = if request.edits { &Edits } else { &Rewrite };
    let system = form.system(request);
    let schema = form.schema(request);
    let key = format!(
        "teaching-repair-answer:{}",
        crate::content_verification::digest(
            &json!({"policy":REPAIR_POLICY,"model":llm.model_name(),"context":llm.max_context_tokens(),"system":system,"prompt":form.prompt(request, request.authoring.clone()),"schema":schema}).to_string()
        )
    );
    if let Some(saved) = crate::lesson_drafts::checkpoint(&key).await? {
        if let Some(raw) = saved.get("answer").and_then(Value::as_str) {
            crate::lesson_progress::stage("Reusing the completed teaching repair");
            return Ok(form.repaired(raw.to_owned()));
        }
        if let Some(reason) = saved.get("refused").and_then(Value::as_str) {
            return Err(AppError::ModelAnswerUnusable(reason.to_owned()));
        }
    }
    // Fitted to the window as the review is.
    let mut requirements = request.authoring.clone();
    let excerpts = WriterExcerpts::take(llm, &mut requirements, request.candidate);
    let prompt = form.prompt(request, requirements);
    let wr = writer_excerpts::Request {
        subject: "the teaching repair",
        system: &system,
        prompt: &prompt,
        context: "originalRequirements",
        schema,
        output_tokens: request.output_tokens,
    };
    let result = writer_excerpts::complete(llm, &wr, excerpts.as_ref(), progress, None, |raw| {
        form.accept(request, raw).map(|()| raw.to_owned())
    })
    .await;
    match &result {
        Ok(raw) => crate::lesson_drafts::record_checkpoint(&key, json!({"answer": raw})).await?,
        Err(AppError::ModelAnswerUnusable(reason)) => {
            crate::lesson_drafts::record_checkpoint(&key, json!({"refused": reason})).await?;
        }
        Err(_) => {}
    }
    result.map(|raw| form.repaired(raw))
}
