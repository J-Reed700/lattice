//! What the user attaches to a claim and which claims it is checked for
//! (UI design §5.3); `attach_learning_claim_evidence` carries both.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttachEvidenceDto {
    AttachSavedSource {
        #[serde(rename = "sourceVersionId")]
        source_version_id: String,
        #[serde(rename = "startByte")]
        start_byte: usize,
        #[serde(rename = "endByte")]
        end_byte: usize,
    },
    AttachLink {
        url: String,
    },
    AttachDocument {
        #[serde(rename = "documentId")]
        document_id: String,
    },
    AttachText {
        title: String,
        text: String,
    },
}

/// The claims the attached source is checked for: the one claim, or the
/// claims the user picked.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum AttachScopeDto {
    Claim,
    Claims {
        #[serde(rename = "claimIds")]
        claim_ids: Vec<String>,
    },
}
