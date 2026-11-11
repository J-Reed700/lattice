//! What a repair request carries: the pending-repair checkpoint and the context sent to the model.
use super::claim_ledger::ClaimLedger;
use super::repair::goal_words;
use super::*;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal};
use lattice_core::domain::learning::claim_review::ClaimId;

#[derive(Serialize, Deserialize)]
pub(super) struct PendingRepair {
    pub(super) fingerprint: String,
    pub(super) defects: usize,
    pub(super) context: Value,
}

pub(crate) async fn has_pending_repair(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    raw: &str,
    references: &ReferenceCollection<'_>,
) -> Result<bool> {
    let candidate: Value = crate::generation::parse_json(raw)?;
    Ok(crate::lesson_drafts::pending_repair(raw)
        .await?
        .and_then(|raw| serde_json::from_str::<PendingRepair>(&raw).ok())
        .is_some_and(|saved| saved.matches(llm, prompt, schema, &candidate, references)))
}

pub(super) fn repair_fingerprint(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
) -> String {
    let mut sources: Vec<_> = references
        .sources
        .iter()
        .map(|source| (&source.id, sha256_hex(&source.excerpt)))
        .collect();
    sources.sort();
    sha256_hex(&json!({"policy":POLICY,"repairPolicy":"pending-repair-v3","model":llm.model_name(),"prompt":prompt,"schema":schema,"candidate":candidate,"sources":sources}).to_string())
}

impl PendingRepair {
    pub(super) fn matches(
        &self,
        llm: &dyn LLMPort,
        prompt: &str,
        schema: &Value,
        candidate: &Value,
        references: &ReferenceCollection<'_>,
    ) -> bool {
        self.defects > 0
            && self.context.get("candidate") == Some(candidate)
            && self.fingerprint == repair_fingerprint(llm, prompt, schema, candidate, references)
    }
}

/// The repair context, with each exact passage sent once rather than per
/// finding; nothing is discarded. Passages carry their source titles so a
/// source index is not read as evidence N. Only `targets` are sent, each with
/// its claim id and, at a cut, `lastAttempt`: that attempt takes the sentence
/// out in code, so the model never reads the flag. A cut whose sentence also
/// states another live statement, or that no single sentence states, is
/// asked of the model instead (`ClaimLedger::cut_of`): no `lastAttempt`, and
/// the narrowing `goal`. A claim whose sentence says more carries
/// `alsoStates`, the other statements on it. A claim at a rewrite carries its
/// node's `goal`, and at the retry of a rewrite `notApplied`, why its first
/// correction could not be applied. `repairHistory` is what the ledger holds
/// of the sentences earlier corrections replaced or removed.
pub(super) fn repair_context(
    prompt: &str,
    candidate: &Value,
    report: &LessonVerificationReport,
    (ledger, policy): (&ClaimLedger, &dyn AcceptancePolicy),
    targets: &[ClaimId],
    references: &ReferenceCollection<'_>,
) -> Result<Value> {
    let escalates = |id: &ClaimId| ledger.cuts_next(policy, id);
    let mates: HashMap<ClaimId, Vec<ClaimId>> = targets
        .iter()
        .map(|id| (id.clone(), ledger.cotenants(id)))
        .collect();
    let citations = section_citations(candidate, references)?;
    let title = |id: &str| {
        references
            .sources
            .iter()
            .find(|source| source.id == id)
            .map(|source| source.title.clone())
    };
    let mut locations = HashMap::new();
    let mut passages = Vec::new();
    let failed: Vec<_> = report.findings.iter().zip(ledger.finding_claims())
        .filter(|(_, id)| targets.contains(id))
        .map(|(finding, id)| {
            let evidence: Vec<_> = finding.evidence.iter().map(|passage| {
                let key = (&passage.source_id, passage.start_byte, passage.end_byte, &passage.text);
                let index = *locations.entry(key).or_insert_with(|| {
                    let index = passages.len();
                    passages.push(json!({"id":format!("evidence-{index}"),"sourceId":passage.source_id,"sourceTitle":title(&passage.source_id),"startByte":passage.start_byte,"endByte":passage.end_byte,"text":passage.text}));
                    index
                });
                format!("evidence-{index}")
            }).collect();
            // The exact lesson text the repair may change (claim sources
            // design §18.5).
            let sentence = ledger.entry(id).map(|entry| entry.location().quote.clone());
            let mut claim = json!({"unit":finding.unit,"claimId":id.0,"statement":citations.label(&finding.statement, references),"sentence":sentence,"verdict":finding.verdict,"reason":finding.reason,"evidence":evidence});
            if let Some(fields) = claim.as_object_mut() {
                if escalates(id) {
                    if ledger.narrows(candidate, id) {
                        fields.insert("goal".into(), json!(goal_words(Goal::KeepTheOthers)));
                    } else {
                        fields.insert("lastAttempt".into(), json!(true));
                    }
                }
                // What the corrected sentence must do, which the reading
                // after it tests, and why the first could not be applied.
                if let Some((goal, why)) = ledger.rewrite_ask(policy, id) {
                    fields.insert("goal".into(), json!(goal_words(goal)));
                    if let Some(reason) = why {
                        fields.insert("notApplied".into(), json!(reason));
                    }
                }
                // What the sentence says besides the failed claim, which the
                // rewrite keeps (claim classes design §3.2).
                let also: Vec<&str> = mates
                    .get(id)
                    .into_iter()
                    .flatten()
                    .filter(|mate| !targets.contains(mate))
                    .filter_map(|mate| ledger.entry(mate).map(|entry| entry.statement()))
                    .collect();
                if !also.is_empty() {
                    fields.insert("alsoStates".into(), json!(also));
                }
            }
            claim
        }).collect();
    let examples: Vec<(&execution::Observation, &ClaimId)> = report
        .executions
        .iter()
        .filter_map(|observation| {
            let id = ledger.example_claim(&observation.id)?;
            targets.contains(id).then_some((observation, id))
        })
        .collect();
    Ok(json!({
        "requirements":crate::generation::parse_json::<Value>(prompt)?,
        "candidate":candidate,"failedClaims":failed,"evidence":passages,
        "executions":examples.iter().map(|(observation, _)| observation).collect::<Vec<_>>(),
        "exampleClaims":examples.iter().map(|(observation, id)| {
            let mut example = json!({"unit":observation.unit,"claimId":id.0});
            if escalates(id) {
                if let Some(fields) = example.as_object_mut() {
                    fields.insert("lastAttempt".into(), json!(true));
                }
            }
            example
        }).collect::<Vec<_>>(),
        "repairHistory":ledger.repair_history(),
    }))
}
