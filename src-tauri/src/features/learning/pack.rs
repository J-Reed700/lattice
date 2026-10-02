//! Versioned, checksummed Learning Studio pack codec.
//!
//! The codec is intentionally independent from database import. A caller first
//! decodes and previews this bounded representation, resolves every conflict,
//! and only then writes through the domain repositories. Archive paths are
//! treated as hostile input on every platform.

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path};

pub const LEARNING_PACK_FORMAT: &str = "lattice.learning-pack";
pub const LEARNING_PACK_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 4_096;
const MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_COMPRESSED_BYTES: usize = 64 * 1024 * 1024;
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
    pub pack_id: String,
    pub title: String,
    pub created_at: i64,
    pub application_version: String,
    pub root_sha256: String,
    pub privacy: LearningPackPrivacyManifest,
    pub entries: Vec<LearningPackManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearningPackEntry {
    pub path: String,
    pub kind: LearningPackEntryKind,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct LearningPackInput {
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
    let has_keys = entries
        .iter()
        .any(|(path, _)| path == "program/answer-keys.json");
    let has_evidence = entries.iter().any(|(path, kind)| {
        matches!(
            kind,
            LearningPackEntryKind::Attempt
                | LearningPackEntryKind::Evidence
                | LearningPackEntryKind::Canvas
        ) || (kind == &LearningPackEntryKind::PracticalArtifact
            && path == "practical/artifacts.json")
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
        if path.starts_with("sources/bodies/") != (kind == &LearningPackEntryKind::SourceBody) {
            return Err(invalid("Source-body entry paths and kinds must agree."));
        }
        // Fixed archive paths are part of the import contract. Checking both
        // directions prevents an otherwise harmless-looking entry from being
        // interpreted as a different payload by a path-based importer.
        let expected_kind = match path.as_str() {
            "program/program.json" => Some(LearningPackEntryKind::Program),
            "program/answer-keys.json"
            | "program/outcomes.json"
            | "curriculum/lesson-state.json" => Some(LearningPackEntryKind::Curriculum),
            "program/attempts.json" => Some(LearningPackEntryKind::Attempt),
            "evidence/learning-aggregate.json"
            | "evidence/events.json"
            | "evidence/follow-ups.json" => Some(LearningPackEntryKind::Evidence),
            "practical/artifacts.json" => Some(LearningPackEntryKind::PracticalArtifact),
            "program/canvases.json" => Some(LearningPackEntryKind::Canvas),
            "sources/library.json" | "sources/selectors.json" | "sources/history.json" => {
                Some(LearningPackEntryKind::SourceMetadata)
            }
            _ => None,
        };
        if expected_kind
            .as_ref()
            .is_some_and(|expected| expected != kind)
        {
            return Err(invalid(
                "A reserved learning-pack path has an invalid entry kind.",
            ));
        }
        if path == "program/answer-keys.json" && kind != &LearningPackEntryKind::Curriculum {
            return Err(invalid("The answer-key entry has an invalid kind."));
        }
        if matches!(kind, LearningPackEntryKind::SourceBody) {
            let name = path.strip_prefix("sources/bodies/").unwrap_or("");
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

fn root_digest(entries: &[LearningPackManifestEntry]) -> String {
    let mut hasher = Sha256::new();
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
                "Learning-pack entry '{}' exceeds 8 MiB.",
                entry.path
            )));
        }
        total = total.saturating_add(entry.bytes.len());
        if total > MAX_TOTAL_BYTES {
            return Err(invalid("Learning-pack content exceeds 64 MiB."));
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
        pack_id: input.pack_id,
        title: input.title.trim().into(),
        created_at: input.created_at,
        application_version: input.application_version.trim().into(),
        root_sha256: root_digest(&manifest_entries),
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
        return Err(invalid("Compressed learning pack exceeds 64 MiB."));
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
            return Err(invalid("A learning-pack entry exceeds 8 MiB."));
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
            return Err(invalid(
                "Decompressed learning-pack content exceeds 64 MiB.",
            ));
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
    let manifest: LearningPackManifest = serde_json::from_slice(
        manifest_bytes
            .as_deref()
            .ok_or_else(|| invalid("A learning pack has no manifest."))?,
    )
    .map_err(|error| AppError::InvalidData(error.to_string()))?;
    if manifest.format != LEARNING_PACK_FORMAT || manifest.version != LEARNING_PACK_VERSION {
        return Err(invalid("This learning-pack version is not supported."));
    }
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
    if sorted != manifest.entries || root_digest(&manifest.entries) != manifest.root_sha256 {
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
mod tests {
    use super::*;

    fn input() -> LearningPackInput {
        LearningPackInput {
            pack_id: "bd02823f-2897-4b67-9610-17b9b3b1baa7".into(),
            title: "Concurrency foundations".into(),
            created_at: 1_791_000_000_000,
            application_version: "1.0.0".into(),
            privacy: LearningPackPrivacyManifest::default(),
            entries: vec![
                LearningPackEntry {
                    path: "program/program.json".into(),
                    kind: LearningPackEntryKind::Program,
                    bytes: br#"{"id":"program-1"}"#.to_vec(),
                },
                LearningPackEntry {
                    path: "sources/source-1.json".into(),
                    kind: LearningPackEntryKind::SourceMetadata,
                    bytes: br#"{"title":"Reference"}"#.to_vec(),
                },
            ],
        }
    }

    fn rearchive(
        manifest: &LearningPackManifest,
        entries: &BTreeMap<String, Vec<u8>>,
    ) -> anyhow::Result<Vec<u8>> {
        let encoder = zstd::stream::write::Encoder::new(Vec::new(), 1)?;
        let mut archive = tar::Builder::new(encoder);
        let bytes = serde_json::to_vec(manifest)?;
        append_tar_file(&mut archive, MANIFEST_PATH, &bytes)?;
        for (path, bytes) in entries {
            append_tar_file(&mut archive, path, bytes)?;
        }
        Ok(archive.into_inner()?.finish()?)
    }

    #[test]
    fn pack_round_trip_is_deterministic_and_checksummed() -> anyhow::Result<()> {
        let encoded = encode_learning_pack(input())?;
        let decoded = decode_learning_pack(&encoded)?;
        assert_eq!(decoded.manifest.format, LEARNING_PACK_FORMAT);
        assert_eq!(decoded.manifest.entries.len(), 2);
        assert_eq!(
            decoded.entries["program/program.json"],
            br#"{"id":"program-1"}"#
        );
        assert_eq!(encoded, encode_learning_pack(input())?);
        Ok(())
    }

    #[test]
    fn portable_paths_reject_traversal_and_platform_specific_escapes() {
        for path in [
            "../outside",
            "content/../../outside",
            "/etc/passwd",
            "C:/Windows/system.ini",
            "C:\\Windows\\system.ini",
            "content//file.json",
            "./file.json",
            MANIFEST_PATH,
        ] {
            assert!(validate_pack_path(path).is_err(), "accepted {path:?}");
        }
    }

    #[test]
    fn privacy_policy_blocks_credentials_chat_and_unlicensed_source_bodies() {
        for mutate in [
            |privacy: &mut LearningPackPrivacyManifest| privacy.includes_credentials = true,
            |privacy: &mut LearningPackPrivacyManifest| privacy.includes_private_chat = true,
            |privacy: &mut LearningPackPrivacyManifest| privacy.includes_full_source_bodies = true,
        ] {
            let mut value = input();
            mutate(&mut value.privacy);
            assert!(encode_learning_pack(value).is_err());
        }
        let mut value = input();
        value.entries.push(LearningPackEntry {
            path: "sources/source-1.txt".into(),
            kind: LearningPackEntryKind::SourceBody,
            bytes: b"copyrighted body".to_vec(),
        });
        assert!(encode_learning_pack(value).is_err());
    }

    #[test]
    fn tampering_is_detected_before_import() -> anyhow::Result<()> {
        let encoded = encode_learning_pack(input())?;
        let decoder = zstd::stream::read::Decoder::new(Cursor::new(&encoded))?;
        let mut archive = tar::Archive::new(decoder);
        let mut files = Vec::new();
        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_string_lossy().into_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            if path == "program/program.json" {
                bytes.extend_from_slice(b"tampered");
            }
            files.push((path, bytes));
        }
        let encoder = zstd::stream::write::Encoder::new(Vec::new(), 1)?;
        let mut builder = tar::Builder::new(encoder);
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o600);
            header.set_cksum();
            builder.append_data(&mut header, path, Cursor::new(bytes))?;
        }
        let encoded = builder.into_inner()?.finish()?;
        assert!(decode_learning_pack(&encoded).is_err());
        Ok(())
    }

    #[test]
    fn decode_rejects_privacy_declaration_mismatches() -> anyhow::Result<()> {
        let decoded = decode_learning_pack(&encode_learning_pack(input())?)?;
        let mut manifest = decoded.manifest.clone();
        manifest.privacy.includes_full_source_bodies = true;
        manifest.privacy.source_body_redistribution_confirmed = true;
        assert!(decode_learning_pack(&rearchive(&manifest, &decoded.entries)?).is_err());

        let mut with_body = input();
        with_body.privacy.includes_full_source_bodies = true;
        with_body.privacy.source_body_redistribution_confirmed = true;
        with_body.entries.push(LearningPackEntry {
            path: format!("sources/bodies/{}.txt", "0".repeat(64)),
            kind: LearningPackEntryKind::SourceBody,
            bytes: b"licensed source body".to_vec(),
        });
        let decoded = decode_learning_pack(&encode_learning_pack(with_body)?)?;
        let mut manifest = decoded.manifest.clone();
        manifest.privacy.includes_full_source_bodies = false;
        manifest.privacy.source_body_redistribution_confirmed = false;
        assert!(decode_learning_pack(&rearchive(&manifest, &decoded.entries)?).is_err());
        Ok(())
    }

    #[test]
    fn source_body_path_cannot_be_mislabeled_to_hide_privacy() -> anyhow::Result<()> {
        let mut pack = input();
        pack.privacy.includes_full_source_bodies = true;
        pack.privacy.source_body_redistribution_confirmed = true;
        pack.entries.push(LearningPackEntry {
            path: format!("sources/bodies/{}.txt", "1".repeat(64)),
            kind: LearningPackEntryKind::SourceBody,
            bytes: b"body".to_vec(),
        });
        let decoded = decode_learning_pack(&encode_learning_pack(pack)?)?;
        let mut tampered = decoded.manifest.clone();
        let body = tampered
            .entries
            .iter_mut()
            .find(|entry| entry.kind == LearningPackEntryKind::SourceBody)
            .ok_or_else(|| anyhow::anyhow!("source body entry"))?;
        body.kind = LearningPackEntryKind::SourceMetadata;
        tampered.root_sha256 = root_digest(&tampered.entries);
        assert!(decode_learning_pack(&rearchive(&tampered, &decoded.entries)?).is_err());

        let mut mislabeled = input();
        mislabeled.entries.push(LearningPackEntry {
            path: format!("sources/bodies/{}.txt", "2".repeat(64)),
            kind: LearningPackEntryKind::SourceMetadata,
            bytes: b"body".to_vec(),
        });
        assert!(encode_learning_pack(mislabeled).is_err());
        Ok(())
    }

    #[test]
    fn reserved_import_paths_require_their_contract_kind() {
        let mut pack = input();
        pack.entries.push(LearningPackEntry {
            path: "sources/library.json".into(),
            kind: LearningPackEntryKind::Canvas,
            bytes: b"{}".to_vec(),
        });
        assert!(encode_learning_pack(pack).is_err());
    }

    #[test]
    fn decode_revalidates_bounded_manifest_metadata() -> anyhow::Result<()> {
        let decoded = decode_learning_pack(&encode_learning_pack(input())?)?;
        let mut oversized_title = decoded.manifest.clone();
        oversized_title.title = "x".repeat(161);
        assert!(decode_learning_pack(&rearchive(&oversized_title, &decoded.entries)?).is_err());

        let mut oversized_version = decoded.manifest.clone();
        oversized_version.application_version = "v".repeat(81);
        assert!(decode_learning_pack(&rearchive(&oversized_version, &decoded.entries)?).is_err());

        let mut invalid_timestamp = decoded.manifest.clone();
        invalid_timestamp.created_at =
            chrono::Utc::now().timestamp_millis() + 500 * 24 * 60 * 60 * 1_000;
        assert!(decode_learning_pack(&rearchive(&invalid_timestamp, &decoded.entries)?).is_err());
        Ok(())
    }

    #[test]
    fn content_privacy_declarations_match_entries_on_encode_and_decode() -> anyhow::Result<()> {
        let decoded = decode_learning_pack(&encode_learning_pack(input())?)?;
        let mut mismatch = decoded.manifest.clone();
        mismatch.privacy.includes_answer_keys = true;
        assert!(decode_learning_pack(&rearchive(&mismatch, &decoded.entries)?).is_err());

        let mut hidden = decoded.manifest.clone();
        hidden.privacy.includes_hidden_evaluators = true;
        assert!(decode_learning_pack(&rearchive(&hidden, &decoded.entries)?).is_err());

        let mut pack = input();
        pack.privacy.includes_learner_evidence = true;
        assert!(encode_learning_pack(pack).is_err());

        let mut pack = input();
        pack.privacy.includes_practical_artifacts = true;
        assert!(encode_learning_pack(pack).is_err());

        let mut pack = input();
        pack.privacy.includes_answer_keys = true;
        pack.entries.push(LearningPackEntry {
            path: "program/answer-keys.json".into(),
            kind: LearningPackEntryKind::Curriculum,
            bytes: b"[]".to_vec(),
        });
        assert!(encode_learning_pack(pack).is_ok());

        let mut pack = input();
        pack.entries.push(LearningPackEntry {
            path: "practical/hidden-evaluator.json".into(),
            kind: LearningPackEntryKind::PracticalArtifact,
            bytes: b"{}".to_vec(),
        });
        assert!(encode_learning_pack(pack).is_err());
        Ok(())
    }
}
