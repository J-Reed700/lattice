//! Stage 3: one call proposes one mutation for each target pattern, as an exact edit of the clean code. Code applies
//! the edits, so the key's line spans are read off the result and never stated by the model.

use super::ask::{
    ask_request, describe_fault, normalise, path_schema, refuse, severity_schema, text_schema,
};
use super::outputs::{Baseline, Mutation, Planted, Trail, Verification};
use super::DrillBuild;
use crate::drills::budget;
use crate::drills::proof::is_behavioural;
use crate::drills::spec::DrillSpec;
use crate::generation::parse_json;
use crate::model_call;
use lattice_core::domain::learning::drills::{
    apply, DrillFile, Edit, EditFault, Family, Pattern, Severity,
};
use lattice_core::shared::error::Result;
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// What the answer to the stage's request has to be.
struct Rules<'a> {
    targets: &'a [&'static Pattern],
    files: &'a [DrillFile],
    tests: &'a [String],
    /// Whether each change comes with a test that observes it, or the reason none can.
    asks_verification: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Proposed {
    pattern: String,
    path: String,
    before: String,
    after: String,
    severity: Severity,
    consequence: String,
    explanation: String,
    #[serde(default)]
    killing_test: Option<String>,
    #[serde(default)]
    no_test_because: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposals {
    mutations: Vec<Proposed>,
}

impl DrillBuild<'_> {
    /// Stage 3. One mutation for each target pattern, in the order of the targets, that applies to the baseline and
    /// can be undone.
    pub(crate) async fn plant(&self, spec: &DrillSpec, baseline: &Baseline) -> Result<Planted> {
        let targets = spec.targets()?;
        let wording = self.material.plant_wording(spec, baseline, &targets);
        let rules = Rules {
            targets: &targets,
            files: &baseline.files,
            tests: &baseline.tests,
            asks_verification: wording.asks_verification,
        };
        let request = model_call::structured(
            &wording.system,
            wording.prompt,
            schema(&targets, &baseline.files, rules.asks_verification),
            budget::mutations(targets.len()),
            "low",
        );
        let asked = ask_request(self.llm, request, |raw| read(raw, &rules)).await?;
        let mut trail = Trail::default();
        trail.call("plant", self.llm.model_name());
        Ok(Planted {
            mutations: asked.value,
            trail,
        })
    }
}

fn schema(targets: &[&Pattern], files: &[DrillFile], asks_verification: bool) -> Value {
    let ids: Vec<&str> = targets.iter().map(|pattern| pattern.id).collect();
    let mut properties = Map::new();
    properties.insert("pattern".into(), json!({"type": "string", "enum": ids}));
    properties.insert("path".into(), path_schema(files));
    properties.insert("before".into(), text_schema());
    properties.insert("after".into(), text_schema());
    properties.insert("severity".into(), severity_schema());
    properties.insert("consequence".into(), text_schema());
    properties.insert("explanation".into(), text_schema());
    let mut required = vec![
        "pattern",
        "path",
        "before",
        "after",
        "severity",
        "consequence",
        "explanation",
    ];
    if asks_verification {
        properties.insert("killingTest".into(), json!({"type": "string"}));
        properties.insert("noTestBecause".into(), json!({"type": "string"}));
        required.extend(["killingTest", "noTestBecause"]);
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["mutations"],
        "properties": {
            "mutations": {
                "type": "array",
                "minItems": targets.len(),
                "maxItems": targets.len(),
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": required,
                    "properties": properties
                }
            }
        }
    })
}

/// Reads the mutations and applies them to the baseline, refusing what cannot stand: a pattern missing or given
/// twice, an edit that does not apply (with the change that fixes it), and a change that cannot be undone.
fn read(raw: &str, rules: &Rules<'_>) -> Result<Vec<Mutation>> {
    let mut proposals = parse_json::<Proposals>(raw)?.mutations;
    let mut ordered: Vec<Proposed> = Vec::with_capacity(rules.targets.len());
    for target in rules.targets {
        let count = proposals
            .iter()
            .filter(|proposal| proposal.pattern == target.id)
            .count();
        if count == 0 {
            return Err(refuse(format!(
                "There is no change for {}. Give exactly one change for each pattern asked for.",
                target.id
            )));
        }
        if count > 1 {
            return Err(refuse(format!(
                "There are {count} changes for {}. Give exactly one change for each pattern.",
                target.id
            )));
        }
        if let Some(position) = proposals
            .iter()
            .position(|proposal| proposal.pattern == target.id)
        {
            ordered.push(proposals.remove(position));
        }
    }
    if let Some(stray) = proposals.first() {
        let asked: Vec<&str> = rules.targets.iter().map(|target| target.id).collect();
        return Err(refuse(format!(
            "{} is not a pattern asked for. Give changes only for: {}.",
            stray.pattern,
            asked.join(", ")
        )));
    }
    let mut mutations: Vec<Mutation> = Vec::with_capacity(ordered.len());
    for (proposal, target) in ordered.into_iter().zip(rules.targets) {
        mutations.push(mutation_of(proposal, target, rules)?);
    }
    let edits: Vec<Edit> = mutations
        .iter()
        .map(|mutation| mutation.edit.clone())
        .collect();
    let labels: Vec<String> = mutations
        .iter()
        .map(|mutation| format!("Edit for {}", mutation.pattern))
        .collect();
    let paths: Vec<String> = rules.files.iter().map(|file| file.path.clone()).collect();
    let (changed, _) = apply(rules.files, &edits)
        .map_err(|fault| refuse(describe_fault(fault, &edits, &labels, &paths)))?;
    for mutation in &mutations {
        let fix = mutation.fix();
        if let Err(fault) = apply(&changed, std::slice::from_ref(&fix)) {
            return Err(refuse(describe_reversal(
                fault,
                &format!("Edit for {}", mutation.pattern),
                &fix.path,
            )));
        }
    }
    Ok(mutations)
}

/// The mutation a proposal makes, when it is one that could stand.
fn mutation_of(proposal: Proposed, target: &Pattern, rules: &Rules<'_>) -> Result<Mutation> {
    let label = format!("Edit for {}", target.id);
    let before = normalise(&proposal.before);
    let after = normalise(&proposal.after);
    if before == after {
        return Err(refuse(format!(
            "{label}: `before` and `after` are the same. Change the code."
        )));
    }
    if after.trim().is_empty() {
        return Err(refuse(format!(
            "{label}: `after` is empty. A deletion keeps a neighbouring line in both `before` and `after`, so that the change can be undone."
        )));
    }
    if proposal.consequence.trim().is_empty() || proposal.explanation.trim().is_empty() {
        return Err(refuse(format!(
            "{label}: give the consequence and the explanation."
        )));
    }
    if target.family == Family::Testing && !rules.tests.contains(&proposal.path) {
        return Err(refuse(format!(
            "{label}: a change for a testing pattern edits a test file ({}), not {}.",
            rules.tests.join(", "),
            proposal.path
        )));
    }
    let verification = if rules.asks_verification && is_behavioural(target.family) {
        let test = proposal
            .killing_test
            .as_deref()
            .map(normalise)
            .filter(|test| !test.trim().is_empty());
        let reason = proposal
            .no_test_because
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty());
        match (test, reason) {
            (Some(test), None) => Verification::KillingTest(test),
            (None, Some(reason)) => Verification::Unobservable(reason.to_owned()),
            (Some(_), Some(_)) => {
                return Err(refuse(format!(
                    "{label}: give either `killingTest` or `noTestBecause`, not both."
                )))
            }
            (None, None) => {
                return Err(refuse(format!(
                    "{label}: give a killing test, or in `noTestBecause` the reason that no deterministic test can observe this defect."
                )))
            }
        }
    } else {
        Verification::NotAsked
    };
    Ok(Mutation {
        pattern: target.id.to_owned(),
        edit: Edit {
            path: proposal.path,
            before,
            after,
        },
        severity: proposal.severity,
        consequence: proposal.consequence,
        explanation: proposal.explanation,
        verification,
    })
}

/// What to tell the model about a change that cannot be undone: the fix, the reverse edit, does not apply alone to
/// the changed code.
fn describe_reversal(fault: EditFault, label: &str, path: &str) -> String {
    match fault {
        EditFault::Ambiguous { count, .. } => format!(
            "{label}: once every change is made, the changed text (`after`) is in {path} {count} times, so the change cannot be undone with certainty. Take in more of the lines around it, in both `before` and `after`, so that the changed text occurs once."
        ),
        EditFault::Empty { .. } => format!(
            "{label}: `after` is empty. A deletion keeps a neighbouring line in both `before` and `after`, so that the change can be undone."
        ),
        _ => format!(
            "{label}: the change cannot be undone. Take in more of the lines around it, in both `before` and `after`."
        ),
    }
}
