//! The repair call: the model's answer to a review's findings.
use crate::lessons::copied_wording::Wording;
use crate::lessons::text_edits;
use crate::lessons::writer_excerpts::{self, WriterExcerpts};
use crate::outline_progress::OutlineProgress;
use lattice_core::{application::ports::LLMPort, shared::error::Result};
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

pub(super) async fn ask(
    llm: &dyn LLMPort,
    request: &Request<'_>,
    progress: Option<&OutlineProgress>,
) -> Result<Repaired> {
    let system = request.system;
    let schema = request.schema;
    let output_tokens = request.output_tokens;
    // Fitted to the window as the review is.
    let mut requirements = request.authoring.clone();
    let excerpts = WriterExcerpts::take(llm, &mut requirements, request.candidate);
    let parts = [
        ("originalRequirements", requirements),
        ("candidate", request.candidate.clone()),
        ("sourceQuoteChecks", json!(request.quote_checks)),
        ("issuesToFix", request.issues.clone()),
    ];
    if request.edits {
        let edit_system = format!("You are editing a rejected draft. Correct only the established teaching defects. A defect that something is not taught is corrected by adding it: before is an existing sentence beside the place, after is that sentence followed by the added text. The learner sees only the lesson: added or changed text teaches in the lesson's own words and never mentions the course plan, the review, the sources' excerpts or these instructions. {}", text_edits::instructions(schema));
        let prompt: Map<String, Value> = parts
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        let wr = writer_excerpts::Request {
            subject: "the teaching repair",
            system: &edit_system,
            prompt: &prompt,
            context: "originalRequirements",
            schema: text_edits::schema_for(request.candidate, schema),
            output_tokens,
        };
        writer_excerpts::complete(llm, &wr, excerpts.as_ref(), progress, None, |raw| {
            text_edits::apply_in(request.candidate, raw, &Value::Null, request.wording)
                .map(|_| raw.to_owned())
        })
        .await
        .map(Repaired::Patch)
    } else {
        let repair_system = format!("{system}\nYou are editing a rejected draft. Apply every listed correction to the actual content. Do not reproduce the draft unchanged.");
        let prompt: Map<String, Value> = [("task", json!("Repair the teaching material"))]
            .into_iter()
            .chain(parts)
            .chain([("instruction", json!("Give the complete corrected material. Change every defective passage, exercise, key or explanation identified above. An unchanged draft is not a repair. Preserve valid content. Do not mention this internal review, the course plan or the sources' excerpts in the learner-facing material."))])
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        let wr = writer_excerpts::Request {
            subject: "the teaching repair",
            system: &repair_system,
            prompt: &prompt,
            context: "originalRequirements",
            schema: schema.clone(),
            output_tokens,
        };
        writer_excerpts::complete(llm, &wr, excerpts.as_ref(), progress, None, |text| {
            Ok(text.to_owned())
        })
        .await
        .map(Repaired::Whole)
    }
}
