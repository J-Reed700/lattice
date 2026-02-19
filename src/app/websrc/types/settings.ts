export interface IndexingSettings {
  chunkSize: number;
  chunkOverlap: number;
  batchSize: number;
  autoIndexNewFiles: boolean;
  fileTypes: string[];
  excludedPaths: string[];
}

export interface SearchSettings {
  maxResults: number;
  similarityThreshold: number;
  enableReranking: boolean;
  hybridSearchAlpha: number;
  enableHybridSearch: boolean;
  vectorWeight: number;
  textWeight: number;
  recencyWeight: number;
  maxAgeDays: number;
}

export interface LLMSettings {
  provider: 'auto' | 'local' | 'ollama';
  model: string;
  temperature: number;
  maxTokens: number;
  contextWindow: number;
  ollamaUrl: string;
  ollamaAuthHeaderName: string;
  ollamaAuthHeaderValue: string;
  timeoutSeconds: number;
  streamResponses: boolean;
}

export interface UISettings {
  theme: 'dark' | 'light' | 'system';
  fontSize: number;
  showPreview: boolean;
  resultsPerPage: number;
  enableAnimations: boolean;
}

export interface SyncSettings {
  syncEnabled: boolean;
  syncUrl: string;
  syncIntervalMinutes: number;
  autoSync: boolean;
  syncOnStartup: boolean;
}

export interface BackupSettings {
  autoBackupEnabled: boolean;
  backupFrequency: 'hourly' | 'daily' | 'weekly';
  backupRetentionDays: number;
  backupPath: string;
  compressBackups: boolean;
}

export interface AppSettings {
  indexing: IndexingSettings;
  search: SearchSettings;
  llm: LLMSettings;
  ui: UISettings;
  sync: SyncSettings;
  backup: BackupSettings;
}

export const VALIDATION_RULES = {
  indexing: {
    chunkSize: { min: 100, max: 5000 },
    chunkOverlap: { min: 0, max: 1000 },
    batchSize: { min: 1, max: 128 },
  },
  search: {
    maxResults: { min: 1, max: 100 },
    similarityThreshold: { min: 0.0, max: 1.0 },
    hybridSearchAlpha: { min: 0.0, max: 1.0 },
    vectorWeight: { min: 0.0, max: 1.0 },
    textWeight: { min: 0.0, max: 1.0 },
    recencyWeight: { min: 0.0, max: 1.0 },
    maxAgeDays: { min: 30, max: 3650 },
  },
  llm: {
    temperature: { min: 0.0, max: 2.0 },
    maxTokens: { min: 100, max: 262144 },
    contextWindow: { min: 1024, max: 262144 },
    timeoutSeconds: { min: 10, max: 300 },
  },
  ui: {
    fontSize: { min: 10, max: 24 },
    resultsPerPage: { min: 5, max: 50 },
  },
  sync: {
    syncIntervalMinutes: { min: 1, max: 1440 },
  },
  backup: {
    backupRetentionDays: { min: 1, max: 365 },
  },
} as const;
