export interface BackupInfo {
  path: string;
  name: string;
  createdAt: string;
  version: string;
  fileCount: number;
  size: number;
}

export interface ExportResult {
  filesExported: number;
  outputPath: string;
  format: 'markdown' | 'json' | 'csv' | 'html';
}

/** What `plugin_create_backup` returns. */
export interface CreateBackupResult {
  backupPath: string;
  size: number;
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

/**
 * Off-device backup (encrypted archive).
 *
 * Mirrors `features/backup/dto.rs` DTOs (camelCase). Declared here rather than
 * pulled from bindings so the UI compiles before the Rust side regenerates.
 */

export type ArchiveAvailability = 'local' | 'placeholder' | 'unknown';

export interface ArchiveRun {
  path: string;
  createdAt: string;
  size: number;
  durationMs: number;
}

export interface ArchiveError {
  at: string;
  message: string;
}

export interface ArchiveFile {
  path: string;
  name: string;
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
  archives: ArchiveFile[];
}

/** What `plugin_begin_archive_setup` / `plugin_rotate_recovery_code` return. */
export interface ArchiveSetup {
  /** 24 BIP-39 English words. */
  recoveryWords: string[];
  /** Three zero-based indices into `recoveryWords` the user must retype. */
  confirmIndices: number[];
}

export interface WordConfirmation {
  index: number;
  word: string;
}

export type RestoreArchiveOutcome = 'restored' | 'cancelled' | 'needs_secret' | 'not_hydrated';

/** What `plugin_restore_archive` returns. */
export interface RestoreArchiveResult {
  outcome: RestoreArchiveOutcome;
  message: string | null;
  restartRequired: boolean;
  reembedRequired: boolean;
  /** Set when the vault was written beside an existing non-empty one. */
  vaultRestoredTo: string | null;
  filesRestored: number;
}
