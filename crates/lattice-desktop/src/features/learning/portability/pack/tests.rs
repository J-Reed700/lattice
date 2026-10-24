//! Codec unit tests: framing, versioning, limits and the privacy contract.
use super::*;

const SCHEMA: i64 = 20_261_012_090_000;

fn input() -> LearningPackInput {
    LearningPackInput {
        schema_version: SCHEMA,
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
    rearchive_raw(&serde_json::to_vec(manifest)?, entries)
}

/// Archive any manifest JSON, including shapes this build would not write.
fn rearchive_raw(manifest: &[u8], entries: &BTreeMap<String, Vec<u8>>) -> anyhow::Result<Vec<u8>> {
    let encoder = zstd::stream::write::Encoder::new(Vec::new(), 1)?;
    let mut archive = tar::Builder::new(encoder);
    append_tar_file(&mut archive, MANIFEST_PATH, manifest)?;
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
fn only_the_current_pack_version_imports() -> anyhow::Result<()> {
    let decoded = decode_learning_pack(&encode_learning_pack(input())?)?;
    assert_eq!(decoded.manifest.version, LEARNING_PACK_VERSION);
    let mut manifest = decoded.manifest.clone();
    assert_eq!(LEARNING_PACK_VERSION, 5);
    for unsupported in [0, 1, 2, 3, 4, LEARNING_PACK_VERSION + 1] {
        manifest.version = unsupported;
        let error = decode_learning_pack(&rearchive(&manifest, &decoded.entries)?)
            .expect_err("only the current pack version imports");
        assert!(error.to_string().contains("version is not supported"));
    }
    Ok(())
}

#[test]
fn a_v3_pack_is_told_its_version_is_unsupported() -> anyhow::Result<()> {
    let decoded = decode_learning_pack(&encode_learning_pack(input())?)?;
    // v3 manifests had no schema stamp; parsing them as v4 would fail on it.
    let mut v3 = serde_json::to_value(&decoded.manifest)?;
    let fields = v3
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("manifest object"))?;
    fields.remove("schemaVersion");
    fields.insert("version".into(), 3.into());
    let error = decode_learning_pack(&rearchive_raw(&serde_json::to_vec(&v3)?, &decoded.entries)?)
        .expect_err("v3 packs do not import");
    assert!(
        matches!(&error, AppError::InvalidInput(message) if message == "This learning-pack version is not supported."),
        "{error}"
    );
    Ok(())
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
    tampered.root_sha256 = root_digest(tampered.schema_version, &tampered.entries);
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

#[test]
fn schema_version_is_part_of_the_checksummed_root() -> anyhow::Result<()> {
    let decoded = decode_learning_pack(&encode_learning_pack(input())?)?;
    assert_eq!(decoded.manifest.schema_version, SCHEMA);

    // Rewriting the stamp without the root is caught as tampering.
    let mut restamped = decoded.manifest.clone();
    restamped.schema_version = SCHEMA + 1;
    let error = decode_learning_pack(&rearchive(&restamped, &decoded.entries)?)
        .expect_err("the root covers the schema stamp");
    assert!(error.to_string().contains("root checksum"));

    // Same entries, different schema: a different root.
    let mut other = input();
    other.schema_version = SCHEMA + 1;
    let other = decode_learning_pack(&encode_learning_pack(other)?)?;
    assert_ne!(other.manifest.root_sha256, decoded.manifest.root_sha256);

    let mut unstamped = input();
    unstamped.schema_version = 0;
    assert!(encode_learning_pack(unstamped).is_err());
    let mut unstamped = decoded.manifest.clone();
    unstamped.schema_version = 0;
    unstamped.root_sha256 = root_digest(0, &unstamped.entries);
    assert!(decode_learning_pack(&rearchive(&unstamped, &decoded.entries)?).is_err());
    Ok(())
}

#[test]
fn entries_up_to_the_entry_cap_fit_and_larger_ones_are_refused_both_ways() -> anyhow::Result<()> {
    let mut large = input();
    large.entries.push(LearningPackEntry {
        path: "sources/large.json".into(),
        kind: LearningPackEntryKind::SourceMetadata,
        bytes: vec![b'a'; 9 * MIB],
    });
    let decoded = decode_learning_pack(&encode_learning_pack(large)?)?;
    assert_eq!(decoded.entries["sources/large.json"].len(), 9 * MIB);

    let mut oversized = input();
    oversized.entries.push(LearningPackEntry {
        path: "sources/oversized.json".into(),
        kind: LearningPackEntryKind::SourceMetadata,
        bytes: vec![b'a'; MAX_ENTRY_BYTES + 1],
    });
    let error = encode_learning_pack(oversized).expect_err("entry cap");
    assert!(error.to_string().contains("exceeds 32 MiB"));

    // A hand-built archive cannot smuggle an oversized entry past decode.
    let decoded = decode_learning_pack(&encode_learning_pack(input())?)?;
    let mut entries = decoded.entries.clone();
    entries.insert(
        "sources/oversized.json".into(),
        vec![b'a'; MAX_ENTRY_BYTES + 1],
    );
    assert!(decode_learning_pack(&rearchive(&decoded.manifest, &entries)?).is_err());

    assert_eq!(MAX_ENTRY_BYTES, 32 * MIB);
    assert_eq!(MAX_TOTAL_BYTES, 256 * MIB);
    assert_eq!(MAX_COMPRESSED_BYTES, 256 * MIB);
    assert_eq!(MAX_ENTRIES, 4_096);
    Ok(())
}

#[test]
fn evidence_table_and_canvas_folders_require_their_kind_and_a_flat_name() {
    let with = |path: &str, kind: LearningPackEntryKind| {
        let mut pack = input();
        pack.privacy.includes_learner_evidence = true;
        pack.entries.push(LearningPackEntry {
            path: path.into(),
            kind,
            bytes: b"{}".to_vec(),
        });
        encode_learning_pack(pack)
    };
    assert!(with(
        "evidence/tables/study_cards.json",
        LearningPackEntryKind::Evidence
    )
    .is_ok());
    assert!(with(
        "program/canvases/canvas-1.json",
        LearningPackEntryKind::Canvas
    )
    .is_ok());
    for (path, kind) in [
        (
            "evidence/tables/study_cards.json",
            LearningPackEntryKind::Curriculum,
        ),
        (
            "program/canvases/canvas-1.json",
            LearningPackEntryKind::Evidence,
        ),
        (
            "evidence/tables/nested/study_cards.json",
            LearningPackEntryKind::Evidence,
        ),
        (
            "evidence/tables/study_cards.txt",
            LearningPackEntryKind::Evidence,
        ),
        ("program/canvases/.json", LearningPackEntryKind::Canvas),
    ] {
        assert!(
            with(path, kind.clone()).is_err(),
            "accepted {path} as {kind:?}"
        );
    }
}
