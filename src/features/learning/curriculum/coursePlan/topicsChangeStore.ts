import { create } from 'zustand';

export type TopicsFailure = { at: string | 'selection'; error: unknown };

export interface TopicsChangeRun {
  pendingAt: string | null;
  failure: TopicsFailure | null;
}

interface TopicsChangeState {
  runs: Record<string, TopicsChangeRun>;
  runAction: (
    programId: string,
    at: string | 'selection',
    action: () => Promise<unknown>,
  ) => Promise<boolean>;
  clearFailure: (programId: string) => void;
}

const initialState = () => ({ runs: {} as Record<string, TopicsChangeRun> });

/** Persists Review Topics pending and failure state per course across route/tab leaves. */
export const useTopicsChangeStore = create<TopicsChangeState>((set, get) => ({
  ...initialState(),
  runAction: async (programId, at, action) => {
    if (get().runs[programId]?.pendingAt) return false;
    set((state) => ({
      runs: {
        ...state.runs,
        [programId]: { pendingAt: at, failure: null },
      },
    }));
    try {
      await action();
      set((state) => ({
        runs: {
          ...state.runs,
          [programId]: { pendingAt: null, failure: null },
        },
      }));
      return true;
    } catch (caught) {
      set((state) => ({
        runs: {
          ...state.runs,
          [programId]: { pendingAt: null, failure: { at, error: caught } },
        },
      }));
      return false;
    }
  },
  clearFailure: (programId) => set((state) => {
    const existing = state.runs[programId];
    if (!existing?.failure) return state;
    return {
      runs: {
        ...state.runs,
        [programId]: { ...existing, failure: null },
      },
    };
  }),
}));

export function resetTopicsChangeStore(): void {
  useTopicsChangeStore.setState(initialState());
}
