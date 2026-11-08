//! The pattern catalogue: the recurring shapes of defect that a drill plants
//! and a learner is asked to recognise, as data. Each pattern has a family, a
//! one-sentence definition, the material it applies to and the source that
//! names it.

use serde::{Deserialize, Serialize};

use super::drill::Material;

mod patterns;
pub use patterns::CATALOGUE;

/// The kind of problem a pattern is. The catalogue lists families in this
/// order, and a pattern's id begins with its family's slug. The first nine are
/// the families of code, the last five those of a passage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Correctness,
    Errors,
    Concurrency,
    Resources,
    Security,
    Performance,
    Design,
    Readability,
    Testing,
    Fidelity,
    Scope,
    Reasoning,
    Definition,
    Consistency,
}

impl Family {
    /// Every family, in the catalogue's order.
    pub const ALL: [Family; 14] = [
        Family::Correctness,
        Family::Errors,
        Family::Concurrency,
        Family::Resources,
        Family::Security,
        Family::Performance,
        Family::Design,
        Family::Readability,
        Family::Testing,
        Family::Fidelity,
        Family::Scope,
        Family::Reasoning,
        Family::Definition,
        Family::Consistency,
    ];

    /// The families of code, in the catalogue's order.
    pub const CODE: [Family; 9] = [
        Family::Correctness,
        Family::Errors,
        Family::Concurrency,
        Family::Resources,
        Family::Security,
        Family::Performance,
        Family::Design,
        Family::Readability,
        Family::Testing,
    ];

    /// The families of a passage, in the catalogue's order.
    pub const PASSAGE: [Family; 5] = [
        Family::Fidelity,
        Family::Scope,
        Family::Reasoning,
        Family::Definition,
        Family::Consistency,
    ];

    /// The family's name as it begins a pattern id: `security` in
    /// `security.sql_injection`.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Correctness => "correctness",
            Self::Errors => "errors",
            Self::Concurrency => "concurrency",
            Self::Resources => "resources",
            Self::Security => "security",
            Self::Performance => "performance",
            Self::Design => "design",
            Self::Readability => "readability",
            Self::Testing => "testing",
            Self::Fidelity => "fidelity",
            Self::Scope => "scope",
            Self::Reasoning => "reasoning",
            Self::Definition => "definition",
            Self::Consistency => "consistency",
        }
    }
}

/// What a pattern can be planted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Languages {
    /// Code in every language of the catalogue.
    Any,
    /// Only these: the pattern exists only where the language has the thing it
    /// is about.
    Only(&'static [&'static str]),
    /// Only an explanation of a passage, never code.
    Passage,
}

impl Languages {
    /// Whether a pattern with these languages applies to code in `language`.
    pub fn includes(self, language: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Only(languages) => languages.contains(&language),
            Self::Passage => false,
        }
    }
}

/// One recurring shape of defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Pattern {
    /// `family.slug`, unique in the catalogue: the family's slug, a dot, then
    /// the pattern's own name in lowercase with underscores.
    pub id: &'static str,
    pub family: Family,
    pub name: &'static str,
    /// One sentence: what the pattern is and why it is a problem.
    pub definition: &'static str,
    pub languages: Languages,
    /// Where the pattern is named: a book and its chapter or entry, a paper, a
    /// CWE id, or the standard term.
    pub source: &'static str,
}

impl Pattern {
    /// Whether the pattern can be planted in `material`.
    pub fn applies_to(&self, material: &Material) -> bool {
        match material {
            Material::Code { language } => self.languages.includes(language),
            Material::Passage => self.languages == Languages::Passage,
        }
    }
}

/// The language ids a drill can be written in.
pub const LANGUAGES: [&str; 15] = [
    "python",
    "javascript",
    "typescript",
    "rust",
    "go",
    "java",
    "csharp",
    "cpp",
    "c",
    "kotlin",
    "swift",
    "ruby",
    "php",
    "sql",
    "shell",
];

/// The pattern with this id.
pub fn pattern(id: &str) -> Option<&'static Pattern> {
    CATALOGUE.iter().find(|pattern| pattern.id == id)
}

/// The patterns that apply to `material`, in the catalogue's order.
pub fn patterns_for(material: &Material) -> impl Iterator<Item = &'static Pattern> + '_ {
    CATALOGUE
        .iter()
        .filter(move |pattern| pattern.applies_to(material))
}

/// The general-purpose programming languages: every language but SQL and shell.
const PROGRAMMING: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "rust",
    "go",
    "java",
    "csharp",
    "cpp",
    "c",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// The languages where a value can be null: all of them but Rust, whose
/// absent values are an `Option` that has to be handled.
const NULLABLE: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "go",
    "java",
    "csharp",
    "cpp",
    "c",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// The languages whose integers have a fixed width.
const FIXED_WIDTH_INTEGERS: &[&str] = &[
    "c", "cpp", "csharp", "go", "java", "kotlin", "rust", "swift",
];

/// The languages with `async` and `await`, or a deferred value to await.
const ASYNC_AWAIT: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "csharp",
    "rust",
    "swift",
    "kotlin",
];

/// The languages where a call reports failure by a returned status or value,
/// not only by an exception.
const ERRORS_AS_VALUES: &[&str] = &["c", "cpp", "go", "rust", "php", "shell"];

/// The languages a web application is commonly written in.
const WEB: &[&str] = &[
    "javascript",
    "typescript",
    "python",
    "ruby",
    "php",
    "java",
    "csharp",
    "kotlin",
    "go",
];

/// The languages whose standard libraries or common formats deserialise
/// arbitrary types.
const OBJECT_DESERIALISATION: &[&str] = &[
    "python",
    "java",
    "csharp",
    "php",
    "ruby",
    "kotlin",
    "javascript",
    "typescript",
];

/// The languages that evaluate a string as code.
const EVAL: &[&str] = &["python", "javascript", "typescript", "ruby", "php", "shell"];

/// The languages with manual memory management.
const MANUAL_MEMORY: &[&str] = &["c", "cpp"];

/// The languages with exceptions.
const EXCEPTIONS: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "java",
    "csharp",
    "cpp",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// The languages with classes, or types that carry methods: the programming
/// languages but C.
const CLASSES: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "rust",
    "go",
    "java",
    "csharp",
    "cpp",
    "kotlin",
    "swift",
    "ruby",
    "php",
];

/// The languages with collection pipelines: filter, map and reduce over a
/// collection.
const PIPELINES: &[&str] = &[
    "python",
    "javascript",
    "typescript",
    "rust",
    "java",
    "csharp",
    "kotlin",
    "swift",
    "ruby",
    "php",
];
