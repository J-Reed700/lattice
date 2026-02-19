/**
 * Update Checker API Types
 *
 * Type definitions for update checking functionality.
 * These types match the Rust backend structures from commands/update_checker.rs
 */

export interface UpdateInfo {
  available: boolean;
  current_version: string;
  latest_version: string | null;
  download_url: string | null;
  release_notes: string | null;
}
