/**
 * Backup and Export API Types
 *
 * Types for backup creation, restore, and data export operations.
 */

/**
 * Backup file metadata
 */
export interface BackupInfo {
  /** Full path to backup file */
  path: string;
  /** Backup filename */
  name: string;
  /** ISO 8601 timestamp of backup creation */
  createdAt: string;
  /** Backup format version */
  version: string;
  /** Number of files in backup (reserved, currently 0) */
  fileCount: number;
  /** Backup file size in bytes */
  size: number;
}

/**
 * Export operation result
 */
export interface ExportResult {
  /** Number of files successfully exported */
  filesExported: number;
  /** Output directory or file path */
  outputPath: string;
  /** Export format used */
  format: 'markdown' | 'json' | 'csv' | 'html';
}

/** What `plugin_create_backup` returns. */
export interface CreateBackupResult {
  /** Full path to the backup file that was written. */
  backupPath: string;
  /** Backup file size in bytes. */
  size: number;
  /** ISO 8601 timestamp of backup creation. */
  createdAt: string;
}

/** What `plugin_restore_backup` returns. */
export interface RestoreBackupResult {
  success: boolean;
  restoredCount: number;
  message: string | null;
}

/** What `plugin_export_markdown` / `plugin_export_json` return. */
export interface ExportSummary {
  /** Conversations plus journal pages written. */
  count: number;
  /** Directory the export landed in. */
  outputDir: string;
}

/* ------------------------------------------------------------------ *
 * Off-device backup (encrypted archive)
 *
 * These mirror the `features/backup/dto.rs` DTOs, which serialise
 * `rename_all = "camelCase"`. They are declared here rather than pulled from
 * `lib/bindings.ts` so the UI compiles before the Rust side regenerates the
 * bindings; the field names below are the contract.
 * ------------------------------------------------------------------ */

/** Whether an archive file is really on disk or only a cloud placeholder. */
export type ArchiveAvailability = 'local' | 'placeholder' | 'unknown';

/** One archive run — a file that was written, or the last one that was. */
export interface ArchiveRun {
  /** Full path to the `.lattice-backup` file. */
  path: string;
  /** ISO 8601 timestamp. */
  createdAt: string;
  /** File size in bytes. */
  size: number;
  /** How long the run took. */
  durationMs: number;
}

/** The last archive run that failed. */
export interface ArchiveError {
  /** ISO 8601 timestamp. */
  at: string;
  message: string;
}

/** One archive already sitting in the destination folder. */
export interface ArchiveFile {
  path: string;
  name: string;
  /** ISO 8601 timestamp. */
  createdAt: string;
  size: number;
  availability: ArchiveAvailability;
}

/** What `plugin_get_archive_status` returns. */
export interface ArchiveStatus {
  /** True once a recovery code has been confirmed and the envelope exists. */
  configured: boolean;
  destination: string | null;
  /** "iCloud Drive", "Dropbox", … when the destination is a known synced folder. */
  destinationProvider: string | null;
  /** The configured destination folder is gone. */
  destinationMissing: boolean;
  keepCount: number;
  hasPassphrase: boolean;
  recoveryConfirmed: boolean;
  lastSuccess: ArchiveRun | null;
  lastError: ArchiveError | null;
  /** Set when the live database itself sits inside a synced folder. */
  dataDirCloudProvider: string | null;
  /** Newest first. */
  archives: ArchiveFile[];
}

/** What `plugin_begin_archive_setup` / `plugin_rotate_recovery_code` return. */
export interface ArchiveSetup {
  /** 24 BIP-39 English words. */
  recoveryWords: string[];
  /** Three zero-based indices into `recoveryWords` the user must retype. */
  confirmIndices: number[];
}

/** One "word N was ..." answer in the confirmation step. */
export interface WordConfirmation {
  /** Zero-based index into the recovery words. */
  index: number;
  word: string;
}

export type RestoreArchiveOutcome = 'restored' | 'cancelled' | 'needs_secret' | 'not_hydrated';

/** What `plugin_restore_archive` returns. */
export interface RestoreArchiveResult {
  outcome: RestoreArchiveOutcome;
  /** Provider name on `not_hydrated`, explanation otherwise. */
  message: string | null;
  restartRequired: boolean;
  reembedRequired: boolean;
  /** Set when the vault was written beside an existing non-empty one. */
  vaultRestoredTo: string | null;
  filesRestored: number;
}
