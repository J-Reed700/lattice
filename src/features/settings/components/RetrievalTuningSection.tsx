import type { Dispatch, SetStateAction } from 'react';

import { ChevronDown, ChevronRight } from 'lucide-react';

import { SettingsRow, SettingsSection } from '@/components/ui';
import type {
  RetrievalTuningSettings as ApiRetrievalTuningSettings,
  SearchSettings as ApiSearchSettings,
} from '@/types/api/settings';

import {
  normalizeFieldValue,
  normalizeTuningPairBounds,
  TUNING_GROUPS,
  type NumericFieldConfig,
  type NumericTuningKey,
} from './retrievalTuningFields';
import { RangeStepperControl, SettingsNumberField } from './SettingControls';

/** The "Advanced retrieval tuning" disclosure and its grouped numeric knobs. */
export function RetrievalTuningSection({
  searchSettings,
  drafts,
  setDraft,
  clearDrafts,
  saveSearchUpdates,
  showAdvancedTuning,
  setShowAdvancedTuning,
}: {
  searchSettings: ApiSearchSettings;
  drafts: Record<string, number>;
  setDraft: (key: string, value: number) => void;
  clearDrafts: (keys: string[]) => void;
  saveSearchUpdates: (updates: Partial<ApiSearchSettings>, draftKeys?: string[]) => void;
  showAdvancedTuning: boolean;
  setShowAdvancedTuning: Dispatch<SetStateAction<boolean>>;
}) {
  const tuningValue = (key: NumericTuningKey, stored: number): number =>
    drafts[`tuning.${String(key)}`] ?? stored;

  const saveTuningField = (key: NumericTuningKey, value: number) => {
    if (!searchSettings) {
      return;
    }

    const normalizedTuning = normalizeTuningPairBounds(key, {
      ...searchSettings.retrievalTuning,
      [key]: value,
    });

    // A paired bound can move its partner, so both drafts go.
    const touched = Object.keys(normalizedTuning).filter(
      (candidate) =>
        normalizedTuning[candidate as keyof ApiRetrievalTuningSettings] !==
        searchSettings.retrievalTuning[candidate as keyof ApiRetrievalTuningSettings],
    );

    saveSearchUpdates(
      { retrievalTuning: normalizedTuning },
      [key, ...touched].map((candidate) => `tuning.${String(candidate)}`),
    );
  };

  const renderTuningField = (field: NumericFieldConfig) => {
    if (!searchSettings) {
      return null;
    }

    const stored = searchSettings.retrievalTuning[field.key];
    const value = tuningValue(field.key, stored);
    const inputId = `search-tuning-${String(field.key)}`;
    const integer = field.step >= 1;

    const handleCommit = (candidate: number) => {
      const normalized = normalizeFieldValue(field, candidate);
      if (normalized === stored) {
        clearDrafts([`tuning.${String(field.key)}`]);
        return;
      }
      saveTuningField(field.key, normalized);
    };

    return (
      <SettingsRow key={field.key} label={field.label} htmlFor={inputId}>
        {field.max !== undefined ? (
          <RangeStepperControl
            id={inputId}
            label={field.label}
            min={field.min}
            max={field.max}
            step={field.step}
            integer={integer}
            value={value}
            onDraftChange={(next) => setDraft(`tuning.${String(field.key)}`, next)}
            onCommit={handleCommit}
          />
        ) : (
          <SettingsNumberField
            id={inputId}
            min={field.min}
            step={field.step}
            integer={integer}
            value={value}
            onDraftChange={(next) => setDraft(`tuning.${String(field.key)}`, next)}
            onCommit={handleCommit}
          />
        )}
      </SettingsRow>
    );
  };

  return (
    <>
      <div className="border-t border-border-subtle">
        <button
          type="button"
          onClick={() => setShowAdvancedTuning((previous) => !previous)}
          aria-expanded={showAdvancedTuning}
          className="flex w-full items-center gap-2 border-b border-border-subtle py-3 text-left text-sm font-medium text-text-primary transition-colors duration-fast hover:text-accent focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
        >
          {showAdvancedTuning ? (
            <ChevronDown className="h-4 w-4 shrink-0 text-text-muted" aria-hidden="true" />
          ) : (
            <ChevronRight className="h-4 w-4 shrink-0 text-text-muted" aria-hidden="true" />
          )}
          Advanced retrieval tuning
        </button>
      </div>

      {showAdvancedTuning ? (
        <div className="mt-8">
          {TUNING_GROUPS.map((group) => (
            <SettingsSection key={group.title} title={group.title}>
              {group.fields.map(renderTuningField)}
            </SettingsSection>
          ))}
        </div>
      ) : null}
    </>
  );
}
