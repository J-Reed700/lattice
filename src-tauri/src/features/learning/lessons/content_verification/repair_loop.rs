//! The verify-and-repair loop and its resumable pending-repair checkpoint.
use super::*;

/// Evidence-driven repairs, with full re-extraction and rechecking afterward.
/// Continue while defects decrease; preserve a draft when repairs stop helping.
/// Operational extraction/audit errors propagate rather than asking the author
/// to rewrite valid content because a checker is unavailable.
#[cfg(test)]
pub(in crate::features::learning) async fn verify_and_repair(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    raw: String,
    sources: &[LearningSourceDto],
    output_tokens: usize,
) -> Result<(String, LessonVerificationReport)> {
    let references = ReferenceCollection::lexical(sources)?;
    verify_and_repair_with_references(llm, prompt, schema, raw, &references, output_tokens).await
}

pub(in crate::features::learning) async fn verify_and_repair_with_references(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    mut raw: String,
    references: &ReferenceCollection<'_>,
    output_tokens: usize,
) -> Result<(String, LessonVerificationReport)> {
    let mut expanded = None;
    let mut previous_issue_fingerprint = None;
    let mut repair_history = Vec::new();
    let mut resume_repair = true;
    let mut checks = ClaimChecks::default();
    loop {
        raw = normalize_example_fences(llm, raw).await?;
        let candidate: Value = crate::features::learning::generation::parse_json(&raw)?;
        let active = expanded.as_ref().unwrap_or(references);
        // A failed rewrite can resume from its saved defects. This only skips
        // repeating the diagnosis; it never skips checking the rewritten text.
        if std::mem::take(&mut resume_repair) {
            if let Some(saved) =
                crate::features::learning::lesson_drafts::pending_repair(&candidate.to_string())
                    .await?
                    .and_then(|raw| serde_json::from_str::<PendingRepair>(&raw).ok())
                    .filter(|saved| saved.matches(llm, prompt, schema, &candidate, active))
            {
                previous_issue_fingerprint = saved.issue_fingerprint.clone();
                repair_history = saved
                    .context
                    .get("repairHistory")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                crate::features::learning::lesson_progress::phase(
                    crate::features::learning::lesson_progress::Phase::Repair,
                    "Resuming the saved evidence-based repair",
                );
                raw = repair::candidate(
                    llm,
                    prompt,
                    schema,
                    saved.context,
                    output_tokens,
                    active,
                    saved.defects,
                )
                .await?;
                continue;
            }
        }
        let mut report = verify_with_references(llm, &candidate, active, &mut checks).await?;
        loop {
            let gaps = report
                .findings
                .iter()
                .filter(|f| {
                    matches!(
                        f.verdict,
                        ClaimVerdict::Unsupported | ClaimVerdict::Contradicted
                    )
                })
                .count();
            let active = expanded.as_ref().unwrap_or(references);
            let Some(additional) = research::expand(llm, active, &report.findings, &checks)
                .boxed()
                .await?
            else {
                break;
            };
            expanded = Some(additional);
            let active = expanded
                .as_ref()
                .ok_or_else(|| AppError::InternalError("Missing expanded references".into()))?;
            report = check_evidence(
                llm,
                &candidate,
                active,
                Inventory {
                    units: report.coverage,
                },
                Coverage {
                    units: report.coverage_audit,
                },
                report.executions,
                &mut checks,
            )
            .await?;
            // More research can close missing-evidence gaps. Contradictions
            // after this expansion need rewriting now, even if another claim
            // improved; otherwise known falsehoods trigger repeated full passes.
            if report
                .findings
                .iter()
                .filter(|f| {
                    matches!(
                        f.verdict,
                        ClaimVerdict::Unsupported | ClaimVerdict::Contradicted
                    )
                })
                .count()
                >= gaps
                || report
                    .findings
                    .iter()
                    .any(|finding| finding.verdict == ClaimVerdict::Contradicted)
            {
                break;
            }
        }
        if report.issues.is_empty() {
            return Ok((raw, report));
        }
        if report
            .findings
            .iter()
            .any(|f| f.verdict == ClaimVerdict::Unverified)
        {
            let unfinished = report
                .findings
                .iter()
                .filter(|f| f.verdict == ClaimVerdict::Unverified)
                .count();
            return Err(AppError::ServiceNotAvailable(format!("The evidence checker could not complete {unfinished} claim checks. No lesson was published. Retry when the checker is available; durable jobs retain the lesson draft.")));
        }
        let issue_fingerprint = repair_issue_fingerprint(&report.issues);
        let repeated_defects = previous_issue_fingerprint.as_ref() == Some(&issue_fingerprint);
        previous_issue_fingerprint = Some(issue_fingerprint.clone());
        remember_failed_claims(&mut repair_history, &report);
        crate::features::learning::lesson_progress::phase(
            crate::features::learning::lesson_progress::Phase::Repair,
            if repeated_defects {
                "Removing a repeatedly unsupported claim"
            } else {
                "Correcting defects found by evidence checks"
            },
        );
        let context = repair_context(
            prompt,
            &candidate,
            &report,
            &repair_history,
            repeated_defects,
        )?;
        let pending = PendingRepair {
            fingerprint: repair_fingerprint(
                llm,
                prompt,
                schema,
                &candidate,
                expanded.as_ref().unwrap_or(references),
            ),
            defects: report.issues.len(),
            issue_fingerprint: Some(issue_fingerprint),
            context: context.clone(),
        };
        crate::features::learning::lesson_drafts::record_pending_repair(
            &candidate.to_string(),
            serde_json::to_string(&pending)?,
        )
        .await?;
        raw = repair::candidate(
            llm,
            prompt,
            schema,
            context,
            output_tokens,
            expanded.as_ref().unwrap_or(references),
            report.issues.len(),
        )
        .await?;
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct PendingRepair {
    pub(super) fingerprint: String,
    pub(super) defects: usize,
    #[serde(default)]
    pub(super) issue_fingerprint: Option<String>,
    pub(super) context: Value,
}

pub(super) fn repair_issue_fingerprint(issues: &[String]) -> String {
    let mut normalized = issues
        .iter()
        .map(|issue| issue.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>();
    normalized.sort();
    digest(&json!({"policy":"repair-issue-set-v1","issues":normalized}).to_string())
}

fn remember_failed_claims(history: &mut Vec<Value>, report: &LessonVerificationReport) {
    for finding in report
        .findings
        .iter()
        .filter(|finding| finding.verdict != ClaimVerdict::Supported)
    {
        let rejected = json!({
            "unit": finding.unit,
            "statement": finding.statement,
            "verdict": finding.verdict,
            "reason": finding.reason,
        });
        if !history.contains(&rejected) {
            history.push(rejected);
        }
    }
}

pub(in crate::features::learning) async fn has_pending_repair(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    raw: &str,
    references: &ReferenceCollection<'_>,
) -> Result<bool> {
    let candidate: Value = crate::features::learning::generation::parse_json(raw)?;
    Ok(
        crate::features::learning::lesson_drafts::pending_repair(raw)
            .await?
            .and_then(|raw| serde_json::from_str::<PendingRepair>(&raw).ok())
            .is_some_and(|saved| saved.matches(llm, prompt, schema, &candidate, references)),
    )
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
        .map(|source| (&source.id, digest(&source.excerpt)))
        .collect();
    sources.sort();
    digest(&json!({"policy":POLICY,"repairPolicy":"pending-repair-v1","model":llm.model_name(),"prompt":prompt,"schema":schema,"candidate":candidate,"sources":sources}).to_string())
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

/// References overlap heavily across related claims. Send each exact passage
/// once instead of multiplying an entire retrieval result by every finding.
/// This changes representation only: no conflicting or unsupported evidence is
/// discarded, and the full candidate still undergoes independent verification.
pub(super) fn repair_context(
    prompt: &str,
    candidate: &Value,
    report: &LessonVerificationReport,
    repair_history: &[Value],
    repeated_defects: bool,
) -> Result<Value> {
    let mut locations = HashMap::new();
    let mut passages = Vec::new();
    let failed: Vec<_> = report.findings.iter()
        .filter(|finding| finding.verdict != ClaimVerdict::Supported)
        .map(|finding| {
            let evidence: Vec<_> = finding.evidence.iter().map(|passage| {
                let key = (&passage.source_id, passage.start_byte, passage.end_byte, &passage.text);
                let index = *locations.entry(key).or_insert_with(|| {
                    let index = passages.len();
                    passages.push(json!({"id":format!("evidence-{index}"),"sourceId":passage.source_id,"startByte":passage.start_byte,"endByte":passage.end_byte,"text":passage.text}));
                    index
                });
                format!("evidence-{index}")
            }).collect();
            json!({"unit":finding.unit,"statement":finding.statement,"verdict":finding.verdict,"reason":finding.reason,"evidence":evidence})
        }).collect();
    Ok(json!({
        "requirements":crate::features::learning::generation::parse_json::<Value>(prompt)?,
        "candidate":candidate,"failedClaims":failed,"evidence":passages,
        "executions":report.executions,
        "repairHistory":repair_history,
        "repeatedDefects":repeated_defects
    }))
}
