use super::{
    pack_repository::LearningPackRepository,
    portability_dto::{
        ApplyLearningPackImportRequestDto, CancelLearningPackImportPreviewRequestDto,
        CreateLearningSourceSelectorRequestDto, DeleteLearningSourceRequestDto,
        ExportLearningPackRequestDto, LearningPackConflictPolicy,
        PreviewLearningPackImportRequestDto,
    },
    recall_repository::LearningRecallRepository,
    repository::LearningRepository,
    source_library::{CapturedLearningSource, LearningSourceLibraryRepository},
    tests::{fixture, pool},
};
use crate::shared::error::Result;

fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn captured(text: &str) -> CapturedLearningSource {
    CapturedLearningSource {
        title: "Portability source".into(),
        publisher: Some("Test publisher".into()),
        requested_url: Some("https://example.org/portable-source".into()),
        resolved_url: Some("https://example.org/portable-source".into()),
        text: text.into(),
        truncated: false,
        extraction_version: "pack_test_capture_v1".into(),
    }
}

async fn test_container() -> Result<crate::interfaces::di::Container> {
    crate::tests::common::setup_test_container().await
}

async fn export_copy(
    repository: &LearningPackRepository,
    container: &crate::interfaces::di::Container,
    program_id: &str,
) -> Result<(String, String)> {
    let exported = repository
        .export(
            container,
            &ExportLearningPackRequestDto {
                operation_id: id(),
                program_id: program_id.into(),
                file_name: format!("roundtrip-{}.lattice-learning", id()),
                include_evidence: true,
                include_practical_artifacts: true,
                include_source_bodies: true,
                source_body_redistribution_confirmed: true,
            },
        )
        .await?;
    let path = exported
        .exports
        .first()
        .expect("export record")
        .destination_path
        .clone();
    let preview = repository
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path: path,
            conflict_policy: LearningPackConflictPolicy::CreateCopy,
        })
        .await?;
    let preview_row = preview.import_previews.first().expect("copy preview");
    let target_program_id = preview_row.incoming_program_id.clone();
    repository
        .apply(
            container,
            &ApplyLearningPackImportRequestDto {
                operation_id: id(),
                preview_id: preview_row.id.clone(),
                expected_root_sha256: preview_row.manifest.root_sha256.clone(),
            },
        )
        .await?;
    Ok((target_program_id, preview_row.id.clone()))
}

#[tokio::test]
async fn create_copy_preserves_tombstones_versions_selectors_and_curriculum_links() -> Result<()> {
    let pool = pool().await?;
    let mut program = fixture();
    program.summary.status = super::dto::LearningProgramStatus::Active;
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let replacement_id = program.modules[0].lessons[1].id.clone();
    sqlx::query("UPDATE learning_lessons SET curriculum_state='replaced',replacement_lesson_id=? WHERE program_id=? AND id=?")
        .bind(&replacement_id)
        .bind(&program.summary.id)
        .bind(&lesson_id)
        .execute(&pool)
        .await
        .expect("lesson state set");

    let sources = LearningSourceLibraryRepository::new(pool.clone());
    let source_id = id();
    let first_version_id = id();
    sources
        .add(
            &id(),
            &source_id,
            &first_version_id,
            &program.summary.id,
            super::dto::LearningSourceKind::Web,
            "add_web",
            "https://example.org/portable-source",
            Some("https://example.org/portable-source"),
            super::dto::LearningSourcePolicy::Manual,
            captured("First immutable passage. A quoted rule appears here."),
            "source-add-payload".into(),
        )
        .await?;
    let quote_start = "First immutable passage. A quoted rule appears here."
        .find("quoted rule")
        .expect("test quote offset");
    let selector_id = id();
    LearningRecallRepository::new(pool.clone())
        .create_selector(&CreateLearningSourceSelectorRequestDto {
            operation_id: id(),
            selector_id: selector_id.clone(),
            program_id: program.summary.id.clone(),
            source_id: source_id.clone(),
            source_version_id: first_version_id.clone(),
            start_byte: quote_start,
            end_byte: quote_start + "quoted rule".len(),
        })
        .await?;
    sources
        .refresh(
            &super::dto::RefreshLearningSourceRequestDto {
                operation_id: id(),
                program_id: program.summary.id.clone(),
                source_id: source_id.clone(),
                expected_revision: 0,
            },
            Some(captured("Second immutable passage.")),
            None,
        )
        .await?;
    sources
        .delete_source(&DeleteLearningSourceRequestDto {
            operation_id: id(),
            program_id: program.summary.id.clone(),
            source_id: source_id.clone(),
            expected_revision: 1,
            reason: "Removed from the active source list".into(),
        })
        .await?;

    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let (copy_id, _) = export_copy(&packs, &container, &program.summary.id).await?;

    let imported_lesson: (String, Option<String>) = sqlx::query_as(
        "SELECT curriculum_state,replacement_lesson_id FROM learning_lessons WHERE program_id=? AND title=?",
    )
    .bind(&copy_id)
    .bind(&program.modules[0].lessons[0].title)
    .fetch_one(&pool)
    .await
    .expect("copied lesson curriculum state");
    assert_eq!(imported_lesson.0, "replaced");
    let imported_replacement_id = imported_lesson.1.expect("replacement lesson link");
    assert_ne!(imported_replacement_id, replacement_id);
    let replacement_title: String =
        sqlx::query_scalar("SELECT title FROM learning_lessons WHERE program_id=? AND id=?")
            .bind(&copy_id)
            .bind(imported_replacement_id)
            .fetch_one(&pool)
            .await
            .expect("replacement lesson points inside copied program");
    assert_eq!(replacement_title, program.modules[0].lessons[1].title);

    let imported_sources = sources.workspace_including_deleted(&copy_id).await?;
    let imported_source = imported_sources
        .sources
        .iter()
        .find(|source| {
            source
                .versions
                .iter()
                .any(|version| version.title == "Portability source")
        })
        .expect("tombstoned source was imported");
    assert_ne!(imported_source.id, source_id);
    assert!(imported_source.deleted_at.is_some());
    assert_eq!(
        imported_source.deletion_reason.as_deref(),
        Some("Removed from the active source list")
    );
    assert_eq!(imported_source.versions.len(), 2);
    assert!(imported_source
        .versions
        .iter()
        .any(|version| version.id != first_version_id));
    let imported_first_version = imported_source
        .versions
        .iter()
        .find(|version| version.version_number == 1)
        .expect("first immutable version");
    let imported_selector: (String, String, String, String, Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT id,source_id,source_version_id,status,start_byte,end_byte FROM learning_source_selectors WHERE program_id=?")
            .bind(&copy_id)
            .fetch_one(&pool)
            .await
            .expect("selector follows source snapshot");
    assert_ne!(imported_selector.0, selector_id);
    assert_eq!(imported_selector.1, imported_source.id);
    assert_eq!(imported_selector.2, imported_first_version.id);
    assert_eq!(imported_selector.3, "exact");
    assert_eq!(
        imported_selector.5.unwrap() - imported_selector.4.unwrap(),
        "quoted rule".len() as i64
    );
    assert_eq!(imported_selector.4, Some(quote_start as i64));
    let body = sources
        .get_version(&super::dto::GetLearningSourceVersionRequestDto {
            program_id: copy_id.clone(),
            source_id: imported_source.id.clone(),
            version_id: imported_first_version.id.clone(),
        })
        .await?;
    assert!(body.full_text.contains("quoted rule"));
    Ok(())
}

#[tokio::test]
async fn create_copy_round_trips_assessment_recall_curriculum_and_practical_history() -> Result<()>
{
    let pool = pool().await?;
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let program_id = &program.summary.id;
    let lesson_id = &program.modules[0].lessons[0].id;
    let module_id = &program.modules[0].id;
    let outcome_id = id();
    sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,?,?,?,?,0,10)")
        .bind(&outcome_id)
        .bind(program_id)
        .bind(module_id)
        .bind(lesson_id)
        .bind("Reason from evidence")
        .bind("Support a claim with evidence")
        .execute(&pool)
        .await
        .expect("outcome definition");

    let blueprint_id = id();
    sqlx::query("INSERT INTO learning_assessment_blueprints(id,program_id,revision,predecessor_revision,purpose,title,instructions,expected_minutes,allowed_aids_json,passing_score,rubric_json,feedback_timing,source_version_ids_json,status,change_reason,created_at) VALUES(?,?,1,NULL,'checkpoint','Evidence check','Answer from the sources',15,'[]',0.7,'[]','after_submission','[]','accepted','initial',11)")
        .bind(&blueprint_id)
        .bind(program_id)
        .execute(&pool)
        .await
        .expect("assessment blueprint");
    let candidate_id = id();
    sqlx::query("INSERT INTO learning_assessment_candidates(id,program_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,source_version_ids_json,authored_by,author_model,content_sha256,created_at) VALUES(?,?,?,'multiple_choice',2,'Which claim is supported?','[\"A\",\"B\"]',NULL,'[]','[]','person',NULL,'candidate-content-hash',12)")
        .bind(&candidate_id)
        .bind(program_id)
        .bind(&outcome_id)
        .execute(&pool)
        .await
        .expect("assessment candidate");
    sqlx::query(
        "INSERT INTO learning_assessment_candidate_outcomes(candidate_id,outcome_id) VALUES(?,?)",
    )
    .bind(&candidate_id)
    .bind(&outcome_id)
    .execute(&pool)
    .await
    .expect("candidate outcome");
    sqlx::query("INSERT INTO learning_assessment_answer_keys(candidate_id,selected_index,accepted_answers_json,ordered_values_json,explanation) VALUES(?,1,'[]','[]','The second claim follows from the passage.')")
        .bind(&candidate_id)
        .execute(&pool)
        .await
        .expect("private assessment key");
    sqlx::query("INSERT INTO learning_assessment_blueprint_slots(blueprint_id,blueprint_revision,ordinal,outcome_id,format,difficulty,difficulty_max,points) VALUES(?,1,0,?,'multiple_choice',2,3,2.0)")
        .bind(&blueprint_id)
        .bind(&outcome_id)
        .execute(&pool)
        .await
        .expect("blueprint slot");
    sqlx::query("INSERT INTO learning_assessment_blueprint_candidates(program_id,blueprint_id,blueprint_revision,candidate_id) VALUES(?,?,1,?)")
        .bind(program_id)
        .bind(&blueprint_id)
        .bind(&candidate_id)
        .execute(&pool)
        .await
        .expect("blueprint candidate");

    let form_id = id();
    sqlx::query("INSERT INTO learning_assessment_forms(id,program_id,blueprint_id,blueprint_revision,retake_of_form_id,status,revision,title,instructions,expected_minutes,allowed_aids_json,purpose,passing_score,rubric_json,feedback_timing,source_version_ids_json,model_name,created_at,updated_at,submitted_at) VALUES(?,?,?,1,NULL,'submitted',2,'Evidence check form','Answer each item',15,'[]','checkpoint',0.7,'[]','after_submission','[]',NULL,13,14,15)")
        .bind(&form_id)
        .bind(program_id)
        .bind(&blueprint_id)
        .execute(&pool)
        .await
        .expect("assessment form");
    sqlx::query("INSERT INTO learning_assessment_form_items(form_id,ordinal,candidate_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,points,previously_exposed) VALUES(?,0,?,?,'multiple_choice',2,'Which claim is supported?','[\"A\",\"B\"]',NULL,'[]',2.0,0)")
        .bind(&form_id)
        .bind(&candidate_id)
        .bind(&outcome_id)
        .execute(&pool)
        .await
        .expect("form item");
    sqlx::query("INSERT INTO learning_assessment_form_item_outcomes(form_id,item_ordinal,outcome_id) VALUES(?,0,?)")
        .bind(&form_id)
        .bind(&outcome_id)
        .execute(&pool)
        .await
        .expect("form item outcome");
    let response_operation_id = id();
    sqlx::query("INSERT INTO learning_assessment_response_revisions(form_id,item_ordinal,revision,operation_id,payload_hash,selected_index,text_response,ordered_values_json,artifact_json,assistance_json,created_at) VALUES(?,0,1,?,'response-hash',1,NULL,'[]',NULL,'[]',16)")
        .bind(&form_id)
        .bind(&response_operation_id)
        .execute(&pool)
        .await
        .expect("response history");
    let submission_operation_id = id();
    sqlx::query("INSERT INTO learning_assessment_submissions(form_id,program_id,operation_id,payload_hash,score,passed,grade_status,grader_model,grader_disagreement_json,feedback,submitted_at) VALUES(?,?,?,'submission-hash',0.8,1,'deterministic',NULL,'[]','Good evidence',17)")
        .bind(&form_id)
        .bind(program_id)
        .bind(&submission_operation_id)
        .execute(&pool)
        .await
        .expect("assessment submission");
    sqlx::query("INSERT INTO learning_assessment_item_results(form_id,item_ordinal,score,correct,grade_status,feedback,criterion_results_json,artifact_quotes_json) VALUES(?,0,0.8,1,'deterministic','Supported','[]','[]')")
        .bind(&form_id)
        .execute(&pool)
        .await
        .expect("assessment result");

    // Two accepted curriculum snapshots establish a predecessor chain.
    let prior_revision_id = id();
    let accepted_revision_id = id();
    sqlx::query("INSERT INTO learning_curriculum_revisions(id,program_id,revision,predecessor_id,status,reason,snapshot_json,snapshot_sha256,required_lesson_count,resume_lesson_id,created_at,accepted_at) VALUES(?,?,1,NULL,'superseded','Initial plan','{\"revision\":1}','snapshot-hash-one',4,?,18,18)")
        .bind(&prior_revision_id)
        .bind(program_id)
        .bind(lesson_id)
        .execute(&pool)
        .await
        .expect("prior curriculum snapshot");
    sqlx::query("INSERT INTO learning_curriculum_revisions(id,program_id,revision,predecessor_id,status,reason,snapshot_json,snapshot_sha256,required_lesson_count,resume_lesson_id,created_at,accepted_at) VALUES(?,?,2,?,'accepted','Refined plan','{\"revision\":2}','snapshot-hash-two',4,?,19,19)")
        .bind(&accepted_revision_id)
        .bind(program_id)
        .bind(&prior_revision_id)
        .bind(lesson_id)
        .execute(&pool)
        .await
        .expect("accepted curriculum snapshot");
    sqlx::query("INSERT INTO learning_curriculum_changes(revision_id,ordinal,operation,lesson_id,before_json,after_json,explanation) VALUES(?,0,'replace',?,'{\"title\":\"Old\"}','{\"title\":\"New\"}','Replace an outdated lesson')")
        .bind(&accepted_revision_id)
        .bind(lesson_id)
        .execute(&pool)
        .await
        .expect("curriculum change");
    let curriculum_operation_id = id();
    sqlx::query("INSERT INTO learning_curriculum_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'accept_revision','curriculum-op-hash',?,20)")
        .bind(&curriculum_operation_id)
        .bind(program_id)
        .bind(&accepted_revision_id)
        .execute(&pool)
        .await
        .expect("curriculum operation receipt");

    // Canonical Study review data and the richer recall profile/version chain.
    let deck_id = id();
    let card_id = id();
    let review_id = id();
    sqlx::query("INSERT INTO study_decks(id,title,focus,study_goal,model_name,created_at) VALUES(?,'Evidence recall','Reasoning','Review claims','test-model',21)")
        .bind(&deck_id)
        .execute(&pool)
        .await
        .expect("study deck");
    sqlx::query("INSERT INTO study_cards(id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at,interval_days,review_count,lapses,format,scheduler_version) VALUES(?,?, 'What supports a claim?','Evidence','[]',0,'Use a source','{}','reasoning',100,4,2,1,'question_answer','fsrs_6_v1')")
        .bind(&card_id)
        .bind(&deck_id)
        .execute(&pool)
        .await
        .expect("canonical recall card");
    sqlx::query("INSERT INTO study_reviews(id,card_id,reviewed_at,rating,mode,correct,selected_option,scheduler_version) VALUES(?,?,22,'good','flashcard',1,NULL,'fsrs_6_v1')")
        .bind(&review_id)
        .bind(&card_id)
        .execute(&pool)
        .await
        .expect("canonical recall review");
    sqlx::query(
        "INSERT INTO learning_memory(program_id,journal_id,deck_id,created_at) VALUES(?,NULL,?,21)",
    )
    .bind(program_id)
    .bind(&deck_id)
    .execute(&pool)
    .await
    .expect("program memory link");
    sqlx::query("INSERT INTO learning_recall_card_profiles(card_id,program_id,format,prompt_json,answer_json,source_version_ids_json,scheduler_version,scheduler_state_json,content_revision,created_at,updated_at) VALUES(?,?,'question_answer','{\"prompt\":\"What supports a claim?\"}','{\"answer\":\"Evidence\"}','[]','fsrs_6_v1','{\"stability\":4.5}',2,21,23)")
        .bind(&card_id)
        .bind(program_id)
        .execute(&pool)
        .await
        .expect("recall profile");
    sqlx::query("INSERT INTO learning_recall_card_versions(card_id,revision,format,prompt_json,answer_json,source_version_ids_json,change_reason,created_at) VALUES(?,1,'question_answer','{\"prompt\":\"Earlier prompt\"}','{\"answer\":\"Earlier answer\"}','[]','Created',21),(?,2,'question_answer','{\"prompt\":\"What supports a claim?\"}','{\"answer\":\"Evidence\"}','[]','Clarified',23)")
        .bind(&card_id)
        .bind(&card_id)
        .execute(&pool)
        .await
        .expect("recall content versions");
    sqlx::query("INSERT INTO learning_recall_scheduler_transitions(id,card_id,review_id,scheduler_version,prior_state_json,rating,next_state_json,due_at,created_at) VALUES(?,?,?,'fsrs_6_v1','{\"stability\":2}','good','{\"stability\":4.5}',100,22)")
        .bind(id())
        .bind(&card_id)
        .bind(&review_id)
        .execute(&pool)
        .await
        .expect("scheduler transition");

    // A two-revision practical activity chain exercises historical ordering and
    // predecessor remapping through the dedicated practical snapshot.
    let older_activity_id = id();
    let current_activity_id = id();
    sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at) VALUES(?,?,?,NULL,'project','Earlier project','Build a first version','retired','practice','[]','[]','[]','[]','none',NULL,NULL,NULL,NULL,NULL,'test-model',0,24,24)")
        .bind(&older_activity_id)
        .bind(program_id)
        .bind(lesson_id)
        .execute(&pool)
        .await
        .expect("older practical activity");
    sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at) VALUES(?,?,?,?,'project','Current project','Build a refined version','ready','demonstrate','[]','[]','[]','[]','none',NULL,NULL,NULL,NULL,NULL,'test-model',1,25,25)")
        .bind(&current_activity_id)
        .bind(program_id)
        .bind(lesson_id)
        .bind(&older_activity_id)
        .execute(&pool)
        .await
        .expect("current practical activity");
    let older_activity_op = id();
    let current_activity_op = id();
    sqlx::query("INSERT INTO learning_practical_activity_operations(operation_id,program_id,activity_id,kind,payload_hash,result_revision,created_at) VALUES(?,?,?,'generate','activity-op-one',0,24),(?,?,?,'revise','activity-op-two',1,25)")
        .bind(&older_activity_op)
        .bind(program_id)
        .bind(&older_activity_id)
        .bind(&current_activity_op)
        .bind(program_id)
        .bind(&current_activity_id)
        .execute(&pool)
        .await
        .expect("practical operation history");
    let run_id = id();
    let run_operation_id = id();
    let frozen_activity = serde_json::json!({
        "id": older_activity_id,
        "programId": program_id,
        "lessonId": lesson_id,
        "revision": 0,
        "brief": "The original project brief used for this run."
    })
    .to_string();
    sqlx::query("INSERT INTO learning_practical_runs(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,engine,image_id,status,learner_files_json,stdout,stderr,output_truncated,exit_code,duration_ms,check_results_json,created_at,completed_at) VALUES(?,?,?,?,?,NULL,?,?,'docker','sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','passed','[]','Build passed','',0,0,120,'[{\"name\":\"project check\",\"status\":\"passed\",\"message\":\"All assertions passed\",\"durationMs\":120}]',26,27)")
        .bind(&run_id)
        .bind(program_id)
        .bind(&older_activity_id)
        .bind(0_i64)
        .bind(&frozen_activity)
        .bind(&run_operation_id)
        .bind("practical-run-payload")
        .execute(&pool)
        .await
        .expect("historical practical run");
    sqlx::query("INSERT INTO learning_practical_run_operations(operation_id,program_id,run_id,kind,payload_hash,created_at) VALUES(?,?,?,'start','practical-run-payload',26)")
        .bind(&run_operation_id)
        .bind(program_id)
        .bind(&run_id)
        .execute(&pool)
        .await
        .expect("practical run receipt");
    sqlx::query("INSERT INTO learning_practical_run_checks(run_id,ordinal,name,status,message,duration_ms) VALUES(?,0,'project check','passed','All assertions passed',120)")
        .bind(&run_id)
        .execute(&pool)
        .await
        .expect("practical run check");

    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let (copy_id, _) = export_copy(&packs, &container, program_id).await?;

    let imported_form: String =
        sqlx::query_scalar("SELECT f.id FROM learning_assessment_forms f WHERE f.program_id=?")
            .bind(&copy_id)
            .fetch_one(&pool)
            .await
            .expect("assessment form restored");
    assert_ne!(imported_form, form_id);
    let assessment = sqlx::query("SELECT f.title,f.status,i.prompt,r.selected_index,r.operation_id,s.score,s.passed,k.selected_index AS key_index,ir.feedback FROM learning_assessment_forms f JOIN learning_assessment_form_items i ON i.form_id=f.id JOIN learning_assessment_response_revisions r ON r.form_id=f.id JOIN learning_assessment_submissions s ON s.form_id=f.id JOIN learning_assessment_item_results ir ON ir.form_id=f.id AND ir.item_ordinal=i.ordinal JOIN learning_assessment_form_items fi ON fi.form_id=f.id JOIN learning_assessment_candidates c ON c.id=fi.candidate_id JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE f.program_id=?")
        .bind(&copy_id)
        .fetch_one(&pool)
        .await
        .expect("assessment keys, response and results restored");
    use sqlx::Row;
    assert_eq!(assessment.get::<String, _>("title"), "Evidence check form");
    assert_eq!(assessment.get::<String, _>("status"), "submitted");
    assert_eq!(
        assessment.get::<String, _>("prompt"),
        "Which claim is supported?"
    );
    assert_eq!(assessment.get::<i64, _>("selected_index"), 1);
    assert_ne!(
        assessment.get::<String, _>("operation_id"),
        response_operation_id
    );
    assert_eq!(assessment.get::<f64, _>("score"), 0.8);
    assert_eq!(assessment.get::<i64, _>("passed"), 1);
    assert_eq!(assessment.get::<i64, _>("key_index"), 1);
    assert_eq!(assessment.get::<String, _>("feedback"), "Supported");

    let curriculum: (i64, String, String, String) = sqlx::query_as("SELECT current.revision,current.status,current.predecessor_id,prior.id FROM learning_curriculum_revisions current JOIN learning_curriculum_revisions prior ON prior.program_id=current.program_id AND prior.revision=1 WHERE current.program_id=? AND current.revision=2")
        .bind(&copy_id)
        .fetch_one(&pool)
        .await
        .expect("curriculum predecessor chain restored");
    assert_eq!(curriculum.0, 2);
    assert_eq!(curriculum.1, "accepted");
    assert_ne!(curriculum.2, prior_revision_id);
    assert_eq!(curriculum.2, curriculum.3);
    let copied_operation_id: String = sqlx::query_scalar(
        "SELECT operation_id FROM learning_curriculum_operations WHERE program_id=?",
    )
    .bind(&copy_id)
    .fetch_one(&pool)
    .await
    .expect("curriculum operation receipt copied");
    assert_ne!(copied_operation_id, curriculum_operation_id);
    let change_lesson: String = sqlx::query_scalar("SELECT c.lesson_id FROM learning_curriculum_changes c JOIN learning_curriculum_revisions r ON r.id=c.revision_id WHERE r.program_id=?")
        .bind(&copy_id)
        .fetch_one(&pool)
        .await
        .expect("curriculum change lesson reference");
    assert_ne!(change_lesson, *lesson_id);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM learning_lessons WHERE program_id=? AND id=?"
        )
        .bind(&copy_id)
        .bind(change_lesson)
        .fetch_one(&pool)
        .await?,
        1
    );

    let recall: (String, String, String, i64, String, String) = sqlx::query_as("SELECT c.id,c.scheduler_version,r.scheduler_version,p.content_revision,v.change_reason,t.rating FROM learning_memory m JOIN study_cards c ON c.deck_id=m.deck_id JOIN study_reviews r ON r.card_id=c.id JOIN learning_recall_card_profiles p ON p.card_id=c.id JOIN learning_recall_card_versions v ON v.card_id=c.id AND v.revision=2 JOIN learning_recall_scheduler_transitions t ON t.card_id=c.id AND t.review_id=r.id WHERE m.program_id=?")
        .bind(&copy_id)
        .fetch_one(&pool)
        .await
        .expect("recall card, review, versions and scheduler transition restored");
    assert_ne!(recall.0, card_id);
    assert_eq!(recall.1, "fsrs_6_v1");
    assert_eq!(recall.2, "fsrs_6_v1");
    assert_eq!(recall.3, 2);
    assert_eq!(recall.4, "Clarified");
    assert_eq!(recall.5, "good");
    let copied_review_id: String = sqlx::query_scalar("SELECT r.id FROM study_reviews r JOIN study_cards c ON c.id=r.card_id JOIN learning_memory m ON m.deck_id=c.deck_id WHERE m.program_id=?")
        .bind(&copy_id)
        .fetch_one(&pool)
        .await
        .expect("copied review identifier");
    assert_ne!(copied_review_id, review_id);

    let activities = super::practical_repository::LearningPracticalRepository::new(pool.clone())
        .workspace(&copy_id)
        .await?
        .with_capabilities(Vec::new(), Vec::new());
    let earlier = activities
        .activities
        .iter()
        .find(|activity| activity.title == "Earlier project")
        .expect("earlier practical revision");
    let current = activities
        .activities
        .iter()
        .find(|activity| activity.title == "Current project")
        .expect("current practical revision");
    assert_ne!(earlier.id, older_activity_id);
    assert_ne!(current.id, current_activity_id);
    assert_eq!(current.predecessor_id.as_deref(), Some(earlier.id.as_str()));
    let copied_activity_operations: Vec<String> = sqlx::query_scalar(
        "SELECT operation_id FROM learning_practical_activity_operations WHERE program_id=? ORDER BY created_at",
    )
    .bind(&copy_id)
    .fetch_all(&pool)
    .await
    .expect("practical operation receipts copied");
    assert_eq!(copied_activity_operations.len(), 2);
    assert!(copied_activity_operations
        .iter()
        .all(|operation| operation != &older_activity_op && operation != &current_activity_op));
    let copied_run = activities
        .runs
        .first()
        .expect("historical practical run copied");
    assert_ne!(copied_run.id, run_id);
    assert_eq!(
        copied_run.status,
        super::practical_dto::LearningPracticalRunStatus::Passed
    );
    assert_eq!(copied_run.checks.len(), 1);
    assert_eq!(copied_run.checks[0].message, "All assertions passed");
    let copied_run_snapshot: (String, String, String, String) = sqlx::query_as(
        "SELECT id,activity_id,operation_id,activity_snapshot_json FROM learning_practical_runs WHERE program_id=?",
    )
    .bind(&copy_id)
    .fetch_one(&pool)
    .await
    .expect("immutable run snapshot copied");
    assert_ne!(copied_run_snapshot.1, older_activity_id);
    assert_ne!(copied_run_snapshot.2, run_operation_id);
    let frozen_snapshot: serde_json::Value =
        serde_json::from_str(&copied_run_snapshot.3).expect("stored activity snapshot JSON");
    assert_eq!(
        frozen_snapshot["brief"],
        "The original project brief used for this run."
    );
    // Make sure the imported candidate and operation receipt also have fresh IDs.
    let copied_candidate_id: String =
        sqlx::query_scalar("SELECT id FROM learning_assessment_candidates WHERE program_id=?")
            .bind(&copy_id)
            .fetch_one(&pool)
            .await
            .expect("copied candidate");
    assert_ne!(copied_candidate_id, candidate_id);
    assert_ne!(
        sqlx::query_scalar::<_, String>(
            "SELECT operation_id FROM learning_assessment_submissions WHERE program_id=?"
        )
        .bind(&copy_id)
        .fetch_one(&pool)
        .await?,
        submission_operation_id
    );
    Ok(())
}

#[tokio::test]
async fn cancelled_preview_and_blocked_replace_do_not_write_program_rows() -> Result<()> {
    let pool = pool().await?;
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    let activity_id = id();
    sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at) VALUES(?,?,?,NULL,'project','Protected project','A project with a private evaluator','ready','practice','[]','[]','[]','[]','none',NULL,NULL,NULL,NULL,NULL,'test-model',0,1,1)")
        .bind(&activity_id)
        .bind(&program.summary.id)
        .bind(lesson_id)
        .execute(&pool)
        .await
        .expect("practical activity inserts");
    sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,0,'checks/test.sh','check','assertion','private-check-hash',0)")
        .bind(&activity_id)
        .execute(&pool)
        .await
        .expect("hidden practical evaluator inserts");

    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let exported = packs
        .export(
            &container,
            &ExportLearningPackRequestDto {
                operation_id: id(),
                program_id: program.summary.id.clone(),
                file_name: format!("preflight-{}.lattice-learning", id()),
                include_evidence: false,
                include_practical_artifacts: false,
                include_source_bodies: false,
                source_body_redistribution_confirmed: false,
            },
        )
        .await?;
    let source_path = exported.exports[0].destination_path.clone();
    let source_program_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learning_programs")
        .fetch_one(&pool)
        .await?;
    let source_hidden_file_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learning_practical_files f JOIN learning_practical_activities a ON a.id=f.activity_id WHERE a.program_id=? AND f.role IN ('check','solution')")
        .bind(&program.summary.id)
        .fetch_one(&pool)
        .await?;

    let blocked = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path: source_path.clone(),
            conflict_policy: LearningPackConflictPolicy::ReplaceAfterBackup,
        })
        .await?;
    let blocked_preview = blocked.import_previews.first().expect("replace preview");
    assert!(!blocked_preview.can_apply);
    assert!(blocked_preview
        .conflicts
        .iter()
        .any(|conflict| conflict.resolution == "blocked"
            && conflict
                .existing_title
                .contains("hidden practical evaluators")));
    let title_after_block: String =
        sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(title_after_block, program.summary.title);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs")
            .fetch_one(&pool)
            .await?,
        source_program_count
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_practical_files f JOIN learning_practical_activities a ON a.id=f.activity_id WHERE a.program_id=? AND f.role IN ('check','solution')")
        .bind(&program.summary.id)
        .fetch_one(&pool)
        .await?,
        source_hidden_file_count
    );

    let cancelled = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path,
            conflict_policy: LearningPackConflictPolicy::CreateCopy,
        })
        .await?;
    let pending = cancelled.import_previews.first().expect("copy preview");
    let pending_id = pending.id.clone();
    let cancelled = packs
        .cancel(&CancelLearningPackImportPreviewRequestDto {
            operation_id: id(),
            preview_id: pending_id.clone(),
        })
        .await?;
    assert_eq!(
        cancelled
            .import_previews
            .iter()
            .find(|preview| preview.id == pending_id)
            .map(|preview| preview.status),
        Some(super::portability_dto::LearningPackPreviewStatus::Cancelled)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs")
            .fetch_one(&pool)
            .await?,
        source_program_count
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_practical_files f JOIN learning_practical_activities a ON a.id=f.activity_id WHERE a.program_id=? AND f.role IN ('check','solution')")
        .bind(&program.summary.id)
        .fetch_one(&pool)
        .await?,
        source_hidden_file_count
    );
    Ok(())
}

#[tokio::test]
async fn successful_replace_returns_a_readable_recovery_backup() -> Result<()> {
    let pool = pool().await?;
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let exported = packs
        .export(
            &container,
            &ExportLearningPackRequestDto {
                operation_id: id(),
                program_id: program.summary.id.clone(),
                file_name: format!("replace-source-{}.lattice-learning", id()),
                include_evidence: false,
                include_practical_artifacts: false,
                include_source_bodies: false,
                source_body_redistribution_confirmed: false,
            },
        )
        .await?;
    let preview = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path: exported.exports[0].destination_path.clone(),
            conflict_policy: LearningPackConflictPolicy::ReplaceAfterBackup,
        })
        .await?;
    let preview = preview.import_previews.first().expect("replace preview");
    assert!(preview.can_apply);
    let preview_id = preview.id.clone();
    let applied = packs
        .apply(
            &container,
            &ApplyLearningPackImportRequestDto {
                operation_id: id(),
                preview_id: preview_id.clone(),
                expected_root_sha256: preview.manifest.root_sha256.clone(),
            },
        )
        .await?;
    let import = applied
        .imports
        .iter()
        .find(|item| item.preview_id == preview_id)
        .expect("replacement import result");
    let backup_id = import
        .backup_id
        .as_ref()
        .expect("replacement returns a recovery backup ID");
    assert!(applied.exports.iter().any(|export| {
        export.id.as_str() == backup_id.as_str()
            && export
                .destination_path
                .ends_with(&format!("backup-{backup_id}.lattice-learning"))
    }));
    let backup_path = container
        .core
        .data_dir()
        .join("learning-packs")
        .join(format!("backup-{backup_id}.lattice-learning"));
    let backup_bytes = tokio::fs::read(&backup_path)
        .await
        .expect("reported recovery backup is present");
    let decoded = super::pack::decode_learning_pack(&backup_bytes)
        .expect("reported recovery backup is a valid learning pack");
    assert_eq!(decoded.manifest.title, program.summary.title);
    let backed_up_program: serde_json::Value = serde_json::from_slice(
        decoded
            .entries
            .get("program/program.json")
            .expect("backup contains its program snapshot"),
    )
    .expect("backup program snapshot parses");
    assert_eq!(
        backed_up_program["summary"]["id"],
        program.summary.id.as_str()
    );
    let restored_title: String =
        sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .expect("replacement program remains available");
    assert_eq!(restored_title, program.summary.title);
    Ok(())
}
