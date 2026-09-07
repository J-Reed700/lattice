/**
 * Routes `model:warmup-status` events into modelWarmupStore.
 * Mount once in App.tsx — multiple mounts cause duplicate dispatches.
 */

import { useEffect } from 'react';


import { useModelWarmupStore } from '../stores/modelWarmupStore';
import { TauriEventNames, EventSchemas, listenValidated } from '../types/events';

import type { UnlistenFn } from '@tauri-apps/api/event';

export function useModelWarmupListener(): void {
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
  }, []);
}
