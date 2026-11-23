/**
 * Composer document files staged for the next turn, keyed by conversation
 * (`__new__` when no conversation ID exists yet).
 *
 * Kept outside component state so switching threads isolates staged files
 * per conversation and switching routes preserves them across unmounts.
 */

import { create } from 'zustand';

export const NEW_CONVERSATION_STAGED_KEY = '__new__';

export interface StagedFile {
  path: string;
  name: string;
}

export function stagedFilesKey(conversationId?: string | null): string {
  return conversationId || NEW_CONVERSATION_STAGED_KEY;
}

const basename = (path: string): string => {
  const normalized = path.replace(/\\/g, '/');
  const name = normalized.slice(normalized.lastIndexOf('/') + 1);
  return name || path;
};

interface StagedFilesState {
  byConversation: Readonly<Record<string, readonly StagedFile[]>>;
  importingByConversation: Readonly<Record<string, boolean>>;
  add: (
    conversationId: string | null | undefined,
    paths: readonly string[],
    stageable?: (_path: string) => boolean
  ) => void;
  remove: (conversationId: string | null | undefined, path: string) => void;
  clear: (conversationId: string | null | undefined) => void;
  migrate: (fromConversationId: string | null | undefined, toConversationId: string) => void;
  setImporting: (conversationId: string | null | undefined, importing: boolean) => void;
}

const EMPTY_STAGED: readonly StagedFile[] = [];

export const useStagedFilesStore = create<StagedFilesState>((set) => ({
  byConversation: {},
  importingByConversation: {},

  add: (conversationId, paths, stageable = () => true) => set((state) => {
    const key = stagedFilesKey(conversationId);
    const usable = [...new Set(paths)].filter((path) => Boolean(path) && stageable(path));
    if (usable.length === 0) return state;
    const current = state.byConversation[key] ?? [];
    const seen = new Set(current.map((file) => file.path));
    const added = usable
      .filter((path) => !seen.has(path))
      .map((path) => ({ path, name: basename(path) }));
    if (added.length === 0) return state;
    return {
      byConversation: {
        ...state.byConversation,
        [key]: [...current, ...added],
      },
    };
  }),

  remove: (conversationId, path) => set((state) => {
    const key = stagedFilesKey(conversationId);
    const current = state.byConversation[key];
    if (!current) return state;
    const next = current.filter((file) => file.path !== path);
    if (next.length === current.length) return state;
    const { [key]: _dropped, ...rest } = state.byConversation;
    return {
      byConversation: next.length > 0 ? { ...rest, [key]: next } : rest,
    };
  }),

  clear: (conversationId) => set((state) => {
    const key = stagedFilesKey(conversationId);
    if (!(key in state.byConversation)) return state;
    const { [key]: _dropped, ...rest } = state.byConversation;
    return { byConversation: rest };
  }),

  migrate: (fromConversationId, toConversationId) => set((state) => {
    const fromKey = stagedFilesKey(fromConversationId);
    const toKey = stagedFilesKey(toConversationId);
    if (fromKey === toKey) return state;
    const moving = state.byConversation[fromKey];
    if (!moving || moving.length === 0) return state;
    const existing = state.byConversation[toKey] ?? [];
    const seen = new Set(existing.map((file) => file.path));
    const merged = [...existing, ...moving.filter((file) => !seen.has(file.path))];
    const { [fromKey]: _dropped, ...rest } = state.byConversation;
    return {
      byConversation: {
        ...rest,
        [toKey]: merged,
      },
    };
  }),

  setImporting: (conversationId, importing) => set((state) => {
    const key = stagedFilesKey(conversationId);
    if (importing) {
      if (state.importingByConversation[key]) return state;
      return {
        importingByConversation: {
          ...state.importingByConversation,
          [key]: true,
        },
      };
    }
    if (!(key in state.importingByConversation)) return state;
    const { [key]: _dropped, ...rest } = state.importingByConversation;
    return { importingByConversation: rest };
  }),
}));

export function useStagedFiles(conversationId?: string | null): readonly StagedFile[] {
  const key = stagedFilesKey(conversationId);
  return useStagedFilesStore((state) => state.byConversation[key] ?? EMPTY_STAGED);
}

export function useIsStagedFilesImporting(conversationId?: string | null): boolean {
  const key = stagedFilesKey(conversationId);
  return useStagedFilesStore((state) => Boolean(state.importingByConversation[key]));
}

export function migrateStagedFiles(
  fromConversationId: string | null | undefined,
  toConversationId: string
): void {
  useStagedFilesStore.getState().migrate(fromConversationId, toConversationId);
}
