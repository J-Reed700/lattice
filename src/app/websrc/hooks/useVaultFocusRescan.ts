/**
 * Belt-and-suspenders safety net for dropped fs-watcher events.
 * Triggers `rescan_vault` on window focus, throttled to 2s.
 * Mount once in App.tsx.
 */

import { useEffect } from 'react';

import { getCurrentWindow } from '@tauri-apps/api/window';

import { VaultAPI } from '../lib/api';
import { toast } from '../stores/toastStore';
import { useVaultImportStore } from '../stores/vaultImportStore';

const RESCAN_THROTTLE_MS = 2000;

export function useVaultFocusRescan(): void {
  useEffect(() => {
    let mounted = true;
    let unlisten: (() => void) | null = null;
    let lastRescanAt = 0;

    const triggerRescan = async () => {
      const now = Date.now();
      if (now - lastRescanAt < RESCAN_THROTTLE_MS) {
        return;
      }
      lastRescanAt = now;

      const result = await VaultAPI.rescanVault();
      if (!result.ok) {
        console.warn('[useVaultFocusRescan] rescan failed:', result.error);
        return;
      }
      const { imported, deleted } = result.data;
      if (imported > 0 || deleted > 0) {
        // Bump the same counter the per-event watcher uses; consumers
        // only care that it ticked.
        const bump = useVaultImportStore.getState().bump;
        bump('');

        const parts: string[] = [];
        if (imported > 0) {
          parts.push(`re-imported ${imported}`);
        }
        if (deleted > 0) {
          parts.push(`synced ${deleted} deletion${deleted === 1 ? '' : 's'}`);
        }
        const total = imported + deleted;
        toast.info(`Vault sync: ${parts.join(', ')} note${total === 1 ? '' : 's'}`);
      }
    };

    (async () => {
      try {
        const window = getCurrentWindow();
        const handle = await window.onFocusChanged(({ payload: focused }) => {
          if (!mounted || !focused) return;
          void triggerRescan();
        });
        if (!mounted) {
          handle();
          return;
        }
        unlisten = handle;
      } catch (err) {
        console.error('[useVaultFocusRescan] failed to subscribe to focus events:', err);
      }
    })();

    return () => {
      mounted = false;
      unlisten?.();
    };
  }, []);
}
