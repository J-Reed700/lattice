//! The trust rows a course pack carries with its sources (§16): every
//! version's provenance row, the relations between them, and the course's
//! source overrides. Judgements are not carried (they depend on the model
//! and are re-derived after import), nor are site overrides (they are the
//! user's own, across courses) or outcomes (calibration is the user's).

use lattice_core::domain::provenance::{
    Origin, ProvenanceRecord, RelationEvidence, RelationTarget, SourceRelation, TrustOverride,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// The layout of [`SourceTrustPackEntry`]; a pack with another is refused.
pub const SOURCE_TRUST_PACK_VERSION: u32 = 1;

/// One relation with when it was detected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackRelation {
    pub relation: SourceRelation,
    pub detected_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceTrustPackEntry {
    pub version: u32,
    pub provenance: Vec<ProvenanceRecord>,
    pub relations: Vec<PackRelation>,
    /// Source-scope overrides only.
    pub overrides: Vec<TrustOverride>,
}

impl SourceTrustPackEntry {
    /// Refuses an entry this build cannot restore faithfully.
    pub fn check(&self) -> Result<()> {
        if self.version != SOURCE_TRUST_PACK_VERSION {
            return Err(AppError::InvalidInput(
                "The pack's source provenance uses a layout this version cannot read.".into(),
            ));
        }
        if self
            .overrides
            .iter()
            .any(|entry| !matches!(entry, TrustOverride::Source { .. }))
        {
            return Err(AppError::InvalidInput(
                "A pack carries only overrides for its own sources.".into(),
            ));
        }
        Ok(())
    }

    /// The entry as the importing course writes it: its program id, and
    /// every source, version and claim id through `remap` (the identity for
    /// a pack restored in place, fresh ids for a copy). Pasted text's origin
    /// is its version, so it follows the version's new id.
    pub fn remapped(mut self, program_id: &str, remap: impl Fn(&str) -> String) -> Self {
        for record in &mut self.provenance {
            let old_id = std::mem::take(&mut record.source_version_id);
            record.source_version_id = remap(&old_id);
            record.program_id = program_id.to_owned();
            if record.origin.key == Origin::pasted(&old_id).key {
                record.origin = Origin::pasted(&record.source_version_id);
            }
        }
        for PackRelation { relation, .. } in &mut self.relations {
            relation.from = remap(&relation.from);
            if let RelationTarget::Version(id) = &mut relation.to {
                *id = remap(id);
            }
            if let RelationEvidence::ClaimCitation { claim_id, .. } = &mut relation.evidence {
                *claim_id = remap(claim_id);
            }
        }
        for entry in &mut self.overrides {
            if let TrustOverride::Source {
                program_id: program,
                source_id,
                ..
            } = entry
            {
                *program = program_id.to_owned();
                *source_id = remap(source_id);
            }
        }
        self
    }
}
