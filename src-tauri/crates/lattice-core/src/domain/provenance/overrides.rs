//! User overrides (§5 steps 1-3, §16, §19; decision 4 of §23).
//!
//! Three scopes:
//!
//! - **Source** (one source in one course) may set any level, Primary
//!   included: the user has looked at this source for this course.
//! - **Site** (a registrable domain, everywhere) sets *standing* only:
//!   "trust example.org" says who the publisher is, not what it owns, so it
//!   can never make a site Primary.
//! - **Site for a subject class** (a registrable domain for one
//!   [`SubjectClass`], everywhere) may set any level, Primary included:
//!   "treat MDN as primary for web platform APIs". Several sites may be
//!   primary for the same class. Primary stays a relation to a subject.

use serde::{Deserialize, Serialize};

use super::authority::TrustLevel;
use super::claim_profile::SubjectClass;

/// Where an override applies: the override table's key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum OverrideScope {
    /// One source (a `learning_source_library` id) in one course.
    Source {
        program_id: String,
        source_id: String,
    },
    /// A registrable domain, in every course.
    Site { registrable_domain: String },
    /// A registrable domain for claims whose subject is in `class`, in every
    /// course.
    SiteSubject {
        registrable_domain: String,
        class: SubjectClass,
    },
}

impl OverrideScope {
    /// The `scope` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Source { .. } => "source",
            Self::Site { .. } => "site",
            Self::SiteSubject { .. } => "site_subject",
        }
    }
}

/// The standings a site override may set. `Distrusted` excludes the site's
/// pages from every bar (§5 step 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SiteStanding {
    Institutional,
    Identified,
    Community,
    Distrusted,
}

wire_names!(SiteStanding {
    Institutional => "institutional",
    Identified => "identified",
    Community => "community",
    Distrusted => "distrusted",
});

/// One override as the user set it. Each scope carries the only kind of
/// value it may set, so a site-wide Primary cannot be represented.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum TrustOverride {
    Source {
        program_id: String,
        source_id: String,
        level: TrustLevel,
        #[serde(default)]
        note: Option<String>,
        set_at: i64,
    },
    Site {
        registrable_domain: String,
        standing: SiteStanding,
        #[serde(default)]
        note: Option<String>,
        set_at: i64,
    },
    SiteSubject {
        registrable_domain: String,
        class: SubjectClass,
        level: TrustLevel,
        #[serde(default)]
        note: Option<String>,
        set_at: i64,
    },
}

/// Which override fixed a level, as the level's reasons cite it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideRef {
    pub scope: OverrideScope,
    #[serde(default)]
    pub note: Option<String>,
}

/// The overrides in force for one course: its source overrides and every
/// site and site-for-subject override.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overrides {
    pub entries: Vec<TrustOverride>,
}
