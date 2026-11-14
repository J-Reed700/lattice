//! A review finding and where it is. Across review passes a finding is
//! identified by what raised it and what it is about (`FindingId`), never by
//! its wording, which the reviewer rewrites every pass. Rows stored before
//! finding ids are identified by kind and place (`FindingKey`).
use lattice_core::domain::learning::claim_review::text_sha256;
use lattice_core::shared::text::plain_text;
use lattice_core::shared::text::same_text;
use lattice_core::shared::text::segmentation::sentences;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// What raised a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    /// The teaching review: a section's check or a reviewer's issue.
    Teaching,
    /// An independent solution disagreed with a question's answer key.
    AnswerKey,
    /// A deterministic rule: the schema, a section's body or rubric, or
    /// the lesson's shape.
    Structure,
    /// A source quotation that matches no saved reference.
    Quotation,
}

impl FindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Teaching => "teaching",
            Self::AnswerKey => "answer_key",
            Self::Structure => "structure",
            Self::Quotation => "quotation",
        }
    }

    pub fn from_column(value: &str) -> Option<Self> {
        [
            Self::Teaching,
            Self::AnswerKey,
            Self::Structure,
            Self::Quotation,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == value)
    }
}

/// The part of the material a finding is about: the application's own
/// zero-based `section-N` and question indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "unit", content = "index")]
pub enum FindingSite {
    Lesson,
    Section(usize),
    Question(usize),
}

impl FindingSite {
    /// The site of flat unit index `unit` in a lesson of `sections` sections: sections first, then questions.
    pub fn of_unit(unit: usize, sections: usize) -> Self {
        match unit.checked_sub(sections) {
            None => Self::Section(unit),
            Some(question) => Self::Question(question),
        }
    }

    /// [`Self::of_unit`] with the length of `candidate`'s `blocks` array (0 when it has none).
    pub fn in_candidate(candidate: &Value, unit: usize) -> Self {
        let sections = candidate
            .get("blocks")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        Self::of_unit(unit, sections)
    }

    /// The flat unit index of this site; none for the whole lesson.
    pub fn unit(self, sections: usize) -> Option<usize> {
        match self {
            Self::Lesson => None,
            Self::Section(index) => Some(index),
            Self::Question(index) => sections.checked_add(index),
        }
    }

    /// The site a JSON pointer into the candidate is in:
    /// `/blocks/3/title` is section 3, `/questions` the whole lesson.
    pub fn of_pointer(pointer: &str) -> Self {
        let mut parts = pointer.trim_start_matches('/').split('/');
        match (parts.next(), parts.next().and_then(index_segment)) {
            (Some("blocks"), Some(index)) => Self::Section(index),
            (Some("questions"), Some(index)) => Self::Question(index),
            _ => Self::Lesson,
        }
    }

    /// The site a candidate part names: an evidence ID (`blocks-3`,
    /// `questions-1`) or a quotation check's path (`blocks[3]`).
    pub fn of_part(id: &str) -> Option<Self> {
        let (field, index) = id
            .strip_suffix(']')
            .and_then(|id| id.split_once('['))
            .or_else(|| id.rsplit_once('-'))?;
        let index = index_segment(index)?;
        match field {
            "blocks" => Some(Self::Section(index)),
            "questions" => Some(Self::Question(index)),
            _ => None,
        }
    }

    /// ("blocks", 3) or ("questions", 1); none for the whole lesson.
    pub fn array(self) -> Option<(&'static str, usize)> {
        match self {
            Self::Lesson => None,
            Self::Section(index) => Some(("blocks", index)),
            Self::Question(index) => Some(("questions", index)),
        }
    }

    /// The pointer of the block or question this site is, or none for the
    /// whole lesson.
    pub fn pointer(self) -> Option<String> {
        self.array().map(|(list, index)| format!("/{list}/{index}"))
    }

    /// The pointer of `field` of this site's block or question.
    pub fn field_pointer(self, field: &str) -> Option<String> {
        self.pointer().map(|pointer| format!("{pointer}/{field}"))
    }

    /// `blocks-3`: how an evidence ID names this site.
    pub fn part_id(self) -> Option<String> {
        self.array().map(|(list, index)| format!("{list}-{index}"))
    }

    /// `blocks[3]`: how a quotation check's path names this site.
    pub fn path(self) -> Option<String> {
        self.array().map(|(list, index)| format!("{list}[{index}]"))
    }

    /// `lesson`, `section-3` or `question-1`: how a site is written in an id.
    pub fn id_text(self) -> String {
        match self {
            Self::Lesson => "lesson".to_owned(),
            Self::Section(index) => format!("section-{index}"),
            Self::Question(index) => format!("question-{index}"),
        }
    }

    /// "The whole lesson", "Section 4" or "Question 2".
    pub fn label(self) -> String {
        match self {
            Self::Lesson => "The whole lesson".to_owned(),
            Self::Section(index) => format!("Section {}", index + 1),
            Self::Question(index) => format!("Question {}", index + 1),
        }
    }

    /// "the lesson", "section 4" or "question 2".
    pub fn plain(self) -> String {
        match self {
            Self::Lesson => "the lesson".to_owned(),
            Self::Section(index) => format!("section {}", index + 1),
            Self::Question(index) => format!("question {}", index + 1),
        }
    }

    /// "Section 4 (candidate blocks-3)", or [`Self::label`] for the whole lesson.
    pub fn note_label(self) -> String {
        self.part_id().map_or_else(
            || self.label(),
            |id| format!("{} (candidate {id})", self.label()),
        )
    }

    /// "Section 2 · Defining Tool Schemas" (the title as plain text, trimmed);
    /// [`Self::label`] for a blank title and for other sites.
    pub fn titled(self, title: &str) -> String {
        let title = plain_text(title);
        let title = title.trim();
        match self {
            Self::Section(_) if !title.is_empty() => format!("{} \u{b7} {title}", self.label()),
            _ => self.label(),
        }
    }
}

/// An RFC 6901 array index: "0", or a nonzero digit followed by digits.
pub(crate) fn index_segment(segment: &str) -> Option<usize> {
    let digits = |rest: &str| rest.bytes().all(|byte| byte.is_ascii_digit());
    let canonical = segment == "0"
        || segment
            .strip_prefix(|c: char| ('1'..='9').contains(&c))
            .is_some_and(digits);
    canonical.then(|| segment.parse().ok()).flatten()
}

/// A deterministic rule a lesson can break; one variant for each place that
/// checks one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    /// The material could not be read as a whole.
    Unreadable,
    /// A schema violation.
    Schema,
    /// A teaching block has no body.
    BlockBody,
    /// A practice block lacks its rubric criteria.
    PracticeRubric,
    /// A quotation matches no saved reference.
    Quotation,
    /// A section has no title.
    TitleMissing,
    /// Two sections share a title.
    TitleRepeated,
    /// A check does not hold its answer back.
    CheckHoldback,
    /// A section opens a code fence it never closes.
    FenceUnclosed,
    /// A question has no prompt.
    PromptMissing,
    /// Two questions ask the same thing.
    PromptRepeated,
    /// A question has no answer explanation.
    ExplanationMissing,
    /// A question lacks [`CHOICES_PER_QUESTION`](crate::assessment::assessment_engine::checks::CHOICES_PER_QUESTION) distinct answer choices.
    Choices,
    /// A question's right choice is much longer than the others.
    KeyLongest,
    /// A question's prompt contains its answer.
    AnswerInPrompt,
    /// A question's rationales do not match its choices.
    RationaleCount,
    /// A wrong choice has no reason.
    RationaleMissing,
    /// A specific sentence cites no source.
    Uncited,
    /// A citation is written into the lesson's text.
    CitationLabel,
    /// A sentence names a source by its number.
    SourceNumber,
    /// A sentence the lesson already said.
    SentenceRepeated,
    /// A check on an idea no example has shown.
    UnshownCheck,
    /// A recap before the reflection.
    RecapFirst,
    /// The mix of section kinds.
    SectionMix,
    /// The count of questions of each kind.
    QuestionCount,
    /// The lesson has no explanation section.
    NoExplanation,
    /// A kind of question is missing.
    QuestionKinds,
    /// A sentence copies the source's wording.
    CopiedWording,
}

/// How a finding can be corrected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remedy {
    /// Edits to existing text of its part, or citations appended or taken back.
    Edits,
    /// The material written again whole.
    Whole,
}

impl Rule {
    /// `Edits` when a fix always exists that replaces text in the finding's
    /// own part, changes an existing number or boolean, or appends or takes
    /// back a citation; `Whole` when a fix may need a missing or empty field,
    /// a part added or removed, a reordering, or material that cannot be read.
    pub fn remedy(self) -> Remedy {
        match self {
            Self::Quotation
            | Self::TitleRepeated
            | Self::CheckHoldback
            | Self::FenceUnclosed
            | Self::PromptRepeated
            | Self::KeyLongest
            | Self::AnswerInPrompt
            | Self::Uncited
            | Self::CitationLabel
            | Self::SourceNumber
            | Self::SentenceRepeated
            | Self::CopiedWording => Remedy::Edits,
            Self::Unreadable
            | Self::Schema
            | Self::BlockBody
            | Self::PracticeRubric
            | Self::TitleMissing
            | Self::PromptMissing
            | Self::ExplanationMissing
            | Self::Choices
            | Self::RationaleCount
            | Self::RationaleMissing
            | Self::UnshownCheck
            | Self::RecapFirst
            | Self::SectionMix
            | Self::QuestionCount
            | Self::NoExplanation
            | Self::QuestionKinds => Remedy::Whole,
        }
    }
}

/// What the check of a lesson against its course plan found.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanCheck {
    Planned,
    Used,
    Contradiction,
    Unseen,
}

impl PlanCheck {
    const ALL: [Self; 4] = [Self::Planned, Self::Used, Self::Contradiction, Self::Unseen];

    /// Whether the check finds something the lesson lacks.
    pub fn lacks(self) -> bool {
        matches!(self, Self::Used | Self::Planned)
    }
}

/// What raised a finding, and what it is about.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "by")]
pub enum Origin {
    /// `subject` is the digest of the one sentence or citation the rule is
    /// about; none for a rule about the whole part.
    Rule { rule: Rule, subject: Option<String> },
    /// The check against the course plan.
    Plan { check: PlanCheck, subject: String },
    /// An independent solution disagreed with the question's key.
    AnswerKey,
    /// The teaching reviewer; `number` is the finding's own among the
    /// reviewer's findings on one lesson.
    Reviewer { number: u32 },
}

/// A finding's identity across passes and reviews.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FindingId(String);

impl Origin {
    /// What raised a finding and which of its checks, without place or
    /// subject: the first part of its id.
    pub fn kind(&self) -> String {
        match self {
            Self::Rule { rule, .. } => format!("rule/{}", serde_name(rule)),
            Self::Plan { check, .. } => format!("plan/{}", serde_name(check)),
            Self::AnswerKey => "key".to_owned(),
            Self::Reviewer { .. } => "reviewer".to_owned(),
        }
    }
}

impl FindingId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A stored id read back.
    pub(crate) fn stored(id: String) -> Self {
        Self(id)
    }

    /// The number of a reviewer's finding's id.
    pub(crate) fn reviewer_number(&self) -> Option<u32> {
        self.0.strip_prefix("reviewer/")?.parse().ok()
    }

    /// The plan check of a plan finding's id.
    fn plan_check(&self) -> Option<PlanCheck> {
        PlanCheck::ALL.into_iter().find(|check| {
            let kind = Origin::Plan {
                check: *check,
                subject: String::new(),
            }
            .kind();
            self.0
                .strip_prefix(&kind)
                .is_some_and(|rest| rest.starts_with('/'))
        })
    }

    /// Whether taking the part at `site` out of the lesson settles the finding: it is about that part's own text, not the whole lesson or something the lesson lacks.
    pub(crate) fn ends_with_its_part(&self, site: FindingSite) -> bool {
        site != FindingSite::Lesson && !self.plan_check().is_some_and(PlanCheck::lacks)
    }
}

/// The name `value` has when serialized as a plain string.
fn serde_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|name| name.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The digest a sentence-level subject is named by: unique among one
/// lesson's sentences, and short enough for a model to copy.
pub(crate) fn subject_of(text: &str) -> String {
    text_sha256(&same_text::fold(text))
        .chars()
        .take(16)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    /// What raised the finding and what it is about.
    pub origin: Origin,
    pub kind: FindingKind,
    /// `None` for a reviewer's issue until the evidence check places it;
    /// an issue it cannot place is about the whole lesson.
    pub site: Option<FindingSite>,
    /// What the repair is asked to correct. It is written for the model.
    pub note: String,
    /// What is wrong, in plain words, for the user. Empty when the producer
    /// wrote none.
    #[serde(default)]
    pub summary: String,
    /// The passage the reviewer chose, when it chose one.
    pub passage: Option<String>,
    /// The whole sentence a sentence-level quotation finding is about.
    #[serde(default)]
    pub sentence: Option<String>,
    /// The second part a finding about two parts is about.
    #[serde(default)]
    pub also: Option<FindingSite>,
}

impl ReviewFinding {
    fn raised(
        origin: Origin,
        kind: FindingKind,
        site: Option<FindingSite>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            origin,
            kind,
            site,
            note: note.into(),
            summary: String::new(),
            passage: None,
            sentence: None,
            also: None,
        }
    }

    /// A broken rule about the whole of the part at `site`.
    pub fn rule(rule: Rule, kind: FindingKind, site: FindingSite, note: impl Into<String>) -> Self {
        Self::raised(
            Origin::Rule {
                rule,
                subject: None,
            },
            kind,
            Some(site),
            note,
        )
    }

    /// `rule`, about one sentence or citation: `text` is that text. A blank
    /// `text` gives no subject.
    pub fn rule_about(
        rule: Rule,
        kind: FindingKind,
        site: FindingSite,
        text: &str,
        note: impl Into<String>,
    ) -> Self {
        let subject = (!same_text::fold(text).is_empty()).then(|| subject_of(text));
        Self::raised(Origin::Rule { rule, subject }, kind, Some(site), note)
    }

    /// A finding of the check against the course plan.
    pub fn plan(
        check: PlanCheck,
        subject: impl Into<String>,
        site: FindingSite,
        note: impl Into<String>,
    ) -> Self {
        Self::raised(
            Origin::Plan {
                check,
                subject: subject.into(),
            },
            FindingKind::Teaching,
            Some(site),
            note,
        )
    }

    /// An independent solution of `question` disagreed with its key.
    pub fn answer_key(question: usize, note: impl Into<String>) -> Self {
        Self::raised(
            Origin::AnswerKey,
            FindingKind::AnswerKey,
            Some(FindingSite::Question(question)),
            note,
        )
    }

    /// The reviewer's finding `number`. `site` is none until the evidence
    /// check places it.
    pub fn reviewer(number: u32, site: Option<FindingSite>, note: impl Into<String>) -> Self {
        Self::raised(
            Origin::Reviewer { number },
            FindingKind::Teaching,
            site,
            note,
        )
    }

    /// A finding of `kind` at `site`, raised as that kind is; a reviewer's is
    /// numbered by its site, so one site is one finding.
    #[cfg(test)]
    pub(crate) fn of_kind(kind: FindingKind, site: FindingSite, note: impl Into<String>) -> Self {
        match kind {
            FindingKind::Teaching => {
                let number = match site {
                    FindingSite::Lesson => 0,
                    FindingSite::Section(index) => 2 * index + 1,
                    FindingSite::Question(index) => 2 * index + 2,
                };
                Self::reviewer(u32::try_from(number).unwrap_or_default(), Some(site), note)
            }
            FindingKind::Quotation => Self::rule(Rule::Quotation, kind, site, note),
            FindingKind::Structure => Self::rule(Rule::Schema, kind, site, note),
            FindingKind::AnswerKey => Self::raised(Origin::AnswerKey, kind, Some(site), note),
        }
    }

    /// This finding with the whole `sentence` it is about; none when blank.
    pub fn with_sentence(mut self, sentence: &str) -> Self {
        self.sentence = (!sentence.trim().is_empty()).then(|| sentence.trim().to_owned());
        self
    }

    /// This finding also about the part at `site`; no change when that is the
    /// part it is already at.
    pub fn with_also(mut self, site: FindingSite) -> Self {
        self.also = (Some(site) != self.site).then_some(site);
        self
    }

    /// This finding with the plain-words `summary` the user is shown.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = summary.into();
        self
    }

    /// This finding with `passage` as the passage it is about; none when
    /// `passage` is blank.
    pub fn with_passage(mut self, passage: &str) -> Self {
        self.passage = (!passage.trim().is_empty()).then(|| passage.to_owned());
        self
    }

    /// How this finding can be corrected: a rule's own remedy, edits for any
    /// other origin.
    pub fn remedy(&self) -> Remedy {
        match &self.origin {
            Origin::Rule { rule, .. } => rule.remedy(),
            Origin::Plan { .. } | Origin::AnswerKey | Origin::Reviewer { .. } => Remedy::Edits,
        }
    }

    /// Whether a repair may change any part to mend this finding: it is about the whole material, or about something the lesson lacks.
    pub fn mends_anywhere(&self) -> bool {
        matches!(self.site, None | Some(FindingSite::Lesson))
            || matches!(
                self.origin,
                Origin::Plan {
                    check: PlanCheck::Planned,
                    ..
                }
            )
    }

    /// Whether the lesson lacks teaching, which its correction adds without changing any existing sentence.
    pub fn adds(&self) -> bool {
        matches!(self.origin, Origin::Plan { check, .. } if check.lacks())
    }

    /// This finding's identity: the same across passes while the same defect
    /// is raised, whatever its note says.
    pub fn id(&self) -> FindingId {
        let site = self.site.unwrap_or(FindingSite::Lesson).id_text();
        let kind = self.origin.kind();
        FindingId(match &self.origin {
            Origin::Rule { subject: None, .. } => format!("{kind}/{site}"),
            Origin::Rule {
                subject: Some(subject),
                ..
            } => format!("{kind}/{site}/{subject}"),
            Origin::Plan {
                check: PlanCheck::Planned,
                subject,
            } => format!("{kind}/{subject}"),
            Origin::Plan { subject, .. } => format!("{kind}/{site}/{subject}"),
            Origin::AnswerKey => format!("{kind}/{site}"),
            Origin::Reviewer { number } => format!("{kind}/{number}"),
        })
    }

    /// The identity of rows stored before finding ids.
    pub(crate) fn place(&self) -> FindingKey {
        FindingKey {
            kind: self.kind,
            site: self.site.unwrap_or(FindingSite::Lesson),
        }
    }
}

/// `items` as a plain list: "a", "a and b", "a, b and c".
pub(crate) fn join_plain(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [first @ .., last] => format!("{} and {last}", first.join(", ")),
    }
}

/// A finding's identity across passes and reviews.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FindingKey {
    pub kind: FindingKind,
    pub site: FindingSite,
}

impl FindingKey {
    /// The stored identity: `teaching:section-3`, `answer_key:question-1`,
    /// `structure:lesson`.
    pub fn id(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.site.id_text())
    }
}

/// What an earlier review left with the user, for text that has not changed.
#[derive(Clone, Debug, Default)]
pub(crate) struct Held {
    /// Findings stored by their own id.
    pub ids: BTreeSet<FindingId>,
    /// Rows stored before finding ids: every finding of the kind at the place.
    pub places: BTreeSet<FindingKey>,
    /// The reviewer's findings among those, each with the digest of its
    /// part's text.
    pub reviewer: Vec<(ReviewFinding, String)>,
    /// One more than the highest number of any reviewer finding stored for
    /// the lesson, changed text or not; none is zero.
    pub next_number: u32,
}

/// A finding the repairs stopped on: it waits for the user.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetAside {
    pub finding: ReviewFinding,
    /// Repairs it was sent to.
    pub repairs: u8,
}

/// The part of the candidate a site names, as the user is shown it.
pub(crate) struct Unit {
    pub title: String,
    /// Binds a "keep as written" to this text: once the part changes, the
    /// user's decision no longer covers it.
    pub sha256: String,
    /// What to show when the reviewer chose no passage.
    pub passage: Option<String>,
}

/// The part of the candidate a site names.
pub(crate) fn part_at(candidate: &Value, site: FindingSite) -> Option<&Value> {
    match site.pointer() {
        None => Some(candidate),
        Some(pointer) => candidate.pointer(&pointer),
    }
}

/// The digest of the text at `site`, which a "keep as written" is bound to.
pub(crate) fn site_sha256(candidate: &Value, site: FindingSite) -> String {
    text_sha256(
        &part_at(candidate, site)
            .map(Value::to_string)
            .unwrap_or_default(),
    )
}

/// Every string of `value` at any depth, with its path relative to `value`
/// ("body", "rubric/0/description"; "" for a string `value`).
pub(crate) fn leaves(value: &Value) -> Vec<(String, &str)> {
    fn walk<'a>(value: &'a Value, path: &str, out: &mut Vec<(String, &'a str)>) {
        let child = |name: &str| {
            if path.is_empty() {
                name.to_owned()
            } else {
                format!("{path}/{name}")
            }
        };
        match value {
            Value::String(text) => out.push((path.to_owned(), text)),
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, &child(&index.to_string()), out);
                }
            }
            Value::Object(fields) => {
                for (name, field) in fields {
                    walk(field, &child(name), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, "", &mut out);
    out
}

/// The whole sentences of the finding's part that its passage or sentence
/// lies in, wherever they occur there.
pub(crate) fn sentences_of(candidate: &Value, finding: &ReviewFinding) -> Vec<String> {
    let Some(part) = part_at(candidate, finding.site.unwrap_or(FindingSite::Lesson)) else {
        return Vec::new();
    };
    let needles = [finding.passage.as_deref(), finding.sentence.as_deref()];
    let mut found: Vec<String> = Vec::new();
    for (_, leaf) in leaves(part) {
        for needle in needles.into_iter().flatten() {
            for span in same_text::find_all(leaf, needle) {
                for (from, to) in same_text::sentences_over(leaf, span) {
                    if let Some(sentence) = leaf.get(from..to) {
                        if !found.iter().any(|known| known == sentence) {
                            found.push(sentence.to_owned());
                        }
                    }
                }
            }
        }
    }
    found
}

/// The lesson bytes of the first string of the part `site` names (the whole
/// candidate for the lesson) that holds `quote` as a copy.
pub(crate) fn part_quote<'c>(
    candidate: &'c Value,
    site: FindingSite,
    quote: &str,
) -> Option<&'c str> {
    leaves(part_at(candidate, site)?)
        .into_iter()
        .find_map(|(_, leaf)| {
            let (start, end) = same_text::find(leaf, quote)?;
            leaf.get(start..end)
        })
}

/// The sentences of every string of `value` that are not blank once folded.
pub(crate) fn sentences_in(value: &Value) -> Vec<String> {
    leaves(value)
        .into_iter()
        .flat_map(|(_, leaf)| {
            sentences(leaf)
                .into_iter()
                .filter_map(move |(from, to)| leaf.get(from..to))
        })
        .filter(|sentence| !same_text::fold(sentence).is_empty())
        .map(str::to_owned)
        .collect()
}

/// What a lesson holds outside its sections and questions.
fn lesson_rest(lesson: &Value) -> Value {
    let mut rest = lesson.clone();
    if let Some(fields) = rest.as_object_mut() {
        fields.remove("blocks");
        fields.remove("questions");
    }
    rest
}

/// For each part a repair changed, the sentences it holds that the lesson did
/// not have before, anywhere: a sentence that only moved is not new.
pub(crate) fn new_sentences(previous: &Value, repaired: &Value) -> Vec<(FindingSite, Vec<String>)> {
    let known: BTreeSet<String> = sentences_in(previous)
        .iter()
        .map(|sentence| subject_of(sentence))
        .collect();
    let mut changed = Vec::new();
    let mut visit = |site: FindingSite, before: Option<&Value>, after: &Value| {
        if before == Some(after) {
            return;
        }
        let fresh: Vec<String> = sentences_in(after)
            .into_iter()
            .filter(|sentence| !known.contains(&subject_of(sentence)))
            .collect();
        if !fresh.is_empty() {
            changed.push((site, fresh));
        }
    };
    let items = |field: &str| {
        repaired
            .get(field)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    for (index, block) in items("blocks").iter().enumerate() {
        let site = FindingSite::Section(index);
        visit(site, part_at(previous, site), block);
    }
    for (index, question) in items("questions").iter().enumerate() {
        let site = FindingSite::Question(index);
        visit(site, part_at(previous, site), question);
    }
    visit(
        FindingSite::Lesson,
        Some(&lesson_rest(previous)),
        &lesson_rest(repaired),
    );
    changed
}

pub(crate) fn unit(candidate: &Value, finding: &ReviewFinding) -> Unit {
    let text = |value: Option<&Value>, field: &str| {
        value
            .and_then(|value| value.get(field))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
    };
    let site = finding.site.unwrap_or(FindingSite::Lesson);
    let part = part_at(candidate, site);
    let title = match site {
        FindingSite::Section(_) => site.titled(&text(part, "title").unwrap_or_default()),
        FindingSite::Lesson | FindingSite::Question(_) => site.label(),
    };
    let passage = match site {
        FindingSite::Question(_) if finding.kind != FindingKind::Quotation => text(part, "prompt"),
        _ => None,
    };
    Unit {
        title,
        sha256: site_sha256(candidate, site),
        passage,
    }
}

#[cfg(test)]
#[path = "review_findings_tests.rs"]
mod tests;
