use super::*;
use crate::features::learning::{
    content_verification::{digest, tests::attest},
    dto::*,
    lesson_evidence,
    outline_draft::{LearningOutlineReviewStatus, OutlineDraft},
    outline_evidence_view,
    pack::{decode_learning_pack, encode_learning_pack, LearningPackEntry, LearningPackInput},
};
use serde_json::{json, Value};

const PROVENANCE: &str = "program/provenance.json";
const QUOTE: &str =
    "A saved citation identifies exactly the immutable passage used to support the lesson claim.";

async fn ready_course(pool: &sqlx::SqlitePool) -> Result<LearningProgramDto> {
    let repository = LearningRepository::new(pool.clone());
    let mut program = fixture();
    program.summary.status = LearningProgramStatus::Active;
    program.sources[0].excerpt = QUOTE.into();
    let request = GenerateLearningProgramRequestDto {
        goal: program.summary.goal.clone(),
        prior_knowledge: program.prior_knowledge.clone(),
        minutes_per_session: 30,
        document_ids: vec![],
        source_urls: vec![],
        course_depth: None,
    };
    let candidate = json!({"modules":program.modules.iter().map(|module| json!({
        "title":module.title,
        "summary":{"text":module.summary,"sourceIndex":0,"quote":QUOTE},
        "outcomes":module.outcomes.iter().map(|text| json!({"text":text,"sourceIndex":0,"quote":QUOTE})).collect::<Vec<_>>(),
        "lessons":module.lessons.iter().map(|lesson| json!({"title":lesson.title,"objective":lesson.objective,"sourceIndex":0,"quote":QUOTE})).collect::<Vec<_>>()
    })).collect::<Vec<_>>()});
    let mut draft = OutlineDraft::new(request, candidate, &program.sources);
    draft.review.status = LearningOutlineReviewStatus::Passed;
    repository
        .create_outline_draft(&program, &[], Some(&draft))
        .await?;
    let lesson = &program.modules[0].lessons[0];
    let question_id = id();
    let mut prepared = PreparedLearningLesson {
        blocks: vec![LearningBlockDto {
            kind: LearningBlockKind::Explanation,
            title: "Traceable teaching".into(),
            body: QUOTE.into(),
            source_ids: vec![program.sources[0].id.clone()],
            rubric: vec![],
        }],
        questions: vec![LearningQuestionDto {
            id: question_id.clone(),
            kind: LearningAssessmentKind::Quiz,
            prompt: "PRIVATE assessment finding".into(),
            options: vec!["First".into(), "Second".into()],
            source_ids: vec![program.sources[0].id.clone()],
        }],
        keys: vec![LearningAnswerKey {
            question_id,
            correct_index: 0,
            explanation: "PRIVATE answer explanation".into(),
        }],
        verification: None,
    };
    let mut report = serde_json::to_value(attest(&lesson.id, &prepared))?;
    report["sources"] = json!([{"id":program.sources[0].id,"sha256":digest(QUOTE)}]);
    report["findings"][0]["evidence"] = json!([{
        "source_id":program.sources[0].id,"text":QUOTE,"start_byte":0,"end_byte":QUOTE.len(),"retrieval_kind":"lexical","score":1.0
    }]);
    prepared.verification = Some(serde_json::from_value(report)?);
    repository
        .prepare(&program.summary.id, &lesson.id, 0, &prepared)
        .await?;
    repository.get(&program.summary.id).await
}

async fn export_without_activity(
    packs: &LearningPackRepository,
    container: &crate::interfaces::di::Container,
    program: &LearningProgramDto,
) -> Result<String> {
    let workspace = packs
        .export(
            container,
            &ExportLearningPackRequestDto {
                operation_id: id(),
                program_id: program.summary.id.clone(),
                file_name: format!("provenance-{}.lattice-learning", id()),
                include_evidence: false,
                include_practical_artifacts: false,
                include_source_bodies: false,
                source_body_redistribution_confirmed: false,
            },
        )
        .await?;
    Ok(workspace.exports[0].destination_path.clone())
}

async fn apply_path(
    packs: &LearningPackRepository,
    container: &crate::interfaces::di::Container,
    path: String,
    policy: LearningPackConflictPolicy,
) -> Result<crate::features::learning::portability_dto::LearningPortabilityWorkspaceDto> {
    let workspace = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path: path,
            conflict_policy: policy,
        })
        .await?;
    let preview = &workspace.import_previews[0];
    assert!(preview.can_apply);
    packs
        .apply(
            container,
            &ApplyLearningPackImportRequestDto {
                operation_id: id(),
                preview_id: preview.id.clone(),
                expected_root_sha256: preview.manifest.root_sha256.clone(),
            },
        )
        .await
}

#[tokio::test]
async fn copied_course_retains_citations_bound_to_copied_ids_without_exposing_answers() -> Result<()>
{
    let pool = pool().await?;
    let original = ready_course(&pool).await?;
    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let (copy_id, _) = export_copy(&packs, &container, &original.summary.id).await?;
    assert_ne!(copy_id, original.summary.id);
    let copy = LearningRepository::new(pool.clone()).get(&copy_id).await?;
    let copied_lesson = &copy.modules[0].lessons[0];
    let evidence = lesson_evidence::get(&pool, &copy_id, &copied_lesson.id)
        .await?
        .unwrap();
    let before = lesson_evidence::get(
        &pool,
        &original.summary.id,
        &original.modules[0].lessons[0].id,
    )
    .await?
    .unwrap();
    assert_eq!(
        evidence.content_sha256, before.content_sha256,
        "import must not mint a new verification fingerprint"
    );
    assert_eq!(evidence.claim_count, 2);
    assert_eq!(evidence.teaching_claims.len(), 1);
    assert!(!serde_json::to_string(&evidence)?.contains("PRIVATE"));
    assert!(evidence.sources_current);
    let passage = &evidence.teaching_claims[0].passages[0];
    assert_eq!(passage.text, QUOTE);
    assert_eq!(passage.start_byte, 0);
    assert_eq!(passage.end_byte, QUOTE.len());
    assert_eq!(
        passage.source_version_id,
        copied_lesson.blocks[0].source_ids[0]
    );
    assert_ne!(passage.source_version_id, original.sources[0].id);
    let outline = outline_evidence_view::get(&pool, &copy_id).await?.unwrap();
    assert!(!outline.citations.is_empty());
    assert_eq!(outline.unavailable_count, 0);
    assert!(outline.citations.iter().all(|citation| copy
        .modules
        .iter()
        .any(|module| module.id == citation.module_id)));
    assert!(outline
        .citations
        .iter()
        .all(|citation| citation.source_id == passage.source_version_id));
    let checkpoint: String = sqlx::query_scalar(
        "SELECT state_json FROM learning_outline_checkpoints WHERE program_id=?",
    )
    .bind(&copy_id)
    .fetch_one(&pool)
    .await?;
    assert!(checkpoint.contains(&copy.modules[0].id));
    assert!(!checkpoint.contains(&original.modules[0].id));
    let private_report: String = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=?",
    )
    .bind(&copy_id)
    .fetch_one(&pool)
    .await?;
    assert!(private_report.contains("PRIVATE assessment finding"));
    Ok(())
}

#[tokio::test]
async fn replacing_course_backs_up_current_provenance_and_restores_exported_provenance(
) -> Result<()> {
    let pool = pool().await?;
    let original = ready_course(&pool).await?;
    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let path = export_without_activity(&packs, &container, &original).await?;
    let decoded = decode_learning_pack(&tokio::fs::read(&path).await?)?;
    assert!(
        decoded.entries.contains_key(PROVENANCE),
        "provenance is course data, not optional learner activity"
    );
    assert!(!decoded.manifest.privacy.includes_learner_evidence);
    sqlx::query("UPDATE learning_lesson_verifications SET report_json=json_set(report_json,'$.findings[0].reason','Current-only receipt') WHERE program_id=?")
        .bind(&original.summary.id).execute(&pool).await?;
    let applied = apply_path(
        &packs,
        &container,
        path,
        LearningPackConflictPolicy::ReplaceAfterBackup,
    )
    .await?;
    let backup_id = applied.imports[0].backup_id.as_ref().unwrap();
    let backup = applied
        .exports
        .iter()
        .find(|export| &export.id == backup_id)
        .unwrap();
    let backup_pack = decode_learning_pack(&tokio::fs::read(&backup.destination_path).await?)?;
    assert!(std::str::from_utf8(&backup_pack.entries[PROVENANCE])
        .unwrap()
        .contains("Current-only receipt"));
    let restored = lesson_evidence::get(
        &pool,
        &original.summary.id,
        &original.modules[0].lessons[0].id,
    )
    .await?
    .unwrap();
    assert_eq!(restored.teaching_claims[0].reason, "Persistence fixture");
    let recovery = apply_path(
        &packs,
        &container,
        backup.destination_path.clone(),
        LearningPackConflictPolicy::CreateCopy,
    )
    .await?;
    let recovered = LearningRepository::new(pool.clone())
        .get(&recovery.program_id)
        .await?;
    let evidence = lesson_evidence::get(
        &pool,
        &recovery.program_id,
        &recovered.modules[0].lessons[0].id,
    )
    .await?
    .unwrap();
    assert_eq!(evidence.teaching_claims[0].reason, "Current-only receipt");
    Ok(())
}

async fn variant(path: &str, edit: impl FnOnce(&mut LearningPackInput)) -> Result<String> {
    let decoded = decode_learning_pack(&tokio::fs::read(path).await?)?;
    let mut input = LearningPackInput {
        pack_id: id(),
        title: decoded.manifest.title,
        created_at: decoded.manifest.created_at,
        application_version: decoded.manifest.application_version,
        privacy: decoded.manifest.privacy,
        entries: decoded
            .manifest
            .entries
            .into_iter()
            .map(|entry| LearningPackEntry {
                bytes: decoded.entries[&entry.path].clone(),
                path: entry.path,
                kind: entry.kind,
            })
            .collect(),
    };
    edit(&mut input);
    let target = format!("{path}.{}.lattice-learning", id());
    tokio::fs::write(&target, encode_learning_pack(input)?).await?;
    Ok(target)
}

#[tokio::test]
async fn older_packs_still_import_and_explain_missing_verification_history() -> Result<()> {
    let pool = pool().await?;
    let original = ready_course(&pool).await?;
    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let path = export_without_activity(&packs, &container, &original).await?;
    let legacy = variant(&path, |input| {
        input.entries.retain(|entry| entry.path != PROVENANCE)
    })
    .await?;
    // Re-create an actual v1 archive, not a malformed v2 pack without its
    // required provenance entry. The content checksums are unchanged.
    let mut decoded = decode_learning_pack(&tokio::fs::read(&legacy).await?)?;
    decoded.manifest.version = 1;
    let encoder = zstd::stream::write::Encoder::new(Vec::new(), 1)?;
    let mut archive = tar::Builder::new(encoder);
    decoded.entries.insert(
        "manifest.json".into(),
        serde_json::to_vec(&decoded.manifest)?,
    );
    for (path, bytes) in decoded.entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive.append_data(&mut header, path, std::io::Cursor::new(bytes))?;
    }
    tokio::fs::write(&legacy, archive.into_inner()?.finish()?).await?;
    let preview = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path: legacy.clone(),
            conflict_policy: LearningPackConflictPolicy::CreateCopy,
        })
        .await?;
    assert!(preview.import_previews[0]
        .warnings
        .iter()
        .any(|warning| warning.contains("no saved outline citations")));
    let applied = apply_path(
        &packs,
        &container,
        legacy,
        LearningPackConflictPolicy::CreateCopy,
    )
    .await?;
    let copy = LearningRepository::new(pool.clone())
        .get(&applied.program_id)
        .await?;
    assert!(
        lesson_evidence::get(&pool, &applied.program_id, &copy.modules[0].lessons[0].id)
            .await?
            .is_none()
    );
    assert!(outline_evidence_view::get(&pool, &applied.program_id)
        .await?
        .is_none());
    Ok(())
}

#[tokio::test]
async fn malformed_or_future_provenance_is_rejected_before_an_import_writes() -> Result<()> {
    let pool = pool().await?;
    let original = ready_course(&pool).await?;
    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let path = export_without_activity(&packs, &container, &original).await?;
    for case in 0..6 {
        let changed = variant(&path, |input| {
            if case == 5 {
                input.entries.retain(|entry| entry.path != PROVENANCE);
                return;
            }
            let entry = input
                .entries
                .iter_mut()
                .find(|entry| entry.path == PROVENANCE)
                .unwrap();
            let mut value: Value = serde_json::from_slice(&entry.bytes).unwrap();
            match case {
                0 => value["version"] = json!(99),
                1 => value["lessons"][0]["lessonId"] = json!(id()),
                2 => value["lessons"][0]["report"]["lesson_id"] = json!(id()),
                3 => value["outline"]["review"]["contentHash"] = json!("changed"),
                _ => {
                    let duplicate = value["lessons"][0].clone();
                    value["lessons"].as_array_mut().unwrap().push(duplicate);
                }
            }
            entry.bytes = serde_json::to_vec(&value).unwrap();
        })
        .await?;
        assert!(packs
            .preview(&PreviewLearningPackImportRequestDto {
                operation_id: id(),
                preview_id: id(),
                source_path: changed,
                conflict_policy: LearningPackConflictPolicy::ReplaceAfterBackup,
            })
            .await
            .is_err());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs")
            .fetch_one(&pool)
            .await?,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_pack_import_previews")
            .fetch_one(&pool)
            .await?,
        0
    );
    assert!(lesson_evidence::get(
        &pool,
        &original.summary.id,
        &original.modules[0].lessons[0].id
    )
    .await?
    .is_some());
    Ok(())
}

#[tokio::test]
async fn failed_provenance_backup_leaves_the_replacement_target_untouched() -> Result<()> {
    let pool = pool().await?;
    let original = ready_course(&pool).await?;
    let container = test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let path = export_without_activity(&packs, &container, &original).await?;
    sqlx::query("UPDATE learning_lesson_verifications SET content_sha256='mismatched-column' WHERE program_id=?")
        .bind(&original.summary.id).execute(&pool).await?;
    let error = apply_path(
        &packs,
        &container,
        path,
        LearningPackConflictPolicy::ReplaceAfterBackup,
    )
    .await
    .expect_err("an incomplete backup must block replacement");
    assert!(
        error
            .to_string()
            .contains("verification columns do not match"),
        "{error}"
    );
    let fingerprint: String = sqlx::query_scalar(
        "SELECT content_sha256 FROM learning_lesson_verifications WHERE program_id=?",
    )
    .bind(&original.summary.id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(fingerprint, "mismatched-column");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_pack_imports")
            .fetch_one(&pool)
            .await?,
        0
    );
    Ok(())
}
