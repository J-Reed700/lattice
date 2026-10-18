//! The verify-and-repair loop and its resumable pending-repair checkpoint.
use super::*;

/// Evidence-driven repairs, with full re-extraction and rechecking afterward.
/// Repairs continue while they make progress: the candidate with the fewest
/// issues so far is the best. A round that does not beat it sends the next
/// repair escalated (repeatedDefects: remove the disputed assertions whole);
/// a round that beats it clears the escalation. When an escalated round also
/// fails to beat the best, the best candidate is saved as the lesson draft
/// and the job stops, so a retry resumes from it. The best count and the
/// escalation survive a resume through the pending-repair checkpoint.
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
    verify_and_repair_with_references(
        llm,
        &BuiltinRuntimes::for_tests(),
        prompt,
        schema,
        raw,
        &references,
        output_tokens,
    )
    .await
}

pub(in crate::features::learning) async fn verify_and_repair_with_references(
    llm: &dyn LLMPort,
    runtimes: &BuiltinRuntimes,
    prompt: &str,
    schema: &Value,
    mut raw: String,
    references: &ReferenceCollection<'_>,
    output_tokens: usize,
) -> Result<(String, LessonVerificationReport)> {
    let mut expanded = None;
    let mut previous_issue_fingerprint = None;
    let mut progress = RepairProgress::default();
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
                progress = saved.progress.clone();
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
                    &progress,
                )
                .await?;
                continue;
            }
        }
        let mut report =
            verify_with_references(llm, runtimes, &candidate, active, &mut checks).await?;
        // The last research round's finding that public search cannot help,
        // so a stop names the true reason the claims stayed unresolved.
        let research_exhausted = loop {
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
            let additional = match research::expand(llm, active, &report.findings, &checks)
                .boxed()
                .await?
            {
                research::Expansion::Saved(additional) => *additional,
                research::Expansion::Exhausted(exhausted) => break Some(exhausted),
                research::Expansion::Unchanged => break None,
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
                break None;
            }
        };
        if report.issues.is_empty() {
            return Ok((raw, report));
        }
        if let Some(error) = unchecked_claims(&report.findings) {
            return Err(error);
        }
        progress
            .record(&raw, &report.issues, research_exhausted)
            .await?;
        let issue_fingerprint = repair_issue_fingerprint(&report.issues);
        let repeated_defects =
            progress.escalated || previous_issue_fingerprint.as_ref() == Some(&issue_fingerprint);
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
            progress: progress.clone(),
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
            &progress,
        )
        .await?;
    }
}

/// The best candidate verified so far, by fewest issues.
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct BestCandidate {
    issues: Vec<String>,
    candidate: String,
}

/// Whether repairs still make progress; see `verify_and_repair_with_references`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct RepairProgress {
    best: Option<BestCandidate>,
    /// The repair under way removes disputed assertions whole because the
    /// last round did not beat the best.
    escalated: bool,
}

impl RepairProgress {
    /// Record a verified candidate that still has `issues`. A round that
    /// beats the best becomes it and clears the escalation; one that does not
    /// escalates the next repair, and after an escalated round it stops: the
    /// best candidate is saved as the lesson draft and an error explains why.
    /// `exhausted` is research's finding that no public search can help the
    /// remaining claims; a stop states it as the reason they remain.
    pub(super) async fn record(
        &mut self,
        raw: &str,
        issues: &[String],
        exhausted: Option<research::Exhausted>,
    ) -> Result<()> {
        match &self.best {
            Some(best) if issues.len() >= best.issues.len() => {
                if self.escalated {
                    crate::features::learning::lesson_drafts::save(&best.candidate).await?;
                    return Err(AppError::InvalidInput(format!(
                        "Repairs stopped reducing the lesson's problems: {} remain ({}).{} The best draft is saved; retry to continue.",
                        best.issues.len(),
                        best.issues
                            .iter()
                            .take(3)
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join("; "),
                        exhausted
                            .map(|exhausted| format!(" {exhausted}."))
                            .unwrap_or_default()
                    )));
                }
                self.escalated = true;
            }
            _ => {
                self.best = Some(BestCandidate {
                    issues: issues.to_vec(),
                    candidate: raw.to_owned(),
                });
                self.escalated = false;
            }
        }
        Ok(())
    }
}

/// The error for claims the checker could not judge. An outage never gets
/// here: it fails the check and the job waits it out. What is left is
/// deterministic (a request that cannot fit, an answer unusable after its
/// retries), and the same inputs would fail the same way, so the job stops
/// with the claims and their causes instead of being deferred.
pub(super) fn unchecked_claims(findings: &[Finding]) -> Option<AppError> {
    let unchecked: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.verdict == ClaimVerdict::Unverified)
        .collect();
    let first = unchecked.first()?;
    let more = unchecked.len() - 1;
    Some(AppError::InvalidInput(format!(
        "The evidence checker could not check {} claim{}. \"{}\": {}{} No lesson was published.",
        unchecked.len(),
        if unchecked.len() == 1 { "" } else { "s" },
        first.statement,
        first.reason,
        if more == 0 {
            String::new()
        } else {
            format!(" ({more} more claims could not be checked.)")
        }
    )))
}

#[derive(Serialize, Deserialize)]
pub(super) struct PendingRepair {
    pub(super) fingerprint: String,
    pub(super) defects: usize,
    #[serde(default)]
    pub(super) issue_fingerprint: Option<String>,
    pub(super) context: Value,
    /// The loop's best candidate and escalation, so a resume keeps them.
    #[serde(default)]
    pub(super) progress: RepairProgress,
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
    for finding in report.findings.iter().filter(|finding| {
        finding.verdict != ClaimVerdict::Supported && !finding.general_knowledge()
    }) {
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
    digest(&json!({"policy":POLICY,"repairPolicy":"pending-repair-v2","model":llm.model_name(),"prompt":prompt,"schema":schema,"candidate":candidate,"sources":sources}).to_string())
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
        .filter(|finding| finding.verdict != ClaimVerdict::Supported && !finding.general_knowledge())
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
