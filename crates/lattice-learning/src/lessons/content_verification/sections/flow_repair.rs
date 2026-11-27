//! Bounded flow repair after claim cuts, including gap extraction and
//! orphaned half-contrast cleanup.
use super::super::*;
use super::{Gap, Taken};
use crate::generation::teaching_sentences;
use crate::lessons::copied_wording::Wording;
use crate::lessons::rows::{self, words, Held, Left, Quotes, Rows};
use crate::lessons::structured_call::{Asking, CallKind, ReasonFirst};
use crate::lessons::trace::{self, Stage};
use crate::review_findings::FindingSite;
use lattice_core::application::services::model_contract::ask::{
    self, ask_each, Alone, Answered, Refusal, Says,
};
use lattice_core::application::services::model_contract::edit::Anchor;
use lattice_core::application::services::model_contract::shape::{Authored, Each, Fields, Form};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};

/// What the flow repair is told besides the form of its answer.
const FLOW: &str = "Sentences were taken out of this lesson section because they were shown to be wrong. Restore the flow of what is left: fix a word that now points at nothing (\"this\", \"these\", \"as above\", a step number) and join the sentences on either side of a gap if they no longer follow, changing as little as you can. gaps lists, for each place a sentence was taken out, the labels of the sentence kept before it and the one kept after it: change only those sentences, or add a short connecting sentence between them. verifiedSentences lists the labels of sentences that are checked and must stay exactly as they are; connect around them. Add no statement of fact: no new claim, figure, name, quotation, cause or consequence. Give an entry with no edits for a gap that reads well as it is.";

const CONTRAST_ORPHAN: &str = "contrastOrphanRemoval is true because this gap sits in a contrast or paired comparison. If removing the claim left one half of a two-part contrast stranded without its partner (for instance, a contrast lead-in, \"both are\", or \"the first\" with \"the second\" missing), remove those orphaned contrast sentences with edits.sentences.remove rather than rewriting checked claims or inventing replacement facts. Rewriting (replace) any sentence in verifiedSentences remains forbidden.";

const CONTRAST_CUES: &[&str] = &[
    "versus",
    "near-miss",
    "near miss",
    "the first is",
    "the second is",
    "both are",
    "on the one hand",
    "on the other hand",
];

fn contains_contrast_cue(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    CONTRAST_CUES.iter().any(|cue| lower.contains(cue))
}

fn has_contrast_gap(section: &Value, gap: &Gap) -> bool {
    if section.get("kind").and_then(Value::as_str) == Some("contrast") {
        return true;
    }
    if gap.before.as_deref().is_some_and(contains_contrast_cue)
        || gap.after.as_deref().is_some_and(contains_contrast_cue)
    {
        return true;
    }
    let text = section
        .pointer(&format!("/{}", gap.field))
        .and_then(Value::as_str)
        .unwrap_or_default();
    text.split("\n\n").any(|paragraph| {
        let touches = gap
            .before
            .as_deref()
            .is_some_and(|before| paragraph.contains(before))
            || gap
                .after
                .as_deref()
                .is_some_and(|after| paragraph.contains(after));
        touches && contains_contrast_cue(paragraph)
    })
}

/// The places `after` has lost teaching sentences of `before` at, each with
/// the sentences kept beside it.
pub(in crate::lessons::content_verification) fn gaps(
    field: &str,
    before: &str,
    after: &str,
) -> Vec<Gap> {
    let (old, new) = (teaching_sentences(before), teaching_sentences(after));
    let mut kept = 0;
    let stays: Vec<bool> = old
        .iter()
        .map(|sentence| {
            let same = new.get(kept) == Some(sentence);
            if same {
                kept += 1;
            }
            same
        })
        .collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < old.len() {
        if stays.get(at) == Some(&true) {
            at += 1;
            continue;
        }
        let end = (at..old.len())
            .find(|next| stays.get(*next) == Some(&true))
            .unwrap_or(old.len());
        found.push(Gap {
            field: field.to_owned(),
            before: at.checked_sub(1).and_then(|k| old.get(k)).cloned(),
            after: old.get(end).cloned(),
        });
        at = end;
    }
    found
}

/// The place `after` lost a code block `before` held at, with the sentences
/// kept beside it.
pub(in crate::lessons::content_verification) fn gap_around_code(
    field: &str,
    before: &str,
    after: &str,
) -> Vec<Gap> {
    let shared = lattice_core::shared::text::same_text::common_prefix(before, after);
    let before_it = before
        .get(..shared)
        .and_then(|head| teaching_sentences(head).pop());
    let after_it = after
        .get(shared..)
        .and_then(|tail| teaching_sentences(tail).into_iter().next());
    if before_it.is_none() && after_it.is_none() {
        return Vec::new();
    }
    vec![Gap {
        field: field.to_owned(),
        before: before_it,
        after: after_it,
    }]
}

/// One place a sentence was taken out of: what the flow repair answers once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Place {
    pub(super) at: usize,
    pub(super) gap: Internal<Gap>,
}

impl Kind for Place {
    const PREFIX: &'static str = "g";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// What a flow repair is asked about one section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FlowInput {
    /// The lesson, for what the other sections already say.
    pub(super) lesson: Internal<Value>,
    /// The section's place in the lesson.
    pub(super) part: Internal<String>,
    pub(super) section: Told<Value>,
    pub(super) gaps: Vec<Place>,
    pub(super) verified: Internal<Vec<Protected>>,
    pub(super) wording: Internal<Wording>,
    pub(super) output_tokens: usize,
}

impl FlowInput {
    /// The lesson with the section as this request holds it.
    fn material(&self) -> Value {
        let mut lesson = self.lesson.value().clone();
        if let Some(slot) = lesson.pointer_mut(self.part.value()) {
            *slot = self.section.0.clone();
        }
        lesson
    }

    fn pointer(&self, field: &str) -> String {
        format!("{}/{field}", self.part.value())
    }
}

/// What the model answers for one gap: the edits that restore its flow.
#[derive(Debug, Clone, PartialEq)]
struct Fix {
    edits: rows::Edits,
}

impl Authored for Fix {
    fn form() -> Form {
        Form::new().group::<rows::Edits>("edits")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            edits: fields.group("edits")?,
        })
    }
}

/// What a flow repair is asked: the edits of the sentences beside each gap.
pub(super) struct FlowRepair;

impl Contract for FlowRepair {
    const ID: ContractId = ContractId::new("flow-repair");
    type Input = FlowInput;
}

fn flow_system(contrast_orphan: bool) -> String {
    let lead = if contrast_orphan {
        format!("{FLOW} {CONTRAST_ORPHAN}")
    } else {
        FLOW.to_owned()
    };
    format!(
        "{lead} {} {} {} {}",
        words::SENTENCES,
        words::NO_KEYS,
        words::EDITING,
        words::NO_CITATIONS
    )
}

fn refuse_outside_gap(
    unit: &lattice_core::application::services::model_contract::shown::Picked<
        lattice_core::application::services::model_contract::units::Unit,
    >,
    verified: &[lattice_core::application::services::model_contract::units::UnitRef],
    before: Option<lattice_core::application::services::model_contract::units::UnitRef>,
    after: Option<lattice_core::application::services::model_contract::units::UnitRef>,
) -> std::result::Result<(), Refusal> {
    if verified.contains(&unit.reference) {
        return Err(Refusal(format!(
            "{} states a checked claim and must stay as it is.",
            unit.handle.written()
        )));
    }
    if Some(unit.reference) != before && Some(unit.reference) != after {
        return Err(Refusal(format!(
            "{} is outside the two sentences beside this gap. Change only those two sentences, or add a sentence between them.",
            unit.handle.written()
        )));
    }
    Ok(())
}

/// Why the entry for the gap `of` is refused, when it is.
fn flow_check(input: &FlowInput, of: usize, fix: &Fix) -> std::result::Result<(), Refusal> {
    let edits = &fix.edits;
    if !edits.cite.is_empty() || !edits.recite.is_empty() || !edits.uncite.is_empty() {
        return Err(Refusal(words::NO_CITATIONS.to_owned()));
    }
    if !edits.keys.is_empty() {
        return Err(Refusal(
            "This repair changes no answer key: leave keys empty.".to_owned(),
        ));
    }
    if edits.is_empty() {
        return Ok(());
    }
    let material = input.material();
    let mut rows = Rows::of_part(&material, input.part.value());
    let Some(place) = input.gaps.iter().find(|place| place.at == of) else {
        return Ok(());
    };
    let gap = place.gap.value();
    let field_pointer = input.pointer(&gap.field);
    let beside = |sentence: &Option<String>| {
        sentence
            .as_deref()
            .and_then(|text| rows.units_over(&field_pointer, text).into_iter().next())
    };
    let (before, after) = (beside(&gap.before), beside(&gap.after));
    let verified: Vec<_> = input
        .verified
        .value()
        .iter()
        .filter_map(|item| {
            rows.units_over(&input.pointer(&item.field), &item.quote)
                .into_iter()
                .next()
        })
        .collect();
    let change = edits.sentences.change();
    for (unit, _) in &change.replace {
        refuse_outside_gap(unit, &verified, before, after)?;
    }
    let contrast_orphan = has_contrast_gap(&input.section.0, gap);
    let field_units: Vec<_> = if contrast_orphan {
        material
            .pointer(&field_pointer)
            .and_then(Value::as_str)
            .map(|text| rows.units_over(&field_pointer, text))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    for unit in &change.remove {
        if contrast_orphan && field_units.contains(&unit.reference) {
            continue;
        }
        refuse_outside_gap(unit, &verified, before, after)?;
    }
    for (anchor, _) in &change.insert {
        if let Anchor::After(unit) = anchor {
            if Some(unit.reference) != before {
                return Err(Refusal(format!(
                    "New text after {} is not between the two sentences beside this gap. Add a sentence only directly after the sentence kept before the gap.",
                    unit.handle.written()
                )));
            }
        }
    }
    if !change.moved.is_empty() {
        return Err(Refusal(
            "A sentence is not moved to restore a gap's flow: change the two sentences beside the gap, or add a sentence between them.".to_owned(),
        ));
    }
    let held = Held {
        wording: input.wording.value(),
    };
    match rows.take(edits, &Quotes::default(), &held) {
        Ok(_) => Ok(()),
        Err(Left::Refused(reasons)) => Err(Refusal(reasons.join(" "))),
        Err(Left::Met(words)) => Err(Refusal(words)),
    }
}

impl Answered for FlowRepair {
    type Answer = Each<Place, Fix>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &FlowInput, showing: &mut Showing<'_>) -> Result<Says> {
        let places = showing.show(&input.gaps)?;
        let material = input.material();
        let rows = Rows::of_part(&material, input.part.value());
        let shown = rows.shown(showing)?;
        let label = |field: &str, sentence: &str| {
            rows.units_over(&input.pointer(field), sentence)
                .into_iter()
                .next()
                .and_then(|unit| shown.handle_of(unit))
                .map(|handle| handle.written())
        };
        let gaps: Vec<Value> = places
            .iter()
            .map(|(handle, place)| {
                let gap = place.gap.value();
                let beside = |sentence: &Option<String>| {
                    sentence.as_deref().and_then(|text| label(&gap.field, text))
                };
                json!({
                    "gap": handle.written(),
                    "field": gap.field,
                    "before": beside(&gap.before),
                    "after": beside(&gap.after),
                })
            })
            .collect();
        let contrast_orphan = places
            .iter()
            .any(|(_, place)| has_contrast_gap(&input.section.0, place.gap.value()));
        let verified: Vec<String> = input
            .verified
            .value()
            .iter()
            .filter_map(|item| label(&item.field, &item.quote))
            .collect();
        let section = repair::shown_section(&rows, &shown, input.part.value());
        let mut task = json!({"section": section, "gaps": gaps, "verifiedSentences": verified});
        if contrast_orphan {
            task["contrastOrphanRemoval"] = json!(true);
        }
        let mut says = Says::new(flow_system(contrast_orphan), task.to_string());
        says.output_tokens = input.output_tokens;
        Ok(says)
    }

    fn accept_for(
        input: &FlowInput,
        of: &Picked<Place>,
        fix: &Fix,
    ) -> std::result::Result<(), Refusal> {
        flow_check(input, of.reference, fix)
    }
}

/// One section's flow repair: the section with the entries that stand, or
/// `None` when none can. A gap whose entry cannot stand stays as cut, and so
/// does a gap whose entry would leave the section no lesson.
pub(super) async fn mend_section(
    llm: &dyn LLMPort,
    (schema, output_tokens): (&Value, usize),
    (unit, block, section): (usize, bool, &Value),
    (taken, protected): (&Taken, &[Protected]),
    (lesson, wording): (&Value, &Wording),
) -> Result<Option<Value>> {
    let Some(part) = FindingSite::in_candidate(lesson, unit).pointer() else {
        return Ok(None);
    };
    let stands = |gap: &Gap, sentence: &Option<String>| -> Option<String> {
        let text = section.pointer(&format!("/{}", gap.field))?.as_str()?;
        sentence
            .clone()
            .filter(|sentence| text.contains(sentence.as_str()))
    };
    let places: Vec<Place> = taken
        .gaps
        .iter()
        .filter_map(|gap| {
            let (before, after) = (stands(gap, &gap.before), stands(gap, &gap.after));
            (before.is_some() || after.is_some()).then(|| Gap {
                field: gap.field.clone(),
                before,
                after,
            })
        })
        .enumerate()
        .map(|(at, gap)| Place {
            at,
            gap: Internal::of(gap),
        })
        .collect();
    if places.is_empty() {
        return Ok(None);
    }
    let base = FlowInput {
        lesson: Internal::of(lesson.clone()),
        part: Internal::of(part.clone()),
        section: Told(section.clone()),
        gaps: Vec::new(),
        verified: Internal::of(protected.to_vec()),
        wording: Internal::of(wording.clone()),
        output_tokens,
    };
    let _model_call = crate::lesson_progress::model_call(crate::lesson_progress::Term::FlowRepair);
    let settled = ask_each::<FlowRepair>(
        llm,
        &Asking {
            progress: None,
            reasoning: Some(ReasonFirst {
                kind: CallKind::Authoring,
                subject: "the section flow repair",
                brief: "Inspect each gap left after cuts, check which surrounding sentences need smoothing without altering protected statements, and write out the exact unit edits.",
            }),
        },
        &mut Scope::for_question(),
        &places,
        |open, _| FlowInput {
            gaps: open.iter().map(|place| (*place).clone()).collect(),
            ..base.clone()
        },
        &crate::model_call::answer_options(),
    )
    .await?;
    let list = if block { "blocks" } else { "questions" };
    let validator = schema
        .pointer(&format!("/properties/{list}/items"))
        .cloned()
        .unwrap_or_else(|| json!({"type":"object"}));
    let validator = jsonschema::JSONSchema::compile(&validator).ok();
    let material = base.material();
    let mut rows = Rows::of_part(&material, &part);
    let held = Held { wording };
    let quotes = Quotes::default();
    for answer in settled {
        let ask::Settled::Answered(fix) = answer else {
            continue;
        };
        if fix.edits.is_empty() {
            continue;
        }
        let mut trial = rows.clone();
        if trial.take(&fix.edits, &quotes, &held).is_err() {
            continue;
        }
        let made = trial.value();
        let standing = rows.value();
        let (Some(before), Some(after)) = (standing.pointer(&part), made.pointer(&part)) else {
            continue;
        };
        if before == after {
            continue;
        }
        let valid = validator
            .as_ref()
            .is_some_and(|validator| validator.is_valid(after))
            && before.get("kind") == after.get("kind");
        if !valid {
            continue;
        }
        if crate::generation::reads_as_part(before, block)
            && !crate::generation::reads_as_part(after, block)
        {
            tracing::info!(
                "claim.state: a flow repair left the section no lesson; it keeps the text as cut"
            );
            trace::refused(
                Stage::FlowRepair,
                &format!("the flow repair of section {unit}"),
                "it left the section no lesson, so the section keeps the text as cut",
                Some((&before.to_string(), &after.to_string())),
            );
            continue;
        }
        rows = trial;
    }
    let repaired = rows.value().pointer(&part).cloned();
    Ok(repaired.filter(|repaired| repaired != section))
}
