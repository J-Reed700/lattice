/**
 * Where each conversation's `/compact` stands.
 *
 * Compacting runs the utility model over the older messages, which takes a
 * minute or more, so the chat says so for as long as it runs and then says how
 * it went. The state lives here rather than in the chat panel: the run carries
 * on when the panel unmounts (leaving Explorer for Chat, switching threads),
 * and coming back has to find it still running, or finished, not forgotten.
 */
import { create } from 'zustand';

import type { CompactionRecord } from '@/types/conversation';

export type CompactionRun =
  | { state: 'running'; startedAt: number }
  | { state: 'done'; startedAt: number; finishedAt: number; record: CompactionRecord }
  | { state: 'failed'; startedAt: number; finishedAt: number; error: string };

interface CompactionStore {
  /** By conversation id. A dismissed run is removed. */
  runs: Readonly<Record<string, CompactionRun>>;
  /** `false` when one is already running for that conversation. */
  start: (_conversationId: string) => boolean;
  finish: (_conversationId: string, _record: CompactionRecord) => void;
  fail: (_conversationId: string, _error: string) => void;
  dismiss: (_conversationId: string) => void;
}

export const useCompactionStore = create<CompactionStore>((set, get) => ({
  runs: {},

  start: (conversationId) => {
    if (get().runs[conversationId]?.state === 'running') return false;
    set({ runs: { ...get().runs, [conversationId]: { state: 'running', startedAt: Date.now() } } });
    return true;
  },

  finish: (conversationId, record) => {
    const startedAt = get().runs[conversationId]?.startedAt ?? Date.now();
    set({
      runs: { ...get().runs, [conversationId]: { state: 'done', startedAt, finishedAt: Date.now(), record } },
    });
  },

  fail: (conversationId, error) => {
    const startedAt = get().runs[conversationId]?.startedAt ?? Date.now();
    set({
      runs: { ...get().runs, [conversationId]: { state: 'failed', startedAt, finishedAt: Date.now(), error } },
    });
  },

  dismiss: (conversationId) => {
    const run = get().runs[conversationId];
    if (!run || run.state === 'running') return;
    const runs = { ...get().runs };
    delete runs[conversationId];
    set({ runs });
  },
}));
