import { useEffect, useState } from 'react';

import { SettingsRow } from '@/components/ui';
import { useSettingsQuery, useUpdateSettingsMutation } from '@/features/settings/hooks/useSettingsQuery';
import { toast } from '@/stores/toastStore';

import { NUMBER_FIELD_CLASS, clamp } from '../AITab/shared';

// The floor still holds a turn; the ceiling is the largest any current model
// is trained for, so a typo cannot ask for something no model supports.
const MIN_WINDOW = 512;
const MAX_WINDOW = 262_144;

/** The window every model on this computer loads with, unless it sets its own. */
export function LocalContextWindowRow() {
  const { data: settings } = useSettingsQuery();
  const { mutateAsync: updateSettings } = useUpdateSettingsMutation();
  // What the user is typing, from when they untick "Model's max" until they
  // leave the field. It starts empty: leaving it empty keeps every model at
  // its maximum.
  const [entry, setEntry] = useState<string | null>(null);
  const saved = settings?.llm?.localContextWindow ?? null;
  const [shown, setShown] = useState<number | null>(saved);
  useEffect(() => { setShown(saved); }, [saved]);
  const usesModelMax = shown === null && entry === null;

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
    const next = value === null ? null : Math.round(clamp(value, MIN_WINDOW, MAX_WINDOW));
    if (next !== shown) void save(next);
  };

  return (
    <SettingsRow
      label="Default context window"
      hint="Each model loads with its own maximum, the window it was trained for, unless you set one here or in the model's settings. A larger window needs more memory; one too large for this machine fails to load and says how much it needed, and one larger than the model was trained for is stretched by rope scaling, which costs quality."
      htmlFor="local-context-window"
    >
      <div className="flex items-center gap-3">
        <label className="flex items-center gap-1.5 whitespace-nowrap text-sm">
          <input
            type="checkbox"
            checked={usesModelMax}
            onChange={(event) => {
              if (!event.target.checked) {
                setEntry('');
                return;
              }
              setEntry(null);
              if (shown !== null) void save(null);
            }}
          />
          Model&apos;s max
        </label>
        <input
          id="local-context-window"
          type="number"
          min={MIN_WINDOW}
          max={MAX_WINDOW}
          step={1}
          disabled={usesModelMax}
          placeholder={usesModelMax ? undefined : 'Tokens'}
          value={entry ?? shown ?? ''}
          onChange={(event) => setEntry(event.target.value)}
          onBlur={commit}
          className={NUMBER_FIELD_CLASS}
        />
      </div>
    </SettingsRow>
  );
}
