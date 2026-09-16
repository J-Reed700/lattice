
/**
 * Settings API Types
 *
 * Type definitions for application settings and configuration.
 * These types match the Rust backend structures from commands/settings.rs
 */

export type AppSettings = import('../../lib/bindings').SettingsDto;

/**
 * Onboarding state — what the user has already been through.
 *
 * SSOT for the first-run gate: the startup path acts on this state, so it
 * lives in the settings repository. Matches Rust `OnboardingSettingsDto`.
 */
export type OnboardingSettings = import('../../lib/bindings').OnboardingSettingsDto;

/**
 * Vault portability settings.
 *
 * Mirrors notes to plain markdown files on disk so users can manage
 * them with their own tools (Obsidian, ripgrep, git, iCloud).
 *
 * - `vaultPath` empty string means "use the default" (~/Lattice).
 * - `enabled` is off by default; flipping false → true triggers a
 *   one-shot backfill of every existing note into the vault folder.
 * - `watchExternalChanges` is the reverse-direction toggle (filesystem
 *   watcher → SQLite re-import). Independent so users can have one-way
 *   export without inviting external editors to write back.
 */
export type VaultSettings = import('../../lib/bindings').VaultSettingsDto;

/**
 * Privacy settings persisted by the backend.
 *
 * Both flags default to OFF. Backend code that sends telemetry or
 * crash reports upstream MUST gate on these via `privacy_gate` helpers
 * in features/settings/privacy_gate.rs.
 */
export type PrivacySettings = import('../../lib/bindings').PrivacySettingsDto;

// All field names use camelCase to match Rust #[serde(rename_all = "camelCase")]
export type IndexingSettings = import('../../lib/bindings').IndexingSettingsDto;

export type SearchSettings = import('../../lib/bindings').SearchSettingsDto;

export type RetrievalTuningSettings = import('../../lib/bindings').RetrievalTuningSettingsDto;

export type LLMSettings = import('../../lib/bindings').LLMSettingsDto;

export type CustomToolSettings = import('../../lib/bindings').CustomToolSettingsDto;

export type LLMVerificationSettings = import('../../lib/bindings').LLMVerificationSettingsDto;

export type RouterSettings = import('../../lib/bindings').RouterSettingsDto;

export type LLMPromptSettings = import('../../lib/bindings').LLMPromptSettingsDto;

export type ToolOutputSettings = import('../../lib/bindings').ToolOutputSettingsDto;

export type ToolOutputTemplates = import('../../lib/bindings').ToolOutputTemplatesDto;

export interface TestOllamaConnectionRequest {
  ollamaUrl: string;
  authHeaderName?: string;
  authHeaderValue?: string;
}

export interface TestOllamaConnectionResponse {
  endpoint: string;
  models: string[];
}

export interface TestCustomToolRequest {
  endpoint: string;
  queryParam: string;
  maxResultsParam?: string | null;
  defaultMaxResults: number;
  query: string;
  maxResults?: number;
}

export interface TestCustomToolResponse {
  finalUrl: string;
  status: number;
  contentType?: string | null;
  bodyPreview: string;
}

export type UISettings = import('../../lib/bindings').UISettingsDto;

export type SyncSettings = import('../../lib/bindings').SyncSettingsDto;

export type BackupSettings = import('../../lib/bindings').BackupSettingsDto;
