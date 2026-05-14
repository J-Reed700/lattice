/**
 * Bumps `useVaultImportStore` on `vault:note-imported` events.
 * Mount once in App.tsx.
 */

import { useEffect } from 'react';

import type { UnlistenFn } from '@tauri-apps/api/event';

import { useVaultImportStore } from '../stores/vaultImportStore';
import { TauriEventNames, EventSchemas, listenValidated } from '../types/events';

export function useVaultImportListener(): void {
  useEffect(() => {
    const isMounted = { current: true };
    let unlisten: UnlistenFn | undefined;

    (async () => {
      try {
        const bump = useVaultImportStore.getState().bump;
        const handle = await listenValidated(
          TauriEventNames.Vault.NoteImported,
          EventSchemas.Vault.NoteImported,
          (event) => {
            bump(event.payload.noteId);
          },
          (err) => {
            console.error(
              '[useVaultImportListener] payload validation failed:',
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
        console.error('[useVaultImportListener] failed to subscribe:', err);
      }
    })();

    return () => {
      isMounted.current = false;
      unlisten?.();
    };
  }, []);
}
