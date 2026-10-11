use super::*;
use crate::features::learning::curriculum_repository::LESSON_PREPARATION;
use crate::features::learning::{
    curriculum_repository::LearningCurriculumRepository, lesson_drafts,
};
use crate::shared::runtime::jobs::{JobStore, RecoveryPolicy};

#[derive(Default)]
struct IncrementalModel {
    calls: Mutex<Vec<String>>,
    fail: bool,
}

#[async_trait::async_trait]
impl LLMPort for IncrementalModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
    }
    fn model_name(&self) -> &str {
        "incremental-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}
impl IncrementalModel {
    async fn respond(&self, prompt: &str) -> Result<String> {
        if let Some(response) = fixture_response(prompt) {
            return Ok(response);
        }
        let (passages, tail) = prompt
            .split_once("\n\nClaim: ")
            .ok_or_else(|| invalid("Unexpected fixture request"))?;
        let claim = tail.split("\n\n").next().unwrap();
        self.calls.lock().unwrap().push(claim.into());
        if self.fail {
            return Err(AppError::Network("Interrupted fixture".into()));
        }
        assert!(
            passages.contains(claim),
            "The original evidence must remain present"
        );
        let verdict = if passages.contains("COUNTEREVIDENCE") {
            "contradicted"
        } else {
            "supported"
        };
        Ok(format!("{verdict}\nReason: Scripted evidence comparison.\nSource quote: {claim}\nSource passage: passage-0"))
    }
}

fn claims(count: usize) -> Inventory {
    Inventory {
        units: (0..count)
            .map(|index| UnitClaims {
                index,
                claims: vec![Claim {
                    quote: format!("mineral{index} property{index}"),
                    statement: format!("mineral{index} property{index}"),
                }],
                non_factual_reason: String::new(),
            })
            .collect(),
    }
}
fn references(count: usize) -> Vec<LearningSourceDto> {
    (0..count)
        .map(|index| LearningSourceDto {
            id: format!("source{index}"),
            title: format!("Source {index}"),
            url: None,
            excerpt: format!("mineral{index} property{index}"),
            acquired_at: 0,
        })
        .collect()
}

#[tokio::test]
async fn one_fact_edit_in_175_claims_rechecks_only_that_fact_and_resume_reuses_all() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let sources = references(175);
    let model = IncrementalModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        // Twenty-two neighbors share the same location passage, as in a real
        // extracted paragraph. Editing one fact changes their surrounding quote.
        let mut inventory = claims(175);
        let mut grouped = Vec::new();
        for (index, group) in inventory.units.chunks(22).enumerate() {
            let quote = group.iter().map(|u| u.claims[0].statement.as_str()).collect::<Vec<_>>().join(". ");
            grouped.push(UnitClaims { index, claims: group.iter().map(|u| Claim { quote: quote.clone(), statement: u.claims[0].statement.clone() }).collect(), non_factual_reason: String::new() });
        }
        inventory.units = grouped;
        let collection = ReferenceCollection::lexical(&sources)?;
        let initial = evidence_checks::check(&model, &collection, &inventory, &[], &ClaimChecks::default()).await?;
        assert_eq!(initial.len(), 175);
        assert_eq!(model.calls.lock().unwrap().len(), 175);
        let before: Vec<_> = inventory.units.iter().map(|u|json!({"kind":"teaching","body":u.claims[0].quote})).collect();
        for unit in &inventory.units { inventory::revisions::save(&model, &before, unit).await?; }
        // A real scalar-text patch, not a replacement section.
        let document = json!({"blocks":before,"questions":[]});
        let revised = crate::features::learning::lessons::text_edits::apply(&document,
            &json!({"edits":[{"path":"/blocks/0/body","before":"mineral0 property0","after":"mineral0"}]}).to_string())?;
        let content: Vec<Value> = serde_json::from_value(revised["blocks"].clone())?;
        struct RevisionModel;
        #[async_trait::async_trait]
        impl LLMPort for RevisionModel {
            async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
                let mut replies = Vec::new();
                for prompt in fixture_prompts(request) {
                    replies.push(self.respond(&prompt).await?);
                }
                Ok(fixture_completion(request, replies))
            }
            fn model_name(&self) -> &str { "incremental-fixture" }
            fn max_context_tokens(&self) -> usize { 128000 }
            fn count_tokens(&self, text: &str) -> usize { text.len()/4 }
            async fn is_ready(&self) -> Result<bool> { Ok(true) }
        }
        impl RevisionModel {
            async fn respond(&self, prompt: &str) -> Result<String> {
                assert!(prompt.starts_with("Update claims after a lesson edit."));
                let data = context(prompt);
                let passage = data["revisedSection"]["passages"].as_array().unwrap().iter().find(|p|p["field"]=="/body").unwrap();
                Ok(json!({"changes":[json!({"claimId":0,"passageId":passage["id"],"statement":"mineral0"})],"nonFactualReason":""}).to_string())
            }
        }

        let updated = inventory::revisions::update(&RevisionModel, &content, 0).await?.unwrap();
        assert_eq!(updated.claims.len(), 22);
        for (old, new) in inventory.units[0].claims.iter().zip(&updated.claims).skip(1) {
            assert_eq!(old.statement, new.statement);
            assert_ne!(old.quote, new.quote);
        }
        inventory.units[0] = updated;
        let result = evidence_checks::check(&model, &collection, &inventory, &[], &ClaimChecks::default()).await?;
        assert_eq!(result.iter().filter(|r|r.3).count(), 174);
        assert_eq!(model.calls.lock().unwrap().len(), 176);
        assert_eq!(model.calls.lock().unwrap().last().unwrap(), "mineral0");
        // Drop memory caches, restore from SQLite, and compare the saved revision.
        let restored = inventory::revisions::update(&RevisionModel, &content, 0).await?.unwrap();
        assert_eq!(restored.claims[0].statement, "mineral0");
        let result = evidence_checks::check(&model, &collection, &inventory, &[], &ClaimChecks::default()).await?;
        assert_eq!(result.iter().filter(|r|r.3).count(), 175);
        assert_eq!(model.calls.lock().unwrap().len(), 176);
        Ok(())
    }).await
}

#[tokio::test]
async fn prior_quote_bound_receipt_is_recovered_without_fuzzy_matching_or_new_approval(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let model = IncrementalModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        let sources = references(1);
        let collection = ReferenceCollection::lexical(&sources)?;
        let original = claims(1);
        let claim = &original.units[0].claims[0];
        let evidence = evidence_for(&claim.statement, 0, &collection, &[]).await?;
        let passages: Vec<_> = evidence.iter().map(|p|(&p.source_id,&p.text,p.start_byte,p.end_byte)).collect();
        let comparison = digest(&json!({"unit":0,"claim":claim,"passages":passages}).to_string());
        lesson_drafts::record_checkpoint(&claim_receipt_key(&model,&comparison), json!({"verdict":"supported","reason":"Prior complete comparison","supporting_quote":claim.statement})).await?;
        lesson_drafts::record_checkpoint("inventory-section-v1:legacy-fixture",json!({"inventory":original.units[0]})).await?;
        let old_selection = format!("claim-evidence-v1:{}",digest(&json!({"policy":POLICY,"model":model.model_name(),"context":model.max_context_tokens(),"unit":0,"claim":claim,"retrieval":collection.mode(),"embedding":collection.embedding_model()}).to_string()));
        lesson_drafts::record_checkpoint(&old_selection,json!({"sources":evidence_selection::sources(&collection),"passages":evidence.iter().map(|p|json!({"source_id":p.source_id,"start":p.start_byte,"end":p.end_byte,"sha256":digest(&p.text)})).collect::<Vec<_>>()})).await?;
        let mut revised = claims(1);
        revised.units[0].claims[0].quote.push_str(". Corrected neighbor.");
        let pending_selection = format!("claim-evidence-v1:{}",digest(&json!({"policy":POLICY,"model":model.model_name(),"context":model.max_context_tokens(),"unit":0,"claim":revised.units[0].claims[0],"retrieval":collection.mode(),"embedding":collection.embedding_model()}).to_string()));
        // The previous implementation re-retrieved after relocation. This
        // unfinished ranking change among already searched sources must not
        // displace the evidence of the completed comparison.
        lesson_drafts::record_checkpoint(&pending_selection,json!({"sources":evidence_selection::sources(&collection),"passages":[{"source_id":"source0","start":0,"end":8,"sha256":digest("mineral0")}]})).await?;
        let result = evidence_checks::check(&model,&collection,&revised,&[],&ClaimChecks::default()).await?;
        assert!(result[0].3);
        assert_eq!(result[0].2.quote,revised.units[0].claims[0].quote);
        assert!(model.calls.lock().unwrap().is_empty());
        revised.units[0].claims[0].statement = "mineral0".into();
        let result = evidence_checks::check(&model,&collection,&revised,&[],&ClaimChecks::default()).await?;
        assert!(!result[0].3, "A changed assertion cannot inherit the old verdict");
        assert_eq!(model.calls.lock().unwrap().len(),1);
        Ok(())
    }).await
}

#[tokio::test]
async fn research_reopens_one_of_170_checks_and_resume_reuses_all_completed_decisions() -> Result<()>
{
    let dir = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(dir.path().join("incremental.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let mut sources = references(170);
    let model = IncrementalModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        let result = evidence_checks::check(
            &model,
            &ReferenceCollection::lexical(&sources)?,
            &claims(170),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(result.len(), 170);
        assert_eq!(model.calls.lock().unwrap().len(), 170);
        Ok(())
    })
    .await?;
    drop(repo);
    pool.close().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    JobStore::new(pool.clone())
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert!(JobStore::new(pool.clone()).claim(&job).await?.is_some());
    let broad_counterevidence = format!(
        "{} COUNTEREVIDENCE",
        (0..170)
            .map(|index| format!("mineral{index} property{index}"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    sources.push(LearningSourceDto {
        id: "new-counterevidence".into(),
        title: "New research".into(),
        url: None,
        excerpt: broad_counterevidence,
        acquired_at: 1,
    });
    let resumed = IncrementalModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        let collection = ReferenceCollection::lexical(&sources)?;
        {
            let transient = ClaimChecks::default();
            evidence_selection::record_research_scope(
                &collection,
                &[Finding {
                    unit: 84,
                    quote: "mineral84 property84".into(),
                    statement: "mineral84 property84".into(),
                    verdict: ClaimVerdict::Unsupported,
                    reason: "Targeted research needs counterevidence.".into(),
                    evidence: Vec::new(),
                    supporting_quote: None,
                }],
                &transient,
            )
            .await?;
        }
        // Drop the in-memory scope above. The fresh checker must restore the
        // target from SQLite rather than treating this broadly matching source
        // as a reason to reopen all 170 completed comparisons.
        let result = evidence_checks::check(
            &resumed,
            &collection,
            &claims(170),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(result.iter().filter(|r| r.3).count(), 169);
        assert_eq!(*resumed.calls.lock().unwrap(), vec!["mineral84 property84"]);
        assert_eq!(
            result[84].2.verdict,
            ClaimVerdict::Contradicted,
            "Research must reopen a previous approval when counterevidence is retrieved"
        );
        assert_eq!(
            result[84].2.evidence.len(),
            2,
            "Retain the original evidence alongside the new source"
        );
        let result = evidence_checks::check(
            &resumed,
            &ReferenceCollection::lexical(&sources)?,
            &claims(170),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(
            result.iter().filter(|r| r.3).count(),
            170,
            "Completed negative verdicts are also reusable; repeating them is not progress"
        );
        assert_eq!(resumed.calls.lock().unwrap().len(), 1);
        let mut edited = claims(170);
        edited.units[0].claims[0]
            .quote
            .push_str(" corrected context");
        let result = evidence_checks::check(
            &resumed,
            &ReferenceCollection::lexical(&sources)?,
            &edited,
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(result.iter().filter(|r| r.3).count(), 170);
        assert_eq!(*resumed.calls.lock().unwrap(), vec!["mineral84 property84"]);
        sources[169].excerpt.push_str(" revised");
        let result = evidence_checks::check(
            &resumed,
            &ReferenceCollection::lexical(&sources)?,
            &edited,
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(
            result.iter().filter(|r| r.3).count(),
            169,
            "Changing one source must not replace evidence for unrelated claims"
        );
        assert_eq!(
            resumed.calls.lock().unwrap().last().unwrap(),
            "mineral169 property169"
        );
        assert_eq!(resumed.calls.lock().unwrap().len(), 2);
        Ok(())
    })
    .await?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn ranking_churn_and_unrelated_research_do_not_replace_pinned_evidence() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let mut sources = references(10);
    for source in &mut sources {
        source.excerpt = "mineral0 property0".into();
    }
    let model = IncrementalModel::default();
    lesson_drafts::run(&repo, &job, &lesson, async {
        let first = evidence_checks::check(
            &model,
            &ReferenceCollection::lexical(&sources)?,
            &claims(1),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        sources.reverse();
        sources.push(LearningSourceDto {
            id: "unrelated".into(),
            title: "Other topic".into(),
            url: None,
            excerpt: "galaxies nebulae".into(),
            acquired_at: 1,
        });
        let references = ReferenceCollection::lexical(&sources)?;
        let fresh = evidence_for("mineral0 property0", 0, &references, &[]).await?;
        let claim = &claims(1).units[0].claims[0];
        assert_ne!(
            ClaimChecks::key(0, claim, &fresh),
            ClaimChecks::key(0, claim, &first[0].2.evidence),
            "The fixture must actually change the old global retrieval results"
        );
        let resumed = evidence_checks::check(
            &model,
            &references,
            &claims(1),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert!(resumed[0].3);
        assert_eq!(model.calls.lock().unwrap().len(), 1);
        assert_eq!(
            ClaimChecks::key(0, claim, &resumed[0].2.evidence),
            ClaimChecks::key(0, claim, &first[0].2.evidence)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn interrupted_new_evidence_cannot_inherit_the_previous_approval() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let mut sources = references(1);
    lesson_drafts::run(&repo, &job, &lesson, async {
        evidence_checks::check(
            &IncrementalModel::default(),
            &ReferenceCollection::lexical(&sources)?,
            &claims(1),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        sources.push(LearningSourceDto {
            id: "counterevidence".into(),
            title: "Research".into(),
            url: None,
            excerpt: "mineral0 property0 COUNTEREVIDENCE".into(),
            acquired_at: 1,
        });
        let failed = IncrementalModel {
            fail: true,
            ..Default::default()
        };
        assert!(evidence_checks::check(
            &failed,
            &ReferenceCollection::lexical(&sources)?,
            &claims(1),
            &[],
            &ClaimChecks::default()
        )
        .await
        .is_err());
        let model = IncrementalModel::default();
        let resumed = evidence_checks::check(
            &model,
            &ReferenceCollection::lexical(&sources)?,
            &claims(1),
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        assert!(!resumed[0].3);
        assert_eq!(resumed[0].2.verdict, ClaimVerdict::Contradicted);
        assert_eq!(model.calls.lock().unwrap().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn teaching_review_survives_added_sources_but_not_changed_content_or_sources() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    lesson_drafts::run(&repo, &job, &lesson, async {
        lesson_drafts::resume("teaching-fixture".into()).await?;
        let candidate = candidate(GOOD).to_string();
        let mut sources = vec![source()];
        let first = crate::features::learning::teaching::review_lesson(
            &Model::new(),
            "system",
            "{}",
            &json!({}),
            candidate.clone(),
            3000,
            &ReferenceCollection::lexical(&sources)?,
        )
        .await?;
        // Keep the model name identical but make any new request fail.
        struct NoRequests;
        #[async_trait::async_trait]
        impl LLMPort for NoRequests {
            async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
                let mut replies = Vec::new();
                for prompt in fixture_prompts(request) {
                    replies.push(self.respond(&prompt).await?);
                }
                Ok(fixture_completion(request, replies))
            }
            fn model_name(&self) -> &str {
                "scripted-evidence-checker"
            }
            fn count_tokens(&self, text: &str) -> usize {
                text.len() / 4
            }
            fn max_context_tokens(&self) -> usize {
                128000
            }
            async fn is_ready(&self) -> Result<bool> {
                Ok(true)
            }
        }
        impl NoRequests {
            async fn respond(&self, _prompt: &str) -> Result<String> {
                Err(invalid("Unexpected repeated teaching review"))
            }
        }

        sources.extend(references(1));
        let restored = crate::features::learning::teaching::review_lesson(
            &NoRequests,
            "system",
            "{}",
            &json!({}),
            first.clone(),
            3000,
            &ReferenceCollection::lexical(&sources)?,
        )
        .await?;
        assert_eq!(restored, first);
        sources[0].excerpt.push_str(" Changed source.");
        assert!(crate::features::learning::teaching::review_lesson(
            &NoRequests,
            "system",
            "{}",
            &json!({}),
            first,
            3000,
            &ReferenceCollection::lexical(&sources)?
        )
        .await
        .is_err());
        assert!(crate::features::learning::teaching::review_lesson(
            &NoRequests,
            "system",
            "{}",
            &json!({}),
            candidate.replace(GOOD, BAD),
            3000,
            &ReferenceCollection::lexical(&sources)?
        )
        .await
        .is_err());
        Ok(())
    })
    .await
}
