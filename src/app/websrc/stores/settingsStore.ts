/**
 * Settings Store - Persistent Storage with Zustand
 *
 * Manages application settings with localStorage persistence,
 * Zod validation, and migration support.
 */

import { z } from 'zod';
import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';

// ===== Zod Schemas =====

export const SearchSettingsSchema = z.object({
  defaultMode: z.enum(['semantic', 'keyword', 'hybrid']),
  resultsPerPage: z.number().min(5).max(100),
  enableReranking: z.boolean(),
  semanticWeight: z.number().min(0).max(1),
  keywordWeight: z.number().min(0).max(1),
});

export const IndexingSettingsSchema = z.object({
  autoIndex: z.boolean(),
  watchFolders: z.array(z.string()),
  excludePatterns: z.array(z.string()),
  batchSize: z.number().min(1).max(128),
});

export const AISettingsSchema = z.object({
  embeddingModel: z.enum(['bge-m3', 'mpnet']),
  ocrModel: z.enum(['qwen2.5-vl-2b', 'qwen2.5-vl-7b', 'florence-2']),
  useQuantization: z.boolean(),
  enableAgenticRAG: z.boolean(),
});

export const DisplaySettingsSchema = z.object({
  theme: z.enum(['light', 'dark', 'system']),
  fontSize: z.enum(['small', 'medium', 'large']),
  compactMode: z.boolean(),
  showPreviews: z.boolean(),
});

export const PrivacySettingsSchema = z.object({
  telemetryEnabled: z.boolean(),
  crashReporting: z.boolean(),
});

export const SettingsSchema = z.object({
  version: z.number(),
  search: SearchSettingsSchema,
  indexing: IndexingSettingsSchema,
  ai: AISettingsSchema,
  display: DisplaySettingsSchema,
  privacy: PrivacySettingsSchema,
});

// ===== TypeScript Types =====

export type SearchSettings = z.infer<typeof SearchSettingsSchema>;
export type IndexingSettings = z.infer<typeof IndexingSettingsSchema>;
export type AISettings = z.infer<typeof AISettingsSchema>;
export type DisplaySettings = z.infer<typeof DisplaySettingsSchema>;
export type PrivacySettings = z.infer<typeof PrivacySettingsSchema>;
export type Settings = z.infer<typeof SettingsSchema>;

// ===== Default Settings =====

export const DEFAULT_SETTINGS: Settings = {
  version: 1,
  search: {
    defaultMode: 'hybrid',
    resultsPerPage: 20,
    enableReranking: true,
    semanticWeight: 0.7,
    keywordWeight: 0.3,
  },
  indexing: {
    autoIndex: true,
    watchFolders: [],
    excludePatterns: [
      '*.git',
      '*.svn',
      '*.hg',
      '*node_modules',
      '*.vscode',
      '*.idea',
      '*.DS_Store',
      '*Thumbs.db',
    ],
    batchSize: 32,
  },
  ai: {
    embeddingModel: 'bge-m3',
    ocrModel: 'qwen2.5-vl-2b',
    useQuantization: true,
    enableAgenticRAG: false,
  },
  display: {
    theme: 'system',
    fontSize: 'medium',
    compactMode: false,
    showPreviews: true,
  },
  privacy: {
    telemetryEnabled: false,
    crashReporting: false,
  },
};

// ===== Migration Function =====

function migrateSettings(stored: unknown): Settings {
  // Handle missing version or old versions
  if (!stored || typeof stored !== 'object' || !('version' in stored) || typeof stored.version !== 'number') {
    console.log('[Settings] Migrating from legacy settings or no version found');
    // Return a deep clone to ensure the object is not frozen
    return JSON.parse(JSON.stringify(DEFAULT_SETTINGS));
  }

  // Detect frozen objects from Tauri WebView and clear them automatically
  if (Object.isFrozen(stored)) {
    console.warn('[Settings] Detected frozen settings object from old Tauri WebView data');
    console.warn('[Settings] Auto-clearing localStorage and using defaults');
    localStorage.removeItem('lattice-settings');
    // Return a deep clone to ensure the object is not frozen
    return JSON.parse(JSON.stringify(DEFAULT_SETTINGS));
  }

  // Future migrations can be added here
  // if (stored.version === 1) {
  //   // Migrate v1 to v2
  //   stored = { ...stored, version: 2, newField: defaultValue };
  // }

  try {
    // Create a deep clone to avoid modifying frozen objects in Tauri WebView
    // This prevents "TypeError: Attempted to assign to readonly property" errors
    const clonedStored = JSON.parse(JSON.stringify(stored));

    // Validate with Zod
    return SettingsSchema.parse(clonedStored);
  } catch (error) {
    console.error('[Settings] Validation failed, using defaults:', error);
    console.error('[Settings] Clearing corrupted settings from localStorage');
    localStorage.removeItem('lattice-settings');
    // Return a deep clone to ensure the object is not frozen
    return JSON.parse(JSON.stringify(DEFAULT_SETTINGS));
  }
}

// ===== Store Interface =====

interface SettingsStore {
  // State
  settings: Settings;

  // Actions
  updateSearch: (_updates: Partial<SearchSettings>) => void;
  updateIndexing: (_updates: Partial<IndexingSettings>) => void;
  updateAI: (_updates: Partial<AISettings>) => void;
  updateDisplay: (_updates: Partial<DisplaySettings>) => void;
  updatePrivacy: (_updates: Partial<PrivacySettings>) => void;

  // Folder management
  addWatchFolder: (_path: string) => void;
  removeWatchFolder: (_path: string) => void;
  addExcludePattern: (_pattern: string) => void;
  removeExcludePattern: (_pattern: string) => void;

  // Utilities
  resetToDefaults: () => void;
  resetCategory: (_category: keyof Omit<Settings, 'version'>) => void;
  exportSettings: () => string;
  importSettings: (_json: string) => boolean;

  // Validation
  validateSettings: (_settings: Settings) => boolean;
}

// ===== Store Implementation =====

export const useSettingsStore = create<SettingsStore>()(
  persist(
    (set, get) => ({
      settings: DEFAULT_SETTINGS,

      updateSearch: (updates) => {
        set((state) => {
          const newSearch = { ...state.settings.search, ...updates };
          try {
            SearchSettingsSchema.parse(newSearch);
            return {
              settings: {
                ...state.settings,
                search: newSearch,
              },
            };
          } catch (error) {
            console.error('[Settings] Invalid search settings:', error);
            return state;
          }
        });
      },

      updateIndexing: (updates) => {
        set((state) => {
          const newIndexing = { ...state.settings.indexing, ...updates };
          try {
            IndexingSettingsSchema.parse(newIndexing);
            return {
              settings: {
                ...state.settings,
                indexing: newIndexing,
              },
            };
          } catch (error) {
            console.error('[Settings] Invalid indexing settings:', error);
            return state;
          }
        });
      },

      updateAI: (updates) => {
        set((state) => {
          const newAI = { ...state.settings.ai, ...updates };
          try {
            AISettingsSchema.parse(newAI);
            return {
              settings: {
                ...state.settings,
                ai: newAI,
              },
            };
          } catch (error) {
            console.error('[Settings] Invalid AI settings:', error);
            return state;
          }
        });
      },

      updateDisplay: (updates) => {
        set((state) => {
          const newDisplay = { ...state.settings.display, ...updates };
          try {
            DisplaySettingsSchema.parse(newDisplay);
            return {
              settings: {
                ...state.settings,
                display: newDisplay,
              },
            };
          } catch (error) {
            console.error('[Settings] Invalid display settings:', error);
            return state;
          }
        });
      },

      updatePrivacy: (updates) => {
        set((state) => {
          const newPrivacy = { ...state.settings.privacy, ...updates };
          try {
            PrivacySettingsSchema.parse(newPrivacy);
            return {
              settings: {
                ...state.settings,
                privacy: newPrivacy,
              },
            };
          } catch (error) {
            console.error('[Settings] Invalid privacy settings:', error);
            return state;
          }
        });
      },

      addWatchFolder: (path) => {
        set((state) => {
          if (!state.settings.indexing.watchFolders.includes(path)) {
            return {
              settings: {
                ...state.settings,
                indexing: {
                  ...state.settings.indexing,
                  watchFolders: [...state.settings.indexing.watchFolders, path],
                },
              },
            };
          }
          return state;
        });
      },

      removeWatchFolder: (path) => {
        set((state) => ({
          settings: {
            ...state.settings,
            indexing: {
              ...state.settings.indexing,
              watchFolders: state.settings.indexing.watchFolders.filter((f) => f !== path),
            },
          },
        }));
      },

      addExcludePattern: (pattern) => {
        set((state) => {
          if (!state.settings.indexing.excludePatterns.includes(pattern)) {
            return {
              settings: {
                ...state.settings,
                indexing: {
                  ...state.settings.indexing,
                  excludePatterns: [...state.settings.indexing.excludePatterns, pattern],
                },
              },
            };
          }
          return state;
        });
      },

      removeExcludePattern: (pattern) => {
        set((state) => ({
          settings: {
            ...state.settings,
            indexing: {
              ...state.settings.indexing,
              excludePatterns: state.settings.indexing.excludePatterns.filter(
                (p) => p !== pattern
              ),
            },
          },
        }));
      },

      resetToDefaults: () => {
        set({ settings: DEFAULT_SETTINGS });
      },

      resetCategory: (category) => {
        set((state) => ({
          settings: {
            ...state.settings,
            [category]: DEFAULT_SETTINGS[category],
          },
        }));
      },

      exportSettings: () => JSON.stringify(get().settings, null, 2),

      importSettings: (json) => {
        try {
          const parsed = JSON.parse(json);
          const migrated = migrateSettings(parsed);
          set({ settings: migrated });
          return true;
        } catch (error) {
          console.error('[Settings] Import failed:', error);
          return false;
        }
      },

      validateSettings: (settings) => {
        try {
          SettingsSchema.parse(settings);
          return true;
        } catch {
          return false;
        }
      },
    }),
    {
      name: 'lattice-settings',
      storage: createJSONStorage(() => localStorage),
      version: 1,
      migrate: (persistedState: unknown, version: number) => {
        console.log('[Settings] Migrating from version', version);
        // The migrateSettings function already handles frozen objects
        // by creating a deep clone before passing to Zod
        return migrateSettings(persistedState);
      },
    }
  )
);

// ===== Selectors =====

export const selectSearchSettings = (state: SettingsStore) => state.settings.search;
export const selectIndexingSettings = (state: SettingsStore) => state.settings.indexing;
export const selectAISettings = (state: SettingsStore) => state.settings.ai;
export const selectDisplaySettings = (state: SettingsStore) => state.settings.display;
export const selectPrivacySettings = (state: SettingsStore) => state.settings.privacy;
export const selectTheme = (state: SettingsStore) => state.settings.display.theme;
