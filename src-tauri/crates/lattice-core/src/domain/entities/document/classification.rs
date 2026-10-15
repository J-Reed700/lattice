//! Document status, language and category, with their display and parse forms.
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};

/// Document status in the indexing lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum DocumentStatus {
    /// Document is pending indexing
    #[default]
    Pending,
    /// Document is currently being processed
    Processing,
    /// Document has been successfully indexed
    Indexed,
    /// Document indexing failed
    Failed,
}

/// Language detected in document content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Language {
    English,
    Spanish,
    French,
    German,
    Chinese,
    Japanese,
    Python,
    Rust,
    JavaScript,
    TypeScript,
    Go,
    Java,
    Cpp,
    CSharp,
    #[default]
    Unknown,
}

/// Document category for organization
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Category {
    Code,
    Documentation,
    Research,
    Notes,
    Reference,
    Tutorial,
    #[default]
    Uncategorized,
}

impl std::fmt::Display for DocumentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Processing => write!(f, "processing"),
            Self::Indexed => write!(f, "indexed"),
            Self::Failed => write!(f, "failed"),
        }
    }
}

impl std::str::FromStr for DocumentStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "processing" => Ok(Self::Processing),
            "indexed" => Ok(Self::Indexed),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("Invalid document status: {}", s)),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::English => write!(f, "english"),
            Self::Spanish => write!(f, "spanish"),
            Self::French => write!(f, "french"),
            Self::German => write!(f, "german"),
            Self::Chinese => write!(f, "chinese"),
            Self::Japanese => write!(f, "japanese"),
            Self::Python => write!(f, "python"),
            Self::Rust => write!(f, "rust"),
            Self::JavaScript => write!(f, "javascript"),
            Self::TypeScript => write!(f, "typescript"),
            Self::Go => write!(f, "go"),
            Self::Java => write!(f, "java"),
            Self::Cpp => write!(f, "cpp"),
            Self::CSharp => write!(f, "csharp"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for Language {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "english" => Ok(Self::English),
            "spanish" => Ok(Self::Spanish),
            "french" => Ok(Self::French),
            "german" => Ok(Self::German),
            "chinese" => Ok(Self::Chinese),
            "japanese" => Ok(Self::Japanese),
            "python" => Ok(Self::Python),
            "rust" => Ok(Self::Rust),
            "javascript" => Ok(Self::JavaScript),
            "typescript" => Ok(Self::TypeScript),
            "go" => Ok(Self::Go),
            "java" => Ok(Self::Java),
            "cpp" => Ok(Self::Cpp),
            "csharp" => Ok(Self::CSharp),
            "unknown" => Ok(Self::Unknown),
            _ => Ok(Self::Unknown), // Default to Unknown for unrecognized languages
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Code => write!(f, "code"),
            Self::Documentation => write!(f, "documentation"),
            Self::Research => write!(f, "research"),
            Self::Notes => write!(f, "notes"),
            Self::Reference => write!(f, "reference"),
            Self::Tutorial => write!(f, "tutorial"),
            Self::Uncategorized => write!(f, "uncategorized"),
        }
    }
}

impl std::str::FromStr for Category {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "code" => Ok(Self::Code),
            "documentation" => Ok(Self::Documentation),
            "research" => Ok(Self::Research),
            "notes" => Ok(Self::Notes),
            "reference" => Ok(Self::Reference),
            "tutorial" => Ok(Self::Tutorial),
            "uncategorized" => Ok(Self::Uncategorized),
            _ => Ok(Self::Uncategorized), // Default to Uncategorized for unrecognized categories
        }
    }
}
