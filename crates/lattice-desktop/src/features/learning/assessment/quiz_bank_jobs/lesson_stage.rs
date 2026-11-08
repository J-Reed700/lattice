//! The lesson job's quiz bank stage (section 5.4) and the lesson's own
//! reviewed questions as bank items.
use super::*;

fn seed_difficulty(kind: &LessonQuestionKind) -> u8 {
    match kind {
        LessonQuestionKind::Practice => 2,
        LessonQuestionKind::Quiz => 3,
        LessonQuestionKind::Test => 4,
    }
}

/// A prepared lesson's own reviewed questions as bank items (single choice;
/// difficulty from their kind: practice 2, quiz 3, test 4). Options are
/// reshuffled so the key's position is code's choice. Questions that fail the
/// bank's checks are left out.
pub(crate) fn lesson_seed_items(
    lesson_id: &str,
    outcome_id: &str,
    questions: &[LessonQuestion],
    keys: &[LessonAnswerKey],
) -> Vec<BankItem> {
    questions
        .iter()
        .filter_map(|q| {
            let key = keys.iter().find(|k| k.question_id == q.id)?;
            let mut candidate = engine::LearningAssessmentCandidate {
                id: q.id.clone(),
                outcome_ids: vec![outcome_id.to_string()],
                format: LearningItemFormat::SingleChoice,
                difficulty: seed_difficulty(&q.kind),
                prompt: q.prompt.clone(),
                options: q.options.clone(),
                left: vec![],
                source_version_ids: q
                    .source_ids
                    .iter()
                    .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                    .take(24)
                    .cloned()
                    .collect(),
                rubric: vec![],
                key: engine::LearningAssessmentKey::Choice(key.correct_index),
                explanation: key.explanation.clone(),
                distractor_rationales: vec![],
            };
            if let Err(error) = engine::validate_candidate(&candidate) {
                tracing::info!(question = %q.id, %error, "A lesson question did not join the quiz bank");
                return None;
            }
            engine::shuffle_authored(&mut candidate);
            Some(BankItem {
                item: candidate.into(),
                source_quote: String::new(),
                lesson_id: Some(lesson_id.to_string()),
                origin: "lesson",
            })
        })
        .collect()
}

/// What the lesson job's bank stage wrote for one lesson, published with it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LessonBank {
    /// The outcome the items address; the lesson's outcome is created with
    /// this ID when it does not exist yet.
    pub outcome_id: String,
    pub items: Vec<AuthoredItem>,
    pub author_model: Option<String>,
}

/// The lesson job's last stage, after claim verification and before the
/// lesson is published (section 5.4): write the rest of the lesson's quiz
/// bank from its verified blocks, around its own reviewed questions.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn lesson_bank_stage(
    llm: &dyn LLMPort,
    pool: &SqlitePool,
    program_id: &str,
    lesson_id: &str,
    title: &str,
    objective: &str,
    blocks: &[LearningBlockDto],
    questions: &[LessonQuestion],
    keys: &[LessonAnswerKey],
) -> Result<LessonBank> {
    let (outcome_id, settings, quiz_formats) = {
        let mut connection = pool.acquire().await.map_err(db)?;
        let existing: Option<String> = sqlx::query_scalar("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND lesson_id=? ORDER BY created_at,id LIMIT 1")
            .bind(program_id)
            .bind(lesson_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
        (
            existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            quiz_blueprints::settings_in(&mut connection, program_id).await?,
            quiz_blueprints::lesson_quiz_formats_in(&mut connection, program_id, lesson_id).await?,
        )
    };
    let facts = CourseFacts {
        program_title: String::new(),
        modules: vec![],
        lessons: vec![LessonFacts {
            id: lesson_id.into(),
            module_id: String::new(),
            title: title.into(),
            objective: objective.into(),
            prepared: true,
            outcome_id: Some(outcome_id.clone()),
            source_version_ids: vec![],
            quiz_formats,
        }],
    };
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(lesson_id.into()),
    };
    let Some(blueprint) = quiz_blueprints::blueprint_for(&scope, &facts, &settings) else {
        return Ok(LessonBank {
            outcome_id,
            ..LessonBank::default()
        });
    };
    let material: Vec<MaterialPassage> = blocks
        .iter()
        .filter(|b| !b.body.trim().is_empty())
        .map(|b| MaterialPassage {
            lesson_title: title.into(),
            title: b.title.clone(),
            text: b.body.clone(),
            source_version_ids: b.source_ids.clone(),
        })
        .collect();
    let seeds: Vec<StoredItem> = lesson_seed_items(lesson_id, &outcome_id, questions, keys)
        .into_iter()
        .map(|s| s.item)
        .collect();
    let mut written: Vec<AuthoredItem> = Vec::new();
    let total = blueprint.slots.len();
    for (index, slot) in blueprint.slots.iter().enumerate() {
        let requirement = &slot.requirement;
        let have = seeds
            .iter()
            .map(|s| engine::LearningAssessmentCandidate::from(s.clone()))
            .chain(written.iter().map(|w| w.candidate.clone().into()))
            .filter(|c| engine::eligible_for(c, requirement))
            .count();
        let count = (3 * requirement.count).saturating_sub(have);
        if count == 0 {
            continue;
        }
        crate::features::learning::lesson_progress::stage(format!(
            "Writing quiz questions (step 3 of 3): {} of {total}",
            index + 1
        ));
        let need = AuthoringNeed {
            requirement: requirement.clone(),
            outcome_title: title.into(),
            outcome_description: objective.into(),
            lesson_id: Some(lesson_id.into()),
            count,
            material: material.clone(),
            existing_prompts: seeds
                .iter()
                .map(|s| s.prompt.clone())
                .chain(written.iter().map(|w| w.candidate.prompt.clone()))
                .collect(),
        };
        written.extend(generation::author_items(llm, &need).await?.items);
    }
    Ok(LessonBank {
        outcome_id,
        items: written,
        author_model: Some(llm.model_name().to_string()),
    })
}
