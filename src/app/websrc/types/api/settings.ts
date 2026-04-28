/**
 * Settings API Types
 *
 * Type definitions for application settings and configuration.
 * These types match the Rust backend structures from commands/settings.rs
 */

export interface AppSettings {
  indexing: IndexingSettings;
  search: SearchSettings;
  llm: LLMSettings;
  ui: UISettings;
  sync: SyncSettings;
  backup: BackupSettings;
  privacy: PrivacySettings;
}

/**
 * Privacy settings (Phase 4b SSOT).
 *
 * Both flags default to OFF. Backend code that sends telemetry or
 * crash reports upstream MUST gate on these via `privacy_gate` helpers
 * in features/settings/privacy_gate.rs.
 */
export interface PrivacySettings {
  telemetryEnabled: boolean;  // Matches Rust telemetry_enabled with camelCase
  crashReporting: boolean;    // Matches Rust crash_reporting with camelCase
}

// All field names use camelCase to match Rust #[serde(rename_all = "camelCase")]
export interface IndexingSettings {
  chunkSize: number;           // Matches Rust chunk_size with camelCase
  chunkOverlap: number;        // Matches Rust chunk_overlap with camelCase
  batchSize: number;           // Matches Rust batch_size with camelCase
  autoIndexNewFiles: boolean;  // Matches Rust auto_index_new_files with camelCase
  fileTypes: string[];         // Matches Rust file_types with camelCase
  excludedPaths: string[];     // Matches Rust excluded_paths with camelCase
}

export interface SearchSettings {
  maxResults: number;          // Matches Rust max_results with camelCase
  similarityThreshold: number; // Matches Rust similarity_threshold with camelCase
  enableReranking: boolean;    // Matches Rust enable_reranking with camelCase
  hybridSearchAlpha: number;   // Matches Rust hybrid_search_alpha with camelCase
  retrievalTuning: RetrievalTuningSettings; // Matches Rust retrieval_tuning with camelCase
}

export interface RetrievalTuningSettings {
  kbSearchMinLimit: number;
  kbSearchMaxLimit: number;
  docShortlistCandidateMin: number;
  docShortlistCandidateMax: number;
  docShortlistDocMin: number;
  docShortlistDocMax: number;
  shortlistGateMinCandidates: number;
  shortlistGateMinDocs: number;
  wikiSearchMaxResults: number;
  wikiSnippetMaxChars: number;
  wikiContextLimit: number;
  webSearchMaxResults: number;
  webSnippetMaxChars: number;
  deepResearchDepth: number;
  deepResearchBranchQueries: number;
  externalSearchMaxWikiTerms: number;
  externalSearchMaxWebTerms: number;
  externalSearchQueryMaxChars: number;
  rerankMaxCandidates: number;
  rerankQueryMaxChars: number;
  overlapMinHitsForMultiTerm: number;
  docSupportMultiHitRatioFactor: number;
  docSupportSingleHitRatioFactor: number;
  docSupportMultiHitRatioMin: number;
  docSupportMultiHitRatioMax: number;
  docSupportSingleHitRatioMin: number;
  docSupportSingleHitRatioMax: number;
}

export interface LLMSettings {
  provider: 'auto' | 'local' | 'ollama';
  model: string;
  temperature: number;
  topP: number;
  topK: number;
  repeatPenalty: number;
  maxTokens: number;           // Matches Rust max_tokens with camelCase
  contextWindow: number;       // Matches Rust context_window with camelCase
  ollamaUrl: string;           // Matches Rust ollama_url with camelCase
  ollamaUtilityModel: string;  // Matches Rust ollama_utility_model with camelCase
  ollamaAuthHeaderName: string;  // Matches Rust ollama_auth_header_name with camelCase
  ollamaAuthHeaderValue: string; // Matches Rust ollama_auth_header_value with camelCase
  timeoutSeconds: number;      // Matches Rust timeout_seconds with camelCase
  streamResponses: boolean;    // Matches Rust stream_responses with camelCase
  prompts: LLMPromptSettings;
  verification: LLMVerificationSettings;
  toolOutput: ToolOutputSettings;
  router: RouterSettings;
  externalModelDirectories: string[]; // Matches Rust external_model_directories with camelCase
  customTools: CustomToolSettings[]; // Matches Rust custom_tools with camelCase
}

export interface CustomToolSettings {
  enabled: boolean;
  name: string;
  description: string;
  endpoint: string;
  queryParam: string;
  maxResultsParam: string | null;
  defaultMaxResults: number;
}

export interface LLMVerificationSettings {
  enabled: boolean;
}

export interface RouterSettings {
  enabled: boolean;
  model: string;
  timeoutMs: number;
  maxTokens: number;
  temperature: number;
  ambiguityThreshold: number;
  preferLastDocument: boolean;
  promptTemplate: string;
  clarifyPromptTemplate: string;
}

export interface LLMPromptSettings {
  systemPrompt: string;
  greetingPromptTemplate: string;
  ragPromptTemplate: string;
  noContextPromptTemplate: string;
  toolFollowupPromptTemplate: string;
}

export interface ToolOutputSettings {
  maxChars: number;
  excerptChars: number;
  maxResults: number;
  highlightTermsMax: number;
  templates: ToolOutputTemplates;
}

export interface ToolOutputTemplates {
  defaultTemplate: string;
  getDocumentTemplate: string;
  semanticSearchTemplate: string;
}

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

export interface UISettings {
  theme: string;
  fontSize: number;            // Matches Rust font_size with camelCase
  showPreview: boolean;        // Matches Rust show_preview with camelCase
  resultsPerPage: number;      // Matches Rust results_per_page with camelCase
  enableAnimations: boolean;   // Matches Rust enable_animations with camelCase
}

export interface SyncSettings {
  syncEnabled: boolean;        // Matches Rust sync_enabled with camelCase
  syncUrl: string;             // Matches Rust sync_url with camelCase
  syncIntervalMinutes: number; // Matches Rust sync_interval_minutes with camelCase
  autoSync: boolean;           // Matches Rust auto_sync with camelCase
  syncOnStartup: boolean;      // Matches Rust sync_on_startup with camelCase
}

export interface BackupSettings {
  autoBackupEnabled: boolean;  // Matches Rust auto_backup_enabled with camelCase
  backupFrequency: string;     // Matches Rust backup_frequency with camelCase
  backupRetentionDays: number; // Matches Rust backup_retention_days with camelCase
  backupPath: string;          // Matches Rust backup_path with camelCase
  compressBackups: boolean;    // Matches Rust compress_backups with camelCase
}
