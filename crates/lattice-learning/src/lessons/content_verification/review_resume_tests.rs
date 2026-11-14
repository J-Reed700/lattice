use super::*;
use crate::lessons::course_generation_tests::ScriptedModel;
use crate::question_review::{self, Outcome};
use crate::teaching_review::{Remit, Scope};
use crate::{lesson_drafts, teaching_review};

fn outcome() -> Outcome<'static> {
    Outcome {
        title: "Lesson",
        description: "Choose the documented answer.".into(),
    }
}

fn solved(chosen: usize) -> String {
    json!({"answers":[{"index":0,"reason":"The source identifies that option as correct.","correctIndices":[chosen],"answer":"","blankAnswers":[],"matches":[],"orderedValues":[]}]}).to_string()
}

fn judged() -> String {
    json!({"checks":[{"id":"new-0","finding":"","hasDefect":false,"repeats":[]}]}).to_string()
}

#[tokio::test]
async fn completed_review_findings_survive_resume_without_losing_defects() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let mut candidate = json!({"blocks":[{"body":"A teaching passage to inspect."}],"questions":[{
        "kind":"quiz","prompt":"Choose the documented answer.","options":["First","Second"],"correctIndex":0,"explanation":"The reference names it.","quote":"The reference identifies the second option."
    }]});
    let review = json!({"issues":["The candidate's answer conflicts with the reference."],"blockChecks":[
        {"sectionId":"section-0","passageId":"section-0-passage-0","finding":"The complete teaching passage was inspected for conflicts.","hasDefect":false}
    ]}).to_string();
    let model = ScriptedModel {
        outputs: Mutex::new(
            vec![
                solved(1),
                judged(),
                review.clone(),
                review.clone(),
                solved(1),
                judged(),
                review,
            ]
            .into(),
        ),
        prompts: Default::default(),
    };
    for _ in 0..2 {
        // Re-enter the durable task scope, as a worker does after restart.
        lesson_drafts::run(&db, &repo, &job, &lesson, async {
            assert_eq!(
                question_review::check(&model, &candidate, outcome())
                    .await?
                    .len(),
                1
            );
            assert_eq!(
                teaching_review::review(
                    &model,
                    "Review",
                    json!({}),
                    &candidate,
                    Scope::default(),
                    Remit::Open,
                    None,
                )
                .await?
                .reported
                .len(),
                1
            );
            Ok(())
        })
        .await?;
    }
    assert_eq!(model.prompts.lock().unwrap().len(), 3);
    candidate["blocks"][0]["body"] = json!("A changed teaching passage to inspect.");
    lesson_drafts::run(&db, &repo, &job, &lesson, async {
        assert_eq!(
            question_review::check(&model, &candidate, outcome())
                .await?
                .len(),
            1
        );
        teaching_review::review(
            &model,
            "Review",
            json!({}),
            &candidate,
            Scope::default(),
            Remit::Open,
            None,
        )
        .await?;
        assert_eq!(
            model.prompts.lock().unwrap().len(),
            4,
            "Only the changed teaching review should rerun"
        );
        candidate["questions"][0]["correctIndex"] = json!(1);
        assert!(question_review::check(&model, &candidate, outcome())
            .await?
            .is_empty());
        teaching_review::review(
            &model,
            "Review",
            json!({}),
            &candidate,
            Scope::default(),
            Remit::Open,
            None,
        )
        .await?;
        assert_eq!(model.prompts.lock().unwrap().len(), 7);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn incomplete_reviews_are_never_saved_as_completed() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let (repo, job, lesson) = batch_scheduler_tests::setup(&db).await?;
    let candidate = json!({"blocks":[{"body":"The complete section needs review."}],"questions":[{
        "kind":"quiz","prompt":"Choose one.","options":["A","B"],"correctIndex":0,"explanation":"A is right."
    }]});
    let model = ScriptedModel {
        outputs: Mutex::new(vec!["{}".to_owned(); 8].into()),
        prompts: Default::default(),
    };
    for _ in 0..2 {
        lesson_drafts::run(&db, &repo, &job, &lesson, async {
            assert!(question_review::check(&model, &candidate, outcome())
                .await
                .is_err());
            assert!(teaching_review::review(
                &model,
                "Review",
                json!({}),
                &candidate,
                Scope::default(),
                Remit::Open,
                None,
            )
            .await
            .is_err());
            Ok(())
        })
        .await?;
    }
    assert_eq!(model.prompts.lock().unwrap().len(), 8);
    Ok(())
}
