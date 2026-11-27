use lattice_core::application::services::model_contract::ask::{
    Alone, Answered, Refusal, Says,
};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shape::{
    self, Authored, Fields, Form,
};
use lattice_core::application::services::model_contract::shown::{Internal, Picked, Showing, Told};
use lattice_core::application::services::model_contract::units::{Grain, Unit, Units};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};

use super::super::simulation_policy::{
    InterviewQuestionsPolicy, ScenarioPolicy, SimulationCue, SimulationEvent, SimulationState,
    TurnPolicy,
};
use super::{invalid, present, refusal, sentence_snapshots, Excerpt};
use crate::practical_dto::{LearningInterviewPlanDto, LearningSimulationSessionDto};

pub(super) struct SimulationTurn;
impl Contract for SimulationTurn {
    const ID: ContractId = ContractId::new("simulation-turn");
    type Input = TurnInput;
}

/// An earlier turn of the transcript and the tokens showing it takes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Said {
    pub(super) turn: Told<serde_json::Value>,
    pub(super) tokens: Internal<usize>,
}

/// What the counterpart or coach reads: the scenario, the transcript and the source passages it may cite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct TurnInput {
    pub(super) system: Told<String>,
    pub(super) context: Told<serde_json::Value>,
    /// Turns before the learner's newest, the newest first.
    pub(super) earlier: Packed<Said>,
    pub(super) earlier_total: Internal<usize>,
    /// The learner's newest turn and every turn after it, oldest first.
    pub(super) latest: Told<Vec<serde_json::Value>>,
    pub(super) latest_tokens: Internal<usize>,
    pub(super) passages: Packed<Excerpt>,
    pub(super) session: Internal<LearningSimulationSessionDto>,
    pub(super) plan: Internal<Option<LearningInterviewPlanDto>>,
    pub(super) finish: Internal<bool>,
}

/// What the model wrote for one turn.
pub(super) struct Turn {
    pub(super) content: String,
    pub(super) cited: Vec<Picked<Unit>>,
}

impl Authored for Turn {
    fn form() -> Form {
        Form::new().text("content").cite::<Unit>("citations")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            content: fields.text("content")?,
            cited: fields.cite("citations"),
        })
    }
}

/// The policy's rule on a reply, for the simulation that asks an interview plan's questions or plays a scenario.
fn check_reply(
    interview: bool,
    content: &str,
    state: &SimulationState<'_>,
    event: SimulationEvent,
) -> Result<()> {
    let policy: &dyn TurnPolicy<SimulationState<'_>, Event = SimulationEvent, Cue = SimulationCue> =
        if interview {
            &InterviewQuestionsPolicy
        } else {
            &ScenarioPolicy
        };
    let cue = policy
        .cue(&event, state)
        .ok_or_else(|| invalid("The simulation has nothing to say."))?;
    policy.check(content, &cue, state)
}

impl Answered for SimulationTurn {
    type Answer = shape::Value<Turn>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &TurnInput, showing: &mut Showing<'_>) -> Result<Says> {
        let held: Vec<&Excerpt> = input.passages.iter().collect();
        let snapshots = sentence_snapshots(showing, &held)?;
        let mut earlier: Vec<&Said> = input.earlier.iter().collect();
        earlier.reverse();
        let omitted = input.earlier_total.value().saturating_sub(earlier.len());
        let transcript: Vec<serde_json::Value> = earlier
            .into_iter()
            .map(|said| said.turn.0.clone())
            .chain(input.latest.0.iter().cloned())
            .collect();
        let mut context = input.context.0.clone();
        if let Some(fields) = context.as_object_mut() {
            fields.insert("transcript".into(), serde_json::Value::Array(transcript));
            fields.insert(
                "earlierTurns".into(),
                serde_json::json!(if omitted == 0 {
                    String::new()
                } else {
                    format!("{omitted} earlier turns not shown")
                }),
            );
            fields.insert("sourceSnapshots".into(), snapshots);
        }
        let mut says = Says::new(input.system.0.clone(), context.to_string());
        says.output_tokens = 1_500;
        Ok(says)
    }

    fn accept(input: &TurnInput, turn: &Turn) -> std::result::Result<(), Refusal> {
        present(&turn.content, "Simulation turn").map_err(refusal)?;
        let state = SimulationState {
            session: input.session.value(),
            interview_plan: input.plan.value().as_ref(),
        };
        let event = if *input.finish.value() {
            SimulationEvent::Finish
        } else {
            SimulationEvent::Turn
        };
        check_reply(input.plan.value().is_some(), &turn.content, &state, event).map_err(refusal)
    }

    fn within(input: &TurnInput, room: usize) -> Option<TurnInput> {
        let latest = *input.latest_tokens.value();
        let turns_room = if input.passages.is_empty() {
            room
        } else {
            room / 2
        };
        let earlier = input.earlier.within(turns_room.saturating_sub(latest));
        let used = earlier.iter().fold(latest, |used, said| {
            used.saturating_add(*said.tokens.value())
        });
        Some(TurnInput {
            passages: input.passages.within(room.saturating_sub(used)),
            earlier,
            ..input.clone()
        })
    }
}

/// The citations a turn's cited sentences stand for: each sentence is copied from the passage it was shown in.
pub(super) fn citations_of(
    passages: &Packed<Excerpt>,
    cited: &[Picked<Unit>],
) -> Vec<crate::dto::LearningPracticeCitationDto> {
    let evidence: Vec<(&Excerpt, Units)> = passages
        .iter()
        .map(|excerpt| (excerpt, Units::of(&excerpt.text.0, Grain::Sentence)))
        .collect();
    let mut citations = Vec::new();
    for pick in cited {
        let Some((excerpt, quote)) = evidence
            .iter()
            .find_map(|(excerpt, units)| Some((*excerpt, units.text_of(pick.reference)?)))
        else {
            continue;
        };
        if quote.trim().is_empty() {
            continue;
        }
        let citation = crate::dto::LearningPracticeCitationDto {
            source_id: excerpt.source.value().clone(),
            version_id: excerpt.version.value().clone(),
            quote: quote.to_owned(),
            number: 0,
            support: lattice_core::application::contracts::replies::SupportDto::verify_quote(
                Some(&excerpt.text.0),
                quote,
            ),
        };
        if !citations.contains(&citation) {
            citations.push(citation);
        }
    }
    for (index, citation) in citations.iter_mut().enumerate() {
        citation.number = (index + 1) as u32;
    }
    citations
}
