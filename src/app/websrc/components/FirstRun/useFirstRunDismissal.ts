/**
 * First-run dismissal state.
 *
 * "Has the user already been offered the model bundle?" is state the app's
 * startup path acts on, so it belongs to the settings repository, not to
 * `localStorage` (CLAUDE.md Repository Barrier rule 3). Reads go through
 * `useSettingsQuery`, the write through `useUpdateSettingsMutation`.
 *
 * Installs that predate this carry the old `lattice:first-run-skipped` key.
 * The first load that sees it writes the setting once and deletes the key, so
 * nobody who already dismissed the modal is asked again.
 */

import { useEffect, useRef, useState } from 'react';

import { useSettingsQuery, useUpdateSettingsMutation } from '../../hooks/queries/useSettingsQuery';

export const LEGACY_FIRST_RUN_SKIPPED_KEY = 'lattice:first-run-skipped';

function readLegacyFlag(): boolean {
  try {
    return localStorage.getItem(LEGACY_FIRST_RUN_SKIPPED_KEY) !== null;
  } catch {
    // Private mode, or a frozen store. No flag is the same as no flag.
    return false;
  }
}

function forgetLegacyFlag(): void {
  try {
    localStorage.removeItem(LEGACY_FIRST_RUN_SKIPPED_KEY);
  } catch {
    // Nothing to do: the setting is already the source of truth.
  }
}

export interface FirstRunDismissal {
  /** `undefined` until the settings document has been read. */
  dismissed: boolean | undefined;
  /** True while the first read is in flight. */
  isPending: boolean;
  /** Records the dismissal against the repository. */
  dismiss: () => void;
}

export function useFirstRunDismissal(): FirstRunDismissal {
  const { data, isPending } = useSettingsQuery();
  const { mutate } = useUpdateSettingsMutation();

  // Snapshotted once: the key is deleted below, and re-reading it every render
  // would flip the answer mid-migration.
  const [legacyFlag, setLegacyFlag] = useState(readLegacyFlag);
  const migrated = useRef(false);

  const stored = data?.onboarding?.firstRunDismissed;

  useEffect(() => {
    if (!legacyFlag || stored === undefined || migrated.current) return;
    migrated.current = true;

    if (stored) {
      // Already recorded; the key is just left over.
      forgetLegacyFlag();
      setLegacyFlag(false);
      return;
    }

    mutate(
      { category: 'onboarding', updates: { firstRunDismissed: true } },
      {
        onSuccess: () => {
          forgetLegacyFlag();
          setLegacyFlag(false);
        },
        onError: () => {
          // Keep the key so the next launch tries the migration again rather
          // than re-opening the modal for someone who already dismissed it.
          migrated.current = false;
        },
      }
    );
  }, [legacyFlag, stored, mutate]);

  const dismiss = () => {
    mutate({ category: 'onboarding', updates: { firstRunDismissed: true } });
  };

  return {
    // The un-migrated legacy key still counts as dismissed, so the modal can't
    // reappear in the window between reading it and persisting the setting.
    dismissed: stored === undefined ? undefined : stored || legacyFlag,
    isPending,
    dismiss,
  };
}
