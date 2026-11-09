import { useEffect, useState } from 'react';

import { SettingsRow } from '@/components/ui';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { toast } from '@/stores/toastStore';
import type { DownloadedModel } from '@/types/downloadedModels';

import { NUMBER_FIELD_CLASS, clamp } from '../AITab/shared';

// The floor still holds a turn; the ceiling is the largest window any model on
// this computer is trained for, so a typo cannot ask for what none supports.
const MIN_WINDOW = 512;
const CEILING_WHEN_UNREPORTED = 262_144;

/** The window every model on this computer loads with, unless it sets its own. */
export function LocalContextWindowRow({ models }: { models: DownloadedModel[] }) {
  const ceiling = Math.max(0, ...models.map((model) => model.trained_context_length ?? 0))
    || CEILING_WHEN_UNREPORTED;
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  // What the user is typing, from when they untick "Fit each model to this
  // machine" until they leave the field. It starts empty: leaving it empty
  // saves no default, so each model loads at the largest window that fits
  // this machine, up to its trained maximum.
  const [entry, setEntry] = useState<string | null>(null);
  const saved = settings?.llm?.localContextWindow ?? null;
  const [shown, setShown] = useState<number | null>(saved);
  useEffect(() => { setShown(saved); }, [saved]);
  const usesDefault = shown === null && entry === null;

  const save = async (localContextWindow: number | null) => {
    setShown(localContextWindow);
    try {
      await updateSettings({ category: 'llm', updates: { localContextWindow } });
    } catch (error) {
      setShown(saved);
      toast.error('Failed to save the context window', {
        message: error instanceof Error ? error.message : String(error),
      });
    }
  };

  const commit = () => {
    if (entry === null) return;
    setEntry(null);
    const typed = entry.trim();
    const value = typed === '' ? null : Number(typed);
    if (value !== null && !Number.isFinite(value)) return;
    const next = value === null ? null : Math.round(clamp(value, MIN_WINDOW, ceiling));
    if (next !== shown) void save(next);
  };

  return (
    <SettingsRow
      label="Default context window"
      hint="Unless you set a window here or in the model's settings, each model loads with the largest window that fits this machine's memory now, up to its maximum, the window it was trained for. A window you set that is too large for this machine fails to load and says how much it needed, and one larger than the model was trained for is stretched by rope scaling, which costs quality."
      htmlFor="local-context-window"
    >
      <div className="flex items-center gap-3">
        <label className="flex items-center gap-1.5 whitespace-nowrap text-sm">
          <input
            type="checkbox"
            checked={usesDefault}
            onChange={(event) => {
              if (!event.target.checked) {
                setEntry('');
                return;
              }
              setEntry(null);
              if (shown !== null) void save(null);
            }}
          />
          Fit each model to this machine
        </label>
        <input
          id="local-context-window"
          type="number"
          min={MIN_WINDOW}
          max={ceiling}
          step={1}
          disabled={usesDefault}
          placeholder={usesDefault ? undefined : 'Tokens'}
          value={entry ?? shown ?? ''}
          onChange={(event) => setEntry(event.target.value)}
          onBlur={commit}
          className={NUMBER_FIELD_CLASS}
        />
      </div>
    </SettingsRow>
  );
}
