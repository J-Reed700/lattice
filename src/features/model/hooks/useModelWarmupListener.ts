/**
 * Routes `model:warmup-status` events into modelWarmupStore, and re-reads
 * what a finished warm-up may have changed: the loaded models and, for the
 * Learning model, its window shortfall.
 * Mount once in App.tsx — multiple mounts cause duplicate dispatches.
 */

import { useEffect } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { TauriEventNames, EventSchemas, listenValidated } from '@/types/events';


import { LEARNING_WINDOW_SHORTFALL_QUERY_KEY } from './useLearningWindowShortfall';
import { LOADED_MODELS_QUERY_KEY } from './useLoadedModels';
import { useModelWarmupStore } from '../stores/modelWarmupStore';

import type { UnlistenFn } from '@tauri-apps/api/event';

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
}
