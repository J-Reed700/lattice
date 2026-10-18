//! What kind of fact a claim asserts, which entity it is about and when it
//! holds (§6, §12.3), and the subject classes site overrides are scoped to.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use super::reason::Reason;

/// The kind of fact a claim asserts: it decides the claim's bar (§6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// How an API, standard, law, rule, protocol or interface is defined.
    Specification,
    /// A number, benchmark, statistic, rate or ranking.
    Measurement,
    /// Tied to a version or date: "since v7.2", "as of 2024".
    VersionBound,
    /// What a term means.
    Definition,
    /// A recommendation or convention: "you should".
    Practice,
    /// An evaluation: "popular", "best".
    Opinion,
    /// Events, dates, people, works.
    Historical,
    /// Widely known, no specifics.
    General,
}

wire_names!(ClaimKind {
    Specification => "specification",
    Measurement => "measurement",
    VersionBound => "version_bound",
    Definition => "definition",
    Practice => "practice",
    Opinion => "opinion",
    Historical => "historical",
    General => "general",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Product,
    Library,
    Standard,
    Organization,
    Law,
    Person,
    Work,
    Event,
    Concept,
    Place,
    Other,
}

wire_names!(SubjectKind {
    Product => "product",
    Library => "library",
    Standard => "standard",
    Organization => "organization",
    Law => "law",
    Person => "person",
    Work => "work",
    Event => "event",
    Concept => "concept",
    Place => "place",
    Other => "other",
});

/// A normalised subject or class name: the form equality is decided on
/// (`SubjectKey::of`). Never compared by substring.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectKey(pub String);

/// The entity a claim is about: the one whose owner could settle it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimSubject {
    pub name: String,
    pub kind: SubjectKind,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// The broader subject classes the subject belongs to, most specific
    /// first ("web platform APIs", "web development"). A site-for-subject
    /// override applies when its class is the subject, an alias or one of
    /// these.
    #[serde(default)]
    pub classes: Vec<String>,
    /// `SubjectKey::of(name)`.
    pub key: SubjectKey,
}

/// A class of subjects a site override is scoped to. Matching is by
/// normalised key: the class's key equals the subject's key, one of its
/// aliases' keys or one of its classes' keys, and the subject's kind is one
/// of `kinds` when `kinds` is not empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubjectClass {
    /// As the user saw it: "Web platform APIs".
    pub label: String,
    /// `SubjectKey::of(label)`.
    pub key: SubjectKey,
    /// Restricts the class to these subject kinds; empty for any kind.
    #[serde(default)]
    pub kinds: Vec<SubjectKind>,
}

/// When a claim holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum TemporalScope {
    Timeless,
    /// As of a date (unix seconds).
    AsOf {
        date: i64,
    },
    /// Since a version or a date.
    Since {
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        date: Option<i64>,
    },
    /// True of the current state of the subject.
    Current,
}

/// Judgement #3's validated output, per claim statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimProfile {
    pub kind: ClaimKind,
    #[serde(default)]
    pub subject: Option<ClaimSubject>,
    pub temporal: TemporalScope,
    /// For a measurement: who measured, when the claim names it. It becomes
    /// the subject the supports' authority is judged against (§6.1).
    #[serde(default)]
    pub measurement_origin: Option<String>,
    /// For an opinion or testimonial claim: whose view it reports.
    #[serde(default)]
    pub attributed_to: Option<String>,
    /// Empty for a judged profile; the cause when the judgement failed and
    /// the profile is the honest `General` fallback.
    #[serde(default)]
    pub reasons: Vec<Reason>,
    pub policy: String,
    pub model: String,
}

impl SubjectKey {
    /// The key of a subject or class name: Unicode NFKC, lower case, every
    /// character other than a letter, a digit, `#` or `+` (which tell `C#`
    /// and `C++` from `C`) read as a space, spaces collapsed.
    pub fn of(name: &str) -> Self {
        let mapped: String = name
            .nfkc()
            .flat_map(char::to_lowercase)
            .map(|c| {
                if c.is_alphanumeric() || c == '#' || c == '+' {
                    c
                } else {
                    ' '
                }
            })
            .collect();
        Self(mapped.split_whitespace().collect::<Vec<_>>().join(" "))
    }
}

impl ClaimSubject {
    /// A subject with its key computed from its name.
    pub fn new(
        name: impl Into<String>,
        kind: SubjectKind,
        aliases: Vec<String>,
        classes: Vec<String>,
    ) -> Self {
        let name = name.into();
        Self {
            key: SubjectKey::of(&name),
            name,
            kind,
            aliases,
            classes,
        }
    }

    /// The keys the subject answers to, most specific first: its own, its
    /// aliases', then its classes' in the order the profile gave them.
    pub fn keys(&self) -> Vec<SubjectKey> {
        let mut keys = vec![self.key.clone()];
        for name in self.aliases.iter().chain(self.classes.iter()) {
            let key = SubjectKey::of(name);
            if !key.0.is_empty() && !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }
}

impl SubjectClass {
    /// A class with its key computed from its label.
    pub fn new(label: impl Into<String>, kinds: Vec<SubjectKind>) -> Self {
        let label = label.into();
        Self {
            key: SubjectKey::of(&label),
            label,
            kinds,
        }
    }

    /// Whether `subject` is in the class, and how specifically: `Some(0)`
    /// when the class is the subject itself, larger for an alias or a
    /// broader class further down the subject's list, `None` when it is not
    /// in the class. Keys are compared whole, never as substrings, so the
    /// class "Python" does not take in "Python packaging".
    pub fn match_rank(&self, subject: &ClaimSubject) -> Option<usize> {
        if !self.kinds.is_empty() && !self.kinds.contains(&subject.kind) {
            return None;
        }
        subject.keys().iter().position(|key| *key == self.key)
    }
}

#[cfg(test)]
#[path = "claim_profile_tests.rs"]
mod tests;
