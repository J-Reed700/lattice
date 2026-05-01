/**
 * Model Warmup Store
 *
 * Tracks the boot-time pre-warm status of the chat / utility / embedding
 * model roles so the chat input can mask while a cold-mmap is in flight.
 *
 * This is NOT backend state — it's a derived view of transient `model:warmup-status`
 * events the Rust container fires once at boot. Zustand is appropriate here because
 * the data has no SQL truth source and no other consumer needs to write it.
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

/// True while the chat model is mid-cold-load. Intended for the chat input
/// skeleton — flips false once the chat role is `ready`, `skipped`, or
/// `failed` (the last two mean we never actually warm — caller falls back).
export const selectIsChatWarming = (s: ModelWarmupStore): boolean =>
  s.chat.phase === 'started';

/// True when the chat model has completed prewarm in any terminal state.
/// Useful for "first ready" toasts.
export const selectIsChatReady = (s: ModelWarmupStore): boolean =>
  s.chat.phase === 'ready' || s.chat.phase === 'skipped';
