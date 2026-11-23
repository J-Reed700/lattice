/**
 * Surfaces `vault:note-conflict` events (plus any conflicts recorded before
 * the frontend mounted) as persistent toasts: a note changed in Lattice and
 * outside it at once, and both versions were kept.
 * Mount once in App.tsx.
 */

import { useEffect } from 'react';

import { useApi } from '@/contexts/ApiContext';

export function useVaultConflictListener(): void {
  const api = useApi();

  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | undefined;

    void import('./vaultConflictSubscription')
      .then(({ startVaultConflictSubscription }) => {
        if (cancelled) return;
        cleanup = startVaultConflictSubscription(api);
      })
      .catch((err) => {
        console.error('[useVaultConflictListener] failed to load listener:', err);
      });

    return () => {
      cancelled = true;
      cleanup?.();
    };
  }, [api]);
}
