//! A published lesson's quiz bank stage (section 5.4), run by its quiz job,
//! and the lesson's own reviewed questions as bank items.
use super::*;

fn seed_difficulty(kind: &LessonQuestionKind) -> u8 {
    match kind {
        LessonQuestionKind::Practice => 2,
        LessonQuestionKind::Quiz => 3,
        LessonQuestionKind::Test => 4,
    }
}

/// A lesson question with its key as the single-choice item it is in the bank
/// and in review. Options and their reasons keep the writer's order.
pub(crate) fn lesson_question_item(
    q: &LessonQuestion,
    key: &LessonAnswerKey,
    outcome_ids: Vec<String>,
) -> engine::LearningAssessmentCandidate {
    engine::LearningAssessmentCandidate {
        id: q.id.clone(),
        outcome_ids,
        format: LearningItemFormat::SingleChoice,
        difficulty: seed_difficulty(&q.kind),
        prompt: q.prompt.clone(),
        options: q.options.clone(),
        left: vec![],
        source_version_ids: q
            .source_ids
            .iter()
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .cloned()
            .collect(),
        rubric: vec![],
        key: engine::LearningAssessmentKey::Choice(key.correct_index),
        explanation: key.explanation.clone(),
        distractor_rationales: key.rationales.clone(),
    }
}

/// A prepared lesson's own reviewed questions as bank items (single choice;
/// difficulty from their kind: practice 2, quiz 3, test 4). Options and their
/// reasons keep the writer's order, which claims and the published editor
/// address them by; every form draw shuffles them. A question that fails the
/// bank's checks stays, marked `dropped`, so the lesson's questions keep their
/// places.
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
            let candidate = lesson_question_item(q, key, vec![outcome_id.to_string()]);
            let review_status = match engine::validate_candidate(&candidate) {
                Ok(()) => "agreed",
                Err(error) => {
                    tracing::info!(question = %q.id, %error, "A lesson question was kept out of the quiz bank's draws");
                    crate::lessons::trace::lesson_span(lesson_id).in_scope(|| {
                        crate::lessons::trace::decided(
                            crate::lessons::trace::Stage::Quiz,
                            &format!("question {}", q.prompt),
                            "kept out of the bank's draws",
                            &error.to_string(),
                        );
                    });
                    "dropped"
                }
            };
            Some(BankItem {
                item: candidate.into(),
                source_quote: String::new(),
                lesson_id: Some(lesson_id.to_string()),
                origin: "lesson",
                review_status,
            })
        })
        .collect()
}

/// What the bank stage wrote for one lesson; saved before it is inserted.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LessonBank {
    /// The outcome the items address.
    pub outcome_id: String,
    pub items: Vec<AuthoredItem>,
    /// Items the checks or the blinded key review left out, with why.
    #[serde(default)]
    pub dropped: Vec<String>,
    pub author_model: Option<String>,
}

/// Write the rest of a published lesson's quiz bank from its blocks
/// (`material`), around its own reviewed questions (`seeds`).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn lesson_bank_stage(
    llm: &dyn LLMPort,
    database: &Database,
    program_id: &str,
    lesson_id: &str,
    title: &str,
    objective: &str,
    material: Vec<MaterialPassage>,
    seeds: Vec<StoredItem>,
) -> Result<LessonBank> {
    let (outcome_id, settings, quiz_formats) = {
        let mut tx = database.begin_read().await?;
        let existing =
            quiz_blueprints::repository::lesson_outcome_in(&mut tx, program_id, lesson_id).await?;
        let settings = quiz_blueprints::settings_in(&mut tx, program_id).await?;
        let quiz_formats =
            quiz_blueprints::lesson_quiz_formats_in(&mut tx, program_id, lesson_id).await?;
        tx.commit().await?;
        (
            existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            settings,
            quiz_formats,
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
    let mut written: Vec<AuthoredItem> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    for slot in &blueprint.slots {
        let requirement = &slot.requirement;
        let have = seeds
            .iter()
            .map(|s| engine::LearningAssessmentCandidate::from(s.clone()))
            .chain(written.iter().map(|w| w.candidate.clone().into()))
            .filter(|c| engine::eligible_for(c, requirement))
            .count();
        let count = requirement.bank_target().saturating_sub(have);
        if count == 0 {
            continue;
        }
        let need = AuthoringNeed {
            requirement: requirement.clone(),
            outcome_title: title.into(),
            outcome_description: objective.into(),
            lesson_id: Some(lesson_id.into()),
            count,
            pass: generation::BankPass::Retakes,
            material: material.clone(),
            existing_prompts: seeds
                .iter()
                .map(|s| s.prompt.clone())
                .chain(written.iter().map(|w| w.candidate.prompt.clone()))
                .collect(),
        };
        let authored = generation::author_items(llm, &need).await?;
        written.extend(authored.items);
        dropped.extend(authored.dropped);
    }
    Ok(LessonBank {
        outcome_id,
        items: written,
        dropped,
        author_model: Some(llm.model_name().to_string()),
    })
}
