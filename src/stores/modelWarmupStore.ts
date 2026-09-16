/**
 * Per-role boot-time warmup status. Powers the chat input skeleton.
 */

import { create } from 'zustand';

export type WarmupRole = 'chat' | 'utility' | 'embedding';
export type WarmupPhase = 'idle' | 'started' | 'ready' | 'skipped' | 'failed';

export interface RoleWarmupState {
  phase: WarmupPhase;
  startedAt: number | null;
  completedAt: number | null;
  error: string | null;
}

interface ModelWarmupStore {
  chat: RoleWarmupState;
  utility: RoleWarmupState;
  embedding: RoleWarmupState;
  setRolePhase: (role: WarmupRole, phase: WarmupPhase, error?: string | null) => void;
  reset: () => void;
}

const idleState: RoleWarmupState = {
  phase: 'idle',
  startedAt: null,
  completedAt: null,
  error: null,
};

export const useModelWarmupStore = create<ModelWarmupStore>((set) => ({
  chat: { ...idleState },
  utility: { ...idleState },
  embedding: { ...idleState },
  setRolePhase: (role, phase, error = null) =>
    set((state) => {
      const now = Date.now();
      const prev = state[role];
      const next: RoleWarmupState = {
        phase,
        startedAt: phase === 'started' ? now : prev.startedAt,
        completedAt:
          phase === 'ready' || phase === 'skipped' || phase === 'failed' ? now : prev.completedAt,
        error,
      };
      return { [role]: next } as Pick<ModelWarmupStore, WarmupRole>;
    }),
  reset: () =>
    set({
      chat: { ...idleState },
      utility: { ...idleState },
      embedding: { ...idleState },
    }),
}));

export const selectIsChatWarming = (s: ModelWarmupStore): boolean =>
  s.chat.phase === 'started';

export const selectIsChatReady = (s: ModelWarmupStore): boolean =>
  s.chat.phase === 'ready' || s.chat.phase === 'skipped';
