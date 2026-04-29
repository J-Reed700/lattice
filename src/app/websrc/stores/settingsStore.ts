/**
 * Settings Store — UI Preferences ONLY
 *
 * Per Phase 4b: Zustand is the home for *pure UI preferences only*. Anything
 * the backend reads — search, indexing, AI, privacy, etc. — must go through
 * React Query (useQuery/useMutation) against the Rust SettingsRepository,
 * which is the SSOT.
 *
 * What lives here:
 *   - theme, fontSize, compactMode, showPreviews
 *
 * What does NOT live here (was here before Phase 4b — moved or deleted):
 *   - search slice          → SearchTab pulls directly from backend
 *   - indexing slice        → IndexingTab uses React Query
 *   - ai slice              → DELETED (all 4 fields had zero backend readers)
 *   - privacy slice         → moved to Rust SettingsRepository
 *
 * Persisted to localStorage. The OS theme override is fine living
 * client-side; the backend never needs to know what UI theme the user picked.
 */

import { z } from 'zod';
import { create } from 'zustand';
import { persist, createJSONStorage } from 'zustand/middleware';

// ===== Zod Schema =====

export const DisplaySettingsSchema = z.object({
  theme: z.enum(['light', 'dark', 'system']),
  fontSize: z.enum(['small', 'medium', 'large']),
  compactMode: z.boolean(),
  showPreviews: z.boolean(),
});

export type DisplaySettings = z.infer<typeof DisplaySettingsSchema>;

// ===== Defaults =====

export const DEFAULT_DISPLAY_SETTINGS: DisplaySettings = {
  theme: 'system',
  fontSize: 'medium',
  compactMode: false,
  showPreviews: true,
};

// ===== Store Interface =====

interface SettingsStore {
  display: DisplaySettings;
  updateDisplay: (_updates: Partial<DisplaySettings>) => void;
  resetDisplay: () => void;
}

// ===== Store Implementation =====

export const useSettingsStore = create<SettingsStore>()(
  persist(
    (set) => ({
      display: DEFAULT_DISPLAY_SETTINGS,

      updateDisplay: (updates) => {
        set((state) => {
          const next = { ...state.display, ...updates };
          try {
            DisplaySettingsSchema.parse(next);
            return { display: next };
          } catch (error) {
            console.error('[Settings] Invalid display settings:', error);
            return state;
          }
        });
      },

      resetDisplay: () => {
        set({ display: DEFAULT_DISPLAY_SETTINGS });
      },
    }),
    {
      name: 'lattice-settings',
      storage: createJSONStorage(() => localStorage),
      // Bumped to v3 in Phase 4b step 4: store now holds *only* the
      // display slice. Anything else previously persisted here is dead
      // weight; the migration drops it.
      version: 3,
      migrate: (persistedState: unknown) => {
        // Pull just the display slice from any legacy shape; fall back to
        // defaults if it isn't there or is malformed.
        if (
          persistedState &&
          typeof persistedState === 'object' &&
          'settings' in persistedState &&
          typeof (persistedState as { settings: unknown }).settings === 'object'
        ) {
          const legacy = (persistedState as { settings: { display?: unknown } }).settings;
          const display = legacy.display;
          const parsed = DisplaySettingsSchema.safeParse(display);
          return { display: parsed.success ? parsed.data : DEFAULT_DISPLAY_SETTINGS };
        }
        if (
          persistedState &&
          typeof persistedState === 'object' &&
          'display' in persistedState
        ) {
          const display = (persistedState as { display: unknown }).display;
          const parsed = DisplaySettingsSchema.safeParse(display);
          return { display: parsed.success ? parsed.data : DEFAULT_DISPLAY_SETTINGS };
        }
        return { display: DEFAULT_DISPLAY_SETTINGS };
      },
    }
  )
);

// ===== Selectors =====

export const selectDisplaySettings = (state: SettingsStore) => state.display;
export const selectTheme = (state: SettingsStore) => state.display.theme;
