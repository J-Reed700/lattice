/**
 * Per-role warmup status: boot prewarm and the warm-up buttons. The chat
 * role's state powers the chat input skeleton, for chats on the default
 * model; Learning is never prewarmed at boot and never masks the composer.
 */

import { create } from 'zustand';

export type WarmupRole = 'chat' | 'utility' | 'embedding' | 'learning';
export type WarmupPhase = 'idle' | 'started' | 'ready' | 'skipped' | 'failed';

export const MAX_WARMUP_AGE_MS = 90_000;

export interface RoleWarmupState {
  phase: WarmupPhase;
  startedAt: number | null;
  completedAt: number | null;
  error: string | null;
  /** Which model this state describes, once something has claimed the role. */
  modelId: string | null;
}

interface ModelWarmupStore {
  chat: RoleWarmupState;
  utility: RoleWarmupState;
  embedding: RoleWarmupState;
  learning: RoleWarmupState;
  setRolePhase: (
    role: WarmupRole,
    phase: WarmupPhase,
    error?: string | null,
    modelId?: string | null,
  ) => void;
  /**
   * Record that `modelId` now holds `role`. A state left by a different
   * model is dropped: without this, a failure recorded for model A stayed
   * on the role, and the row of model B — never loaded — reported A's error.
   * Boot-time state has no model id, so the current holder adopts it.
   */
  claimRole: (role: WarmupRole, modelId: string) => void;
  /** Release any role stuck in `started` past `MAX_WARMUP_AGE_MS` if its terminal event was dropped. */
  expireStaleWarmups: (now?: number) => void;
  reset: () => void;
}

const WARMUP_ROLES: readonly WarmupRole[] = ['chat', 'utility', 'embedding', 'learning'];

export function isWarmupExpired(
  roleState: Pick<RoleWarmupState, 'phase' | 'startedAt'>,
  now = Date.now(),
): boolean {
  return (
    roleState.phase === 'started' &&
    roleState.startedAt !== null &&
    now - roleState.startedAt >= MAX_WARMUP_AGE_MS
  );
}

const idleState: RoleWarmupState = {
  phase: 'idle',
  startedAt: null,
  completedAt: null,
  error: null,
  modelId: null,
};

export const useModelWarmupStore = create<ModelWarmupStore>((set) => ({
  chat: { ...idleState },
  utility: { ...idleState },
  embedding: { ...idleState },
  learning: { ...idleState },
  setRolePhase: (role, phase, error = null, modelId) =>
    set((state) => {
      const now = Date.now();
      const prev = state[role];
      const next: RoleWarmupState = {
        phase,
        startedAt: phase === 'started' ? now : prev.startedAt,
        completedAt:
          phase === 'ready' || phase === 'skipped' || phase === 'failed' ? now : prev.completedAt,
        error,
        modelId: modelId === undefined ? prev.modelId : modelId,
      };
      return { [role]: next } as Pick<ModelWarmupStore, WarmupRole>;
    }),
  claimRole: (role, modelId) =>
    set((state) => {
      const prev = state[role];
      if (prev.modelId === modelId) return {};
      if (prev.modelId === null) {
        // Boot-time state belongs to whoever holds the role now.
        return { [role]: { ...prev, modelId } } as Pick<ModelWarmupStore, WarmupRole>;
      }
      return { [role]: { ...idleState, modelId } } as Pick<ModelWarmupStore, WarmupRole>;
    }),
  expireStaleWarmups: (now = Date.now()) =>
    set((state) => {
      const updates: Partial<Pick<ModelWarmupStore, WarmupRole>> = {};
      let changed = false;
      for (const role of WARMUP_ROLES) {
        const prev = state[role];
        if (!isWarmupExpired(prev, now)) continue;
        updates[role] = {
          ...prev,
          phase: 'skipped',
          completedAt: now,
          error: null,
        };
        changed = true;
      }
      return changed ? updates : {};
    }),
  reset: () =>
    set({
      chat: { ...idleState },
      utility: { ...idleState },
      embedding: { ...idleState },
      learning: { ...idleState },
    }),
}));

export const selectIsChatWarming = (s: ModelWarmupStore): boolean =>
  s.chat.phase === 'started' && !isWarmupExpired(s.chat);

export const selectIsChatReady = (s: ModelWarmupStore): boolean =>
  s.chat.phase === 'ready' || s.chat.phase === 'skipped';
