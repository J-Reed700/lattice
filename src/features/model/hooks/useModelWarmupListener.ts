/**
 * Routes `model:warmup-status` events into modelWarmupStore, and re-reads
 * what a finished warm-up may have changed: the loaded models and, for the
 * Learning model, its window shortfall.
 * Mount once in App.tsx — multiple mounts cause duplicate dispatches.
 */

import { useEffect } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import type { LoadedModelDto } from '@/lib/bindings';
import { TauriEventNames, EventSchemas, listenValidated } from '@/types/events';

import { LEARNING_WINDOW_SHORTFALL_QUERY_KEY } from './useLearningWindowShortfall';
import { LOADED_MODELS_QUERY_KEY } from './useLoadedModels';
import {
  MAX_WARMUP_AGE_MS,
  useModelWarmupStore,
  type RoleWarmupState,
  type WarmupRole,
} from '../stores/modelWarmupStore';

import type { UnlistenFn } from '@tauri-apps/api/event';

const WARMUP_ROLES: readonly WarmupRole[] = ['chat', 'utility', 'embedding', 'learning'];

function hasLoadedChatModel(
  loaded: LoadedModelDto[] | undefined,
  chat: Pick<RoleWarmupState, 'phase' | 'startedAt' | 'modelId'>,
): boolean {
  if (chat.phase !== 'started' || !loaded || loaded.length === 0) return false;
  return loaded.some((entry) => {
    const holdsChat = entry.pinned_by.some(
      (tag) => tag === 'chat' || tag.startsWith('chat:'),
    );
    const matchesModelId =
      chat.modelId !== null &&
      entry.model.kind === 'local' &&
      entry.model.model_id === chat.modelId;
    if (!holdsChat && !matchesModelId) return false;
    if (!entry.loaded_at || chat.startedAt === null) return true;
    const loadedAtMs = Date.parse(entry.loaded_at);
    return Number.isNaN(loadedAtMs) || loadedAtMs >= chat.startedAt;
  });
}

export function useModelWarmupListener(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    const isMounted = { current: true };
    let unlisten: UnlistenFn | undefined;

    void (async () => {
      try {
        const setRolePhase = useModelWarmupStore.getState().setRolePhase;
        const handle = await listenValidated(
          TauriEventNames.Models.WarmupStatus,
          EventSchemas.Models.WarmupStatus,
          (event) => {
            const { role, phase, error } = event.payload;
            setRolePhase(role, phase, error ?? null);
            if (phase === 'started') return;
            void queryClient.invalidateQueries({ queryKey: LOADED_MODELS_QUERY_KEY });
            if (role === 'learning') {
              void queryClient.invalidateQueries({ queryKey: LEARNING_WINDOW_SHORTFALL_QUERY_KEY });
            }
          },
          (err) => {
            console.error('[useModelWarmupListener] payload validation failed:', err.format());
          },
        );

        if (!isMounted.current) {
          handle();
          return;
        }
        unlisten = handle;
      } catch (err) {
        console.error('[useModelWarmupListener] failed to subscribe:', err);
      }
    })();

    return () => {
      isMounted.current = false;
      unlisten?.();
    };
  }, [queryClient]);

  useEffect(() => {
    let timeoutId: ReturnType<typeof setTimeout> | null = null;

    const clearTimer = () => {
      if (timeoutId !== null) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
    };

    const reconcileLoaded = () => {
      const store = useModelWarmupStore.getState();
      if (store.chat.phase !== 'started') return;
      const loaded = queryClient.getQueryData<LoadedModelDto[]>(LOADED_MODELS_QUERY_KEY);
      if (hasLoadedChatModel(loaded, store.chat)) {
        store.setRolePhase('chat', 'ready', null);
      }
    };

    const scheduleExpiry = () => {
      clearTimer();
      const now = Date.now();
      const state = useModelWarmupStore.getState();
      let nextDelay: number | null = null;
      for (const role of WARMUP_ROLES) {
        const entry = state[role];
        if (entry.phase !== 'started' || entry.startedAt === null) continue;
        const remaining = Math.max(0, MAX_WARMUP_AGE_MS - (now - entry.startedAt));
        nextDelay = nextDelay === null ? remaining : Math.min(nextDelay, remaining);
      }
      if (nextDelay === null) return;
      timeoutId = setTimeout(() => {
        useModelWarmupStore.getState().expireStaleWarmups();
        scheduleExpiry();
      }, nextDelay);
    };

    reconcileLoaded();
    scheduleExpiry();

    const unsubscribeStore = useModelWarmupStore.subscribe(() => {
      reconcileLoaded();
      scheduleExpiry();
    });
    const unsubscribeCache = queryClient.getQueryCache().subscribe((event) => {
      if (event.query.queryKey[0] === LOADED_MODELS_QUERY_KEY[0]) {
        reconcileLoaded();
      }
    });

    return () => {
      clearTimer();
      unsubscribeStore();
      unsubscribeCache();
    };
  }, [queryClient]);
}
