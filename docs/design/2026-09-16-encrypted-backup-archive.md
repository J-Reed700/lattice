# Encrypted backup archive (Phase 1 of cloud durability)

Status: implemented, 2026-09-16 (uncommitted on `architecture-refactor`). Research and rationale:
`docs/research/2026-09-16-cloud-durability/README.md`.

## Goal

One scheduled or manual action produces a single encrypted, self-contained
`.lattice-backup` file in a folder the user chose with a native picker. If that
folder is iCloud Drive, Dropbox, OneDrive, Google Drive, a NAS, or a USB stick,
the user's data survives a dead drive. Restore works on a fresh machine given
the file plus either the passphrase or the 24-word recovery code.

Non-goals for this phase: direct API upload, sync, accounts, servers, dedup,
incremental archives.

## What goes in

| Content | Source | Tar entry |
|---|---|---|
| Notes, tags, decks, conversations, settings rows, everything non-derivable | `VACUUM INTO` snapshot of `lattice.db` with derivable tables cleared | `db/lattice.db` |
| Imported source files | content-addressed library `~/.lattice/files/{sha256}/name` | `files/{sha256}/name` |
| Vault markdown | `settings.vault.vault_path` when `settings.vault.enabled` | `vault/**` |
| Settings file | `<app_data_dir>/settings.json` | `settings.json` |
| Manifest | generated | `manifest.json` (first entry) |

Cleared tables (rows deleted, schema kept so migrations and triggers stay
valid): `text_embeddings`, `image_embeddings`, `conversation_memory_vectors`,
`chunk_sparse_terms`, `cluster_members`, `clusters`, `cluster_runs`,
`chat_starter_cache`. FTS5 tables (`documents_fts`, `chunks_fts`,
`conversation_search_fts`) are rebuilt on restore if they are content tables,
otherwise kept. Downloaded models are never included.

## File format

Authoritative: `src/src/src/features/backup/archive/format.rs`.

```
MAGIC "LATTBKP\x01" | header_len u32 LE | header JSON | STREAM chunks...
chunk = [is_last u8][ct_len u32 LE][ciphertext]
plaintext = zstd(tar(manifest.json, db/lattice.db, files/**, vault/**, settings.json))
```

- Cipher: ChaCha20-Poly1305 in the STREAM (BE32) construction from
  `aead-stream` 0.6 / `chacha20poly1305` 0.11, 1 MiB plaintext chunks. The
  bytes `MAGIC || header_len || header JSON` are associated data on every
  chunk. Truncation, reordering, and tampering are detected.
- Per-archive file key: `HKDF-SHA256(master_key, salt = header.file_salt,
  info = INFO_FILE_KEY)`. Fresh 16-byte salt and 7-byte stream nonce per file.
- Master key: 256-bit, generated once at setup, long-lived, stored in the OS
  keyring (service `tech.lattice.app`, user `backup-master-key`, base64) with
  a `0600` file fallback at `<app_data_dir>/backup/master.key` when the
  keyring is unavailable.
- Key envelope (static, built once, copied into every header):
  - Recovery slot (mandatory): 24 BIP-39 English words = 256 bits entropy.
    `KEK = HKDF-SHA256(entropy, salt, INFO_RECOVERY_KEK)`;
    `wrapped_key = ChaCha20-Poly1305(KEK, nonce, master_key, aad = AAD_SLOT_RECOVERY)`.
  - Passphrase slot (optional): `KEK = Argon2id(passphrase, salt, m=64 MiB,
    t=3, p=1)`; params in the slot so they can be raised later.
  - `kcv = first 8 bytes of HKDF-SHA256(master_key, no salt, INFO_KCV)` so
    "wrong secret" is distinguishable from "corrupt file".
- Why not `age`: its spec forbids a passphrase stanza alongside any other
  recipient, which blocks the two-slot design.

## Local state (owned by the backup slice, not the settings DTO)

`<app_data_dir>/backup/archive-config.json`:

```json
{
  "version": 1,
  "destination": "/Users/x/Library/Mobile Documents/com~apple~CloudDocs",
  "keep_count": 5,
  "envelope": { "slots": [...], "kcv": "..." },
  "recovery_confirmed_at": "2026-09-16T12:00:00Z",
  "created_at": "...",
  "last_success": { "path": "...", "created_at": "...", "size": 123, "duration_ms": 4500 },
  "last_error": { "at": "...", "message": "..." }
}
```

Archives land in `<destination>/Lattice Backups/lattice-backup-<UTC ts>.lattice-backup`.
Written as `.part`, fsynced, renamed. Retention deletes only files whose name
passes `format::is_archive_file_name` and keeps the newest `keep_count`.
The previous archive is never overwritten.

Scheduler: the existing `BackupScheduler` tick keeps writing the local `.db`
snapshot and, when `archive-config.json` has a destination and an envelope,
also writes an archive. Failures are logged at error level with the same
"NO BACKUPS ARE BEING CREATED" wording and recorded in `last_error`.

## Paths never cross IPC

The destination folder and the restore source file are chosen with
`tauri_plugin_dialog` **from Rust** (`app.dialog().file().blocking_pick_folder()`
/ `blocking_pick_file()` inside `spawn_blocking`). The webview never supplies a
path, so a hostile renderer cannot turn "restore" into a state-injection
primitive or "backup" into an arbitrary-write primitive. This is the same rule
`Container::exports_path` already documents.

## Command surface (plugin `backup`, all `#[tauri::command] #[specta::specta]`)

DTOs live in `features/backup/dto.rs`, `rename_all = "camelCase"`, derive
`specta::Type`. Names below are the Rust struct names; TS gets camelCase fields.

```
plugin_get_archive_status() -> ArchiveStatusDto
  ArchiveStatusDto {
    configured: bool,                 // envelope exists
    destination: Option<String>,
    destination_provider: Option<String>,   // "iCloud Drive" etc. or null
    destination_missing: bool,        // configured path no longer exists
    keep_count: u32,
    has_passphrase: bool,
    recovery_confirmed: bool,
    last_success: Option<ArchiveRunDto>,     // { path, created_at, size, duration_ms }
    last_error: Option<ArchiveErrorDto>,     // { at, message }
    data_dir_cloud_provider: Option<String>, // set when the live DB sits in a synced folder
    archives: Vec<ArchiveFileDto>,           // { path, name, created_at, size, availability: "local"|"placeholder"|"unknown" } newest first
  }

plugin_begin_archive_setup() -> ArchiveSetupDto
  ArchiveSetupDto { recovery_words: Vec<String> /*24*/, confirm_indices: Vec<u32> /*3, zero-based*/ }
  Generates a master key + recovery code, held in memory as "pending setup".
  Nothing is persisted until confirmed. Calling again replaces the pending state.

plugin_confirm_archive_setup(request: ConfirmArchiveSetupRequestDto) -> ArchiveStatusDto
  ConfirmArchiveSetupRequestDto { confirmations: Vec<WordConfirmationDto{index:u32, word:String}>, passphrase: Option<String> }
  Verifies the three words (case/whitespace-insensitive), rejects with
  InvalidInput otherwise. Builds the envelope, stores master key in keyring,
  writes archive-config.json. Passphrase, if present, must be >= 8 chars.

plugin_choose_archive_destination() -> ArchiveStatusDto
  Rust-side folder picker. Cancel => unchanged status. Stores destination.
  Rejects a destination equal to or inside <app_data_dir>.

plugin_set_archive_keep_count(request: { keep_count: u32 }) -> ArchiveStatusDto   // 1..=50
plugin_set_archive_passphrase(request: { passphrase: Option<String> }) -> ArchiveStatusDto  // None removes the slot; needs master key from keyring
plugin_rotate_recovery_code() -> ArchiveSetupDto   // new code; confirm with plugin_confirm_archive_setup (passphrase ignored, slot kept)
plugin_disable_archive() -> ArchiveStatusDto        // clears destination only; key and envelope stay

plugin_create_archive_now() -> ArchiveRunDto
  Rate-limited through the existing backup limiter. Audit-logged as BackupCreated with backup_type "archive".

plugin_restore_archive(request: RestoreArchiveRequestDto) -> RestoreArchiveResultDto
  RestoreArchiveRequestDto { secret: Option<String> }
  RestoreArchiveResultDto {
    outcome: "restored" | "cancelled" | "needs_secret" | "not_hydrated",
    message: Option<String>,
    restart_required: bool,
    reembed_required: bool,
    vault_restored_to: Option<String>,   // path if written beside a non-empty vault
    files_restored: u64,
  }
  Rust-side file picker filtered to *.lattice-backup. Order: placeholder check
  (return not_hydrated with provider name, do not block) -> read header ->
  try keyring master key (kcv) -> else secret (auto-detect phrase vs passphrase)
  -> else needs_secret -> decrypt+extract to scratch -> validate snapshot
  (integrity_check; migration_version must be <= this build's newest) ->
  swap DB exactly like BackupAdapter::restore_backup (safety copy, close pool,
  rename) -> merge files library (content-addressed, skip existing) -> vault:
  write into vault_path if it is missing or empty, otherwise beside it as
  `<vault>-restored-<ts>` -> settings.json restored only if absent ->
  store the unwrapped master key in the keyring if it was not there ->
  write `<app_data_dir>/.reembed-required` marker -> audit BackupRestored.
```

Capabilities: add `backup:allow-plugin-<name>` for each in
`src/src/capabilities/main.json`. Register each in
`src/src/src/export_bindings.rs` and `features/backup/plugin.rs`.
Regenerate `websrc/lib/bindings.ts` with `npm run bindings:generate`.

## Frontend

`components/Settings/BackupSection.tsx` gains an "Off-device backup"
block under the existing local backups list:

- Not configured: one paragraph, "Set up encrypted backup" button.
- Setup wizard (modal): (1) what it does and that the recovery code is the
  only way back in if the passphrase is forgotten; (2) the 24 words in a
  numbered 4x6 grid with Copy and Print; (3) confirm three words by index;
  (4) optional passphrase with confirm field; (5) choose folder (calls the
  backend picker). Cannot finish without step 3.
- Configured: destination row (provider badge when known, "Change"),
  keep count, last backup time/size, last error, "Back up now", "Restore from
  file", "Set/remove passphrase", "Generate new recovery code", "Turn off".
- Restore: on `needs_secret` prompt for passphrase or recovery code and retry
  with `secret`. On `not_hydrated` show the provider name and a "Try again"
  button. On `restored` show the same "quit and reopen" state used today and
  mention re-indexing embeddings.
- Warning banner when `data_dir_cloud_provider` is set: the live database is
  inside a synced folder, which can corrupt it; the archive is the supported
  way to use cloud storage.

Tests follow `BackupSection.test.tsx` (vitest + testing-library, `vi.mock('@/lib/api')`).

## As built

Code lives in `src/src/src/features/backup/` (the repo was flattened from
`src/app/` during implementation).

| File | Role |
|---|---|
| `archive/format.rs` | On-disk layout, header, manifest, `ArchiveError` |
| `archive/crypto.rs` | Two-slot key envelope, BIP-39 recovery code, STREAM writer/reader |
| `archive/snapshot.rs` | `VACUUM INTO` snapshot, derivable-table clearing, tar+zstd pack/unpack |
| `archive/placeholder.rs` | Dataless/placeholder detection, cloud-folder classification, hydration nudge |
| `archive/config.rs`, `archive/key_store.rs` | `archive-config.json`, keyring with `0600` file fallback |
| `archive/writer.rs`, `archive/restore.rs` | Create with retention; restore with DB swap, file merge, vault placement |
| `archive/service.rs` | `ArchiveService`: pending setup, status, every command's logic |
| `commands/archive.rs`, `plugin.rs` | Rate limiting, audit, Rust-side pickers, the ten commands |

Differences from the plan above:

- **No `use_cases/archive_*.rs`.** `ArchiveService` owns the orchestration; a
  use-case layer would only have forwarded calls.
- **Two named request DTOs**: `SetArchiveKeepCountRequestDto` and
  `SetArchivePassphraseRequestDto`.
- **Setup never takes a passphrase.** The wizard confirms the words with
  `passphrase: null`, then calls `plugin_set_archive_passphrase` as its own step.
- **Restore remembers the picked file** until a terminal outcome, so a
  `needs_secret` retry or "Try again" never reopens the picker.
- **`not_hydrated` carries only the provider name** in `message`; the UI writes
  the sentence. Before returning it, restore opens the file on a blocking
  thread and waits up to two minutes so the sync client starts downloading.
- **A wrong secret is a command error** with the exact message
  `wrong passphrase or recovery code`, not an outcome.
- **The files-library root is injected** into writer and restorer so tests
  never touch `~/.lattice/files`.
- **The cloud-folder warning follows the database file**, not the app data root.
- **Scheduling is independent of local auto-backup.** One ticker runs both
  jobs; each checks its own switch every tick. Choosing a destination starts
  the ticker, and turning local auto-backup off keeps it running while an
  archive destination is set. If only the archive is on and the frequency
  setting is invalid, it runs daily.
- **Re-embedding after restore.** Restore writes
  `<app_data_dir>/.reembed-required` (`shared::constants::REEMBED_MARKER_FILE`).
  Search startup already backfills missing chunk-first vectors; it now clears
  the marker once coverage is complete and logs a warning while it is not.
  Late chunking never backfills, so documents must be re-indexed. Sparse
  postings and clusters come back through indexing and recomputation.
- **No `brctl download`.** The command no longer exists on current macOS; a
  one-byte read is the nudge.
- **Symlinks are stat'd through to their target** by placeholder detection,
  because APFS reports symlinks with zero blocks, like evicted files.

Verification: `cargo test --lib features::backup` covers every module,
including `archive_round_trip_restores_database` (setup, passphrase, archive,
then restore into a fresh data directory with an empty keyring via the
recovery words). `BackupSection.test.tsx` covers the wizard and restore flows.

## Not done yet

- Progress events to the UI during create and restore; progress is logged only.
- Queuing a re-index automatically when late chunking is on after a restore.
- Phase 2 direct upload (S3-compatible, OneDrive, Google Drive).
