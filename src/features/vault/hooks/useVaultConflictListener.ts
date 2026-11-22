/**
 * Surfaces `vault:note-conflict` events (plus any conflicts recorded before
 * the frontend mounted) as persistent toasts: a note changed in Lattice and
 * outside it at once, and both versions were kept.
 * Mount once in App.tsx.
 */

import { useEffect } from 'react';

import { useApi } from '@/contexts/ApiContext';
import { toast, useToastStore } from '@/stores/toastStore';
import { TauriEventNames, EventSchemas, listenValidated } from '@/types/events';

import type { UnlistenFn } from '@tauri-apps/api/event';

const MAX_INDIVIDUAL_CONFLICT_TOASTS = 3;

function formatIndividualConflict(conflict: EventSchemas.Vault.NoteConflict): {
  title: string;
  message: string;
} {
  if (conflict.kind === 'keptCopy') {
    return {
      title: `Kept both versions of "${conflict.noteTitle}"`,
      message: `It was changed in Lattice and in its file at the same time. The note now has the file's version. Lattice's version is in "${conflict.copyTitle}".`,
    };
  }
  return {
    title: `Kept the file that replaced "${conflict.noteTitle}"`,
    message: `Its file was replaced by one that isn't this note. That file is kept at ${conflict.setAsidePath}, and the note was saved back to its own file.`,
  };
}

function formatAggregatedConflicts(conflicts: EventSchemas.Vault.NoteConflict[]): {
  title: string;
  message: string;
} {
  const count = conflicts.length;
  const title = `Kept both versions for ${count} more note${count === 1 ? '' : 's'}`;
  const lines = conflicts.map((conflict) =>
    conflict.kind === 'keptCopy'
      ? `"${conflict.noteTitle}" -> "${conflict.copyTitle}"`
      : `"${conflict.noteTitle}" (replaced file at ${conflict.setAsidePath})`,
  );
  return {
    title,
    message: lines.join('\n'),
  };
}

export function useVaultConflictListener(): void {
  const api = useApi();

  useEffect(() => {
    const isMounted = { current: true };
    let unlisten: UnlistenFn | undefined;
    const seenIds = new Set<string>();
    const activeIndividualIds = new Map<string, string>();
    let summaryToastId: string | null = null;
    let summaryConflicts: EventSchemas.Vault.NoteConflict[] = [];

    const acknowledge = (ids: string[]) => {
      if (ids.length === 0) return;
      void api.dismissVaultConflicts(ids).catch((err) => {
        console.warn('[useVaultConflictListener] failed to dismiss conflicts:', err);
      });
    };

    const unsubscribeStore = useToastStore.subscribe((state) => {
      const present = new Set(state.toasts.map((t) => t.id));
      const dismissedIds: string[] = [];

      for (const [toastId, conflictId] of activeIndividualIds.entries()) {
        if (!present.has(toastId)) {
          activeIndividualIds.delete(toastId);
          dismissedIds.push(conflictId);
        }
      }

      if (summaryToastId !== null && !present.has(summaryToastId)) {
        summaryToastId = null;
        for (const conflict of summaryConflicts) {
          dismissedIds.push(conflict.id);
        }
        summaryConflicts = [];
      }

      acknowledge(dismissedIds);
    });

    const recordConflict = (conflict: EventSchemas.Vault.NoteConflict) => {
      if (seenIds.has(conflict.id)) {
        return;
      }
      seenIds.add(conflict.id);

      if (
        activeIndividualIds.size < MAX_INDIVIDUAL_CONFLICT_TOASTS &&
        summaryToastId === null
      ) {
        const { title, message } = formatIndividualConflict(conflict);
        const toastId = toast.warning(title, {
          message,
          duration: 0,
        });
        activeIndividualIds.set(toastId, conflict.id);
        return;
      }

      summaryConflicts.push(conflict);
      const { title, message } = formatAggregatedConflicts(summaryConflicts);
      if (summaryToastId === null) {
        summaryToastId = toast.warning(title, {
          message,
          duration: 0,
        });
      } else {
        useToastStore.getState().updateToast(summaryToastId, { title, message });
      }
    };

    void (async () => {
      try {
        const handle = await listenValidated(
          TauriEventNames.Vault.NoteConflict,
          EventSchemas.Vault.NoteConflict,
          (event) => {
            recordConflict(event.payload);
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
        return;
      }

      try {
        const result = await api.listVaultConflicts();
        if (!isMounted.current) return;
        if (result.ok) {
          result.data.forEach((conflict) => {
            const parsed = EventSchemas.Vault.NoteConflict.safeParse(conflict);
            if (parsed.success) {
              recordConflict(parsed.data);
            }
          });
        } else {
          console.warn(
            '[useVaultConflictListener] failed to load pending conflicts:',
            result.error,
          );
        }
      } catch (err) {
        console.warn('[useVaultConflictListener] failed to load pending conflicts:', err);
      }
    })();

    return () => {
      isMounted.current = false;
      unsubscribeStore();
      unlisten?.();
    };
  }, [api]);
}
