//! A closing coach selects evidence; the app owns source IDs and exact quotes.
use super::{bounded, invalid, SimulationJson};
use crate::features::learning::{
    assessment_engine::LearningRubricCriterion,
    dto::LearningPracticeCitationDto,
    practical_dto::{
        LearningInterviewPlanDto, LearningSimulationSessionDto, LearningSimulationSpeaker,
    },
    practice::interview_answers::InterviewAnswerGuides,
};
use lattice_core::shared::error::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

struct Choice {
    id: String,
    question: String,
    citation: LearningPracticeCitationDto,
}

pub(super) struct ClosingEvidence {
    questions: Vec<(String, String)>,
    choices: Vec<Choice>,
    plan: Value,
    guides: Value,
    criteria: Vec<LearningRubricCriterion>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Response {
    reference_approach: String,
    next_practice_step: String,
    scorecard: Vec<Observation>,
    evaluated_question_ids: Vec<String>,
    citation_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Observed,
    InsufficientEvidence,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Observation {
    criterion_id: String,
    status: Status,
    points: Option<f64>,
    learner_quotes: Vec<String>,
    feedback: String,
}

pub(super) const CONTRACT: &str = "Return the structured closing scorecard, referenceApproach, nextPracticeStep, evaluatedQuestionIds and citationIds. Use the supplied short question, criterion and evidence labels only in their structured fields. Read each complete learner turn, including all sentences, before assessing it. In each scorecard row, observed requires exact learnerQuotes and provisional points; insufficient_evidence requires null points and no learnerQuotes. Include each rubric criterion exactly once. Do not assign zero to an unobserved skill. When an answered question's saved answerGuides fullCredit scoringCriteria are satisfied (for example, explaining how deduplication preserves the intended effect AND how to verify a retry after a lost response/acknowledgement), award full maxPoints on the observed rubric criterion rather than deducting points for not repeating generic rubric phrasing. Quote the learner's own words across all substantive sentences of their turn (including both their mechanism/reasoning sentence and their verification/retry test sentence), never the interviewer, a reference answer or a paraphrase. Credit verification the learner actually describes, including dropping an acknowledgement or simulating a failure, retrying and checking that only one effect was committed; discuss that verification explicitly in feedback and ask for further detail only in nextPracticeStep without saying no verification was given. Use referenceApproach only for a concise reference approach to answered questions, and nextPracticeStep for an actionable next exercise; put learner assessment and all scores only in scorecard. Never write internal labels or numeric grades in the narrative fields. Select at least one citationId belonging to each sourced evaluated question from answerGuideEvidence; the app supplies its saved source and exact quote. Do not write citation objects or invent evidence.";

impl ClosingEvidence {
    pub(super) fn new(
        plan: &LearningInterviewPlanDto,
        guides: &InterviewAnswerGuides,
        criteria: &[LearningRubricCriterion],
    ) -> Result<Self> {
        guides.validate_plan(plan)?;
        let mut questions = Vec::new();
        let mut choices: Vec<Choice> = Vec::new();
        let mut public_questions = Vec::new();
        let mut standards = Vec::new();
        for (index, question) in plan.questions.iter().enumerate() {
            let label = format!("q{}", index + 1);
            questions.push((label.clone(), question.id.clone()));
            public_questions.push(json!({
                "id":label,"round":question.round,"question":question.question,
            }));
            let guide = guides
                .questions
                .iter()
                .find(|guide| guide.question_id == question.id)
                .ok_or_else(|| invalid("The closing standard has no matching question."))?;
            let points = guide
                .standard
                .key_points
                .iter()
                .map(|point| {
                    let ids = point
                        .citations
                        .iter()
                        .map(|citation| {
                            if let Some(existing) = choices.iter().find(|choice| {
                                choice.question == label
                                    && same_citation(&choice.citation, citation)
                            }) {
                                return existing.id.clone();
                            }
                            let id = format!("e{}", choices.len() + 1);
                            choices.push(Choice {
                                id: id.clone(),
                                question: label.clone(),
                                citation: citation.clone(),
                            });
                            id
                        })
                        .collect::<Vec<_>>();
                    json!({"point":point.point,"evidence":point.evidence,"citationIds":ids})
                })
                .collect::<Vec<_>>();
            standards.push(json!({
                "questionId":label,"question":guide.question,
                "standard":{
                    "referenceAnswer":guide.standard.reference_answer,"keyPoints":points,
                    "acceptableAlternatives":guide.standard.acceptable_alternatives,
                    "commonMistakes":guide.standard.common_mistakes,
                    "scoringCriteria":guide.standard.scoring_criteria,
                    "evidenceGaps":guide.standard.evidence_gaps,
                }
            }));
        }
        Ok(Self {
            questions,
            choices,
            criteria: criteria.to_vec(),
            plan: json!({
                "companyName":plan.company_name,"companyUrl":plan.company_url,
                "targetRole":plan.target_role,"roundKind":plan.round_kind,
                "companySummary":plan.company_summary,"questions":public_questions,
            }),
            guides: json!({"questions":standards}),
        })
    }

    pub(super) fn add_to_prompt(&self, prompt: &mut Value) -> Result<()> {
        let fields = prompt
            .as_object_mut()
            .ok_or_else(|| invalid("Simulation prompt must be an object."))?;
        fields.insert("interviewPlan".into(), self.plan.clone());
        fields.insert("answerGuides".into(), self.guides.clone());
        fields.insert("rubric".into(), json!(self.criteria.iter().enumerate().map(|(index, criterion)| {
            json!({"id":format!("c{}", index+1),"title":criterion.title,"description":criterion.description,"maxPoints":criterion.max_points})
        }).collect::<Vec<_>>()));
        fields.insert(
            "answerGuideEvidence".into(),
            json!(self.choices.iter().map(|choice| {
            json!({"id":choice.id,"questionId":choice.question,"quote":choice.citation.quote})
        }).collect::<Vec<_>>()),
        );
        Ok(())
    }

    pub(super) fn schema(&self) -> Value {
        let evidence = if self.choices.is_empty() {
            json!({"type":"string"})
        } else {
            json!({"type":"string","enum":self.choices.iter().map(|choice| &choice.id).collect::<Vec<_>>()})
        };
        let criterion = if self.criteria.is_empty() {
            json!({"type":"string"})
        } else {
            json!({"type":"string","enum":(1..=self.criteria.len()).map(|index| format!("c{index}")).collect::<Vec<_>>()})
        };
        json!({
            "type":"object","additionalProperties":false,
            "required":["referenceApproach","nextPracticeStep","scorecard","evaluatedQuestionIds","citationIds"],
            "properties":{
                "referenceApproach":{"type":"string","minLength":1},
                "nextPracticeStep":{"type":"string","minLength":1},
                "scorecard":{"type":"array","minItems":self.criteria.len(),"maxItems":self.criteria.len(),"items":{
                    "type":"object","additionalProperties":false,
                    "required":["criterionId","status","points","learnerQuotes","feedback"],
                    "properties":{
                        "criterionId":criterion,"status":{"enum":["observed","insufficient_evidence"]},
                        "points":{"type":["number","null"],"minimum":0},
                        "learnerQuotes":{"type":"array","maxItems":4,"items":{"type":"string","minLength":1}},
                        "feedback":{"type":"string","minLength":1}
                    }
                }},
                "evaluatedQuestionIds":{"type":"array","maxItems":6,"uniqueItems":true,
                    "items":{"type":"string","enum":self.questions.iter().map(|(label,_)| label).collect::<Vec<_>>()}},
                "citationIds":{"type":"array","maxItems":if self.choices.is_empty() {0} else {12},
                    "uniqueItems":true,"items":evidence}
            }
        })
    }

    pub(super) fn read(
        &self,
        raw: &str,
        session: &LearningSimulationSessionDto,
    ) -> Result<SimulationJson> {
        let response: Response = serde_json::from_str(raw)
            .map_err(|_| invalid("Return the structured closing scorecard, referenceApproach, nextPracticeStep, evaluatedQuestionIds and citationIds."))?;
        bounded(&response.reference_approach, "Reference approach", 4_000)?;
        bounded(&response.next_practice_step, "Next practice step", 1_200)?;
        if response.evaluated_question_ids.len() > 6 || response.citation_ids.len() > 12 {
            return Err(invalid(
                "Closing feedback allows at most six questions and twelve evidence choices.",
            ));
        }
        let mut evaluated = HashSet::new();
        let mut question_ids = Vec::new();
        for label in &response.evaluated_question_ids {
            if !evaluated.insert(label.as_str()) {
                return Err(invalid(
                    "Interview feedback repeated an evaluated question.",
                ));
            }
            let (_, id) = self
                .questions
                .iter()
                .find(|(known, _)| known == label)
                .ok_or_else(|| {
                    invalid("Select evaluatedQuestionIds only from the supplied question labels.")
                })?;
            question_ids.push(id.clone());
        }
        let mut cited = HashSet::new();
        let mut supported = HashSet::new();
        let mut citations = Vec::new();
        for id in &response.citation_ids {
            if !cited.insert(id.as_str()) {
                return Err(invalid("Interview feedback repeated an evidence choice."));
            }
            let choice = self
                .choices
                .iter()
                .find(|choice| &choice.id == id)
                .ok_or_else(|| {
                    invalid(
                        "Select citationIds only from the supplied answerGuideEvidence choices.",
                    )
                })?;
            if !evaluated.contains(choice.question.as_str()) {
                return Err(invalid(
                    "Each citationId must belong to a question in evaluatedQuestionIds.",
                ));
            }
            supported.insert(choice.question.as_str());
            if !citations
                .iter()
                .any(|citation| same_citation(citation, &choice.citation))
            {
                citations.push(choice.citation.clone());
            }
        }
        for label in evaluated {
            if self.choices.iter().any(|choice| choice.question == label)
                && !supported.contains(label)
            {
                return Err(invalid("Include at least one citationId from each sourced evaluated question's answerGuideEvidence."));
            }
        }
        let content = self.render_scorecard(&response, session)?;
        Ok(SimulationJson {
            content,
            citations,
            evaluated_question_ids: Some(question_ids),
        })
    }

    fn render_scorecard(
        &self,
        response: &Response,
        session: &LearningSimulationSessionDto,
    ) -> Result<String> {
        if response.scorecard.len() != self.criteria.len() {
            return Err(invalid(
                "Include each supplied rubric criterion exactly once in scorecard.",
            ));
        }
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        for observation in &response.scorecard {
            let (index, criterion) = self
                .criteria
                .iter()
                .enumerate()
                .find(|(index, _)| observation.criterion_id == format!("c{}", index + 1))
                .ok_or_else(|| {
                    invalid("Scorecard criterionId must use a supplied rubric label.")
                })?;
            if !seen.insert(index) {
                return Err(invalid("Scorecard repeated a criterion."));
            }
            bounded(&observation.feedback, "Criterion feedback", 1_200)?;
            if observation.learner_quotes.len() > 4 {
                return Err(invalid(
                    "Each scorecard criterion allows at most four learnerQuotes.",
                ));
            }
            for quote in &observation.learner_quotes {
                bounded(quote, "Learner evidence quote", 700)?;
                if !session.turns.iter().any(|turn| {
                    turn.speaker == LearningSimulationSpeaker::Learner
                        && turn.content.contains(quote)
                }) {
                    return Err(invalid("Each learnerQuote must exactly match text in a complete learner turn, not the interviewer or reference answer."));
                }
            }
            let (heading, feedback) = match observation.status {
                Status::InsufficientEvidence => {
                    if observation.points.is_some() || !observation.learner_quotes.is_empty() {
                        return Err(invalid("insufficient_evidence requires null points and empty learnerQuotes; never assign zero to an unobserved skill."));
                    }
                    (format!(
                        "**{}: insufficient evidence; no points assigned**",
                        criterion.title
                    ), "This attempt does not contain enough learner evidence to assess this criterion.")
                }
                Status::Observed => {
                    let points = observation
                        .points
                        .filter(|points| {
                            points.is_finite()
                                && *points >= 0.0
                                && *points <= f64::from(criterion.max_points)
                        })
                        .ok_or_else(|| {
                            invalid(
                                "Observed criteria need provisional points within their maxPoints.",
                            )
                        })?;
                    if observation.learner_quotes.is_empty()
                        || response.evaluated_question_ids.is_empty()
                    {
                        return Err(invalid("Observed criteria require exact learnerQuotes and a substantively answered question."));
                    }
                    (
                        format!(
                            "**{}: {points}/{} (provisional)**",
                            criterion.title, criterion.max_points
                        ),
                        observation.feedback.as_str(),
                    )
                }
            };
            let quotes = observation
                .learner_quotes
                .iter()
                .map(|quote| format!("Learner evidence: “{quote}”"))
                .collect::<Vec<_>>()
                .join("\n\n");
            rows.push(format!(
                "{heading}\n\n{}{}{}",
                feedback,
                if quotes.is_empty() { "" } else { "\n\n" },
                quotes
            ));
        }
        rows.push(format!(
            "**Reference approach**\n\n{}",
            response.reference_approach
        ));
        let mut gaps = Vec::new();
        for guide in self
            .guides
            .get("questions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if !response.evaluated_question_ids.iter().any(|label| {
                guide.get("questionId").and_then(Value::as_str) == Some(label.as_str())
            }) {
                continue;
            }
            for gap in guide
                .get("standard")
                .and_then(|standard| standard.get("evidenceGaps"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !gaps.contains(&gap) {
                    gaps.push(gap);
                }
            }
        }
        if !gaps.is_empty() {
            rows.push(format!("**Source limits**\n\n{}", gaps.join("\n\n")));
        }
        rows.push(format!(
            "**Next practice step**\n\n{}",
            response.next_practice_step
        ));
        Ok(rows.join("\n\n"))
    }
}

fn same_citation(left: &LearningPracticeCitationDto, right: &LearningPracticeCitationDto) -> bool {
    left.source_id == right.source_id
        && left.version_id == right.version_id
        && left.quote == right.quote
}

#[cfg(test)]
#[path = "interview_feedback_tests.rs"]
mod tests;
