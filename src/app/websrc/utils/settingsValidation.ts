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

export interface ValidationError {
  field: string;
  message: string;
}

export function validateChunkSize(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.indexing.chunkSize;
  if (value < min || value > max) {
    return { field: 'chunkSize', message: `Chunk size must be between ${min} and ${max}` };
  }
  return null;
}

export function validateChunkOverlap(overlap: number, chunkSize: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.indexing.chunkOverlap;
  if (overlap < min || overlap > max) {
    return { field: 'chunkOverlap', message: `Chunk overlap must be between ${min} and ${max}` };
  }
  if (overlap >= chunkSize) {
    return { field: 'chunkOverlap', message: 'Chunk overlap must be less than chunk size' };
  }
  return null;
}

export function validateBatchSize(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.indexing.batchSize;
  if (value < min || value > max) {
    return { field: 'batchSize', message: `Batch size must be between ${min} and ${max}` };
  }
  return null;
}

export function validateMaxResults(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.search.maxResults;
  if (value < min || value > max) {
    return { field: 'maxResults', message: `Max results must be between ${min} and ${max}` };
  }
  return null;
}

export function validateSimilarityThreshold(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.search.similarityThreshold;
  if (value < min || value > max) {
    return { field: 'similarityThreshold', message: `Similarity threshold must be between ${min} and ${max}` };
  }
  return null;
}

export function validateHybridSearchWeights(vectorWeight: number, textWeight: number): ValidationError | null {
  const total = vectorWeight + textWeight;
  if (Math.abs(total - 1.0) > 0.01) {
    return {
      field: 'weights',
      message: `Vector weight and text weight must sum to 1.0, got ${total.toFixed(2)}`
    };
  }
  return null;
}

export function validateTemperature(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.llm.temperature;
  if (value < min || value > max) {
    return { field: 'temperature', message: `Temperature must be between ${min} and ${max}` };
  }
  return null;
}

export function validateMaxTokens(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.llm.maxTokens;
  if (value < min || value > max) {
    return { field: 'maxTokens', message: `Max tokens must be between ${min} and ${max}` };
  }
  return null;
}

export function validateContextWindow(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.llm.contextWindow;
  if (value < min || value > max) {
    return { field: 'contextWindow', message: `Context window must be between ${min} and ${max}` };
  }
  return null;
}

export function validateOllamaUrl(value: string): ValidationError | null {
  if (!value) {
    return { field: 'ollamaUrl', message: 'Ollama URL is required' };
  }
  if (!value.startsWith('http://') && !value.startsWith('https://')) {
    return { field: 'ollamaUrl', message: 'Ollama URL must start with http:// or https://' };
  }
  try {
    new URL(value);
  } catch {
    return { field: 'ollamaUrl', message: 'Invalid URL format' };
  }
  return null;
}

export function validateSyncUrl(value: string): ValidationError | null {
  if (!value) return null;

  if (!value.startsWith('http://') && !value.startsWith('https://')) {
    return { field: 'syncUrl', message: 'Sync URL must start with http:// or https://' };
  }
  try {
    new URL(value);
  } catch {
    return { field: 'syncUrl', message: 'Invalid URL format' };
  }
  return null;
}

export function validateFontSize(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.ui.fontSize;
  if (value < min || value > max) {
    return { field: 'fontSize', message: `Font size must be between ${min} and ${max}` };
  }
  return null;
}

export function validateResultsPerPage(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.ui.resultsPerPage;
  if (value < min || value > max) {
    return { field: 'resultsPerPage', message: `Results per page must be between ${min} and ${max}` };
  }
  return null;
}

export function validateSyncInterval(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.sync.syncIntervalMinutes;
  if (value < min || value > max) {
    return { field: 'syncIntervalMinutes', message: `Sync interval must be between ${min} and ${max} minutes` };
  }
  return null;
}

export function validateBackupRetention(value: number): ValidationError | null {
  const { min, max } = VALIDATION_RULES.backup.backupRetentionDays;
  if (value < min || value > max) {
    return { field: 'backupRetentionDays', message: `Backup retention must be between ${min} and ${max} days` };
  }
  return null;
}

export function validateFileTypes(fileTypes: string[]): ValidationError | null {
  if (fileTypes.length === 0) {
    return { field: 'fileTypes', message: 'At least one file type must be specified' };
  }

  for (const ext of fileTypes) {
    if (!ext.startsWith('.')) {
      return { field: 'fileTypes', message: `File type "${ext}" must start with a dot` };
    }
  }

  return null;
}

export const validators = {
  indexing: {
    chunkSize: validateChunkSize,
    chunkOverlap: validateChunkOverlap,
    batchSize: validateBatchSize,
    fileTypes: validateFileTypes,
  },
  search: {
    maxResults: validateMaxResults,
    similarityThreshold: validateSimilarityThreshold,
    hybridSearchWeights: validateHybridSearchWeights,
  },
  llm: {
    temperature: validateTemperature,
    maxTokens: validateMaxTokens,
    contextWindow: validateContextWindow,
    ollamaUrl: validateOllamaUrl,
  },
  ui: {
    fontSize: validateFontSize,
    resultsPerPage: validateResultsPerPage,
  },
  sync: {
    syncUrl: validateSyncUrl,
    syncInterval: validateSyncInterval,
  },
  backup: {
    backupRetention: validateBackupRetention,
  },
};
