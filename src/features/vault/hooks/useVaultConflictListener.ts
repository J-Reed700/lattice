/**
 * Surfaces `vault:note-conflict` events as persistent toasts: a note changed
 * in Lattice and outside it at once, and both versions were kept.
 * Mount once in App.tsx.
 */

import { useEffect } from 'react';


import { toast } from '@/stores/toastStore';
import { TauriEventNames, EventSchemas, listenValidated } from '@/types/events';

import type { UnlistenFn } from '@tauri-apps/api/event';

export function useVaultConflictListener(): void {
  useEffect(() => {
    const isMounted = { current: true };
    let unlisten: UnlistenFn | undefined;

    void (async () => {
      try {
        const handle = await listenValidated(
          TauriEventNames.Vault.NoteConflict,
          EventSchemas.Vault.NoteConflict,
          (event) => {
            const conflict = event.payload;
            if (conflict.kind === 'keptCopy') {
              toast.warning(`Kept both versions of "${conflict.noteTitle}"`, {
                message: `It was changed in Lattice and in its file at the same time. The note now has the file's version. Lattice's version is in "${conflict.copyTitle}".`,
                duration: 0,
              });
            } else {
              toast.warning(`Kept the file that replaced "${conflict.noteTitle}"`, {
                message: `Its file was replaced by one that isn't this note. That file is kept at ${conflict.setAsidePath}, and the note was saved back to its own file.`,
                duration: 0,
              });
            }
          },
          (err) => {
            console.error(
              '[useVaultConflictListener] payload validation failed:',
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
        console.error('[useVaultConflictListener] failed to subscribe:', err);
      }
    })();

    return () => {
      isMounted.current = false;
      unlisten?.();
    };
  }, []);
}
