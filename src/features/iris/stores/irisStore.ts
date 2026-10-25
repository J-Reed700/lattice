import { create } from 'zustand';

/**
 * Iris's UI-only preferences. Whether she is shown survives a restart; which
 * notes you've seen belongs to the session, so she mentions them once per launch.
 */

const ENABLED_KEY = 'iris.enabled';

function readEnabled(): boolean {
  try {
    return localStorage.getItem(ENABLED_KEY) !== '0';
  } catch {
    return true;
  }
}

function writeEnabled(value: boolean): void {
  try {
    localStorage.setItem(ENABLED_KEY, value ? '1' : '0');
  } catch {
    // Preference only.
  }
}

interface IrisState {
  enabled: boolean;
  setEnabled: (enabled: boolean) => void;
  /** Notes you have already seen in the card. */
  seenNoteIds: ReadonlySet<string>;
  markSeen: (noteIds: readonly string[]) => void;
}

export const useIrisStore = create<IrisState>((set) => ({
  enabled: readEnabled(),
  setEnabled: (enabled) => {
    writeEnabled(enabled);
    set({ enabled });
  },
  seenNoteIds: new Set<string>(),
  markSeen: (noteIds) =>
    set((state) => {
      if (noteIds.every((id) => state.seenNoteIds.has(id))) return state;
      return { seenNoteIds: new Set([...state.seenNoteIds, ...noteIds]) };
    }),
}));
