/**
 * Per-role warmup status: the backend's record of each model role's latest
 * warm-up, read when the window opens and followed by `model:warmup-status`
 * events. The chat role's state powers the chat input skeleton, for chats on
 * the default model; Learning never masks the composer.
 */

import { create } from 'zustand';

import type { WarmupRole } from '@/lib/bindings';
import type { EventSchemas } from '@/types/events';

export type { WarmupRole } from '@/lib/bindings';
export type WarmupPhase = 'idle' | EventSchemas.Models.WarmupStatus['phase'];

export interface RoleWarmupState {
  phase: WarmupPhase;
  error: string | null;
  /** Which model this state describes, once something has claimed the role. */
  modelId: string | null;
  /** The backend's revision of the status held; 0 before any arrived. */
  revision: number;
}

interface ModelWarmupStore {
  chat: RoleWarmupState;
  utility: RoleWarmupState;
  embedding: RoleWarmupState;
  learning: RoleWarmupState;
  /**
   * Take a warm-up status from the backend. The backend numbers every
   * warm-up record, so a status older than the one held is dropped, whichever
   * way it arrived. Returns whether the status was taken.
   */
  applyStatus: (status: EventSchemas.Models.WarmupStatus) => boolean;
  /**
   * Record that `modelId` now holds `role`. A state left by a different
   * model is dropped: without this, a failure recorded for model A stayed
   * on the role, and the row of model B — never loaded — reported A's error.
   * Boot-time state has no model id, so the current holder adopts it.
   */
  claimRole: (role: WarmupRole, modelId: string) => void;
  reset: () => void;
}

const idleState: RoleWarmupState = {
  phase: 'idle',
  error: null,
  modelId: null,
  revision: 0,
};

export const useModelWarmupStore = create<ModelWarmupStore>((set, get) => ({
  chat: { ...idleState },
  utility: { ...idleState },
  embedding: { ...idleState },
  learning: { ...idleState },
  applyStatus: ({ role, phase, error, revision }) => {
    const prev = get()[role];
    if (revision <= prev.revision) return false;
    const next: RoleWarmupState = { phase, error: error ?? null, modelId: prev.modelId, revision };
    set({ [role]: next } as Pick<ModelWarmupStore, WarmupRole>);
    return true;
  },
  claimRole: (role, modelId) =>
    set((state) => {
      const prev = state[role];
      if (prev.modelId === modelId) return {};
      if (prev.modelId === null) {
        // Boot-time state belongs to whoever holds the role now.
        return { [role]: { ...prev, modelId } } as Pick<ModelWarmupStore, WarmupRole>;
      }
      return { [role]: { ...idleState, modelId, revision: prev.revision } } as Pick<
        ModelWarmupStore,
        WarmupRole
      >;
    }),
  reset: () =>
    set({
      chat: { ...idleState },
      utility: { ...idleState },
      embedding: { ...idleState },
      learning: { ...idleState },
    }),
}));

export const selectIsChatWarming = (s: ModelWarmupStore): boolean => s.chat.phase === 'started';

export const selectIsChatReady = (s: ModelWarmupStore): boolean =>
  s.chat.phase === 'ready' || s.chat.phase === 'skipped';
