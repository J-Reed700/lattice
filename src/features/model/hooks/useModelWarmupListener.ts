/**
 * Applies the backend's warm-up record into modelWarmupStore: the whole board
 * once on mount, then each `model:warmup-status` event. A finished warm-up
 * also re-reads what it may have changed: the loaded models and, for the
 * Learning model, its window shortfall.
 * Mount once in App.tsx — multiple mounts cause duplicate dispatches.
 */

import { useEffect } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { TauriEventNames, EventSchemas, listenValidated } from '@/types/events';

import { LEARNING_WINDOW_SHORTFALL_QUERY_KEY } from './useLearningWindowShortfall';
import { LOADED_MODELS_QUERY_KEY } from './useLoadedModels';
import { useModelWarmupStore } from '../stores/modelWarmupStore';

import type { UnlistenFn } from '@tauri-apps/api/event';

export function useModelWarmupListener(): void {
  const queryClient = useQueryClient();
  const api = useApi();
  useEffect(() => {
    const isMounted = { current: true };
    let unlisten: UnlistenFn | undefined;

    const apply = (status: EventSchemas.Models.WarmupStatus) => {
      if (!useModelWarmupStore.getState().applyStatus(status)) return;
      if (status.phase === 'started') return;
      void queryClient.invalidateQueries({ queryKey: LOADED_MODELS_QUERY_KEY });
      if (status.role === 'learning') {
        void queryClient.invalidateQueries({ queryKey: LEARNING_WINDOW_SHORTFALL_QUERY_KEY });
      }
    };

    void (async () => {
      try {
        const handle = await listenValidated(
          TauriEventNames.Models.WarmupStatus,
          EventSchemas.Models.WarmupStatus,
          (event) => apply(event.payload),
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
        return;
      }

      try {
        const result = await api.warmupStatus();
        if (!isMounted.current) return;
        if (result.ok) {
          result.data.forEach(apply);
        } else {
          console.error('[useModelWarmupListener] failed to read the warm-up record:', result.error);
        }
      } catch (err) {
        console.error('[useModelWarmupListener] failed to read the warm-up record:', err);
      }
    })();

    return () => {
      isMounted.current = false;
      unlisten?.();
    };
  }, [queryClient, api]);
}
