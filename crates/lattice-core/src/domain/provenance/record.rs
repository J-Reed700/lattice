//! What capture stores about one source version (§16 `source_provenance`).

use serde::{Deserialize, Serialize};

use super::independence::Sketch;
use super::origin::Origin;
use super::signals::{OutboundLink, ProvenanceSignals};

/// Where the signals were read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractedFrom {
    Fetch,
    BrowserReader,
    Snapshot,
    Library,
    /// Saved text only (links, shingles and in-text dates).
    Text,
}

wire_names!(ExtractedFrom {
    Fetch => "fetch",
    BrowserReader => "browser_reader",
    Snapshot => "snapshot",
    Library => "library",
    Text => "text",
});

/// One version's provenance row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub source_version_id: String,
    pub program_id: String,
    pub origin: Origin,
    pub signals: ProvenanceSignals,
    /// sha256 of the canonical signals JSON: the standing judgement's key.
    pub signals_sha256: String,
    pub sketch: Sketch,
    /// Links in the version's text, for every kind of source.
    pub links: Vec<OutboundLink>,
    pub extracted_from: ExtractedFrom,
    pub extraction_version: String,
    pub extracted_at: i64,
}
