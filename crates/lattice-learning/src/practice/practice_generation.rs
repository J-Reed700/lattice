//! The grounded practice tutor, reference solution and grader.
use crate::dto::*;
use crate::lessons::structured_call::Asking;
use crate::reference_collection::{item_tokens, ReferenceCollection};
use lattice_core::{
    application::{
        ports::LLMPort,
        services::model_contract::{
            ask::{self, ask, ask_each, Alone, Answered, Refusal, Says, Settled},
            packed::Packed,
            shape::{self, Authored, Each, Fields, Form, Vocabulary},
            shown::{Drawn, Internal, Kind, Picked, Scope, Showing, Told},
            units::{Grain, Quoted, Texts, Unit, Units},
            Contract, ContractId,
        },
    },
    shared::error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The task of a lesson with no written exercise of its own: its objective, and what the learner said they are
/// preparing for when they said anything.
pub fn fallback_task_prompt(objective: &str, details: Option<&str>) -> String {
    let task = format!(
        "{}\n\nCreate a short written artifact that demonstrates your reasoning. State what you would do and why; use a new example where useful.",
        objective.trim()
    );
    match details.map(str::trim).filter(|details| !details.is_empty()) {
        Some(details) => format!("{task}\n\nYou are preparing for: {details}"),
        None => task,
    }
}

fn invalid(message: &str) -> AppError {
    AppError::InvalidInput(message.into())
}

/// What a criterion's result says when the grader never judged it.
pub const NOT_JUDGED: &str =
    "Evidence could not be graded reliably; review this response with a person.";

pub(crate) struct TutorReply;

impl Contract for TutorReply {
    const ID: ContractId = ContractId::new("practice-tutor-reply");
    type Input = TutorInput;
}

pub(crate) struct ReferenceSolution;

impl Contract for ReferenceSolution {
    const ID: ContractId = ContractId::new("practice-reference-solution");
    type Input = SolutionInput;
}

pub(crate) struct PracticeGrade;

impl Contract for PracticeGrade {
    const ID: ContractId = ContractId::new("practice-grade");
    type Input = GradeInput;
}

#[derive(Debug)]
pub struct TutorJson {
    pub response: String,
    pub citations: Vec<LearningPracticeCitationDto>,
    pub proposals: Vec<ProposalJson>,
}

#[derive(Debug)]
pub struct ProposalJson {
    pub kind: LearningPracticeProposalKind,
    pub text: String,
    pub evidence_quote: Option<String>,
}

#[derive(Debug)]
pub struct SolutionJson {
    pub solution: String,
    pub citations: Vec<LearningPracticeCitationDto>,
}

const TUTOR_SYSTEM: &str = "You are a subject-neutral learning tutor. Do not write or replace the learner's artifact. Keep feedback actionable and tied to the visible rubric. Respond to the learner's actual work and previous tutor turns. Prefer making the learner retrieve or explain before telling: ask what they remember about the relevant idea, or why their last step works. For an orienting hint ask one question about where to start; for a concept hint point to the relevant idea; for a partial strategy give only the next step. Never give the final answer in Practice mode, hints or critiques. A critique should identify the first specific reasoning gap, name the misconception behind it when one shows, and ask the learner to revise it. Acknowledge correct steps without praising unsupported conclusions. Cite only sentences of the supplied frozen snapshots, each by the id in square brackets before it. If none are supplied, tutor from the task and general knowledge, acknowledge uncertainty, and return empty citations. Never claim mastery.";
const TUTOR_FORM: &str = " Write response, then citations, the ids of the snapshot sentences you cite, then proposals; each proposal has a kind, its text, and evidence, the ids of the sentences of the learner's artifact it rests on, empty when it rests on none.";
const SOLUTION_SYSTEM: &str = "Give a concise worked reference solution for the learning task. Show each step with the reason it is taken, name the mistake learners most often make at the hardest step and why it fails, and end with one short variation of the task for the learner to try from memory, without its answer. When source snapshots are supplied, use them for factual claims. Without sources, use the stated task and general knowledge, acknowledge uncertainty, and return no citations. Cite snapshot sentences by the id in square brackets before each, and do not imply that viewing this solution is evidence of learner ability.";
const SOLUTION_FORM: &str =
    " Write solution, then citations, the ids of the snapshot sentences you cite.";
const GRADE_SYSTEM: &str = "Evaluate the learner artifact against each visible criterion. Scores are limited, provisional evidence, not mastery claims. For each criterion, identify what the learner did, the specific gap if any, and one actionable next revision. Each evidence cites the sentences of the submitted artifact that show what you observed, by the ids in square brackets before them; cite only sentences shown. Do not reward length or confident wording. A correct answer with flawed reasoning must lose reasoning credit. Give an observation tied to artifact text. Use none, never a score of zero, when there is no assessable work for a criterion. Zero means an observed incorrect attempt. Use none for every criterion when the artifact lacks enough evidence to judge. Return an entries array with one entry for each criterion in rubric, and set each entry's of to that criterion's id. In each entry write evidence, then observation, then score: the id of one of that criterion's scores, or none.";

/// A passage of a frozen source snapshot that a search found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Passage {
    pub(crate) at: usize,
    pub(crate) source_id: Internal<String>,
    pub(crate) version_id: Internal<String>,
    pub(crate) title: Told<String>,
    pub(crate) text: Told<String>,
}

/// A tutor turn as the model is shown it, with the tokens showing it takes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Turn {
    pub(crate) entry: Told<serde_json::Value>,
    pub(crate) tokens: usize,
}

/// What a tutor request says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TutorInput {
    pub(crate) task_prompt: Told<String>,
    pub(crate) objective: Told<String>,
    pub(crate) artifact: Told<String>,
    pub(crate) rubric: Told<serde_json::Value>,
    pub(crate) mode: Told<serde_json::Value>,
    pub(crate) revision_context: Told<Vec<serde_json::Value>>,
    pub(crate) request_kind: Told<serde_json::Value>,
    pub(crate) hint_level: Told<serde_json::Value>,
    pub(crate) learner_request: Told<String>,
    pub(crate) turns: Packed<Turn>,
    pub(crate) turns_held: usize,
    pub(crate) passages: Packed<Passage>,
}

/// What a reference-solution request says.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SolutionInput {
    pub(crate) task_prompt: Told<String>,
    pub(crate) objective: Told<String>,
    pub(crate) passages: Packed<Passage>,
}

/// What a tutor proposal is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ProposalWord {
    Misconception,
    FollowUp,
}

impl Vocabulary for ProposalWord {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("misconception", Self::Misconception),
        ("follow_up", Self::FollowUp),
    ];
}

/// One proposal the tutor made, with the sentences of the learner's artifact it rests on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Proposal {
    kind: ProposalWord,
    text: String,
    evidence: Vec<Picked<Unit>>,
}

impl Authored for Proposal {
    fn form() -> Form {
        Form::new()
            .word::<ProposalWord>("kind")
            .text("text")
            .cite::<Unit>("evidence")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            kind: fields.word("kind")?,
            text: fields.text("text")?,
            evidence: fields.cite("evidence"),
        })
    }
}

/// The tutor's reply.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TutorSays {
    response: String,
    citations: Vec<Picked<Quoted>>,
    proposals: Vec<Proposal>,
}

impl Authored for TutorSays {
    fn form() -> Form {
        Form::new()
            .text("response")
            .cite::<Quoted>("citations")
            .list::<Proposal>("proposals")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            response: fields.text("response")?,
            citations: fields.cite("citations"),
            proposals: fields.list("proposals")?,
        })
    }
}

/// The worked solution.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SolutionSays {
    solution: String,
    citations: Vec<Picked<Quoted>>,
}

impl Authored for SolutionSays {
    fn form() -> Form {
        Form::new().text("solution").cite::<Quoted>("citations")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            solution: fields.text("solution")?,
            citations: fields.cite("citations"),
        })
    }
}

/// The passages as texts of sentences, each a text of its place in the search.
fn passage_texts<'a>(passages: impl Iterator<Item = &'a Passage>) -> Texts<usize> {
    let mut texts = Texts::new();
    for passage in passages {
        texts.push(
            passage.at,
            &passage.at.to_string(),
            &passage.text.0,
            Grain::Sentence,
        );
    }
    texts
}

/// The snapshots as the request shows them: per source version, its passages' sentences as rows.
fn snapshots(passages: &Packed<Passage>, showing: &mut Showing<'_>) -> Result<serde_json::Value> {
    let texts = passage_texts(passages.iter());
    let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
    for (passage, (_, units)) in passages.iter().zip(texts.iter()) {
        let quotable = units.quoted();
        let shown = showing.show(&quotable)?;
        let rows = units.rows(&shown);
        let version = passage.version_id.value();
        match groups.iter_mut().find(|(held, _, _)| held == version) {
            Some((_, _, rows_of)) => rows_of.push(rows),
            None => groups.push((version.clone(), passage.title.0.clone(), vec![rows])),
        }
    }
    Ok(serde_json::Value::Array(
        groups
            .into_iter()
            .map(|(_, title, rows)| json!({"title": title, "passages": rows}))
            .collect(),
    ))
}

impl Answered for TutorReply {
    type Answer = shape::Value<TutorSays>;
    const ALONE: Alone = Alone::Fails;

    fn within(input: &TutorInput, room: usize) -> Option<TutorInput> {
        let turns = input.turns.within(room / 2);
        let used: usize = turns.iter().map(|turn| turn.tokens).sum();
        Some(TutorInput {
            turns,
            passages: input.passages.within(room.saturating_sub(used)),
            ..input.clone()
        })
    }

    fn says(input: &TutorInput, showing: &mut Showing<'_>) -> Result<Says> {
        let units = Units::of(&input.artifact.0, Grain::Sentence);
        let artifact = units.shown(showing)?;
        let snapshots = snapshots(&input.passages, showing)?;
        let turns: Vec<serde_json::Value> = input
            .turns
            .iter()
            .map(|turn| turn.entry.0.clone())
            .collect();
        let earlier = input.turns_held.saturating_sub(turns.len());
        let earlier_note = if earlier == 0 {
            String::new()
        } else {
            format!("{earlier} earlier turns not shown")
        };
        let task = json!({"taskPrompt":input.task_prompt,"objective":input.objective,"learnerArtifact":units.rows(&artifact),"rubric":input.rubric,"mode":input.mode,"recentTurns":turns,"earlierTurns":earlier_note,"revisionContext":input.revision_context,"requestKind":input.request_kind,"hintLevel":input.hint_level,"learnerRequest":input.learner_request,"sourceSnapshots":snapshots});
        let mut says = Says::new(format!("{TUTOR_SYSTEM}{TUTOR_FORM}"), task.to_string());
        says.output_tokens = 1400;
        Ok(says)
    }

    fn accept(_input: &TutorInput, said: &TutorSays) -> std::result::Result<(), Refusal> {
        if said.response.trim().is_empty() {
            return Err(Refusal("Write the tutor's reply in response.".into()));
        }
        if said.proposals.iter().any(|p| p.text.trim().is_empty()) {
            return Err(Refusal("Write the text of every proposal.".into()));
        }
        Ok(())
    }
}

impl Answered for ReferenceSolution {
    type Answer = shape::Value<SolutionSays>;
    const ALONE: Alone = Alone::Fails;

    fn within(input: &SolutionInput, room: usize) -> Option<SolutionInput> {
        Some(SolutionInput {
            passages: input.passages.within(room),
            ..input.clone()
        })
    }

    fn says(input: &SolutionInput, showing: &mut Showing<'_>) -> Result<Says> {
        let snapshots = snapshots(&input.passages, showing)?;
        let task = json!({"taskPrompt":input.task_prompt,"objective":input.objective,"sourceSnapshots":snapshots});
        let mut says = Says::new(
            format!("{SOLUTION_SYSTEM}{SOLUTION_FORM}"),
            task.to_string(),
        );
        says.output_tokens = 1400;
        Ok(says)
    }

    fn accept(_input: &SolutionInput, said: &SolutionSays) -> std::result::Result<(), Refusal> {
        if said.solution.trim().is_empty() {
            return Err(Refusal("Write the worked solution in solution.".into()));
        }
        Ok(())
    }
}

/// The passages of the frozen snapshots that `query` retrieves, best first, while the first `room` tokens hold them all.
async fn found_passages(
    llm: &dyn LLMPort,
    sources: &[LearningSourceVersionDto],
    query: &str,
    room: usize,
) -> Result<Vec<Passage>> {
    let documents: Vec<LearningSourceDto> = sources
        .iter()
        .map(|source| LearningSourceDto {
            id: source.version.id.clone(),
            title: source.version.title.clone(),
            url: None,
            excerpt: source.full_text.clone(),
            acquired_at: 0,
        })
        .collect();
    let collection = ReferenceCollection::lexical(&documents)?;
    let found = collection
        .retrieve_within(query, |passages| {
            passages
                .iter()
                .map(|passage| item_tokens(llm, &json!(passage.text)))
                .sum::<usize>()
                <= room
        })
        .await?;
    Ok(found
        .into_iter()
        .enumerate()
        .filter_map(|(at, passage)| {
            let source = sources
                .iter()
                .find(|source| source.version.id == passage.source_id)?;
            Some(Passage {
                at,
                source_id: Internal::of(source.source_id.clone()),
                version_id: Internal::of(source.version.id.clone()),
                title: Told(source.version.title.clone()),
                text: Told(passage.text),
            })
        })
        .collect())
}

/// The passages as material a request packs, the best first.
fn packed(llm: &dyn LLMPort, passages: &[Passage]) -> Packed<Passage> {
    Packed::new(passages.to_vec(), |passage| {
        item_tokens(llm, &json!(passage.text))
    })
}

/// The tutor turns, the newest first, as material a request packs.
fn recent(llm: &dyn LLMPort, turns: &[LearningPracticeTutorTurnDto]) -> Packed<Turn> {
    let shown = turns
        .iter()
        .rev()
        .map(|turn| {
            let entry =
                json!({"request":turn.prompt,"response":turn.response,"hintLevel":turn.hint_level});
            let tokens = llm.count_tokens(&entry.to_string()) + 1;
            Turn {
                entry: Told(entry),
                tokens,
            }
        })
        .collect();
    Packed::new(shown, |turn| turn.tokens)
}

/// The citations the cited sentences make, each with the source version its passage is of.
fn citations(passages: &[Passage], cited: &[Picked<Quoted>]) -> Vec<LearningPracticeCitationDto> {
    let texts = passage_texts(passages.iter());
    cited
        .iter()
        .filter_map(|picked| {
            let (at, quote) = texts.found(picked.reference)?;
            let passage = passages.iter().find(|passage| passage.at == *at)?;
            Some(LearningPracticeCitationDto {
                source_id: passage.source_id.value().clone(),
                version_id: passage.version_id.value().clone(),
                quote: quote.to_owned(),
            })
        })
        .collect()
}

/// The words of the cited sentences of the learner's artifact, or none when it cites none.
fn quoted(units: &Units, evidence: &[Picked<Unit>]) -> Option<String> {
    let words: Vec<&str> = evidence
        .iter()
        .filter_map(|picked| units.text_of(picked.reference))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn asking() -> Asking<'static> {
    Asking {
        progress: None,
        reasoning: None,
    }
}

fn unusable(what: &str) -> AppError {
    AppError::ModelAnswerUnusable(format!("The {what} could not be used."))
}

pub async fn tutor(
    llm: &dyn LLMPort,
    session: &LearningPracticeSessionDto,
    request: &RequestLearningTutorResponseRequestDto,
    sources: &[LearningSourceVersionDto],
) -> Result<TutorJson> {
    let call = crate::model_call::answer_options();
    let revision_context = session
        .assistance
        .iter()
        .filter(|a| a.details.get("revisesSessionId").is_some())
        .map(|a| a.details.clone())
        .collect();
    let mut input = TutorInput {
        task_prompt: Told(session.task_prompt.clone()),
        objective: Told(session.lesson_objective.clone()),
        artifact: Told(session.artifact.text.clone()),
        rubric: Told(json!(session.rubric)),
        mode: Told(json!(session.summary.mode)),
        revision_context: Told(revision_context),
        request_kind: Told(json!(request.request_kind)),
        hint_level: Told(json!(request.hint_level)),
        learner_request: Told(request.prompt.clone()),
        turns: recent(llm, &session.tutor_turns),
        turns_held: session.tutor_turns.len(),
        passages: Packed::default(),
    };
    let room = ask::room::<TutorReply>(
        llm,
        &TutorInput {
            turns: Packed::default(),
            ..input.clone()
        },
        &call,
    )?;
    let query = format!("{} {}", session.task_prompt, request.prompt);
    let found = found_passages(llm, sources, &query, room).await?;
    input.passages = packed(llm, &found);
    let settled =
        ask::<TutorReply>(llm, &asking(), &mut Scope::for_question(), &input, &call).await?;
    let Settled::Answered(said) = settled else {
        return Err(unusable("tutor's answer"));
    };
    let units = Units::of(&session.artifact.text, Grain::Sentence);
    Ok(TutorJson {
        response: said.response,
        citations: citations(&found, &said.citations),
        proposals: said
            .proposals
            .into_iter()
            .map(|proposal| ProposalJson {
                kind: match proposal.kind {
                    ProposalWord::Misconception => LearningPracticeProposalKind::Misconception,
                    ProposalWord::FollowUp => LearningPracticeProposalKind::FollowUp,
                },
                text: proposal.text,
                evidence_quote: quoted(&units, &proposal.evidence),
            })
            .collect(),
    })
}

/// A worked solution, its snapshots sized from the model's window as the
/// tutor's are.
pub async fn solution(
    llm: &dyn LLMPort,
    session: &LearningPracticeSessionDto,
    sources: &[LearningSourceVersionDto],
) -> Result<SolutionJson> {
    let call = crate::model_call::answer_options();
    let mut input = SolutionInput {
        task_prompt: Told(session.task_prompt.clone()),
        objective: Told(session.lesson_objective.clone()),
        passages: Packed::default(),
    };
    let room = ask::room::<ReferenceSolution>(llm, &input, &call)?;
    let query = format!("{} {}", session.task_prompt, session.lesson_objective);
    let found = found_passages(llm, sources, &query, room).await?;
    input.passages = packed(llm, &found);
    let settled =
        ask::<ReferenceSolution>(llm, &asking(), &mut Scope::for_question(), &input, &call).await?;
    let Settled::Answered(said) = settled else {
        return Err(unusable("reference solution"));
    };
    Ok(SolutionJson {
        solution: said.solution,
        citations: citations(&found, &said.citations),
    })
}

pub fn validate_citations(
    citations: &[LearningPracticeCitationDto],
    sources: &[LearningSourceVersionDto],
) -> Result<()> {
    for citation in citations {
        if citation.quote.trim().is_empty() {
            return Err(invalid("Citation quote must not be blank."));
        }
        let source = sources
            .iter()
            .find(|s| s.source_id == citation.source_id && s.version.id == citation.version_id)
            .ok_or_else(|| invalid("Citation refers to a source version outside this attempt."))?;
        if !source.full_text.contains(&citation.quote) {
            return Err(invalid(
                "Citation quote must occur exactly in the saved source version.",
            ));
        }
    }
    Ok(())
}

/// A criterion of the rubric: what a grade entry is about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Criterion {
    pub(crate) criterion: LearningPracticeCriterionDto,
}

impl Kind for Criterion {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.criterion.id.clone()
    }
}

/// A score a criterion may be given: its own points, from zero to its own maximum.
#[derive(Debug, Clone)]
pub(crate) struct Level {
    points: i64,
}

impl Kind for Level {
    const PREFIX: &'static str = "l";
    const DRAWN: Drawn = Drawn::Number;
    type Ref = i64;

    fn reference(&self) -> i64 {
        self.points
    }
}

/// What a grade request says: the criteria still to grade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GradeInput {
    pub(crate) task_prompt: Told<String>,
    pub(crate) artifact: Told<String>,
    pub(crate) criteria: Vec<Criterion>,
}

/// What the grader decided for one criterion: the sentences it rests on, what it saw, and the score, none when it cannot tell.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Verdict {
    evidence: Vec<Picked<Unit>>,
    observation: String,
    score: Option<Picked<Level>>,
}

impl Authored for Verdict {
    fn form() -> Form {
        Form::new()
            .cite::<Unit>("evidence")
            .text("observation")
            .own_or_none::<Level>("score")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            evidence: fields.cite("evidence"),
            observation: fields.text("observation")?,
            score: fields.own_or_none("score")?,
        })
    }
}

impl Answered for PracticeGrade {
    type Answer = Each<Criterion, Verdict>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &GradeInput, showing: &mut Showing<'_>) -> Result<Says> {
        let units = Units::of(&input.artifact.0, Grain::Sentence);
        let artifact = units.shown(showing)?;
        let criteria = showing.show(&input.criteria)?;
        let mut rubric = Vec::new();
        for (handle, item) in criteria.iter() {
            let c = &item.criterion;
            let levels: Vec<Level> = (0..=c.max_points).map(|points| Level { points }).collect();
            let shown = showing.under(handle, &levels)?;
            let scores: Vec<serde_json::Value> = shown
                .iter()
                .map(|(level, held)| json!({"id": level.written(), "points": held.points}))
                .collect();
            rubric.push(json!({"id":handle.written(),"dimension":c.dimension,"title":c.title,"description":c.description,"maxPoints":c.max_points,"scores":scores}));
        }
        let task = json!({"taskPrompt":input.task_prompt,"artifact":units.rows(&artifact),"rubric":rubric});
        let mut says = Says::new(GRADE_SYSTEM, task.to_string());
        says.output_tokens = input.criteria.len().saturating_mul(500).saturating_add(800);
        Ok(says)
    }

    fn accept(_input: &GradeInput, verdict: &Verdict) -> std::result::Result<(), Refusal> {
        if verdict.observation.trim().is_empty() {
            return Err(Refusal(
                "Give the observation for this criterion in observation.".into(),
            ));
        }
        if verdict.score.is_some() && verdict.evidence.is_empty() {
            return Err(Refusal(
                "A criterion with a score cites at least one sentence of the artifact in evidence. Naming text in observation does not cite it.".into(),
            ));
        }
        Ok(())
    }
}

pub async fn grade(
    llm: &dyn LLMPort,
    session: &LearningPracticeSessionDto,
) -> Result<(
    LearningPracticeGradeStatus,
    Vec<LearningPracticeCriterionResultDto>,
)> {
    let call = crate::model_call::answer_options();
    let criteria: Vec<Criterion> = session
        .rubric
        .iter()
        .map(|criterion| Criterion {
            criterion: criterion.clone(),
        })
        .collect();
    let task_prompt = Told(session.task_prompt.clone());
    let artifact = Told(session.artifact.text.clone());
    let settled = ask_each::<PracticeGrade>(
        llm,
        &asking(),
        &mut Scope::for_question(),
        &criteria,
        |open, _| GradeInput {
            task_prompt: task_prompt.clone(),
            artifact: artifact.clone(),
            criteria: open.iter().map(|item| (*item).clone()).collect(),
        },
        &call,
    )
    .await?;
    let units = Units::of(&session.artifact.text, Grain::Sentence);
    let results: Vec<LearningPracticeCriterionResultDto> = criteria
        .iter()
        .zip(settled)
        .map(|(item, settled)| {
            let c = &item.criterion;
            let (score, observation, evidence_quote) = match settled {
                Settled::Answered(verdict) => (
                    verdict.score.map(|picked| picked.reference),
                    verdict.observation,
                    quoted(&units, &verdict.evidence),
                ),
                Settled::Unanswered(_) => (None, NOT_JUDGED.to_owned(), None),
            };
            LearningPracticeCriterionResultDto {
                criterion_id: c.id.clone(),
                dimension: c.dimension.clone(),
                score,
                max_points: c.max_points,
                observation,
                evidence_quote,
            }
        })
        .collect();
    let status = if results.iter().any(|result| result.score.is_some()) {
        LearningPracticeGradeStatus::Provisional
    } else {
        LearningPracticeGradeStatus::Uncertain
    };
    Ok((status, results))
}
