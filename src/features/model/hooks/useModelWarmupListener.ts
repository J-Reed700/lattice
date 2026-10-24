import { useEffect } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { TauriEventNames, EventSchemas, listenValidated } from '@/types/events';

import { LEARNING_WINDOW_SHORTFALL_QUERY_KEY } from './useLearningWindowShortfall';
import { LOADED_MODELS_QUERY_KEY } from './useLoadedModels';
import { useModelWarmupStore } from '../stores/modelWarmupStore';

import type { UnlistenFn } from '@tauri-apps/api/event';

/** Routes model warmup events into modelWarmupStore. Mount once at the app root. */
export function useModelWarmupListener(): void {
  const queryClient = useQueryClient();
  useEffect(() => {
    const isMounted = { current: true };
    let unlisten: UnlistenFn | undefined;

    (async () => {
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
