//! Tests for the key envelope and the streaming AEAD.
use super::*;
use std::io::{Read, Write};

const CHUNK: usize = 1024;

fn params() -> Argon2Params {
    Argon2Params::for_tests()
}

fn code() -> RecoveryCode {
    RecoveryCode::generate().unwrap_or_else(|e| panic!("generate: {e}"))
}

fn envelope_with(passphrase: Option<&str>) -> (MasterKey, RecoveryCode, KeyEnvelope) {
    let master = MasterKey::generate();
    let recovery = code();
    let envelope = build_envelope(&master, passphrase, &recovery, params())
        .unwrap_or_else(|e| panic!("build_envelope: {e}"));
    (master, recovery, envelope)
}

/// A syntactically valid 24-word phrase whose checksum word is wrong.
fn phrase_with_bad_checksum() -> String {
    for _ in 0..64 {
        let good = code();
        let other = code();
        let mut words = good.words().to_vec();
        let last_index = words.len().saturating_sub(1);
        if let (Some(slot), Some(replacement)) = (words.get_mut(last_index), other.words().last()) {
            if slot == replacement {
                continue;
            }
            *slot = replacement.clone();
        }
        let candidate = words.join(" ");
        if matches!(
            RecoveryCode::parse(&candidate),
            Err(ArchiveError::WrongSecret)
        ) {
            return candidate;
        }
    }
    panic!("could not construct a bad-checksum phrase");
}

// -- recovery code -----------------------------------------------------

#[test]
fn generate_produces_24_words_and_round_trips() {
    let generated = code();
    assert_eq!(generated.words().len(), 24);
    assert!(RecoveryCode::looks_like_phrase(&generated.phrase()));
    let parsed = RecoveryCode::parse(&generated.phrase()).unwrap_or_else(|e| panic!("parse: {e}"));
    assert_eq!(parsed.entropy(), generated.entropy());
    assert_eq!(parsed.words(), generated.words());
}

#[test]
fn parse_normalizes_case_and_whitespace() {
    let generated = code();
    let messy = format!(
        "  {}  ",
        generated
            .words()
            .iter()
            .map(|w| w.to_uppercase())
            .collect::<Vec<_>>()
            .join("\n \t ")
    );
    let parsed = RecoveryCode::parse(&messy).unwrap_or_else(|e| panic!("parse messy: {e}"));
    assert_eq!(parsed.entropy(), generated.entropy());
}

#[test]
fn parse_rejects_bad_input() {
    assert!(matches!(
        RecoveryCode::parse("not a recovery code"),
        Err(ArchiveError::WrongSecret)
    ));
    assert!(matches!(
        RecoveryCode::parse(&phrase_with_bad_checksum()),
        Err(ArchiveError::WrongSecret)
    ));
    let short = code()
        .words()
        .get(..12)
        .map(|w| w.join(" "))
        .unwrap_or_default();
    assert!(matches!(
        RecoveryCode::parse(&short),
        Err(ArchiveError::WrongSecret)
    ));
}

// -- envelope ----------------------------------------------------------

#[test]
fn envelope_round_trips_with_passphrase_and_with_recovery_phrase() {
    let (master, recovery, envelope) = envelope_with(Some("correct horse battery"));
    assert!(envelope.has_recovery_slot());
    assert!(envelope.has_passphrase_slot());

    let by_passphrase = unwrap_master_key(&envelope, "correct horse battery")
        .unwrap_or_else(|e| panic!("passphrase unwrap: {e}"));
    assert_eq!(by_passphrase.as_bytes(), master.as_bytes());

    let by_phrase = unwrap_master_key(&envelope, &recovery.phrase())
        .unwrap_or_else(|e| panic!("phrase unwrap: {e}"));
    assert_eq!(by_phrase.as_bytes(), master.as_bytes());
}

#[test]
fn recovery_only_envelope_round_trips() {
    let (master, recovery, envelope) = envelope_with(None);
    assert!(!envelope.has_passphrase_slot());
    let unwrapped = unwrap_master_key(&envelope, &recovery.phrase())
        .unwrap_or_else(|e| panic!("phrase unwrap: {e}"));
    assert_eq!(unwrapped.as_bytes(), master.as_bytes());
    // No passphrase slot: a passphrase-shaped secret is WrongSecret.
    assert!(matches!(
        unwrap_master_key(&envelope, "anything"),
        Err(ArchiveError::WrongSecret)
    ));
}

#[test]
fn wrong_secrets_all_report_wrong_secret() {
    let (_master, _recovery, envelope) = envelope_with(Some("hunter2hunter2"));

    assert!(matches!(
        unwrap_master_key(&envelope, "hunter2hunter3"),
        Err(ArchiveError::WrongSecret)
    ));
    assert!(matches!(
        unwrap_master_key(&envelope, &code().phrase()),
        Err(ArchiveError::WrongSecret)
    ));
    assert!(matches!(
        unwrap_master_key(&envelope, &phrase_with_bad_checksum()),
        Err(ArchiveError::WrongSecret)
    ));
    assert!(matches!(
        unwrap_master_key(&envelope, ""),
        Err(ArchiveError::WrongSecret)
    ));
}

#[test]
fn kcv_matches_only_the_wrapped_key() {
    let (master, _recovery, envelope) = envelope_with(None);
    assert!(kcv_matches(&envelope, &master));
    assert!(!kcv_matches(&envelope, &MasterKey::generate()));

    let mut broken = envelope.clone();
    broken.kcv = "!!!not base64!!!".to_string();
    assert!(!kcv_matches(&broken, &master));

    let mut empty = envelope;
    empty.kcv = String::new();
    assert!(!kcv_matches(&empty, &master));
}

#[test]
fn kcv_mismatch_is_wrong_secret_even_when_the_slot_opens() {
    let (_master, recovery, mut envelope) = envelope_with(None);
    envelope.kcv = BASE64.encode([0u8; KCV_LEN]);
    assert!(matches!(
        unwrap_master_key(&envelope, &recovery.phrase()),
        Err(ArchiveError::WrongSecret)
    ));
}

#[test]
fn rotate_recovery_slot_invalidates_the_old_phrase() {
    let (master, old, mut envelope) = envelope_with(Some("keep me"));
    let new = code();
    rotate_recovery_slot(&mut envelope, &master, &new).unwrap_or_else(|e| panic!("rotate: {e}"));

    assert_eq!(
        envelope
            .slots
            .iter()
            .filter(|s| matches!(s, KeySlot::Recovery { .. }))
            .count(),
        1
    );
    assert!(matches!(
        unwrap_master_key(&envelope, &old.phrase()),
        Err(ArchiveError::WrongSecret)
    ));
    let unwrapped =
        unwrap_master_key(&envelope, &new.phrase()).unwrap_or_else(|e| panic!("new phrase: {e}"));
    assert_eq!(unwrapped.as_bytes(), master.as_bytes());
    // The passphrase slot survives rotation.
    let by_passphrase =
        unwrap_master_key(&envelope, "keep me").unwrap_or_else(|e| panic!("passphrase: {e}"));
    assert_eq!(by_passphrase.as_bytes(), master.as_bytes());
}

#[test]
fn set_and_remove_passphrase_slot() {
    let (master, recovery, mut envelope) = envelope_with(None);
    assert!(!envelope.has_passphrase_slot());

    set_passphrase_slot(&mut envelope, &master, "first pass", params())
        .unwrap_or_else(|e| panic!("set: {e}"));
    assert!(envelope.has_passphrase_slot());
    assert_eq!(
        unwrap_master_key(&envelope, "first pass")
            .unwrap_or_else(|e| panic!("first: {e}"))
            .as_bytes(),
        master.as_bytes()
    );

    // Replacing, not appending.
    set_passphrase_slot(&mut envelope, &master, "second pass", params())
        .unwrap_or_else(|e| panic!("replace: {e}"));
    assert_eq!(
        envelope
            .slots
            .iter()
            .filter(|s| matches!(s, KeySlot::Passphrase { .. }))
            .count(),
        1
    );
    assert!(matches!(
        unwrap_master_key(&envelope, "first pass"),
        Err(ArchiveError::WrongSecret)
    ));
    assert_eq!(
        unwrap_master_key(&envelope, "second pass")
            .unwrap_or_else(|e| panic!("second: {e}"))
            .as_bytes(),
        master.as_bytes()
    );

    remove_passphrase_slot(&mut envelope);
    assert!(!envelope.has_passphrase_slot());
    assert!(envelope.has_recovery_slot());
    assert!(matches!(
        unwrap_master_key(&envelope, "second pass"),
        Err(ArchiveError::WrongSecret)
    ));
    // The recovery slot still works after the passphrase slot is gone.
    assert_eq!(
        unwrap_master_key(&envelope, &recovery.phrase())
            .unwrap_or_else(|e| panic!("recovery: {e}"))
            .as_bytes(),
        master.as_bytes()
    );
}

#[test]
fn passphrase_slot_records_its_cost_parameters() {
    let (master, _recovery, envelope) = envelope_with(Some("pw"));
    let slot = envelope
        .slots
        .iter()
        .find(|s| matches!(s, KeySlot::Passphrase { .. }))
        .cloned();
    match slot {
        Some(KeySlot::Passphrase {
            salt,
            m_cost_kib,
            t_cost,
            p_cost,
            nonce,
            wrapped_key,
        }) => {
            assert_eq!(m_cost_kib, params().m_cost_kib);
            assert_eq!(t_cost, params().t_cost);
            assert_eq!(p_cost, params().p_cost);
            assert_eq!(
                decode_fixed::<SLOT_SALT_LEN>(&salt).map(|s| s.len()),
                Some(16)
            );
            assert_eq!(
                decode_fixed::<WRAP_NONCE_LEN>(&nonce).map(|n| n.len()),
                Some(12)
            );
            assert_eq!(
                BASE64.decode(&wrapped_key).map(|w| w.len()).unwrap_or(0),
                32 + TAG_LEN
            );
        }
        _ => panic!("expected a passphrase slot"),
    }
    assert!(kcv_matches(&envelope, &master));
}

#[test]
fn derive_file_key_differs_per_salt_and_per_master() {
    let master = MasterKey::generate();
    let salt_a = random_bytes::<16>();
    let salt_b = random_bytes::<16>();
    let a = derive_file_key(&master, &salt_a);
    let b = derive_file_key(&master, &salt_b);
    let a_again = derive_file_key(&master, &salt_a);
    assert_ne!(a.as_ref(), b.as_ref());
    assert_eq!(a.as_ref(), a_again.as_ref());
    assert_ne!(a.as_ref(), &[0u8; 32]);

    let other = derive_file_key(&MasterKey::generate(), &salt_a);
    assert_ne!(a.as_ref(), other.as_ref());
}

// -- streaming ---------------------------------------------------------

fn encrypt_all(plain: &[u8], key: &[u8; 32], nonce: &[u8; 7], aad: &[u8]) -> Vec<u8> {
    let mut writer = EncryptingWriter::new(Vec::new(), key, nonce, aad, CHUNK)
        .unwrap_or_else(|e| panic!("writer: {e}"));
    writer
        .write_all(plain)
        .unwrap_or_else(|e| panic!("write: {e}"));
    writer.finish().unwrap_or_else(|e| panic!("finish: {e}"))
}

fn decrypt_all(
    ciphertext: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 7],
    aad: &[u8],
) -> std::io::Result<Vec<u8>> {
    let mut reader = DecryptingReader::new(
        std::io::Cursor::new(ciphertext.to_vec()),
        key,
        nonce,
        aad,
        CHUNK,
    )
    .map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut out = Vec::new();
    reader.read_to_end(&mut out)?;
    Ok(out)
}

fn sample(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[test]
fn stream_round_trips_at_every_chunk_boundary() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"MAGIC|header".as_slice();
    for len in [0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 2 * CHUNK, CHUNK * 7 / 2] {
        let plain = sample(len);
        let ciphertext = encrypt_all(&plain, &key, &nonce, aad);
        let decrypted = decrypt_all(&ciphertext, &key, &nonce, aad)
            .unwrap_or_else(|e| panic!("len {len}: {e}"));
        assert_eq!(decrypted, plain, "round trip failed for {len} bytes");
    }
}

#[test]
fn exact_multiple_of_chunk_size_still_emits_a_flagged_last_chunk() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"aad".as_slice();
    let ciphertext = encrypt_all(&sample(2 * CHUNK), &key, &nonce, aad);
    // two full frames plus an empty flagged frame (flag + len + tag).
    let full_frame = 1 + 4 + CHUNK + TAG_LEN;
    let empty_last = 1 + 4 + TAG_LEN;
    assert_eq!(ciphertext.len(), 2 * full_frame + empty_last);
    assert_eq!(ciphertext.get(2 * full_frame).copied(), Some(1u8));
}

#[test]
fn writes_in_small_pieces_match_one_big_write() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"aad".as_slice();
    let plain = sample(CHUNK * 7 / 2);

    let one_shot = encrypt_all(&plain, &key, &nonce, aad);

    let mut writer = EncryptingWriter::new(Vec::new(), &key, &nonce, aad, CHUNK)
        .unwrap_or_else(|e| panic!("writer: {e}"));
    for piece in plain.chunks(37) {
        writer
            .write_all(piece)
            .unwrap_or_else(|e| panic!("write: {e}"));
    }
    let piecemeal = writer.finish().unwrap_or_else(|e| panic!("finish: {e}"));
    assert_eq!(one_shot, piecemeal);
}

#[test]
fn read_to_end_and_tiny_reads_agree() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"aad".as_slice();
    let plain = sample(CHUNK * 7 / 2 + 13);
    let ciphertext = encrypt_all(&plain, &key, &nonce, aad);

    let via_read_to_end =
        decrypt_all(&ciphertext, &key, &nonce, aad).unwrap_or_else(|e| panic!("read_to_end: {e}"));

    let mut reader =
        DecryptingReader::new(std::io::Cursor::new(ciphertext), &key, &nonce, aad, CHUNK)
            .unwrap_or_else(|e| panic!("reader: {e}"));
    let mut via_small = Vec::new();
    let mut scratch = [0u8; 7];
    loop {
        let n = reader
            .read(&mut scratch)
            .unwrap_or_else(|e| panic!("small read: {e}"));
        if n == 0 {
            break;
        }
        via_small.extend_from_slice(scratch.get(..n).unwrap_or_default());
    }

    assert_eq!(via_read_to_end, plain);
    assert_eq!(via_small, plain);
}

#[test]
fn empty_read_buffer_is_not_end_of_stream() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"aad".as_slice();
    let ciphertext = encrypt_all(&sample(64), &key, &nonce, aad);
    let mut reader =
        DecryptingReader::new(std::io::Cursor::new(ciphertext), &key, &nonce, aad, CHUNK)
            .unwrap_or_else(|e| panic!("reader: {e}"));
    assert_eq!(reader.read(&mut []).unwrap_or(9), 0);
    let mut out = Vec::new();
    reader
        .read_to_end(&mut out)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(out.len(), 64);
}

fn assert_invalid_data(result: std::io::Result<Vec<u8>>, what: &str) {
    match result {
        Ok(_) => panic!("{what} should not have decrypted"),
        Err(e) => assert_eq!(
            e.kind(),
            std::io::ErrorKind::InvalidData,
            "{what}: unexpected error {e}"
        ),
    }
}

#[test]
fn truncation_is_rejected() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"aad".as_slice();
    let plain = sample(2 * CHUNK);
    let ciphertext = encrypt_all(&plain, &key, &nonce, aad);

    // Drop the whole final (empty, flagged) chunk.
    let empty_last = 1 + 4 + TAG_LEN;
    let without_last = ciphertext
        .get(..ciphertext.len() - empty_last)
        .unwrap_or_default()
        .to_vec();
    assert_invalid_data(
        decrypt_all(&without_last, &key, &nonce, aad),
        "dropping the last chunk",
    );

    // Cut in the middle of a chunk.
    let mid = ciphertext
        .get(..ciphertext.len() / 2)
        .unwrap_or_default()
        .to_vec();
    assert_invalid_data(decrypt_all(&mid, &key, &nonce, aad), "cutting mid-chunk");

    // Lose a single trailing byte.
    let one_short = ciphertext
        .get(..ciphertext.len() - 1)
        .unwrap_or_default()
        .to_vec();
    assert_invalid_data(decrypt_all(&one_short, &key, &nonce, aad), "one byte short");

    // Nothing at all.
    assert_invalid_data(decrypt_all(&[], &key, &nonce, aad), "empty input");
}

#[test]
fn tampering_is_rejected() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"aad".as_slice();
    let plain = sample(2 * CHUNK);
    let ciphertext = encrypt_all(&plain, &key, &nonce, aad);

    // Flip one ciphertext byte.
    let mut flipped = ciphertext.clone();
    if let Some(byte) = flipped.get_mut(100) {
        *byte ^= 0x01;
    }
    assert_invalid_data(decrypt_all(&flipped, &key, &nonce, aad), "a flipped byte");

    // Swap the two full chunks (same size, so the framing stays valid).
    let full_frame = 1 + 4 + CHUNK + TAG_LEN;
    let mut swapped = Vec::with_capacity(ciphertext.len());
    swapped.extend_from_slice(
        ciphertext
            .get(full_frame..2 * full_frame)
            .unwrap_or_default(),
    );
    swapped.extend_from_slice(ciphertext.get(..full_frame).unwrap_or_default());
    swapped.extend_from_slice(ciphertext.get(2 * full_frame..).unwrap_or_default());
    assert_eq!(swapped.len(), ciphertext.len());
    assert_invalid_data(decrypt_all(&swapped, &key, &nonce, aad), "swapped chunks");

    // Extra data after the flagged last chunk.
    let mut extended = ciphertext.clone();
    extended.extend_from_slice(&[0u8; 8]);
    assert_invalid_data(decrypt_all(&extended, &key, &nonce, aad), "trailing data");

    // A bogus is_last flag byte.
    let mut bad_flag = ciphertext.clone();
    if let Some(byte) = bad_flag.first_mut() {
        *byte = 7;
    }
    assert_invalid_data(decrypt_all(&bad_flag, &key, &nonce, aad), "a bogus flag");

    // An implausible chunk length.
    let mut bad_len = ciphertext.clone();
    for (offset, byte) in u32::MAX.to_le_bytes().iter().enumerate() {
        if let Some(slot) = bad_len.get_mut(1 + offset) {
            *slot = *byte;
        }
    }
    assert_invalid_data(decrypt_all(&bad_len, &key, &nonce, aad), "a bogus length");
}

#[test]
fn a_failed_chunk_poisons_every_later_read() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"aad".as_slice();
    let ciphertext = encrypt_all(&sample(2 * CHUNK), &key, &nonce, aad);
    let empty_last = 1 + 4 + TAG_LEN;
    let truncated_bytes = ciphertext
        .get(..ciphertext.len() - empty_last)
        .unwrap_or_default()
        .to_vec();

    let mut reader = DecryptingReader::new(
        std::io::Cursor::new(truncated_bytes),
        &key,
        &nonce,
        aad,
        CHUNK,
    )
    .unwrap_or_else(|e| panic!("reader: {e}"));

    let mut scratch = vec![0u8; 4096];
    let mut saw_error = false;
    for _ in 0..8 {
        match reader.read(&mut scratch) {
            Ok(0) => panic!("a truncated stream must never read as complete"),
            Ok(_) => {}
            Err(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::InvalidData);
                saw_error = true;
                break;
            }
        }
    }
    assert!(saw_error, "expected the truncated stream to fail");
    // Every later read repeats the failure instead of reporting EOF.
    for _ in 0..3 {
        match reader.read(&mut scratch) {
            Ok(n) => panic!("poisoned reader returned {n} bytes"),
            Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::InvalidData),
        }
    }
}

#[test]
fn wrong_key_nonce_or_aad_is_rejected() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let aad = b"MAGIC|header-v1".as_slice();
    let plain = sample(CHUNK + 5);
    let ciphertext = encrypt_all(&plain, &key, &nonce, aad);

    assert_invalid_data(
        decrypt_all(&ciphertext, &random_bytes::<32>(), &nonce, aad),
        "a wrong key",
    );
    assert_invalid_data(
        decrypt_all(&ciphertext, &key, &random_bytes::<7>(), aad),
        "a wrong stream nonce",
    );
    assert_invalid_data(
        decrypt_all(&ciphertext, &key, &nonce, b"MAGIC|header-v2"),
        "a wrong aad",
    );
    assert_invalid_data(decrypt_all(&ciphertext, &key, &nonce, b""), "an empty aad");
}

#[test]
fn dropping_the_writer_without_finishing_does_not_panic() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    let mut writer = EncryptingWriter::new(Vec::new(), &key, &nonce, b"aad", CHUNK)
        .unwrap_or_else(|e| panic!("writer: {e}"));
    writer
        .write_all(&sample(CHUNK + 3))
        .unwrap_or_else(|e| panic!("write: {e}"));
    drop(writer);
}

#[test]
fn zero_chunk_size_is_rejected() {
    let key = random_bytes::<32>();
    let nonce = random_bytes::<7>();
    assert!(EncryptingWriter::new(Vec::new(), &key, &nonce, b"", 0).is_err());
    assert!(DecryptingReader::new(std::io::Cursor::new(Vec::new()), &key, &nonce, b"", 0).is_err());
}

#[test]
fn end_to_end_envelope_and_stream() {
    let master = MasterKey::generate();
    let recovery = code();
    let envelope = build_envelope(&master, Some("pass phrase"), &recovery, params())
        .unwrap_or_else(|e| panic!("envelope: {e}"));

    let file_salt = random_bytes::<16>();
    let stream_nonce = random_bytes::<7>();
    let aad = b"LATTBKP\x01....header".as_slice();
    let plain = sample(CHUNK * 3 + 17);

    let file_key = derive_file_key(&master, &file_salt);
    let ciphertext = encrypt_all(&plain, &file_key, &stream_nonce, aad);

    // A fresh process only has the envelope and the user's secret.
    let recovered =
        unwrap_master_key(&envelope, &recovery.phrase()).unwrap_or_else(|e| panic!("unwrap: {e}"));
    let recovered_key = derive_file_key(&recovered, &file_salt);
    assert_eq!(recovered_key.as_ref(), file_key.as_ref());

    let decrypted = decrypt_all(&ciphertext, &recovered_key, &stream_nonce, aad)
        .unwrap_or_else(|e| panic!("decrypt: {e}"));
    assert_eq!(decrypted, plain);
}
