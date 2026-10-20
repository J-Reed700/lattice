//! One verification pass over a candidate lesson: its claim inventory and
//! coverage audit (reused per unchanged section), its executed examples, and
//! the evidence check of every claim.
use super::*;

pub(super) async fn verify_with_references(
    llm: &dyn LLMPort,
    runtimes: &BuiltinRuntimes,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
    checks: &mut ClaimChecks,
) -> Result<LessonVerificationReport> {
    let candidate_text = candidate.to_string();
    let content = units(candidate)?;
    let checkpoint = crate::features::learning::lesson_drafts::claim_inventory(&candidate_text)
        .await?
        .and_then(|raw| serde_json::from_str::<InventoryCheckpoint>(&raw).ok())
        .filter(|saved| {
            saved.policy == INVENTORY_POLICY
                && saved.model == llm.model_name()
                && saved.model_context == llm.max_context_tokens()
                && saved.content_sha256 == digest(&candidate_text)
                && validate_inventory(&saved.inventory, &content).is_ok()
        });
    let (inventory, coverage) = match checkpoint {
        Some(mut saved) => {
            if saved.coverage_policy.as_deref() != Some(coverage::POLICY)
                || !valid_saved_coverage(&saved.coverage, &content)
            {
                saved.coverage.units.clear();
            }
            crate::features::learning::lesson_progress::phase(
                crate::features::learning::lesson_progress::Phase::Coverage,
                "Resuming saved claim coverage with the current policy",
            );
            complete_inventory(
                llm,
                &content,
                saved.inventory,
                &candidate_text,
                Some(saved.coverage),
            )
            .await?
        }
        None => extract_audited_inventory(llm, &content, &candidate_text).await?,
    };
    let checkpoint = InventoryCheckpoint {
        policy: INVENTORY_POLICY.into(),
        model: llm.model_name().into(),
        model_context: llm.max_context_tokens(),
        content_sha256: digest(&candidate_text),
        inventory,
        coverage,
        coverage_policy: Some(coverage::POLICY.into()),
    };
    crate::features::learning::lesson_drafts::record_claim_inventory(
        &candidate_text,
        serde_json::to_string(&checkpoint)?,
    )
    .await?;
    let (inventory, coverage) = (checkpoint.inventory, checkpoint.coverage);
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Examples,
        "Checking executable examples",
    );
    let executions = execution::observe(runtimes, candidate).await?;
    check_evidence(
        llm, candidate, references, inventory, coverage, executions, checks,
    )
    .await
}

pub(super) async fn check_evidence(
    llm: &dyn LLMPort,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
    inventory: Inventory,
    coverage: Coverage,
    executions: Vec<execution::Observation>,
    completed: &mut ClaimChecks,
) -> Result<LessonVerificationReport> {
    let mut issues = Vec::new();
    if executions
        .iter()
        .any(|observation| observation.unavailable())
    {
        return Err(AppError::ServiceNotAvailable("An example runtime is unavailable or cancelled. No lesson was published; retry when the runtime is available.".into()));
    }
    for observation in &executions {
        if !observation.passed() {
            issues.push(format!(
                "Example {} did not complete: {}",
                observation.id,
                observation.evidence()
            ));
        }
    }
    let results = evidence_checks::check_with_citations(
        llm,
        references,
        &inventory,
        &executions,
        completed,
        &section_citations(candidate, references)?,
    )
    .await?;
    let findings: Vec<_> = results
        .into_iter()
        .map(|(_, key, finding, _)| {
            completed.record(key, &finding);
            finding
        })
        .collect();
    // Every claim a passage does not support is an issue until the claim
    // ledger closes it: a general-knowledge proposal waits for the user (D1).
    for finding in &findings {
        if !finding.publishable() {
            issues.push(format!(
                "Section {}: {} — {}",
                finding.unit + 1,
                finding.statement,
                finding.reason
            ));
        }
    }
    Ok(LessonVerificationReport {
        policy: POLICY.into(),
        lesson_id: String::new(),
        content_sha256: String::new(),
        checker_model: llm.model_name().into(),
        checked_at: chrono::Utc::now().timestamp_millis(),
        sources: references
            .sources
            .iter()
            .map(|s| SourceBinding {
                id: s.id.clone(),
                sha256: digest(&s.excerpt),
            })
            .collect(),
        coverage: inventory.units,
        coverage_audit: coverage.units,
        findings,
        executions,
        confirmed_examples: None,
        issues,
        retrieval_mode: references.mode().label().into(),
        embedding_model: references.embedding_model(),
        unexecuted_languages: execution::unexecuted_languages(candidate),
    })
}
