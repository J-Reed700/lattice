/**
 * Backup and Export API Types
 *
 * Types for backup creation, restore, and data export operations.
 */

export type BackupInfo = import('../../lib/bindings').BackupInfoDto;

export type CreateBackupResult = import('../../lib/bindings').CreateBackupResultDto;

export type RestoreBackupResult = import('../../lib/bindings').RestoreBackupResultDto;

/** What `plugin_export_markdown` / `plugin_export_json` return. */
export type ExportSummary = import('../../lib/bindings').ExportResultDto;

/* Off-device backup (encrypted archive). */

export type ArchiveRun = import('../../lib/bindings').ArchiveRunDto;

export type ArchiveStatus = import('../../lib/bindings').ArchiveStatusDto;

export type ArchiveSetup = import('../../lib/bindings').ArchiveSetupDto;

export type WordConfirmation = import('../../lib/bindings').WordConfirmationDto;

export type RestoreArchiveResult = import('../../lib/bindings').RestoreArchiveResultDto;
