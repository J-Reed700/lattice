//! Versioned, checksummed Learning Studio pack codec.
//!
//! The codec is intentionally independent from database import. A caller first
//! decodes and previews this bounded representation, resolves every conflict,
//! and only then writes through the domain repositories. Archive paths are
//! treated as hostile input on every platform.

use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path};

pub mod sections;
use sections::{SectionPath, KEYS_ENTRY, PRACTICAL_ENTRY, SECTIONS, SOURCE_BODY_PREFIX};

pub const LEARNING_PACK_FORMAT: &str = "lattice.learning-pack";
// v4 stores learner evidence one table per entry and canvases one per entry,
// and records the database schema that wrote it; v5 adds the lesson claim
// reviews, one table per entry. Only this version imports:
// both computers run the same build, which `schema_version` proves.
pub const LEARNING_PACK_VERSION: u32 = 5;
const MAX_ENTRIES: usize = 4_096;
const MIB: usize = 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 32 * MIB;
const MAX_TOTAL_BYTES: usize = 256 * MIB;
const MAX_COMPRESSED_BYTES: usize = 256 * MIB;
const MANIFEST_PATH: &str = "manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningPackEntryKind {
    Program,
    Curriculum,
    SourceMetadata,
    SourceExcerpt,
    SourceBody,
    Notebook,
    Canvas,
    Recall,
    Attempt,
    Evidence,
    PracticalActivity,
    PracticalArtifact,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackPrivacyManifest {
    pub includes_private_chat: bool,
    pub includes_credentials: bool,
    pub includes_full_source_bodies: bool,
    pub source_body_redistribution_confirmed: bool,
    pub includes_answer_keys: bool,
    pub includes_hidden_evaluators: bool,
    pub includes_learner_evidence: bool,
    pub includes_practical_artifacts: bool,
    pub omitted_items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackManifestEntry {
    pub path: String,
    pub kind: LearningPackEntryKind,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningPackManifest {
    pub format: String,
    pub version: u32,
    /// Highest applied database migration on the exporting computer.
    pub schema_version: i64,
    pub pack_id: String,
    pub title: String,
    pub created_at: i64,
    pub application_version: String,
    pub root_sha256: String,
    pub privacy: LearningPackPrivacyManifest,
    pub entries: Vec<LearningPackManifestEntry>,
}

/// The two manifest fields every pack version has.
#[derive(Deserialize)]
struct ManifestStamp {
    format: String,
    version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearningPackEntry {
    pub path: String,
    pub kind: LearningPackEntryKind,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct LearningPackInput {
    pub schema_version: i64,
    pub pack_id: String,
    pub title: String,
    pub created_at: i64,
    pub application_version: String,
    pub privacy: LearningPackPrivacyManifest,
    pub entries: Vec<LearningPackEntry>,
}

#[derive(Debug, Clone)]
pub struct DecodedLearningPack {
    pub manifest: LearningPackManifest,
    pub entries: BTreeMap<String, Vec<u8>>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Validate against both Unix and Windows path syntax even when decoding on a
/// different host. Packs contain ordinary files only, never directory entries.
pub fn validate_pack_path(value: &str) -> Result<()> {
    if value.is_empty()
        || value.chars().count() > 240
        || value.contains('\0')
        || value.contains('\\')
        || value.contains("//")
        || value.starts_with('/')
        || value.contains(':')
    {
        return Err(invalid("A learning-pack entry has an unsafe path."));
    }
    if value == MANIFEST_PATH {
        return Err(invalid("Pack content cannot replace its manifest."));
    }
    let mut count = 0;
    for component in Path::new(value).components() {
        match component {
            Component::Normal(value) if !value.is_empty() => count += 1,
            _ => return Err(invalid("A learning-pack entry cannot escape its archive.")),
        }
    }
    if count == 0 {
        return Err(invalid("A learning-pack entry must name a file."));
    }
    Ok(())
}

fn validate_privacy(privacy: &LearningPackPrivacyManifest) -> Result<()> {
    if privacy.includes_private_chat || privacy.includes_credentials {
        return Err(invalid(
            "Learning packs cannot contain private chat history or credentials.",
        ));
    }
    if privacy.includes_full_source_bodies && !privacy.source_body_redistribution_confirmed {
        return Err(invalid(
            "Full source bodies require explicit redistribution confirmation.",
        ));
    }
    if privacy.omitted_items.len() > 1_000
        || privacy
            .omitted_items
            .iter()
            .any(|item| item.chars().count() > 500)
    {
        return Err(invalid("The pack privacy summary is too large."));
    }
    Ok(())
}

fn validate_manifest_metadata(manifest: &LearningPackManifest) -> Result<()> {
    if manifest.title.trim().is_empty() || manifest.title.chars().count() > 160 {
        return Err(invalid(
            "A learning-pack title must contain 1–160 characters.",
        ));
    }
    if manifest.application_version.trim().is_empty()
        || manifest.application_version.chars().count() > 80
    {
        return Err(invalid(
            "A learning pack needs a bounded application version.",
        ));
    }
    if manifest.created_at < 0
        || manifest.created_at
            > chrono::Utc::now()
                .timestamp_millis()
                .saturating_add(366 * 24 * 60 * 60 * 1_000)
    {
        return Err(invalid(
            "A learning-pack creation time is outside its allowed range.",
        ));
    }
    validate_schema_version(manifest.schema_version)
}

fn validate_schema_version(schema_version: i64) -> Result<()> {
    if schema_version <= 0 {
        return Err(invalid(
            "A learning pack must name the database schema that wrote it.",
        ));
    }
    Ok(())
}

/// A path a section owns carries exactly the kind that section declares, and
/// a folder section's entries sit directly inside it. The importer derives an
/// entry's meaning from its path, so a harmless-looking entry can never be
/// read as a different payload.
fn validate_section_path(path: &str, kind: &LearningPackEntryKind) -> Result<()> {
    for descriptor in SECTIONS {
        let name = match descriptor.path {
            SectionPath::Entry(entry) if entry == path => None,
            SectionPath::Entry(_) => continue,
            SectionPath::Folder { prefix, suffix } => match path.strip_prefix(prefix) {
                Some(name) => Some(name.strip_suffix(suffix).unwrap_or("")),
                None => continue,
            },
        };
        if kind != &descriptor.kind {
            return Err(invalid(
                "A reserved learning-pack path has an invalid entry kind.",
            ));
        }
        if name.is_some_and(|stem| stem.is_empty() || stem.contains('/')) {
            return Err(invalid(
                "A learning-pack entry in a reserved folder has an invalid name.",
            ));
        }
    }
    Ok(())
}

fn validate_source_body_policy(
    privacy: &LearningPackPrivacyManifest,
    has_source_body: bool,
) -> Result<()> {
    if privacy.includes_full_source_bodies != has_source_body {
        return Err(invalid(
            "The source-body privacy declaration must match the pack entries.",
        ));
    }
    if has_source_body && !privacy.source_body_redistribution_confirmed {
        return Err(invalid(
            "Full source bodies require explicit redistribution confirmation.",
        ));
    }
    Ok(())
}

fn validate_content_policy(
    privacy: &LearningPackPrivacyManifest,
    entries: impl IntoIterator<Item = (String, LearningPackEntryKind)>,
) -> Result<()> {
    let entries: Vec<(String, LearningPackEntryKind)> = entries.into_iter().collect();
    let has_keys = entries.iter().any(|(path, _)| path == KEYS_ENTRY);
    let has_evidence = entries.iter().any(|(path, kind)| {
        matches!(
            kind,
            LearningPackEntryKind::Attempt
                | LearningPackEntryKind::Evidence
                | LearningPackEntryKind::Canvas
        ) || (kind == &LearningPackEntryKind::PracticalArtifact && path == PRACTICAL_ENTRY)
    });
    let has_practical = entries.iter().any(|(_, kind)| {
        matches!(
            kind,
            LearningPackEntryKind::PracticalActivity | LearningPackEntryKind::PracticalArtifact
        )
    });
    if privacy.includes_hidden_evaluators {
        return Err(invalid(
            "Learning packs cannot contain hidden evaluator files.",
        ));
    }
    if privacy.includes_answer_keys != has_keys {
        return Err(invalid(
            "The answer-key privacy declaration must match the pack entries.",
        ));
    }
    if privacy.includes_learner_evidence != has_evidence {
        return Err(invalid(
            "The learner-evidence privacy declaration must match the pack entries.",
        ));
    }
    if privacy.includes_practical_artifacts != has_practical {
        return Err(invalid(
            "The practical-artifact privacy declaration must match the pack entries.",
        ));
    }
    for (path, _) in &entries {
        let lower = path.to_ascii_lowercase();
        if lower.contains("evaluator") || lower.contains("solution") || lower.contains("check-key")
        {
            return Err(invalid(
                "A learning-pack entry exposes a protected evaluator or solution payload.",
            ));
        }
    }
    for (path, kind) in &entries {
        if path.starts_with(SOURCE_BODY_PREFIX) != (kind == &LearningPackEntryKind::SourceBody) {
            return Err(invalid("Source-body entry paths and kinds must agree."));
        }
        validate_section_path(path, kind)?;
        if matches!(kind, LearningPackEntryKind::SourceBody) {
            let name = path.strip_prefix(SOURCE_BODY_PREFIX).unwrap_or("");
            if name.len() != 68
                || !name.ends_with(".txt")
                || !name
                    .get(..64)
                    .is_some_and(|prefix| prefix.bytes().all(|byte| byte.is_ascii_hexdigit()))
            {
                return Err(invalid("A source-body entry has an invalid content path."));
            }
        }
    }
    Ok(())
}

fn root_digest(schema_version: i64, entries: &[LearningPackManifestEntry]) -> String {
    let mut hasher = Sha256::new();
    // The schema stamp is part of the checksummed root, like every entry.
    hasher.update(b"schema");
    hasher.update([0]);
    hasher.update(schema_version.to_be_bytes());
    hasher.update([b'\n']);
    for entry in entries {
        hasher.update(entry.path.as_bytes());
        hasher.update([0]);
        hasher.update(entry.sha256.as_bytes());
        hasher.update([0]);
        hasher.update(entry.bytes.to_be_bytes());
        hasher.update([0]);
        let kind: &[u8] = match entry.kind {
            LearningPackEntryKind::Program => b"program",
            LearningPackEntryKind::Curriculum => b"curriculum",
            LearningPackEntryKind::SourceMetadata => b"source_metadata",
            LearningPackEntryKind::SourceExcerpt => b"source_excerpt",
            LearningPackEntryKind::SourceBody => b"source_body",
            LearningPackEntryKind::Notebook => b"notebook",
            LearningPackEntryKind::Canvas => b"canvas",
            LearningPackEntryKind::Recall => b"recall",
            LearningPackEntryKind::Attempt => b"attempt",
            LearningPackEntryKind::Evidence => b"evidence",
            LearningPackEntryKind::PracticalActivity => b"practical_activity",
            LearningPackEntryKind::PracticalArtifact => b"practical_artifact",
        };
        hasher.update(kind);
        hasher.update([b'\n']);
    }
    format!("{:x}", hasher.finalize())
}

fn append_tar_file<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    bytes: &[u8],
) -> Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_mode(0o600);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(bytes.len() as u64);
    header.set_cksum();
    builder
        .append_data(&mut header, path, Cursor::new(bytes))
        .map_err(|error| AppError::Serialization(error.to_string()))
}

pub fn encode_learning_pack(mut input: LearningPackInput) -> Result<Vec<u8>> {
    uuid::Uuid::parse_str(&input.pack_id)
        .map_err(|_| invalid("A learning pack needs a valid pack ID."))?;
    if input.created_at < 0
        || input.created_at
            > chrono::Utc::now()
                .timestamp_millis()
                .saturating_add(366 * 24 * 60 * 60 * 1_000)
    {
        return Err(invalid(
            "A learning-pack creation time is outside its allowed range.",
        ));
    }
    if input.title.trim().is_empty() || input.title.chars().count() > 160 {
        return Err(invalid(
            "A learning-pack title must contain 1–160 characters.",
        ));
    }
    if input.application_version.trim().is_empty() || input.application_version.chars().count() > 80
    {
        return Err(invalid(
            "A learning pack needs a bounded application version.",
        ));
    }
    validate_schema_version(input.schema_version)?;
    validate_privacy(&input.privacy)?;
    if input.entries.is_empty() || input.entries.len() > MAX_ENTRIES {
        return Err(invalid(format!(
            "A learning pack must contain 1–{MAX_ENTRIES} entries."
        )));
    }
    input
        .entries
        .sort_by(|left, right| left.path.cmp(&right.path));
    let mut seen = HashSet::new();
    let mut total = 0usize;
    let mut manifest_entries = Vec::with_capacity(input.entries.len());
    for entry in &input.entries {
        validate_pack_path(&entry.path)?;
        if !seen.insert(entry.path.as_str()) {
            return Err(invalid("A learning pack contains duplicate paths."));
        }
        if entry.bytes.len() > MAX_ENTRY_BYTES {
            return Err(invalid(format!(
                "Learning-pack entry '{}' exceeds {} MiB.",
                entry.path,
                MAX_ENTRY_BYTES / MIB
            )));
        }
        total = total.saturating_add(entry.bytes.len());
        if total > MAX_TOTAL_BYTES {
            return Err(invalid(format!(
                "Learning-pack content exceeds {} MiB.",
                MAX_TOTAL_BYTES / MIB
            )));
        }
        if entry.kind == LearningPackEntryKind::SourceBody
            && !input.privacy.source_body_redistribution_confirmed
        {
            return Err(invalid(
                "A source body was included without redistribution confirmation.",
            ));
        }
        manifest_entries.push(LearningPackManifestEntry {
            path: entry.path.clone(),
            kind: entry.kind.clone(),
            sha256: sha256(&entry.bytes),
            bytes: entry.bytes.len() as u64,
        });
    }
    validate_source_body_policy(
        &input.privacy,
        input
            .entries
            .iter()
            .any(|entry| entry.kind == LearningPackEntryKind::SourceBody),
    )?;
    validate_content_policy(
        &input.privacy,
        input
            .entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.kind.clone())),
    )?;
    let manifest = LearningPackManifest {
        format: LEARNING_PACK_FORMAT.into(),
        version: LEARNING_PACK_VERSION,
        schema_version: input.schema_version,
        pack_id: input.pack_id,
        title: input.title.trim().into(),
        created_at: input.created_at,
        application_version: input.application_version.trim().into(),
        root_sha256: root_digest(input.schema_version, &manifest_entries),
        privacy: input.privacy,
        entries: manifest_entries,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let encoder = zstd::stream::write::Encoder::new(Vec::new(), 9)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let mut archive = tar::Builder::new(encoder);
    archive.mode(tar::HeaderMode::Deterministic);
    append_tar_file(&mut archive, MANIFEST_PATH, &manifest_bytes)?;
    for entry in &input.entries {
        append_tar_file(&mut archive, &entry.path, &entry.bytes)?;
    }
    let encoder = archive
        .into_inner()
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    let compressed = encoder
        .finish()
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    if compressed.len() > MAX_COMPRESSED_BYTES {
        return Err(invalid(format!(
            "Compressed learning pack exceeds {} MiB.",
            MAX_COMPRESSED_BYTES / MIB
        )));
    }
    Ok(compressed)
}

pub fn decode_learning_pack(compressed: &[u8]) -> Result<DecodedLearningPack> {
    if compressed.is_empty() || compressed.len() > MAX_COMPRESSED_BYTES {
        return Err(invalid("Learning-pack archive size is invalid."));
    }
    let decoder = zstd::stream::read::Decoder::new(Cursor::new(compressed))
        .map_err(|error| AppError::InvalidData(error.to_string()))?;
    // The extra byte lets us distinguish an exactly-full valid payload from a
    // decompression stream that attempts to exceed the advertised ceiling.
    let limited = decoder.take((MAX_TOTAL_BYTES + MAX_ENTRY_BYTES + 1) as u64);
    let mut archive = tar::Archive::new(limited);
    let entries = archive
        .entries()
        .map_err(|error| AppError::InvalidData(error.to_string()))?;
    let mut manifest_bytes = None;
    let mut content = BTreeMap::new();
    let mut total = 0usize;
    let mut count = 0usize;
    for entry in entries {
        let mut entry = entry.map_err(|error| AppError::InvalidData(error.to_string()))?;
        if !entry.header().entry_type().is_file() {
            return Err(invalid("Learning packs may contain only ordinary files."));
        }
        let path = entry
            .path()
            .map_err(|error| AppError::InvalidData(error.to_string()))?
            .to_str()
            .ok_or_else(|| invalid("A learning-pack path is not valid Unicode."))?
            .to_owned();
        if path != MANIFEST_PATH {
            validate_pack_path(&path)?;
        }
        if entry.size() as usize > MAX_ENTRY_BYTES && path != MANIFEST_PATH {
            return Err(invalid(format!(
                "A learning-pack entry exceeds {} MiB.",
                MAX_ENTRY_BYTES / MIB
            )));
        }
        let maximum = if path == MANIFEST_PATH {
            2 * 1024 * 1024
        } else {
            MAX_ENTRY_BYTES
        };
        let mut bytes = Vec::with_capacity((entry.size() as usize).min(maximum));
        entry
            .by_ref()
            .take((maximum + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > maximum {
            return Err(invalid("A learning-pack entry exceeds its size limit."));
        }
        total = total.saturating_add(bytes.len());
        if total > MAX_TOTAL_BYTES {
            return Err(invalid(format!(
                "Decompressed learning-pack content exceeds {} MiB.",
                MAX_TOTAL_BYTES / MIB
            )));
        }
        count += 1;
        if count > MAX_ENTRIES + 1 {
            return Err(invalid("A learning pack contains too many entries."));
        }
        if path == MANIFEST_PATH {
            if manifest_bytes.replace(bytes).is_some() {
                return Err(invalid("A learning pack contains multiple manifests."));
            }
        } else if content.insert(path, bytes).is_some() {
            return Err(invalid("A learning pack contains duplicate paths."));
        }
    }
    let manifest_bytes = manifest_bytes
        .as_deref()
        .ok_or_else(|| invalid("A learning pack has no manifest."))?;
    // Read the version before the rest: an older pack lacks fields this
    // manifest requires, and should be told it is too old, not malformed.
    let stamp: ManifestStamp = serde_json::from_slice(manifest_bytes)
        .map_err(|error| AppError::InvalidData(error.to_string()))?;
    if stamp.format != LEARNING_PACK_FORMAT || stamp.version != LEARNING_PACK_VERSION {
        return Err(invalid("This learning-pack version is not supported."));
    }
    let manifest: LearningPackManifest = serde_json::from_slice(manifest_bytes)
        .map_err(|error| AppError::InvalidData(error.to_string()))?;
    uuid::Uuid::parse_str(&manifest.pack_id)
        .map_err(|_| invalid("The learning-pack manifest has an invalid ID."))?;
    validate_privacy(&manifest.privacy)?;
    validate_manifest_metadata(&manifest)?;
    if manifest.entries.len() != content.len() || manifest.entries.len() > MAX_ENTRIES {
        return Err(invalid(
            "Learning-pack content does not match its manifest.",
        ));
    }
    let mut sorted = manifest.entries.clone();
    sorted.sort_by(|left, right| left.path.cmp(&right.path));
    if sorted != manifest.entries
        || root_digest(manifest.schema_version, &manifest.entries) != manifest.root_sha256
    {
        return Err(invalid("The learning-pack root checksum is invalid."));
    }
    let mut seen = HashSet::new();
    let has_source_body = manifest
        .entries
        .iter()
        .any(|entry| entry.kind == LearningPackEntryKind::SourceBody);
    validate_source_body_policy(&manifest.privacy, has_source_body)?;
    validate_content_policy(
        &manifest.privacy,
        manifest
            .entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.kind.clone())),
    )?;
    for expected in &manifest.entries {
        validate_pack_path(&expected.path)?;
        if !seen.insert(expected.path.as_str()) {
            return Err(invalid(
                "The learning-pack manifest contains duplicate paths.",
            ));
        }
        let bytes = content
            .get(&expected.path)
            .ok_or_else(|| invalid("A learning-pack entry is missing."))?;
        if bytes.len() as u64 != expected.bytes || sha256(bytes) != expected.sha256 {
            return Err(invalid(format!(
                "Learning-pack entry '{}' failed checksum validation.",
                expected.path
            )));
        }
        if expected.kind == LearningPackEntryKind::SourceBody
            && !manifest.privacy.source_body_redistribution_confirmed
        {
            return Err(invalid("A source body lacks redistribution confirmation."));
        }
    }
    Ok(DecodedLearningPack {
        manifest,
        entries: content,
    })
}

#[cfg(test)]
mod tests;
