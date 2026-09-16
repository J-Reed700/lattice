//! Encrypted off-device backup archives (`.lattice-backup`).
//!
//! Design: `docs/design/2026-09-16-encrypted-backup-archive.md`.
//!
//! - [`format`] — on-disk layout, header, manifest, error type (the contract)
//! - [`crypto`] — key envelope (passphrase + recovery code) and streaming AEAD
//! - [`snapshot`] — SQLite snapshot with derivable tables cleared, tar+zstd payload
//! - [`placeholder`] — cloud placeholder (dataless file) and cloud-folder detection

pub mod config;
pub mod crypto;
pub mod format;
pub mod key_store;
pub mod placeholder;
pub mod restore;
pub mod service;
pub mod snapshot;
pub mod writer;
