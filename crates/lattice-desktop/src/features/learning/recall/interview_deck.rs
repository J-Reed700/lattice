//! A recall deck made from an interview activity's questions and its private
//! answer guides. Each question is answered from memory before its guide is
//! shown, then comes back on a schedule that fits the interview date, so the
//! guide is studied as feedback on an attempt rather than read in advance.
use super::study_dto::{StudyCardDto, StudyCardFormat, StudyDeckDto, StudySourceDto};
use crate::features::learning::practice::{
    interview_answers::{InterviewAnswerGuide, InterviewAnswerGuides, INTERVIEW_ANSWERS_FILE_PATH},
    practical_dto::{
        LearningInterviewPlanDto, LearningInterviewQuestionDto, LearningInterviewRoundKind,
    },
    practical_generation::INTERVIEW_PLAN_FILE_PATH,
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

fn db(error: sqlx::Error) -> AppError {
    AppError::Database(error.to_string())
}

fn decode<T: serde::de::DeserializeOwned>(content: &str, what: &str) -> Result<T> {
    serde_json::from_str(content)
        .map_err(|error| AppError::Database(format!("Saved interview {what} is invalid: {error}")))
}

/// What a deck is made from: the activity's title, its plan, and its answer
/// guides when it has them. Activities made before guides existed have none.
pub(crate) struct InterviewMaterial {
    pub title: String,
    pub plan: LearningInterviewPlanDto,
    pub guides: Option<InterviewAnswerGuides>,
    /// Title and link of each cited source version, by version ID.
    pub sources: HashMap<String, (String, Option<String>)>,
}

pub(crate) async fn load(
    pool: &SqlitePool,
    program_id: &str,
    activity_id: &str,
) -> Result<InterviewMaterial> {
    let activity = sqlx::query(
        "SELECT title,kind,status FROM learning_practical_activities WHERE program_id=? AND id=?",
    )
    .bind(program_id)
    .bind(activity_id)
    .fetch_optional(pool)
    .await
    .map_err(db)?
    .ok_or_else(|| AppError::NotFound("Interview activity not found".into()))?;
    if activity.get::<String, _>("kind") != "interview" {
        return Err(AppError::InvalidInput(
            "Only interview activities can become an interview drill.".into(),
        ));
    }
    if activity.get::<String, _>("status") != "ready" {
        return Err(AppError::InvalidInput(
            "This interview is not ready yet. Wait for it to finish preparing.".into(),
        ));
    }
    let files = sqlx::query(
        "SELECT path,content FROM learning_practical_files WHERE activity_id=? AND path IN (?,?)",
    )
    .bind(activity_id)
    .bind(INTERVIEW_PLAN_FILE_PATH)
    .bind(INTERVIEW_ANSWERS_FILE_PATH)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    let mut plan = None;
    let mut guides = None;
    for file in files {
        let content: String = file.get("content");
        if file.get::<String, _>("path") == INTERVIEW_PLAN_FILE_PATH {
            plan = Some(decode::<LearningInterviewPlanDto>(&content, "plan")?);
        } else {
            guides = Some(decode::<InterviewAnswerGuides>(&content, "answer guides")?);
        }
    }
    let plan = plan
        .filter(|plan| !plan.questions.is_empty())
        .ok_or_else(|| {
            AppError::InvalidInput("This interview has no saved questions to drill.".into())
        })?;
    if let Some(guides) = &guides {
        guides.validate_plan(&plan)?;
    }
    let mut sources: HashMap<String, (String, Option<String>)> = HashMap::new();
    if let Some(guides) = &guides {
        for version in guides.cited_versions() {
            if sources.contains_key(version) {
                continue;
            }
            if let Some(row) = sqlx::query("SELECT title,COALESCE(resolved_url,requested_url) url FROM learning_source_versions WHERE program_id=? AND id=?")
                .bind(program_id)
                .bind(version)
                .fetch_optional(pool)
                .await
                .map_err(db)?
            {
                sources.insert(version.to_owned(), (row.get("title"), row.get("url")));
            }
        }
    }
    Ok(InterviewMaterial {
        title: activity.get("title"),
        plan,
        guides,
        sources,
    })
}

fn round_label(round: LearningInterviewRoundKind) -> &'static str {
    match round {
        LearningInterviewRoundKind::TechnicalDeepDive => "Technical deep-dive",
        LearningInterviewRoundKind::SystemDesign => "System design",
        LearningInterviewRoundKind::Behavioral => "Behavioral",
        LearningInterviewRoundKind::DebuggingAndCode => "Debugging and code",
        LearningInterviewRoundKind::FullLoop => "Interview",
    }
}

fn bullets(items: &[String]) -> String {
    items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn section(out: &mut Vec<String>, heading: &str, body: String) {
    if !body.trim().is_empty() {
        out.push(format!("**{heading}**\n\n{body}"));
    }
}

/// One card per question. The answer is what a strong answer covers, short
/// enough to check an attempt against; the explanation holds the full guide.
fn card(
    deck_id: &str,
    now: i64,
    material: &InterviewMaterial,
    question: &LearningInterviewQuestionDto,
    guide: Option<&InterviewAnswerGuide>,
) -> StudyCardDto {
    let mut explanation = Vec::new();
    let answer = match guide {
        Some(guide) => {
            let points = guide
                .standard
                .key_points
                .iter()
                .map(|point| point.point.clone())
                .collect::<Vec<_>>();
            section(
                &mut explanation,
                "A strong answer",
                guide.standard.reference_answer.trim().to_owned(),
            );
            section(
                &mut explanation,
                "Also acceptable",
                bullets(&guide.standard.acceptable_alternatives),
            );
            section(
                &mut explanation,
                "Common mistakes",
                bullets(&guide.standard.common_mistakes),
            );
            if points.is_empty() {
                guide.standard.reference_answer.trim().to_owned()
            } else {
                bullets(&points)
            }
        }
        None => bullets(&question.evaluation_signals),
    };
    section(
        &mut explanation,
        "What the interviewer listens for",
        bullets(&question.evaluation_signals),
    );
    section(
        &mut explanation,
        "Follow-ups to expect",
        bullets(&question.follow_up_probes),
    );
    section(
        &mut explanation,
        "Why they ask",
        question.company_rationale.trim().to_owned(),
    );
    let mut citations: Vec<StudySourceDto> = Vec::new();
    for citation in guide
        .into_iter()
        .flat_map(|guide| &guide.standard.key_points)
        .flat_map(|point| &point.citations)
    {
        if citations
            .iter()
            .any(|seen| seen.chunk_id == citation.version_id && seen.excerpt == citation.quote)
        {
            continue;
        }
        let (title, url) = material
            .sources
            .get(&citation.version_id)
            .cloned()
            .unwrap_or_else(|| ("Interview research".into(), None));
        citations.push(StudySourceDto {
            chunk_id: citation.version_id.clone(),
            document_id: String::new(),
            file_name: title,
            file_path: String::new(),
            excerpt: citation.quote.clone(),
            url,
        });
    }
    let source = citations
        .first()
        .cloned()
        .unwrap_or_else(|| StudySourceDto {
            chunk_id: question.id.clone(),
            document_id: String::new(),
            file_name: format!("Answer guide · {}", material.title),
            file_path: String::new(),
            excerpt: guide
                .map(|guide| guide.standard.reference_answer.clone())
                .unwrap_or_else(|| question.company_rationale.clone()),
            url: None,
        });
    StudyCardDto {
        id: uuid::Uuid::new_v4().to_string(),
        format: StudyCardFormat::QuestionAnswer,
        deck_id: deck_id.to_owned(),
        question: question.question.trim().to_owned(),
        answer: if answer.trim().is_empty() {
            "Answer it aloud, then compare with the notes below.".into()
        } else {
            answer
        },
        options: Vec::new(),
        correct_index: 0,
        explanation: explanation.join("\n\n"),
        source,
        citations,
        topic: round_label(question.round).into(),
        due_at: now,
        interval_days: 0,
        review_count: 0,
        lapses: 0,
    }
}

/// The deck for `material`, every card due now.
pub(crate) fn deck(material: &InterviewMaterial, now: i64) -> StudyDeckDto {
    let id = uuid::Uuid::new_v4().to_string();
    let plan = &material.plan;
    let cards = plan
        .questions
        .iter()
        .map(|question| {
            let guide = material.guides.as_ref().and_then(|guides| {
                guides
                    .questions
                    .iter()
                    .find(|guide| guide.question_id == question.id)
            });
            card(&id, now, material, question, guide)
        })
        .collect();
    let company = plan.company_name.trim();
    let role = plan.target_role.trim();
    let title = match (company.is_empty(), role.is_empty()) {
        (false, false) => format!("{company} · {role} interview drill"),
        (false, true) => format!("{company} interview drill"),
        (true, false) => format!("{role} interview drill"),
        (true, true) => format!("{} drill", material.title.trim()),
    };
    StudyDeckDto {
        id,
        title: title.chars().take(120).collect(),
        focus: round_label(plan.round_kind).into(),
        study_goal: "Answer each question aloud from memory, then check it against the guide"
            .into(),
        model_name: material
            .guides
            .as_ref()
            .map(|guides| guides.author_model.clone())
            .unwrap_or_else(|| "Interview plan".into()),
        created_at: now,
        target_at: None,
        cards,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan() -> LearningInterviewPlanDto {
        serde_json::from_value(json!({
            "companyName": "Acme Corp",
            "targetRole": "Backend Engineer",
            "roundKind": "full_loop",
            "companySummary": "Streaming pipelines.",
            "learnerRole": "Candidate",
            "counterpartRole": "Interviewer",
            "questions": [
                {
                    "id": "q-backpressure",
                    "round": "technical_deep_dive",
                    "question": "How do you handle backpressure in event streams?",
                    "companyRationale": "Acme runs streaming pipelines.",
                    "followUpProbes": ["What do you drop first?"],
                    "evaluationSignals": ["Bounds every queue"]
                },
                {
                    "id": "q-conflict",
                    "round": "behavioral",
                    "question": "Tell me about a disagreement with a teammate.",
                    "companyRationale": "Teams own services end to end.",
                    "followUpProbes": [],
                    "evaluationSignals": ["Specific situation", "Owns the outcome"]
                }
            ]
        }))
        .expect("plan fixture")
    }

    fn guides() -> InterviewAnswerGuides {
        let quote = "Producers slow down when consumers fall behind.";
        serde_json::from_value(json!({
            "schemaVersion": 1,
            "authorModel": "author-model",
            "reviewerModel": "reviewer-model",
            "questions": [
                {
                    "questionId": "q-backpressure",
                    "question": "How do you handle backpressure in event streams?",
                    "standard": {
                        "referenceAnswer": "Bound the queues and signal producers to slow down.",
                        "keyPoints": [
                            {"point": "Bounded queues", "evidence": "sourced",
                             "citations": [{"sourceId": "s1", "versionId": "v1", "quote": quote}]},
                            {"point": "Producer signalling", "evidence": "sourced",
                             "citations": [{"sourceId": "s1", "versionId": "v1", "quote": quote}]}
                        ],
                        "acceptableAlternatives": ["Load shedding with metrics"],
                        "commonMistakes": ["Unbounded buffers"],
                        "scoringCriteria": [],
                        "evidenceGaps": []
                    }
                },
                {
                    "questionId": "q-conflict",
                    "question": "Tell me about a disagreement with a teammate.",
                    "standard": {
                        "referenceAnswer": "Situation, task, action, result.",
                        "keyPoints": [],
                        "acceptableAlternatives": [],
                        "commonMistakes": [],
                        "scoringCriteria": [],
                        "evidenceGaps": []
                    }
                }
            ]
        }))
        .expect("guide fixture")
    }

    fn material(guides: Option<InterviewAnswerGuides>) -> InterviewMaterial {
        InterviewMaterial {
            title: "Acme loop".into(),
            plan: plan(),
            guides,
            sources: HashMap::from([(
                "v1".to_owned(),
                (
                    "Acme engineering blog".to_owned(),
                    Some("https://acme.example.com/blog".to_owned()),
                ),
            )]),
        }
    }

    #[test]
    fn each_question_becomes_a_due_recall_card_checked_against_its_guide() {
        let material = material(Some(guides()));
        material
            .guides
            .as_ref()
            .map(|g| g.validate_plan(&material.plan))
            .transpose()
            .expect("valid");
        let deck = deck(&material, 5_000);
        assert_eq!(deck.title, "Acme Corp · Backend Engineer interview drill");
        assert_eq!(deck.model_name, "author-model");
        assert_eq!(deck.cards.len(), 2);
        let [technical, behavioral] = [&deck.cards[0], &deck.cards[1]];
        assert!(deck.cards.iter().all(|card| card.deck_id == deck.id
            && card.due_at == 5_000
            && card.format == StudyCardFormat::QuestionAnswer
            && card.options.is_empty()));
        assert_eq!(technical.answer, "- Bounded queues\n- Producer signalling");
        assert_eq!(technical.topic, "Technical deep-dive");
        assert!(technical
            .explanation
            .contains("**A strong answer**\n\nBound the queues"));
        assert!(technical
            .explanation
            .contains("**Common mistakes**\n\n- Unbounded buffers"));
        assert!(technical
            .explanation
            .contains("**Follow-ups to expect**\n\n- What do you drop first?"));
        assert_eq!(
            technical.citations.len(),
            1,
            "a quote cited twice is one citation"
        );
        assert_eq!(technical.source.file_name, "Acme engineering blog");
        assert_eq!(
            technical.source.url.as_deref(),
            Some("https://acme.example.com/blog")
        );
        assert_eq!(behavioral.topic, "Behavioral");
        assert_eq!(
            behavioral.answer, "Situation, task, action, result.",
            "no key points: the reference answer"
        );
        assert!(behavioral.citations.is_empty());
        assert_eq!(behavioral.source.file_name, "Answer guide · Acme loop");
    }

    #[test]
    fn an_interview_without_guides_drills_what_the_interviewer_listens_for() {
        let deck = deck(&material(None), 5_000);
        assert_eq!(deck.model_name, "Interview plan");
        assert_eq!(
            deck.cards[1].answer,
            "- Specific situation\n- Owns the outcome"
        );
        assert!(!deck.cards[1].explanation.contains("A strong answer"));
    }

    /// Saves an activity row and its files the way the practical repository
    /// stores them: the plan as a reference file, the guides as a hidden check.
    async fn save(
        pool: &SqlitePool,
        id: &str,
        kind: &str,
        status: &str,
        files: &[(&str, &str, String)],
    ) -> Result<()> {
        sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,generator_model,created_at,updated_at) VALUES(?,'program','lesson',?,'Acme loop','',?,'practice','[]','[]','[]','[]','none','model',0,0)")
            .bind(id).bind(kind).bind(status)
            .execute(pool).await.map_err(db)?;
        for (ordinal, (path, role, content)) in files.iter().enumerate() {
            sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,?,?,?,?,'sha',0)")
                .bind(id).bind(ordinal as i64).bind(*path).bind(*role).bind(content)
                .execute(pool).await.map_err(db)?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn a_saved_ready_interview_loads_with_its_guides_and_cited_sources() -> Result<()> {
        let pool = crate::features::learning::tests::pool().await?;
        // Only the rows `load` reads; their program, lesson and source library are not under test.
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&pool)
            .await
            .map_err(db)?;
        let json = |value: serde_json::Result<String>| {
            value.map_err(|error| AppError::Serialization(error.to_string()))
        };
        let plan_file = (
            INTERVIEW_PLAN_FILE_PATH,
            "reference",
            json(serde_json::to_string(&plan()))?,
        );
        let guide_file = (
            INTERVIEW_ANSWERS_FILE_PATH,
            "check",
            json(serde_json::to_string(&guides()))?,
        );
        save(
            &pool,
            "ready",
            "interview",
            "ready",
            &[plan_file.clone(), guide_file.clone()],
        )
        .await?;
        sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES('v1','program','s1',1,'Acme engineering blog',NULL,'https://acme.example.com/requested','https://acme.example.com/blog','text','text','sha',1,0,'test',0)")
            .execute(&pool).await.map_err(db)?;

        let material = load(&pool, "program", "ready").await?;
        assert_eq!(material.title, "Acme loop");
        assert_eq!(material.plan.questions.len(), 2);
        assert_eq!(
            material
                .guides
                .as_ref()
                .map(|guides| guides.questions.len()),
            Some(2)
        );
        assert_eq!(
            material.sources.get("v1"),
            Some(&(
                "Acme engineering blog".to_owned(),
                Some("https://acme.example.com/blog".to_owned())
            ))
        );
        assert_eq!(
            deck(&material, 1).cards[0].source.file_name,
            "Acme engineering blog"
        );

        save(
            &pool,
            "plan-only",
            "interview",
            "ready",
            &[plan_file.clone()],
        )
        .await?;
        assert!(load(&pool, "program", "plan-only").await?.guides.is_none());
        save(&pool, "draft", "interview", "draft", &[plan_file.clone()]).await?;
        save(&pool, "lab", "code_lab", "ready", &[]).await?;
        save(&pool, "empty", "interview", "ready", &[]).await?;
        for activity in ["draft", "lab", "empty"] {
            assert!(
                matches!(
                    load(&pool, "program", activity).await,
                    Err(AppError::InvalidInput(_))
                ),
                "{activity} cannot become a drill"
            );
        }
        assert!(matches!(
            load(&pool, "other-program", "ready").await,
            Err(AppError::NotFound(_))
        ));
        Ok(())
    }
}
