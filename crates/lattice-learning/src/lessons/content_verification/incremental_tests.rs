use super::readings_tests::paged;
use super::*;
use crate::curriculum_repository::LESSON_PREPARATION;
use crate::{curriculum_repository::LearningCurriculumRepository, lesson_drafts};
use lattice_core::shared::runtime::jobs::{JobStore, RecoveryPolicy};

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
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let sources = references(175);
    let model = IncrementalModel::default();
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        // Twenty-two facts to a section, a sentence each, as in a real
        // extracted paragraph. Editing one fact rewrites its sentence alone.
        let sentence = |n: usize| format!("Mineral {n} is recorded.");
        let mut inventory = claims(175);
        let mut grouped = Vec::new();
        for (index, group) in inventory.units.chunks(22).enumerate() {
            let claims = group.iter().enumerate().map(|(offset, unit)| Claim { quote: sentence(index * 22 + offset), statement: unit.claims[0].statement.clone() }).collect();
            grouped.push(UnitClaims { index, claims, non_factual_reason: String::new() });
        }
        inventory.units = grouped;
        let collection = ReferenceCollection::lexical(&sources)?;
        let initial = paged(&model, &collection, &inventory, &ClaimChecks::default()).await?;
        assert_eq!(initial.len(), 175);
        assert_eq!(model.calls.lock().unwrap().len(), 175);
        let before: Vec<_> = inventory.units.iter().map(|u|json!({"kind":"teaching","body":u.claims.iter().map(|claim| claim.quote.as_str()).collect::<Vec<_>>().join(" ")})).collect();
        for unit in &inventory.units { inventory::revisions::save(&model, &before, unit).await?; }
        // A real scalar-text patch, not a replacement section.
        let document = json!({"blocks":before,"questions":[]});
        let revised = crate::lessons::text_edits::apply(&document,
            &json!({"edits":[{"path":"/blocks/0/body","before":sentence(0),"after":"Mineral 0 is catalogued."}]}).to_string())?;
        let content: Vec<Value> = serde_json::from_value(revised["blocks"].clone())?;
        struct DeltaModel;
        #[async_trait::async_trait]
        impl LLMPort for DeltaModel {
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
        impl DeltaModel {
            async fn respond(&self, prompt: &str) -> Result<String> {
                assert!(prompt.starts_with("Extract lesson claims."));
                let data = fixture_task(prompt).unwrap();
                let passage = &data["units"][0]["passages"][0];
                Ok(inventory::fixture_response(vec![json!({"index":0,"claims":[{"sentenceId":sentence_id(passage, "mineral0"),"statement":"mineral0"}],"nonFactualReason":""})]).to_string())
            }
        }

        let updated = inventory::revisions::update(&DeltaModel, &content, 0).await?.unwrap();
        // The twenty-one sentences that stand keep their claims as they were,
        // and the one that was rewritten has the claim read from its new words.
        assert_eq!(updated.claims.len(), 22);
        for (old, new) in inventory.units[0].claims.iter().skip(1).zip(&updated.claims) {
            assert_eq!(old.statement, new.statement);
            assert_eq!(old.quote, new.quote);
        }
        assert_eq!(updated.claims[21].statement, "mineral0");
        assert_eq!(updated.claims[21].quote, "Mineral 0 is catalogued.");
        inventory.units[0] = updated;
        let result = paged(&model, &collection, &inventory, &ClaimChecks::default()).await?;
        assert_eq!(result.iter().filter(|r| r.reused()).count(), 174);
        assert_eq!(model.calls.lock().unwrap().len(), 176);
        assert_eq!(model.calls.lock().unwrap().last().unwrap(), "mineral0");
        // Drop memory caches, restore from SQLite, and compare the saved delta.
        let restored = inventory::revisions::update(&DeltaModel, &content, 0).await?.unwrap();
        assert_eq!(restored.claims[21].statement, "mineral0");
        let result = paged(&model, &collection, &inventory, &ClaimChecks::default()).await?;
        assert_eq!(result.iter().filter(|r| r.reused()).count(), 175);
        assert_eq!(model.calls.lock().unwrap().len(), 176);
        Ok(())
    }).await
}

#[tokio::test]
async fn a_receipt_is_found_only_for_the_statement_on_the_quote_it_was_saved_for() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let model = IncrementalModel::default();
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        // Two sources, so the claim's words tell its passage from the other.
        let sources = references(2);
        let collection = ReferenceCollection::lexical(&sources)?;
        let original = claims(1);
        let claim = &original.units[0].claims[0];
        let evidence = evidence_for(&model, &claim.statement, 0, &collection, &[]).await?;
        let passages: Vec<_> = evidence.iter().map(|p|(&p.source_id,&p.text,p.start_byte,p.end_byte)).collect();
        let comparison = sha256_hex(json!({"unit":0,"claim":claim,"passages":passages}).to_string());
        lesson_drafts::record_checkpoint(&claim_receipt_key(&model,&comparison), json!({"verdict":"supported","reason":"Prior complete comparison","supporting_quote":claim.statement})).await?;
        // The unit's saved inventory holds that claim on its quote: where the
        // same statement on another quote of the unit used to look for it.
        lesson_drafts::record_checkpoint("inventory-section-v1:legacy-fixture",json!({"inventory":original.units[0]})).await?;
        let old_selection = format!("claim-evidence-v1:{}",sha256_hex(json!({"policy":POLICY,"model":model.model_name(),"context":model.max_context_tokens(),"unit":0,"claim":claim,"retrieval":collection.mode().label(),"embedding":collection.embedding_model()}).to_string()));
        lesson_drafts::record_checkpoint(&old_selection,json!({"sources":evidence_selection::sources(&collection),"passages":evidence.iter().map(|p|json!({"source_id":p.source_id,"start":p.start_byte,"end":p.end_byte,"sha256":sha256_hex(&p.text)})).collect::<Vec<_>>()})).await?;
        let mut revised = claims(1);
        revised.units[0].claims[0].quote.push_str(". Corrected neighbor.");
        let pending_selection = format!("claim-evidence-v1:{}",sha256_hex(json!({"policy":POLICY,"model":model.model_name(),"context":model.max_context_tokens(),"unit":0,"claim":revised.units[0].claims[0],"retrieval":collection.mode().label(),"embedding":collection.embedding_model()}).to_string()));
        // The previous implementation re-retrieved after relocation. This
        // unfinished ranking change among already searched sources must not
        // displace the evidence of the completed comparison.
        lesson_drafts::record_checkpoint(&pending_selection,json!({"sources":evidence_selection::sources(&collection),"passages":[{"source_id":"source0","start":0,"end":8,"sha256":sha256_hex("mineral0")}]})).await?;
        let result = paged(&model,&collection,&revised,&ClaimChecks::default()).await?;
        assert!(!result[0].reused(), "A receipt saved for the statement on another quote was found for this one");
        assert_eq!(result[0].finding().quote,revised.units[0].claims[0].quote);
        assert_eq!(model.calls.lock().unwrap().len(),1);
        revised.units[0].claims[0].statement = "mineral0".into();
        let result = paged(&model,&collection,&revised,&ClaimChecks::default()).await?;
        assert!(!result[0].reused(), "A changed assertion cannot inherit the old verdict");
        assert_eq!(model.calls.lock().unwrap().len(),2);
        Ok(())
    }).await
}

#[tokio::test]
async fn research_reopens_one_of_170_checks_and_resume_reuses_all_completed_decisions() -> Result<()>
{
    let dir = tempfile::tempdir()?;
    let options = crate::test_support::file_options(dir.path().join("incremental.db"));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await?;
    let db = Database::new(pool.clone());
    sqlx::migrate!("../lattice-desktop/migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let mut sources = references(170);
    let model = IncrementalModel::default();
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let result = paged(
            &model,
            &ReferenceCollection::lexical(&sources)?,
            &claims(170),
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
        .await?;
    let db = Database::new(pool.clone());
    let repo = LearningCurriculumRepository::new(db.clone());
    JobStore::new(db.clone())
        .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
        .await?;
    assert!(JobStore::new(db.clone()).claim(&job).await?.is_some());
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
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
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
                    resolution: None,
                    claim_id: None,
                    two_pass: Default::default(),
                    label: None,
                    class: None,
                    trust: None,
                }],
                &[],
                &transient,
            )
            .await?;
        }
        // Drop the in-memory scope above. The fresh checker must restore the
        // target from SQLite rather than treating this broadly matching source
        // as a reason to reopen all 170 completed comparisons.
        let result = paged(&resumed, &collection, &claims(170), &ClaimChecks::default()).await?;
        assert_eq!(result.iter().filter(|r| r.reused()).count(), 169);
        assert_eq!(*resumed.calls.lock().unwrap(), vec!["mineral84 property84"]);
        assert_eq!(
            result[84].finding().verdict,
            ClaimVerdict::Contradicted,
            "Research must reopen a previous approval when counterevidence is retrieved"
        );
        assert_eq!(
            result[84].finding().evidence.len(),
            2,
            "Retain the original evidence alongside the new source"
        );
        let result = paged(
            &resumed,
            &ReferenceCollection::lexical(&sources)?,
            &claims(170),
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(
            result.iter().filter(|r| r.reused()).count(),
            170,
            "Completed negative verdicts are also reusable; repeating them is not progress"
        );
        assert_eq!(resumed.calls.lock().unwrap().len(), 1);
        let mut edited = claims(170);
        edited.units[0].claims[0]
            .quote
            .push_str(" corrected context");
        let result = paged(
            &resumed,
            &ReferenceCollection::lexical(&sources)?,
            &edited,
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(result.iter().filter(|r| r.reused()).count(), 170);
        assert_eq!(*resumed.calls.lock().unwrap(), vec!["mineral84 property84"]);
        sources[169].excerpt.push_str(" revised");
        let result = paged(
            &resumed,
            &ReferenceCollection::lexical(&sources)?,
            &edited,
            &ClaimChecks::default(),
        )
        .await?;
        assert_eq!(
            result.iter().filter(|r| r.reused()).count(),
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
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let mut sources = references(10);
    let model = IncrementalModel::default();
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let first = paged(
            &model,
            &ReferenceCollection::lexical(&sources)?,
            &claims(1),
            &ClaimChecks::default(),
        )
        .await?;
        // The collection reorders and grows by a source researched for
        // another claim, which ranks above the claim's own for its words.
        sources.reverse();
        sources.push(LearningSourceDto {
            id: "researched-for-another".into(),
            title: "Other research".into(),
            url: None,
            excerpt: "mineral0 property0 mineral0 property0".into(),
            acquired_at: 1,
        });
        let references = ReferenceCollection::lexical(&sources)?;
        evidence_selection::record_research_scope(
            &references,
            &[Finding {
                unit: 5,
                quote: "mineral5 property5".into(),
                statement: "mineral5 property5".into(),
                verdict: ClaimVerdict::Unsupported,
                reason: "Targeted research for another claim.".into(),
                evidence: Vec::new(),
                supporting_quote: None,
                resolution: None,
                trust: None,
                claim_id: None,
                two_pass: Default::default(),
                label: None,
                class: None,
            }],
            &[],
            &ClaimChecks::default(),
        )
        .await?;
        let fresh = evidence_for(&model, "mineral0 property0", 0, &references, &[]).await?;
        let claim = &claims(1).units[0].claims[0];
        assert_ne!(
            ClaimChecks::key(0, claim, &fresh),
            ClaimChecks::key(0, claim, &first[0].finding().evidence),
            "The fixture must actually change the claim's fresh retrieval"
        );
        let resumed = paged(&model, &references, &claims(1), &ClaimChecks::default()).await?;
        assert!(resumed[0].reused());
        assert_eq!(model.calls.lock().unwrap().len(), 1);
        assert_eq!(
            ClaimChecks::key(0, claim, &resumed[0].finding().evidence),
            ClaimChecks::key(0, claim, &first[0].finding().evidence)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_ranking_bump_rebuilds_the_first_page_of_the_course_once() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let sources = references(3);
    let model = IncrementalModel::default();
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        let references = ReferenceCollection::lexical(&sources)?;
        let claim = &claims(1).units[0].claims[0];
        // A selection the former ranking saved for this claim, pointing at a
        // passage the claim's words are not in: restored, the scripted
        // checker would refuse it.
        let stale = evidence_selection::selection_key_under(
            &model,
            &references,
            (0, &claim.statement),
            "lesson-sources-first",
        );
        lesson_drafts::record_checkpoint(
            &stale,
            serde_json::to_value(evidence_selection::selection_of(
                &evidence_selection::sources(&references),
                &evidence_for(&model, "mineral1 property1", 0, &references, &[]).await?,
            ))?,
        )
        .await?;
        let first = paged(&model, &references, &claims(1), &ClaimChecks::default()).await?;
        assert_eq!(first[0].finding().verdict, ClaimVerdict::Supported);
        assert!(first[0].finding().evidence[0]
            .text
            .contains("mineral0 property0"));
        assert_eq!(model.calls.lock().unwrap().len(), 1);
        // Rebuilt once: a second pass with unchanged sources makes no call.
        let second = paged(&model, &references, &claims(1), &ClaimChecks::default()).await?;
        assert!(second[0].reused());
        assert_eq!(model.calls.lock().unwrap().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn interrupted_new_evidence_cannot_inherit_the_previous_approval() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let mut sources = references(2);
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        paged(
            &IncrementalModel::default(),
            &ReferenceCollection::lexical(&sources)?,
            &claims(1),
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
        assert!(paged(
            &failed,
            &ReferenceCollection::lexical(&sources)?,
            &claims(1),
            &ClaimChecks::default()
        )
        .await
        .is_err());
        let model = IncrementalModel::default();
        let resumed = paged(
            &model,
            &ReferenceCollection::lexical(&sources)?,
            &claims(1),
            &ClaimChecks::default(),
        )
        .await?;
        assert!(!resumed[0].reused());
        assert_eq!(resumed[0].finding().verdict, ClaimVerdict::Contradicted);
        assert_eq!(model.calls.lock().unwrap().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn teaching_review_survives_added_sources_but_not_changed_content_or_sources() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        lesson_drafts::resume("teaching-fixture".into()).await?;
        let candidate = candidate(GOOD).to_string();
        let mut sources = vec![source()];
        let first = crate::teaching::review_lesson(
            &Model::new(),
            "system",
            "{}",
            crate::teaching::ReviewSchema::Other(&json!({})),
            candidate.clone(),
            3000,
            (&ReferenceCollection::lexical(&sources)?, &[]),
            &[],
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
        let restored = crate::teaching::review_lesson(
            &NoRequests,
            "system",
            "{}",
            crate::teaching::ReviewSchema::Other(&json!({})),
            first.clone(),
            3000,
            (&ReferenceCollection::lexical(&sources)?, &[]),
            &[],
        )
        .await?;
        assert_eq!(restored, first);
        sources[0].excerpt.push_str(" Changed source.");
        assert!(crate::teaching::review_lesson(
            &NoRequests,
            "system",
            "{}",
            crate::teaching::ReviewSchema::Other(&json!({})),
            first,
            3000,
            (&ReferenceCollection::lexical(&sources)?, &[]),
            &[],
        )
        .await
        .is_err());
        assert!(crate::teaching::review_lesson(
            &NoRequests,
            "system",
            "{}",
            crate::teaching::ReviewSchema::Other(&json!({})),
            candidate.replace(GOOD, BAD),
            3000,
            (&ReferenceCollection::lexical(&sources)?, &[]),
            &[],
        )
        .await
        .is_err());
        Ok(())
    })
    .await
}
