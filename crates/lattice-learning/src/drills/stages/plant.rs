//! Stage 3: one call proposes one mutation for each target pattern, as an exact edit of the clean code. Code applies
//! the edits, so the key's line spans are read off the result and never stated by the model. When the brief has
//! evidence, up to half the changes (rounded up) may be lesson issues: each cites the evidence it breaks and is of any
//! pattern of the catalogue, and the others are for the first targets no lesson issue has.

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
    apply, patterns_for, DrillFile, Edit, EditFault, Family, Pattern, Severity,
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
    /// The most changes that may be lesson issues: 0 when none is asked for.
    lesson_issues: usize,
    /// How many units of evidence the brief holds, which a lesson issue cites by number.
    evidence: usize,
    /// The patterns of the material, one of which a lesson issue is an instance of.
    patterns: &'a [&'static Pattern],
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
    #[serde(default)]
    lesson_sentences: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposals {
    #[serde(default, rename = "lessonUse")]
    _lesson_use: String,
    mutations: Vec<Proposed>,
}

impl DrillBuild<'_> {
    /// Stage 3. One mutation for each target pattern, in the order of the targets, that applies to the baseline and
    /// can be undone.
    pub(crate) async fn plant(&self, spec: &DrillSpec, baseline: &Baseline) -> Result<Planted> {
        let targets = spec.targets()?;
        let wording = self.material.plant_wording(spec, baseline, &targets);
        let patterns: Vec<&'static Pattern> = patterns_for(&spec.material).collect();
        let evidence = baseline
            .brief
            .as_ref()
            .map_or(0, |brief| brief.evidence.len());
        let rules = Rules {
            targets: &targets,
            files: &baseline.files,
            tests: &baseline.tests,
            asks_verification: wording.asks_verification,
            lesson_issues: wording.lesson_issues,
            evidence,
            patterns: &patterns,
        };
        let request = model_call::structured(
            &wording.system,
            wording.prompt,
            schema(&rules),
            budget::mutations(targets.len()),
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

fn schema(rules: &Rules<'_>) -> Value {
    let (targets, asks_verification) = (rules.targets, rules.asks_verification);
    let lesson = rules.lesson_issues > 0;
    let ids: Vec<&str> = if lesson { rules.patterns } else { targets }
        .iter()
        .map(|pattern| pattern.id)
        .collect();
    let mut properties = Map::new();
    if lesson {
        properties.insert(
            "lessonSentences".into(),
            json!({
                "type": "array",
                "items": {"type": "integer", "minimum": 1, "maximum": rules.evidence}
            }),
        );
    }
    properties.insert("pattern".into(), json!({"type": "string", "enum": ids}));
    properties.insert("path".into(), path_schema(rules.files));
    properties.insert("before".into(), text_schema());
    properties.insert("after".into(), text_schema());
    properties.insert("severity".into(), severity_schema());
    properties.insert("consequence".into(), text_schema());
    properties.insert("explanation".into(), text_schema());
    let mut required = if lesson {
        vec!["lessonSentences"]
    } else {
        Vec::new()
    };
    required.extend([
        "pattern",
        "path",
        "before",
        "after",
        "severity",
        "consequence",
        "explanation",
    ]);
    if asks_verification {
        properties.insert("killingTest".into(), json!({"type": "string"}));
        properties.insert("noTestBecause".into(), json!({"type": "string"}));
        required.extend(["killingTest", "noTestBecause"]);
    }
    let mut top = Map::new();
    if lesson {
        top.insert("lessonUse".into(), json!({"type": "string"}));
    }
    top.insert(
        "mutations".into(),
        json!({
            "type": "array",
            "minItems": targets.len(),
            "maxItems": targets.len(),
            "items": {
                "type": "object",
                "additionalProperties": false,
                "required": required,
                "properties": properties
            }
        }),
    );
    let top_required: &[&str] = if lesson {
        &["lessonUse", "mutations"]
    } else {
        &["mutations"]
    };
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": top_required,
        "properties": top
    })
}

/// Reads the mutations and applies them to the baseline, refusing what cannot stand: a lesson issue not asked for or
/// citing what the brief does not hold, a pattern missing or given twice, an edit that does not apply (with the
/// change that fixes it), and a change that cannot be undone.
fn read(raw: &str, rules: &Rules<'_>) -> Result<Vec<Mutation>> {
    let (lessons, mut proposals): (Vec<Proposed>, Vec<Proposed>) = parse_json::<Proposals>(raw)?
        .mutations
        .into_iter()
        .partition(|proposal| !proposal.lesson_sentences.is_empty());
    let taken = lesson_patterns(&lessons, rules)?;
    let expected: Vec<&'static Pattern> = rules
        .targets
        .iter()
        .filter(|target| !taken.iter().any(|held| held.id == target.id))
        .take(rules.targets.len().saturating_sub(lessons.len()))
        .copied()
        .collect();
    let note = if lessons.is_empty() {
        String::new()
    } else {
        format!(
            " With {} lesson {} ({}), the other changes are for: {}.",
            lessons.len(),
            if lessons.len() == 1 {
                "issue"
            } else {
                "issues"
            },
            ids_of(&taken),
            ids_of(&expected)
        )
    };
    let mut ordered: Vec<Proposed> = Vec::with_capacity(expected.len());
    for target in &expected {
        let count = proposals
            .iter()
            .filter(|proposal| proposal.pattern == target.id)
            .count();
        if count == 0 {
            return Err(refuse(format!(
                "There is no change for {}. Give exactly one change for each pattern asked for.{note}",
                target.id
            )));
        }
        if count > 1 {
            return Err(refuse(format!(
                "There are {count} changes for {}. Give exactly one change for each pattern.{note}",
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
        return Err(refuse(format!(
            "{} is not a pattern asked for. Give changes only for: {}.{note}",
            stray.pattern,
            ids_of(&expected)
        )));
    }
    let mut mutations: Vec<Mutation> = Vec::with_capacity(ordered.len() + lessons.len());
    for (proposal, target) in ordered.into_iter().zip(&expected) {
        mutations.push(mutation_of(proposal, target, rules)?);
    }
    for (proposal, target) in lessons.into_iter().zip(&taken) {
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

/// The ids of `patterns`, comma-separated.
fn ids_of(patterns: &[&Pattern]) -> String {
    patterns
        .iter()
        .map(|pattern| pattern.id)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The patterns of the lesson issues among the proposals, in the order given, when each is one that could stand.
fn lesson_patterns(lessons: &[Proposed], rules: &Rules<'_>) -> Result<Vec<&'static Pattern>> {
    if lessons.is_empty() {
        return Ok(Vec::new());
    }
    if rules.lesson_issues == 0 {
        return Err(refuse(
            "No lesson issue is asked for: give every change an empty lessonSentences.".to_owned(),
        ));
    }
    if lessons.len() > rules.lesson_issues {
        return Err(refuse(format!(
            "There are {} lesson issues. Give at most {}: the other changes are for the patterns asked for.",
            lessons.len(),
            rules.lesson_issues
        )));
    }
    let mut taken: Vec<&'static Pattern> = Vec::with_capacity(lessons.len());
    for lesson in lessons {
        let Some(known) = rules
            .patterns
            .iter()
            .find(|known| known.id == lesson.pattern)
            .copied()
        else {
            return Err(refuse(format!(
                "{} is not a pattern of the catalogue for this code.",
                lesson.pattern
            )));
        };
        for (position, &number) in lesson.lesson_sentences.iter().enumerate() {
            if number == 0 || number > rules.evidence {
                return Err(refuse(format!(
                    "The lesson issue for {} cites L{number}, which is not in the brief's evidence (L1 to L{}).",
                    known.id, rules.evidence
                )));
            }
            if lesson
                .lesson_sentences
                .iter()
                .take(position)
                .any(|earlier| *earlier == number)
            {
                return Err(refuse(format!(
                    "The lesson issue for {} cites L{number} twice.",
                    known.id
                )));
            }
        }
        if taken.iter().any(|earlier| earlier.id == known.id) {
            return Err(refuse(format!(
                "There are 2 lesson issues for {}. Every change is of a different pattern.",
                known.id
            )));
        }
        taken.push(known);
    }
    Ok(taken)
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
        lesson_sentences: proposal.lesson_sentences,
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
