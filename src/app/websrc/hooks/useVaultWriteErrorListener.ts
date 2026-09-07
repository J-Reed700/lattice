/**
 * Surfaces `vault:write-error` events as persistent toasts. SQL commit
 * already succeeded — only the markdown mirror is missing.
 * Mount once in App.tsx.
 */

import { useEffect } from 'react';


import { toast } from '../stores/toastStore';
import { TauriEventNames, EventSchemas, listenValidated } from '../types/events';

import type { UnlistenFn } from '@tauri-apps/api/event';

export function useVaultWriteErrorListener(): void {
  useEffect(() => {
    const isMounted = { current: true };
    let unlisten: UnlistenFn | undefined;

    (async () => {
      try {
        const handle = await listenValidated(
          TauriEventNames.Vault.WriteError,
          EventSchemas.Vault.WriteError,
          (event) => {
            const { target, error } = event.payload;
            // duration: 0 = persistent. Title is "saved-but-divergent"
            // so the user understands the SQL save worked.
            toast.error('Note saved, but vault mirror failed', {
              message: `${error}\n\nFile: ${target}`,
              duration: 0,
            });
          },
          (err) => {
            console.error(
              '[useVaultWriteErrorListener] payload validation failed:',
              err.format(),
            );
          },
        );

        if (!isMounted.current) {
          handle();
          return;
        }
        unlisten = handle;
      } catch (err) {
        console.error('[useVaultWriteErrorListener] failed to subscribe:', err);
      }
    })();

    return () => {
      isMounted.current = false;
      unlisten?.();
    };
  }, []);
}
