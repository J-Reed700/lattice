/**
 * Bumps `useVaultImportStore` on `vault:note-imported` events.
 * Mount once in App.tsx.
 */

import { useEffect } from 'react';

import { useQueryClient } from '@tanstack/react-query';

import { WORKSPACE_NOTES_QUERY_KEY } from '@/lib/queryClient';
import { TauriEventNames, EventSchemas, listenValidated } from '@/types/events';

import { useVaultImportStore } from '../stores/vaultImportStore';

import type { UnlistenFn } from '@tauri-apps/api/event';

export function useVaultImportListener(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    const isMounted = { current: true };
    let unlisten: UnlistenFn | undefined;

    void (async () => {
      try {
        const bump = useVaultImportStore.getState().bump;
        const handle = await listenValidated(
          TauriEventNames.Vault.NoteImported,
          EventSchemas.Vault.NoteImported,
          (event) => {
            bump(event.payload.noteId);
            void queryClient.invalidateQueries({ queryKey: WORKSPACE_NOTES_QUERY_KEY });
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
  }, [queryClient]);
}
