//! One verification pass over a candidate lesson: its claim inventory and
//! coverage audit (reused per unchanged section), its executed examples, and
//! the evidence check of every claim.
use super::*;
use crate::features::learning::lessons::call_plan::Fate;

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
    let citations = section_citations(candidate, references)?;
    let claims: Vec<(usize, Claim)> = inventory
        .units
        .iter()
        .flat_map(|unit| {
            unit.claims
                .iter()
                .cloned()
                .map(move |claim| (unit.index, claim))
        })
        .collect();
    let classes = class_checks::sort(llm, candidate, &claims).await?;
    let origins: Vec<Class> = classes.iter().map(|classed| classed.class).collect();
    let routes = class_checks::check(llm, &claims, classes, &citations).await?;
    // The strict check is asked about the statements that need a source, a
    // section at a time in the inventory's order.
    let mut needs_source: Vec<UnitClaims> = Vec::new();
    let mut strict_at: Vec<usize> = Vec::new();
    for (at, ((unit, claim), route)) in claims.iter().zip(&routes).enumerate() {
        if !matches!(route, class_checks::Route::Strict(_)) {
            continue;
        }
        strict_at.push(at);
        match needs_source.last_mut() {
            Some(last) if last.index == *unit => last.claims.push(claim.clone()),
            _ => needs_source.push(UnitClaims {
                index: *unit,
                claims: vec![claim.clone()],
                non_factual_reason: String::new(),
            }),
        }
    }
    let checked = evidence_checks::check_with_citations(
        llm,
        references,
        &Inventory {
            units: needs_source,
        },
        &executions,
        completed,
        &citations,
    )
    .await?;
    let fates: Vec<(Class, Fate)> = checked
        .iter()
        .filter_map(|(ordinal, _, finding, _)| {
            let at = *strict_at.get(*ordinal)?;
            Some((*origins.get(at)?, Fate::of_strict(finding.verdict)))
        })
        .collect();
    crate::features::learning::lesson_progress::strict_checked(&fates);
    let mut strict: HashMap<usize, (String, Finding)> = HashMap::new();
    for (ordinal, key, mut finding, _) in checked {
        let at = strict_at.get(ordinal).copied();
        if let Some(class_checks::Route::Strict(classed)) = at.and_then(|at| routes.get(at)) {
            finding.class = Some(*classed);
        }
        if let Some(at) = at {
            strict.insert(at, (key, finding));
        }
    }
    // The statements left out of the lesson's claims are not in its coverage
    // either: the report accounts for each claim by one finding.
    let mut left = routes
        .iter()
        .map(|route| matches!(route, class_checks::Route::Left));
    let mut accounted: Vec<UnitClaims> = Vec::new();
    for unit in &inventory.units {
        let claims: Vec<Claim> = unit
            .claims
            .iter()
            .filter(|_| !left.next().unwrap_or(false))
            .cloned()
            .collect();
        let non_factual_reason = if claims.is_empty() && unit.non_factual_reason.trim().is_empty() {
            "Its statements are about the lesson itself: a scenario, a rubric, an option or a remark about its sources.".to_owned()
        } else {
            unit.non_factual_reason.clone()
        };
        accounted.push(UnitClaims {
            index: unit.index,
            claims,
            non_factual_reason,
        });
    }
    let mut findings = Vec::with_capacity(claims.len());
    for (at, route) in routes.into_iter().enumerate() {
        match route {
            class_checks::Route::Left => {}
            class_checks::Route::Done(finding, _) => findings.push(*finding),
            class_checks::Route::Strict(_) => {
                let (key, finding) = strict.remove(&at).ok_or_else(|| {
                    AppError::InternalError("A statement was left without a check".into())
                })?;
                completed.record(key, &finding);
                findings.push(finding);
            }
        }
    }
    // Every claim a passage does not support is an issue until the claim
    // ledger closes it.
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
        coverage: accounted,
        coverage_audit: coverage.units,
        findings,
        executions,
        confirmed_examples: None,
        issues,
        retrieval_mode: references.mode().label().into(),
        embedding_model: references.embedding_model(),
        page_budget: Some(evidence_checks::page_budget_note(llm, completed)),
        unexecuted_languages: execution::unexecuted_languages(candidate),
    })
}
