/**
 * Tick on every external `.md` import. Consumers watch the counter
 * (not the value) and refetch.
 */

import { create } from 'zustand';

interface VaultImportStore {
  importTick: number;
  /// Useful for scoped refetches (e.g. only the currently-open note).
  lastImportedId: string | null;
  bump: (noteId: string) => void;
}

export const useVaultImportStore = create<VaultImportStore>((set) => ({
  importTick: 0,
  lastImportedId: null,
  bump: (noteId) =>
    set((s) => ({ importTick: s.importTick + 1, lastImportedId: noteId })),
}));
