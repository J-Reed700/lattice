//! The `quiz_grading` job: written answers and short-answer near-misses
//! graded a few at a time with checked rationales, and contests' blind
//! regrades. A stopped job leaves its items pending, never failed.
use super::*;

/// Grade one written answer as the `quiz_grading` job does: the model's
/// rubric grade, then the section 4.3a checks. `Checked::Unexplained` is
/// "Couldn't grade yet", never a wrong answer.
pub async fn grade_written_answer(
    llm: &dyn LLMPort,
    answer: &WrittenAnswer,
    reference_version: Option<&str>,
) -> Result<Checked> {
    let raw = generation::grade_written(llm, std::slice::from_ref(answer)).await?;
    Ok(checked_grade(
        raw.iter().find(|r| r.item_id == answer.item_id),
        &answer.rubric,
        &answer.answer,
        &answer.reference,
        reference_version,
        &Grader {
            model: llm.model_name(),
            graded_at: now(),
        },
    ))
}

/// Judge one short answer that matched no accepted variant, in the same
/// rationale shape.
pub async fn judge_short_answer(llm: &dyn LLMPort, answer: &ShortAnswer) -> Result<Checked> {
    let raw = generation::judge_short_answers(llm, std::slice::from_ref(answer)).await?;
    Ok(checked_grade(
        raw.iter().find(|r| r.item_id == answer.item_id),
        &grade_rationale::equivalence_rubric(),
        &answer.answer,
        &answer.reference,
        None,
        &Grader {
            model: llm.model_name(),
            graded_at: now(),
        },
    ))
}

/// Grades a submitted quiz's pending items, or regrades a contest.
#[derive(Clone)]
pub struct QuizGradingWorker {
    pub pool: SqlitePool,
    pub load_llm: ModelLoader,
}

/// What a pending item needs to be graded.
enum PendingItem {
    Written(
        WrittenAnswer,
        i64,
        Vec<engine::LearningRubricCriterion>,
        Option<String>,
    ),
    Short(ShortAnswer, i64, Option<String>),
}

/// Check a grader's answer and turn it into the item's grade.
fn checked_grade(
    raw: Option<&RawGrade>,
    rubric: &[engine::LearningRubricCriterion],
    answer: &str,
    reference: &str,
    version: Option<&str>,
    grader: &Grader<'_>,
) -> Checked {
    let Some(raw) = raw else {
        return Checked::Unexplained(grade_rationale::UNEXPLAINED.into());
    };
    grade_rationale::check(
        rubric,
        answer,
        (!reference.trim().is_empty()).then_some(Reference {
            text: reference,
            version_id: version,
        }),
        raw.item.clone(),
        grader,
    )
}

fn short_detail(
    answer: &str,
    fraction: f64,
    rationale: &crate::features::learning::dto::LearningGradeRationaleDto,
) -> String {
    if fraction >= 1.0 {
        return String::new();
    }
    let why = rationale
        .criteria
        .first()
        .map(|c| c.what_was_missing.clone())
        .unwrap_or_default();
    format!(
        "“{}” isn't an accepted answer; the model judged it not equivalent because {}",
        answer.trim(),
        why.trim_end_matches('.').to_string() + "."
    )
}

impl QuizGradingWorker {
    async fn pending_items(
        &self,
        program_id: &str,
        form_id: &str,
        only: Option<i64>,
        note: Option<&str>,
    ) -> Result<Vec<PendingItem>> {
        let repo = LearningAssessmentRepository::new(self.pool.clone());
        let form = repo.get_form(form_id).await?;
        let Some(submission) = &form.submission else {
            return Ok(vec![]);
        };
        let mut connection = self.pool.acquire().await.map_err(db)?;
        let mut out = Vec::new();
        for (ordinal, (item, result)) in form.items.iter().zip(&submission.item_results).enumerate()
        {
            let ordinal = ordinal as i64;
            let wanted = match only {
                Some(o) => o == ordinal,
                None => result.grade_status == LearningAssessmentGradeStatus::Pending,
            };
            if !wanted {
                continue;
            }
            let stored = LearningAssessmentRepository::stored_candidate_in(
                &mut connection,
                program_id,
                &item.id,
            )
            .await?;
            let version = stored.candidate.source_version_ids.first().cloned();
            match (&item.format, &stored.candidate.key) {
                (LearningItemFormat::ShortAnswer, engine::LearningAssessmentKey::Text(key)) => out
                    .push(PendingItem::Short(
                        ShortAnswer {
                            item_id: item.id.clone(),
                            prompt: item.prompt.clone(),
                            accepted: key.variants.clone(),
                            answer: item.text_response.clone().unwrap_or_default(),
                            reference: stored.source_quote.clone(),
                            learner_note: note.map(str::to_owned),
                        },
                        ordinal,
                        version,
                    )),
                (LearningItemFormat::Explanation | LearningItemFormat::Artifact, _) => {
                    let answer = match (&item.text_response, &item.artifact_json) {
                        (Some(text), Some(artifact)) => format!("{text}\n{artifact}"),
                        (Some(text), None) => text.clone(),
                        (None, Some(artifact)) => artifact.to_string(),
                        (None, None) => String::new(),
                    };
                    out.push(PendingItem::Written(
                        WrittenAnswer {
                            item_id: item.id.clone(),
                            prompt: item.prompt.clone(),
                            rubric: item.rubric.clone(),
                            answer,
                            reference: stored.source_quote.clone(),
                            learner_note: note.map(str::to_owned),
                        },
                        ordinal,
                        item.rubric.clone(),
                        version,
                    ))
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// Grade items through the model and check every rationale.
    async fn grade(
        llm: &dyn LLMPort,
        items: &[PendingItem],
    ) -> Result<Vec<(i64, Checked, Option<String>, bool, String)>> {
        let written: Vec<WrittenAnswer> = items
            .iter()
            .filter_map(|i| match i {
                PendingItem::Written(a, ..) => Some(a.clone()),
                _ => None,
            })
            .collect();
        let short: Vec<ShortAnswer> = items
            .iter()
            .filter_map(|i| match i {
                PendingItem::Short(a, ..) => Some(a.clone()),
                _ => None,
            })
            .collect();
        let mut raw = generation::grade_written(llm, &written).await?;
        raw.extend(generation::judge_short_answers(llm, &short).await?);
        let grader = Grader {
            model: llm.model_name(),
            graded_at: now(),
        };
        let equivalence = grade_rationale::equivalence_rubric();
        Ok(items
            .iter()
            .map(|item| match item {
                PendingItem::Written(a, ordinal, rubric, version) => {
                    let found = raw.iter().find(|r| r.item_id == a.item_id);
                    (
                        *ordinal,
                        checked_grade(
                            found,
                            rubric,
                            &a.answer,
                            &a.reference,
                            version.as_deref(),
                            &grader,
                        ),
                        found.and_then(|r| r.response_to_note.clone()),
                        false,
                        a.answer.clone(),
                    )
                }
                PendingItem::Short(a, ordinal, version) => {
                    let found = raw.iter().find(|r| r.item_id == a.item_id);
                    (
                        *ordinal,
                        checked_grade(
                            found,
                            &equivalence,
                            &a.answer,
                            &a.reference,
                            version.as_deref(),
                            &grader,
                        ),
                        found.and_then(|r| r.response_to_note.clone()),
                        true,
                        a.answer.clone(),
                    )
                }
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl JobHandler for QuizGradingWorker {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let job = context.job();
        let program_id = requested_str(job, "programId")?.to_string();
        let form_id = requested_str(job, "formId")?.to_string();
        let contest_id = job
            .requested
            .get("contestId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let repo = LearningAssessmentRepository::new(self.pool.clone());
        let contest = match &contest_id {
            Some(id) => match repo.contest_input(id).await? {
                Some(contest) => Some(contest),
                None => {
                    return Ok(JobOutcome::Completed {
                        result_ref: format!("quiz_regrade:{id}"),
                        message: "This contest was already decided.".into(),
                    })
                }
            },
            None => None,
        };
        let items = self
            .pending_items(
                &program_id,
                &form_id,
                contest.as_ref().map(|c| c.ordinal),
                contest.as_ref().map(|c| c.note.as_str()),
            )
            .await?;
        if items.is_empty() {
            return Ok(JobOutcome::Completed {
                result_ref: format!("quiz_grading:{form_id}"),
                message: "Nothing is waiting to be graded.".into(),
            });
        }
        let Some(pin) = context.until_cancelled((self.load_llm)()).await else {
            return Ok(JobOutcome::Stopped);
        };
        let pin = pin?;
        let llm = pin.llm.as_ref();
        let model = llm.model_name().to_string();
        if let (Some(contest_id), Some(_)) = (&contest_id, &contest) {
            context.progress(0, "Regrading your answer", None).await?;
            let Some(graded) = context.until_cancelled(Self::grade(llm, &items)).await else {
                return Ok(JobOutcome::Stopped);
            };
            let (_, checked, response_to_note, short, _) = graded?
                .into_iter()
                .next()
                .ok_or_else(|| AppError::InvalidState("The regrade returned nothing.".into()))?;
            let regrade = match checked {
                Checked::Graded {
                    rationale,
                    fraction,
                    status,
                } => crate::features::learning::assessment_repository::Regrade::Graded {
                    fraction,
                    correct: short.then_some(fraction >= 1.0),
                    status,
                    rationale,
                    response_to_note,
                },
                Checked::Unexplained(reason) => {
                    crate::features::learning::assessment_repository::Regrade::Failed(reason)
                }
            };
            repo.apply_regrade(contest_id, regrade, &model).await?;
            return Ok(JobOutcome::Completed {
                result_ref: format!("quiz_regrade:{contest_id}"),
                message: "Regrade finished.".into(),
            });
        }
        // Grade a few answers at a time, so Stop keeps what was graded.
        let total = context.job().progress_total;
        let mut done = 0usize;
        let mut unexplained = 0usize;
        for chunk in items.chunks(4) {
            context
                .progress(
                    (done as u32).min(total),
                    &format!("Grading written answers ({} of {})", done + 1, items.len()),
                    None,
                )
                .await?;
            let Some(graded) = context.until_cancelled(Self::grade(llm, chunk)).await else {
                return Ok(JobOutcome::Stopped);
            };
            let grades: Vec<ItemGrade> = graded?
                .into_iter()
                .map(|(ordinal, checked, _, short, answer)| match checked {
                    Checked::Graded {
                        rationale,
                        fraction,
                        status,
                    } => ItemGrade::Graded {
                        ordinal,
                        fraction,
                        correct: short.then_some(fraction >= 1.0),
                        status,
                        detail: if short {
                            short_detail(&answer, fraction, &rationale)
                        } else {
                            String::new()
                        },
                        rationale: Some(rationale),
                    },
                    Checked::Unexplained(reason) => {
                        unexplained += 1;
                        ItemGrade::Unexplained { ordinal, reason }
                    }
                })
                .collect();
            repo.apply_grades(&form_id, &grades, &model).await?;
            done += chunk.len();
        }
        Ok(JobOutcome::Completed {
            result_ref: format!("quiz_grading:{form_id}"),
            message: if unexplained == 0 {
                "Written answers graded.".into()
            } else {
                format!("{unexplained} answers couldn't be graded yet; retry grading.")
            },
        })
    }

    fn failure(&self, job: &JobRecord, error: &AppError) -> JobFailure {
        crate::features::learning::curriculum_repository::lesson_failure(job, error)
    }
}
