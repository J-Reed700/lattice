//! A review finding and where it is. Across review passes a finding is
//! identified by its kind and the part of the lesson it is about, never by
//! its wording, which the reviewer rewrites every pass.
use lattice_core::domain::learning::claim_review::text_sha256;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// The site a JSON pointer into the candidate is in:
    /// `/blocks/3/title` is section 3, `/questions` the whole lesson.
    pub fn of_pointer(pointer: &str) -> Self {
        let mut parts = pointer.trim_start_matches('/').split('/');
        match (parts.next(), parts.next().and_then(|i| i.parse().ok())) {
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
        let index = index.parse().ok()?;
        match field {
            "blocks" => Some(Self::Section(index)),
            "questions" => Some(Self::Question(index)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
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
}

impl ReviewFinding {
    pub fn new(kind: FindingKind, site: Option<FindingSite>, note: impl Into<String>) -> Self {
        Self {
            kind,
            site,
            note: note.into(),
            summary: String::new(),
            passage: None,
            sentence: None,
        }
    }

    /// This finding with the whole `sentence` it is about; none when blank.
    pub fn with_sentence(mut self, sentence: &str) -> Self {
        self.sentence = (!sentence.trim().is_empty()).then(|| sentence.trim().to_owned());
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

    pub fn key(&self) -> FindingKey {
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

/// How a summary names the part a site is: "section 3", "question 2" or
/// "the lesson".
pub(crate) fn plain_part(site: FindingSite) -> String {
    match site {
        FindingSite::Lesson => "the lesson".to_owned(),
        FindingSite::Section(index) => format!("section {}", index + 1),
        FindingSite::Question(index) => format!("question {}", index + 1),
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
        let site = match self.site {
            FindingSite::Lesson => "lesson".to_owned(),
            FindingSite::Section(index) => format!("section-{index}"),
            FindingSite::Question(index) => format!("question-{index}"),
        };
        format!("{}:{site}", self.kind.as_str())
    }
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

/// The digest of the text at `site`, which a "keep as written" is bound to.
pub(crate) fn site_sha256(candidate: &Value, site: FindingSite) -> String {
    let part = match site {
        FindingSite::Lesson => Some(candidate),
        FindingSite::Section(index) => candidate.pointer(&format!("/blocks/{index}")),
        FindingSite::Question(index) => candidate.pointer(&format!("/questions/{index}")),
    };
    text_sha256(&part.map(Value::to_string).unwrap_or_default())
}

pub(crate) fn unit(candidate: &Value, finding: &ReviewFinding) -> Unit {
    let text = |value: Option<&Value>, field: &str| {
        value
            .and_then(|value| value.get(field))
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_owned)
    };
    let site = finding.key().site;
    let sha256 = site_sha256(candidate, site);
    match site {
        FindingSite::Lesson => Unit {
            title: "Whole lesson".into(),
            sha256,
            passage: None,
        },
        FindingSite::Section(index) => {
            let block = candidate.pointer(&format!("/blocks/{index}"));
            Unit {
                title: text(block, "title").map_or_else(
                    || format!("Section {}", index + 1),
                    |title| format!("Section {}: {title}", index + 1),
                ),
                sha256,
                passage: None,
            }
        }
        FindingSite::Question(index) => {
            let question = candidate.pointer(&format!("/questions/{index}"));
            Unit {
                title: format!("Question {}", index + 1),
                sha256,
                passage: if finding.kind == FindingKind::Quotation {
                    None
                } else {
                    text(question, "prompt")
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sites_come_from_the_application_s_own_indices() {
        assert_eq!(
            FindingSite::of_pointer("/blocks/3/title"),
            FindingSite::Section(3)
        );
        assert_eq!(
            FindingSite::of_pointer("/questions/1"),
            FindingSite::Question(1)
        );
        assert_eq!(FindingSite::of_pointer("/blocks"), FindingSite::Lesson);
        assert_eq!(FindingSite::of_pointer(""), FindingSite::Lesson);
        assert_eq!(
            FindingSite::of_part("blocks-10"),
            Some(FindingSite::Section(10))
        );
        assert_eq!(
            FindingSite::of_part("questions[2]"),
            Some(FindingSite::Question(2))
        );
        assert_eq!(FindingSite::of_part("reference-0"), None);
        assert_eq!(FindingSite::of_part("requirements"), None);
    }

    #[test]
    fn a_finding_is_keyed_by_kind_and_place_not_wording() {
        let first = ReviewFinding::new(
            FindingKind::Teaching,
            Some(FindingSite::Section(2)),
            "The example returns the string \"4\".",
        );
        let reworded = ReviewFinding::new(
            FindingKind::Teaching,
            Some(FindingSite::Section(2)),
            "Section 3 shows a string result where the code returns a number.",
        );
        assert_eq!(first.key(), reworded.key());
        assert_eq!(first.key().id(), "teaching:section-2");
        let answer =
            ReviewFinding::new(FindingKind::AnswerKey, Some(FindingSite::Question(2)), "x");
        assert_ne!(first.key(), answer.key());
        assert_eq!(
            ReviewFinding::new(FindingKind::Teaching, None, "lesson-wide")
                .key()
                .id(),
            "teaching:lesson"
        );
    }

    #[test]
    fn a_list_reads_as_plain_words() {
        let list =
            |items: &[&str]| join_plain(&items.iter().map(|i| (*i).to_owned()).collect::<Vec<_>>());
        assert_eq!(list(&[]), "");
        assert_eq!(list(&["30"]), "30");
        assert_eq!(list(&["30", "seconds"]), "30 and seconds");
        assert_eq!(
            list(&["30", "seconds", "retry once"]),
            "30, seconds and retry once"
        );
    }

    #[test]
    fn a_part_is_named_by_its_place_in_the_lesson_counted_from_one() {
        assert_eq!(plain_part(FindingSite::Section(2)), "section 3");
        assert_eq!(plain_part(FindingSite::Question(1)), "question 2");
        assert_eq!(plain_part(FindingSite::Lesson), "the lesson");
    }

    #[test]
    fn a_summary_is_set_apart_from_the_note_written_for_the_model() {
        let finding = ReviewFinding::new(FindingKind::Teaching, None, "Fix blocks[2].")
            .with_summary("The example is wrong.");
        assert_eq!(finding.note, "Fix blocks[2].");
        assert_eq!(finding.summary, "The example is wrong.");
        assert_eq!(
            ReviewFinding::new(FindingKind::Teaching, None, "x").summary,
            ""
        );
    }

    #[test]
    fn a_quotation_finding_is_not_handed_the_part_s_quote_as_its_passage() {
        let candidate = json!({"blocks":[{"title":"Sums","body":"Add.","quote":"The excerpt this whole section cites."}],"questions":[{"prompt":"What is 2+2?","quote":"The excerpt this question cites."}]});
        for site in [FindingSite::Section(0), FindingSite::Question(0)] {
            let finding = ReviewFinding::new(FindingKind::Quotation, Some(site), "x");
            assert_eq!(unit(&candidate, &finding).passage, None, "{site:?}");
        }
    }

    #[test]
    fn a_unit_s_hash_changes_with_its_text_only() {
        let candidate = json!({"blocks":[{"title":"Sums","body":"Add."},{"title":"Other","body":"B"}],"questions":[{"prompt":"What is 2+2?"}]});
        let finding = ReviewFinding::new(FindingKind::Teaching, Some(FindingSite::Section(0)), "x");
        let before = unit(&candidate, &finding);
        assert_eq!(before.title, "Section 1: Sums");
        let mut other_changed = candidate.clone();
        other_changed["blocks"][1]["body"] = json!("C");
        assert_eq!(unit(&other_changed, &finding).sha256, before.sha256);
        let mut changed = candidate.clone();
        changed["blocks"][0]["body"] = json!("Add twice.");
        assert_ne!(unit(&changed, &finding).sha256, before.sha256);
        let question =
            ReviewFinding::new(FindingKind::AnswerKey, Some(FindingSite::Question(0)), "x");
        assert_eq!(
            unit(&candidate, &question).passage.as_deref(),
            Some("What is 2+2?")
        );
    }
}
