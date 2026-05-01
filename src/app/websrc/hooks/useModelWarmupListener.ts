/**
 * useModelWarmupListener
 *
 * Subscribes once at app mount to the backend `model:warmup-status` event
 * stream and routes each phase update into the modelWarmupStore so any
 * component (chat input mask, header, etc.) can read the per-role state
 * with a simple selector.
 *
 * Mount this exactly once — at App.tsx, NOT in every consumer. Multiple
 * mounts produce duplicate dispatches.
 */

import { useEffect } from 'react';

import type { UnlistenFn } from '@tauri-apps/api/event';

import { useModelWarmupStore } from '../stores/modelWarmupStore';
import { TauriEventNames, EventSchemas, listenValidated } from '../types/events';

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
