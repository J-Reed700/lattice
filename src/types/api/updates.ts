/**
 * Update Checker API Types
 *
 * Type definitions for update checking functionality.
 * These types match the Rust backend structures from commands/update_checker.rs
 */

export interface UpdateInfo {
  available: boolean;
  currentVersion: string;
  latestVersion: string | null;
  downloadUrl: string | null;
  releaseNotes: string | null;
}

/** What `get_version_info` returns. */
export interface VersionInfo {
  version: string;
  buildDate: string | null;
  commitHash: string | null;
}
