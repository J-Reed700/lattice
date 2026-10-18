use super::*;
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use crate::features::learning::{answer_review, lesson_drafts, teaching_review};

#[tokio::test]
async fn completed_review_findings_survive_resume_without_losing_defects() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let mut candidate = json!({"blocks":[{"body":"A teaching passage to inspect."}],"questions":[{
        "prompt":"Choose the documented answer.","options":["First","Second"],"correctIndex":0,"quote":"The reference identifies the second option."
    }]});
    let answer = json!({"answers":[{"index":0,"correctIndices":[1],"reason":"The source identifies the second option as correct."}]}).to_string();
    let review = json!({"issues":["The candidate's answer conflicts with the reference."],"blockChecks":{
        "section-0":{"passageId":"section-0-passage-0","finding":"The complete teaching passage was inspected for conflicts.","hasDefect":false}
    }}).to_string();
    let model = ScriptedModel {
        outputs: Mutex::new(
            vec![
                answer.clone(),
                review.clone(),
                review.clone(),
                answer,
                review,
            ]
            .into(),
        ),
        prompts: Default::default(),
    };
    for _ in 0..2 {
        // Re-enter the durable task scope, as a worker does after restart.
        lesson_drafts::run(&repo, &job, &lesson, async {
            assert_eq!(answer_review::check(&model, &candidate).await?.len(), 1);
            assert_eq!(
                teaching_review::review(&model, "Review", json!({}), &candidate, None)
                    .await?
                    .len(),
                1
            );
            Ok(())
        })
        .await?;
    }
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    candidate["blocks"][0]["body"] = json!("A changed teaching passage to inspect.");
    lesson_drafts::run(&repo, &job, &lesson, async {
        assert_eq!(answer_review::check(&model, &candidate).await?.len(), 1);
        teaching_review::review(&model, "Review", json!({}), &candidate, None).await?;
        assert_eq!(
            model.prompts.lock().unwrap().len(),
            3,
            "Only the changed teaching review should rerun"
        );
        candidate["questions"][0]["correctIndex"] = json!(1);
        assert!(answer_review::check(&model, &candidate).await?.is_empty());
        teaching_review::review(&model, "Review", json!({}), &candidate, None).await?;
        assert_eq!(model.prompts.lock().unwrap().len(), 5);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn incomplete_reviews_are_never_saved_as_completed() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let candidate = json!({"blocks":[{"body":"The complete section needs review."}],"questions":[{
        "prompt":"Choose one.","options":["A","B"],"correctIndex":0
    }]});
    let model = ScriptedModel {
        outputs: Mutex::new(vec!["{}".to_owned(); 8].into()),
        prompts: Default::default(),
    };
    for _ in 0..2 {
        lesson_drafts::run(&repo, &job, &lesson, async {
            assert!(answer_review::check(&model, &candidate).await.is_err());
            assert!(
                teaching_review::review(&model, "Review", json!({}), &candidate, None)
                    .await
                    .is_err()
            );
            Ok(())
        })
        .await?;
    }
    assert_eq!(model.prompts.lock().unwrap().len(), 8);
    Ok(())
}
