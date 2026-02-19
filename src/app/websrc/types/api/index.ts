/**
 * API Types Index
 *
 * Central export point for all API-related types.
 * These types provide strong typing for Tauri IPC commands.
 */

// Settings
export * from './settings';

// Daily Notes
export * from './dailyNotes';

// Tags
export * from './tags';

// Favorites
export * from './favorites';

// LLM/Q&A
export * from './llm';

// Cache (re-export specific types to avoid conflict with modelCatalog)
export type { CacheMetrics, LLMCacheStats } from './cache';

// Health
export * from './health';

// Mentions
export * from './mentions';

// Files
export * from './files';

// Updates
export * from './updates';

// Metrics
export * from './metrics';

// Embeddings
export * from './embeddings';

// Extraction
export * from './extraction';

// Backup
export * from './backup';

// Web
export * from './web';

// Function Calling (Wave 2B)
export * from './function_calling';

// Batch Jobs (Wave 3)
export * from './batch';

// Credentials (Wave 3)
export * from './credentials';

// Models (Wave 4B)
export * from './models';

// Conversations (Wave 4)
export * from './conversation';

// Watch Folders (Wave 4)
export * from './watch';

// Statistics (Wave 4)
export * from './stats';
