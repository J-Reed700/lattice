//! # Settings Contracts
//!
//! Data Transfer Objects for settings management operations.
//!
//! These DTOs handle requests and responses for application settings management,
//! including retrieval, updates, import/export, and validation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

mod generation;
mod llm;
mod preferences;
mod retrieval;
mod sourcing;
mod sourcing_status;

pub use generation::{
    presence_penalty_problem, LocalModelConfigDto, SamplingProfileDto, SpeculativeDecodingDto,
    ThinkingOverrideDto, DEFAULT_MTP_DRAFT_N_MAX, MAX_MTP_DRAFT_N_MAX,
};

pub use llm::{
    CustomToolSettingsDto, LLMPromptSettingsDto, LLMProvider, LLMSettingsDto,
    LLMVerificationSettingsDto, LlamaCppSettingsDto, RouterSettingsDto, ToolOutputSettingsDto,
    ToolOutputTemplatesDto, VisionDeclarationDto, DEFAULT_LLM_STALL_TIMEOUT_SECONDS,
    MAX_LLM_STALL_TIMEOUT_SECONDS, MIN_LLM_STALL_TIMEOUT_SECONDS,
};
pub use preferences::{
    BackupSettingsDto, OnboardingSettingsDto, PrivacySettingsDto, SyncSettingsDto, UISettingsDto,
    VaultSettingsDto,
};
pub use retrieval::{
    EmbeddingStrategySettingDto, IndexingSettingsDto, RetrievalTuningSettingsDto,
    SearchSettingsDto, VectorIndexCompressionModeDto, VectorIndexCompressionSettingsDto,
    VectorQuantizationDto, DEFAULT_VECTOR_COMPRESSION_DIMS, MAX_VECTOR_COMPRESSION_DIMS,
    MIN_VECTOR_COMPRESSION_DIMS,
};
pub use sourcing::{FinderSettingDto, SourcingSettingsDto};
pub use sourcing_status::{
    FinderGateDto, FinderHealthDto, FinderHealthStateDto, FinderNeedDto, FinderStatusDto,
    KeyServiceDto, KeyStatusDto, ReachGroupDto, SourcesStatusDto,
};

/// Complete application settings structure.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct SettingsDto {
    /// Indexing configuration
    pub indexing: IndexingSettingsDto,

    /// Search configuration
    pub search: SearchSettingsDto,

    /// LLM configuration
    pub llm: LLMSettingsDto,

    /// UI configuration
    pub ui: UISettingsDto,

    /// Sync configuration
    pub sync: SyncSettingsDto,

    /// Backup configuration
    pub backup: BackupSettingsDto,

    /// Privacy configuration (telemetry, crash reporting)
    #[serde(default)]
    pub privacy: PrivacySettingsDto,

    #[serde(default)]
    pub vault: VaultSettingsDto,

    /// Onboarding state (what the user has already been shown)
    #[serde(default)]
    pub onboarding: OnboardingSettingsDto,

    /// Source finding: finder switches, the SearXNG instance, the polite-pool
    /// contact and the archive fallback. Keys live in the keyring.
    #[serde(default)]
    pub sourcing: SourcingSettingsDto,
}

/// Settings category identifier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum SettingsCategory {
    /// Indexing settings
    Indexing,
    /// Search settings
    Search,
    /// LLM settings
    Llm,
    /// UI settings
    Ui,
    /// Sync settings
    Sync,
    /// Backup settings
    Backup,
    /// Privacy settings (telemetry + crash reporting)
    Privacy,
    /// Vault portability (markdown mirror folder)
    Vault,
    /// Onboarding state (first-run gate)
    Onboarding,
    /// Source finding (Settings → Sources)
    Sourcing,
}

impl SettingsCategory {
    /// Get all categories.
    pub fn all() -> Vec<SettingsCategory> {
        vec![
            SettingsCategory::Indexing,
            SettingsCategory::Search,
            SettingsCategory::Llm,
            SettingsCategory::Ui,
            SettingsCategory::Sync,
            SettingsCategory::Backup,
            SettingsCategory::Privacy,
            SettingsCategory::Vault,
            SettingsCategory::Onboarding,
            SettingsCategory::Sourcing,
        ]
    }

    /// Convert to string key.
    pub fn as_str(&self) -> &'static str {
        match self {
            SettingsCategory::Indexing => "indexing",
            SettingsCategory::Search => "search",
            SettingsCategory::Llm => "llm",
            SettingsCategory::Ui => "ui",
            SettingsCategory::Sync => "sync",
            SettingsCategory::Backup => "backup",
            SettingsCategory::Privacy => "privacy",
            SettingsCategory::Vault => "vault",
            SettingsCategory::Onboarding => "onboarding",
            SettingsCategory::Sourcing => "sourcing",
        }
    }
}

impl FromStr for SettingsCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "indexing" => Ok(SettingsCategory::Indexing),
            "search" => Ok(SettingsCategory::Search),
            "llm" => Ok(SettingsCategory::Llm),
            "ui" => Ok(SettingsCategory::Ui),
            "sync" => Ok(SettingsCategory::Sync),
            "backup" => Ok(SettingsCategory::Backup),
            "privacy" => Ok(SettingsCategory::Privacy),
            "vault" => Ok(SettingsCategory::Vault),
            "onboarding" => Ok(SettingsCategory::Onboarding),
            "sourcing" => Ok(SettingsCategory::Sourcing),
            _ => Err(format!("Unknown settings category: {}", s)),
        }
    }
}

/// Request to update settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequestDto {
    /// Category to update (optional - updates all if not specified)
    pub category: Option<SettingsCategory>,

    /// Settings updates as key-value pairs
    pub updates: HashMap<String, serde_json::Value>,
}

/// Request to reset settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResetSettingsRequestDto {
    /// Category to reset (optional - resets all if not specified)
    pub category: Option<SettingsCategory>,
}

/// Request to export settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettingsRequestDto {
    /// Path where settings should be exported
    pub path: String,
}

/// Response from export operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportSettingsResponseDto {
    /// Path where settings were exported
    pub path: PathBuf,

    /// Status message
    pub status: String,
}

/// Request to import settings.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettingsRequestDto {
    /// Path to settings file
    pub path: String,

    /// Merge with existing settings instead of replacing
    pub merge: bool,
}

/// Response from import operation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettingsResponseDto {
    /// Updated settings
    pub settings: SettingsDto,

    /// Status message
    pub status: String,
}

/// Result of settings validation.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    /// Overall validation success
    pub valid: bool,

    /// Validation errors by category
    pub errors: HashMap<String, Vec<String>>,

    /// Validation warnings
    pub warnings: HashMap<String, Vec<String>>,
}

impl ValidationResult {
    /// Create a successful validation result.
    pub fn success() -> Self {
        Self {
            valid: true,
            errors: HashMap::new(),
            warnings: HashMap::new(),
        }
    }

    /// Create a failed validation result.
    pub fn failure() -> Self {
        Self {
            valid: false,
            errors: HashMap::new(),
            warnings: HashMap::new(),
        }
    }

    /// Add an error to a category.
    pub fn add_error(&mut self, category: &str, error: String) {
        self.valid = false;
        self.errors
            .entry(category.to_string())
            .or_default()
            .push(error);
    }

    /// Add a warning to a category.
    pub fn add_warning(&mut self, category: &str, warning: String) {
        self.warnings
            .entry(category.to_string())
            .or_default()
            .push(warning);
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Check if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod memory_defaults_tests;
