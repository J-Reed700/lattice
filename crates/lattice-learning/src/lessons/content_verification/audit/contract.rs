//! What the Verifier audit asks and how the answer is read: the claims it is
//! asked to judge, the passages and fetched pages whose sentences it may cite,
//! and what it states for each claim.

use serde::{Deserialize, Serialize};

use lattice_core::application::services::model_contract::ask::{
    Alone, Answered, Says, Settled, Why,
};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shape::{
    Authored, Each, Fields, Form, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Shown, Showing, Told,
};
use lattice_core::application::services::model_contract::units::{Grain, Unit, Units};
use lattice_core::domain::learning::claim_review::{CheckResult, VerdictKind};
use lattice_core::shared::error::Result;

use super::batches::{paragraph, Context, Item};
use super::calls::VerifierAudit;
use super::policy::{Judged, SYSTEM_PROMPT};
use super::{AuditClaim, AuditPassage, AuditUnit};

/// A passage's identity: the source version and the span of it that was compared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Span {
    version: String,
    start: usize,
    end: usize,
}

/// A passage the Verifier may cite, or a page it fetched and the app saved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Passage {
    version: Internal<String>,
    title: Told<String>,
    start: Internal<usize>,
    end: Internal<usize>,
    text: Told<String>,
}

impl Passage {
    pub(super) fn of(passage: &AuditPassage) -> Self {
        Self {
            version: Internal::of(passage.source_version_id.clone()),
            title: Told(passage.title.clone()),
            start: Internal::of(passage.start_byte),
            end: Internal::of(passage.end_byte),
            text: Told(passage.text.clone()),
        }
    }

    pub(super) fn text(&self) -> &str {
        &self.text.0
    }

    pub(super) fn title(&self) -> &str {
        &self.title.0
    }
}

impl Kind for Passage {
    const PREFIX: &'static str = "p";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = Span;

    fn reference(&self) -> Span {
        Span {
            version: self.version.value().clone(),
            start: *self.start.value(),
            end: *self.end.value(),
        }
    }
}

/// A lesson section a claim is read in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Section {
    index: Told<usize>,
    title: Told<String>,
    text: Told<String>,
}

/// How much of its lesson section a claim carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) enum Setting {
    /// The whole section, sent once however many claims cite it.
    Section(Told<usize>),
    /// Only the paragraph holding the claim's quote.
    Paragraph(Told<String>),
    /// No section is known.
    Bare,
}

/// Which part of a claim's passages a request carries, when they are split.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Part {
    index: Told<usize>,
    of: Told<usize>,
}

/// A claim, or one part of a split claim: its place among the audit's claims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Place {
    claim: usize,
    part: Option<usize>,
}

/// One claim, or one part of a split claim, as the Verifier is asked to judge it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Claim {
    at: Place,
    statement: Told<String>,
    lesson_text: Told<String>,
    setting: Setting,
    passages: Vec<Passage>,
    general_knowledge: bool,
    part: Option<Part>,
}

impl Kind for Claim {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = Place;

    fn reference(&self) -> Place {
        self.at.clone()
    }
}

impl Claim {
    /// The claim `item` stands for; `None` when the audit has no such claim.
    pub(super) fn of(claims: &[AuditClaim], units: &[AuditUnit], item: &Item) -> Option<Self> {
        let claim = claims.get(item.claim)?;
        let unit = units.iter().find(|unit| unit.index == claim.unit_index);
        let setting = match (&item.context, unit) {
            (Context::Unit, Some(unit)) => Setting::Section(Told(unit.index)),
            (Context::Paragraph, Some(unit)) => {
                Setting::Paragraph(Told(paragraph(&unit.text, &claim.quote).to_owned()))
            }
            (_, None) => Setting::Bare,
        };
        Some(Self {
            at: Place {
                claim: item.claim,
                part: item.part.map(|(index, _)| index),
            },
            statement: Told(claim.statement.clone()),
            lesson_text: Told(claim.quote.clone()),
            setting,
            passages: item
                .passages
                .iter()
                .filter_map(|index| claim.passages.get(*index))
                .map(Passage::of)
                .collect(),
            general_knowledge: claim.general_knowledge,
            part: item.part.map(|(index, of)| Part {
                index: Told(index),
                of: Told(of),
            }),
        })
    }
}

/// What one request asks about the claims still open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
    sections: Vec<Section>,
    claims: Vec<Claim>,
    /// Pages the Verifier fetched, the first fetched first.
    fetched: Packed<Passage>,
    /// The tokens the answer is promised.
    reserve: Internal<usize>,
}

impl Input {
    pub(super) fn of(
        claims: Vec<Claim>,
        units: &[AuditUnit],
        fetched: Packed<Passage>,
        reserve: usize,
    ) -> Self {
        let mut sections: Vec<Section> = Vec::new();
        for claim in &claims {
            let Setting::Section(index) = &claim.setting else {
                continue;
            };
            if sections.iter().any(|seen| seen.index.0 == index.0) {
                continue;
            }
            if let Some(unit) = units.iter().find(|unit| unit.index == index.0) {
                sections.push(Section {
                    index: Told(unit.index),
                    title: Told(unit.title.clone()),
                    text: Told(unit.text.clone()),
                });
            }
        }
        Self {
            sections,
            claims,
            fetched,
            reserve: Internal::of(reserve),
        }
    }

    /// Each passage the claims cite, once, in the order first cited.
    fn cited(&self) -> Vec<Passage> {
        let mut cited: Vec<Passage> = Vec::new();
        for passage in self.claims.iter().flat_map(|claim| &claim.passages) {
            if !cited
                .iter()
                .any(|seen| seen.reference() == passage.reference())
            {
                cited.push(passage.clone());
            }
        }
        cited
    }
}

/// A Verifier's verdict on a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ruling {
    Supported,
    Contradicted,
    Unsupported,
    CouldNotVerify,
}

impl Vocabulary for Ruling {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("supported", Self::Supported),
        ("contradicted", Self::Contradicted),
        ("unsupported", Self::Unsupported),
        ("could_not_verify", Self::CouldNotVerify),
    ];
}

impl Ruling {
    pub(super) fn word(self) -> &'static str {
        Self::WORDS
            .iter()
            .find(|(_, ruling)| *ruling == self)
            .map_or("", |(word, _)| *word)
    }
}

/// What the Verifier states for one claim.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Stated {
    cited: Vec<Picked<Unit>>,
    ruling: Ruling,
    reason: String,
}

impl Authored for Stated {
    fn form() -> Form {
        Form::new()
            .cite::<Unit>("sentencesUsed")
            .word::<Ruling>("verdict")
            .text("reason")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            cited: fields.cite("sentencesUsed"),
            ruling: fields.word("verdict")?,
            reason: fields.text("reason")?,
        })
    }
}

/// `passages` as blocks: the passage's handle and title, then its sentences one to a row.
fn blocks(showing: &mut Showing<'_>, passages: &Shown<'_, Passage>) -> Result<String> {
    let mut text = String::new();
    for (handle, passage) in passages.iter() {
        let units = Units::of(passage.text(), Grain::Sentence);
        let rows = units.shown(showing)?;
        text.push_str(&format!(
            "{} {}\n{}\n\n",
            handle.bracketed(),
            passage.title(),
            units.rows(&rows)
        ));
    }
    Ok(text)
}

impl Answered for VerifierAudit {
    type Answer = Each<Claim, Stated>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let cited = input.cited();
        let fetched: Vec<Passage> = input.fetched.iter().cloned().collect();
        let shown_cited = showing.show(&cited)?;
        let shown_fetched = showing.show(&fetched)?;
        let shown_claims = showing.show(&input.claims)?;
        let mut task = String::new();
        if !input.sections.is_empty() {
            task.push_str("Lesson sections\n\n");
            for section in &input.sections {
                task.push_str(&format!(
                    "[S{}] {}\n{}\n\n",
                    section.index.0, section.title.0, section.text.0
                ));
            }
        }
        if !shown_cited.is_empty() {
            task.push_str("Reference passages\n\n");
            task.push_str(&blocks(showing, &shown_cited)?);
        }
        if !shown_fetched.is_empty() {
            task.push_str("Pages you fetched\n\n");
            task.push_str(&blocks(showing, &shown_fetched)?);
        }
        task.push_str("Claims\n\n");
        let claim_blocks: Vec<String> = shown_claims
            .iter()
            .map(|(handle, claim)| {
                let context = match &claim.setting {
                    Setting::Section(index) => format!("Section: S{}", index.0),
                    Setting::Paragraph(text) => format!("Context: \"{}\"", text.0),
                    Setting::Bare => String::from("Context: the lesson text above only"),
                };
                let labels: Vec<String> = claim
                    .passages
                    .iter()
                    .filter_map(|passage| {
                        shown_cited
                            .iter()
                            .find(|(_, shown)| shown.reference() == passage.reference())
                            .map(|(handle, _)| handle.bracketed())
                    })
                    .collect();
                let passages = match (labels.is_empty(), claim.general_knowledge) {
                    (true, true) => String::from(
                        "none (accepted as general knowledge: check it for contradiction only)",
                    ),
                    (true, false) => String::from("none"),
                    (false, _) => labels.join(", "),
                };
                let part = claim
                    .part
                    .as_ref()
                    .map(|part| {
                        format!(
                            "\nPart {} of {} of this claim's passages.",
                            part.index.0, part.of.0
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "{} {}\nLesson text: \"{}\"\n{context}\nPassages: {passages}{part}",
                    handle.bracketed(),
                    claim.statement.0,
                    claim.lesson_text.0
                )
            })
            .collect();
        task.push_str(&claim_blocks.join("\n\n"));
        let mut says = Says::new(SYSTEM_PROMPT, task);
        says.output_tokens = *input.reserve.value();
        Ok(says)
    }

    fn within(input: &Input, room: usize) -> Option<Input> {
        if input.fetched.is_empty() {
            return None;
        }
        Some(Input {
            fetched: input.fetched.within(room),
            ..input.clone()
        })
    }
}

/// The passages a batch's claims may cite, with the sentences each was shown as.
pub(super) struct Evidence {
    passages: Vec<(Passage, Units)>,
}

impl Evidence {
    /// The passages `claims` cite and the pages `fetched`, each once.
    pub(super) fn of(claims: &[Claim], fetched: &[Passage]) -> Self {
        let mut passages: Vec<(Passage, Units)> = Vec::new();
        for passage in claims
            .iter()
            .flat_map(|claim| &claim.passages)
            .chain(fetched)
        {
            if !passages
                .iter()
                .any(|(seen, _)| seen.reference() == passage.reference())
            {
                let units = Units::of(passage.text(), Grain::Sentence);
                passages.push((passage.clone(), units));
            }
        }
        Self { passages }
    }

    /// The passage a cited sentence is in, and the sentence's words.
    fn sentence(&self, cited: &Picked<Unit>) -> Option<(&Passage, &str)> {
        self.passages
            .iter()
            .find_map(|(passage, units)| Some((passage, units.text_of(cited.reference)?)))
    }

    /// The grounded verdict `settled` comes to: a verdict the Verifier cannot
    /// tie to a sentence it was shown counts as could not verify.
    pub(super) fn judged(&self, settled: &Settled<Stated>) -> Judged {
        match settled {
            Settled::Answered(stated) => self.ground(stated),
            Settled::Unanswered(why) => Judged::could_not_verify(unanswered(why)),
        }
    }

    fn ground(&self, stated: &Stated) -> Judged {
        let reason = stated.reason.trim();
        let quoted: Vec<(&Passage, &str)> = stated
            .cited
            .iter()
            .filter_map(|cited| self.sentence(cited))
            .collect();
        let ungrounded = |said: &str| {
            Judged::could_not_verify(format!(
                "The Verifier said {said} but quoted no passage it was given, so its verdict counts \
                 as could not verify. Its reason: {reason}"
            ))
        };
        match stated.ruling {
            Ruling::Supported => match quoted.first() {
                Some((passage, words)) => Judged {
                    result: CheckResult::Supported {
                        source_version_id: passage.version.value().clone(),
                        supporting_quote: (*words).to_owned(),
                    },
                    reason: reason.to_owned(),
                },
                None => ungrounded("supported"),
            },
            Ruling::Contradicted => match quoted.first() {
                Some((passage, words)) => Judged {
                    result: CheckResult::Failed {
                        kind: VerdictKind::Contradicted,
                    },
                    reason: format!("{reason} {}: \"{words}\"", passage.title()),
                },
                None => ungrounded("contradicted"),
            },
            Ruling::Unsupported => Judged {
                result: CheckResult::Failed {
                    kind: VerdictKind::Unsupported,
                },
                reason: reason.to_owned(),
            },
            Ruling::CouldNotVerify => Judged::could_not_verify(reason),
        }
    }
}

/// Why a statement was left without a usable answer, in words for the audit's reason.
fn unanswered(why: &Why) -> String {
    let cause = match why {
        Why::NoRoom => {
            return "The Verifier was not asked: the statement and its evidence do not fit in this model's context window."
                .to_owned()
        }
        Why::NotRead => "it could not be read",
        Why::CutOff => "it was cut off before it ended",
        Why::Missing => "it said nothing about this statement",
        Why::Unplaced => "it pointed at something that was not shown",
        Why::Contradicted(_) => "it gave two different verdicts for this statement",
        Why::Declined => "the Verifier declined to answer",
        Why::Refused(_) => "it was refused",
    };
    format!("The Verifier's answer could not be used: {cause}.")
}

#[cfg(test)]
#[path = "contract_tests.rs"]
mod tests;
