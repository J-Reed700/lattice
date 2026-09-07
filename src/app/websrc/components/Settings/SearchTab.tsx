import { useState } from 'react';

import { ChevronDown, ChevronRight } from 'lucide-react';

import { NUMBER_FIELD_CLASS, SWITCH_CLASS } from './settingsStyles';
import { useSettingsQuery, useUpdateSettingsMutation } from '../../hooks/queries/useSettingsQuery';
import { toast } from '../../stores/toastStore';
import { PageHeader, SettingsRow, SettingsSection, Switch } from '../ui';

import type {
  RetrievalTuningSettings as ApiRetrievalTuningSettings,
  SearchSettings as ApiSearchSettings,
} from '../../types/api/settings';

type NumericFieldConfig = {
  key: keyof ApiRetrievalTuningSettings;
  label: string;
  min: number;
  max: number;
  step: number;
};

type TuningGroup = {
  title: string;
  fields: NumericFieldConfig[];
};

const CANDIDATE_FIELDS: NumericFieldConfig[] = [
  { key: 'kbSearchMinLimit', label: 'KB search min limit', min: 1, max: 256, step: 1 },
  { key: 'kbSearchMaxLimit', label: 'KB search max limit', min: 1, max: 512, step: 1 },
  {
    key: 'docShortlistCandidateMin',
    label: 'Doc shortlist candidate min',
    min: 1,
    max: 512,
    step: 1,
  },
  {
    key: 'docShortlistCandidateMax',
    label: 'Doc shortlist candidate max',
    min: 1,
    max: 512,
    step: 1,
  },
  { key: 'docShortlistDocMin', label: 'Doc shortlist doc min', min: 1, max: 128, step: 1 },
  { key: 'docShortlistDocMax', label: 'Doc shortlist doc max', min: 1, max: 128, step: 1 },
  {
    key: 'shortlistGateMinCandidates',
    label: 'Shortlist gate min candidates',
    min: 1,
    max: 128,
    step: 1,
  },
  { key: 'shortlistGateMinDocs', label: 'Shortlist gate min docs', min: 1, max: 128, step: 1 },
];

const EXTERNAL_SOURCE_FIELDS: NumericFieldConfig[] = [
  { key: 'wikiSearchMaxResults', label: 'Wiki search max results', min: 1, max: 50, step: 1 },
  { key: 'wikiSnippetMaxChars', label: 'Wiki snippet max chars', min: 64, max: 4000, step: 1 },
  { key: 'wikiContextLimit', label: 'Wiki context limit', min: 1, max: 20, step: 1 },
  { key: 'webSearchMaxResults', label: 'Web search max results', min: 1, max: 50, step: 1 },
  { key: 'webSnippetMaxChars', label: 'Web snippet max chars', min: 64, max: 4000, step: 1 },
  {
    key: 'externalSearchMaxWikiTerms',
    label: 'External max wiki terms',
    min: 1,
    max: 64,
    step: 1,
  },
  {
    key: 'externalSearchMaxWebTerms',
    label: 'External max web terms',
    min: 1,
    max: 64,
    step: 1,
  },
  {
    key: 'externalSearchQueryMaxChars',
    label: 'External search query max chars',
    min: 32,
    max: 2000,
    step: 1,
  },
];

const DEEP_RESEARCH_FIELDS: NumericFieldConfig[] = [
  { key: 'deepResearchDepth', label: 'Deep research depth', min: 1, max: 4, step: 1 },
  {
    key: 'deepResearchBranchQueries',
    label: 'Deep research branch queries',
    min: 1,
    max: 4,
    step: 1,
  },
];

const RERANK_FIELDS: NumericFieldConfig[] = [
  { key: 'rerankMaxCandidates', label: 'Rerank max candidates', min: 1, max: 256, step: 1 },
  { key: 'rerankQueryMaxChars', label: 'Rerank query max chars', min: 32, max: 8000, step: 1 },
  {
    key: 'overlapMinHitsForMultiTerm',
    label: 'Overlap min hits (multi-term)',
    min: 1,
    max: 8,
    step: 1,
  },
];

const DOC_SUPPORT_FIELDS: NumericFieldConfig[] = [
  {
    key: 'docSupportMultiHitRatioFactor',
    label: 'Doc support multi-hit factor',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportSingleHitRatioFactor',
    label: 'Doc support single-hit factor',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportMultiHitRatioMin',
    label: 'Doc support multi-hit min',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportMultiHitRatioMax',
    label: 'Doc support multi-hit max',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportSingleHitRatioMin',
    label: 'Doc support single-hit min',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportSingleHitRatioMax',
    label: 'Doc support single-hit max',
    min: 0,
    max: 1,
    step: 0.01,
  },
];

const TUNING_GROUPS: TuningGroup[] = [
  { title: 'Candidates and shortlist', fields: CANDIDATE_FIELDS },
  { title: 'External sources', fields: EXTERNAL_SOURCE_FIELDS },
  { title: 'Deep research', fields: DEEP_RESEARCH_FIELDS },
  { title: 'Rerank and overlap', fields: RERANK_FIELDS },
  { title: 'Document support ratios', fields: DOC_SUPPORT_FIELDS },
];

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function toFinite(value: string, fallback: number): number {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function normalizeFieldValue(field: NumericFieldConfig, value: number): number {
  const clamped = clamp(value, field.min, field.max);
  if (field.step >= 1) {
    return Math.round(clamped);
  }
  return Number(clamped.toFixed(2));
}

function normalizeTuningPairBounds(
  key: keyof ApiRetrievalTuningSettings,
  tuning: ApiRetrievalTuningSettings
): ApiRetrievalTuningSettings {
  const next = { ...tuning };

  if (key === 'kbSearchMinLimit' && next.kbSearchMinLimit > next.kbSearchMaxLimit) {
    next.kbSearchMaxLimit = next.kbSearchMinLimit;
  }
  if (key === 'kbSearchMaxLimit' && next.kbSearchMaxLimit < next.kbSearchMinLimit) {
    next.kbSearchMinLimit = next.kbSearchMaxLimit;
  }

  if (
    key === 'docShortlistCandidateMin' &&
    next.docShortlistCandidateMin > next.docShortlistCandidateMax
  ) {
    next.docShortlistCandidateMax = next.docShortlistCandidateMin;
  }
  if (
    key === 'docShortlistCandidateMax' &&
    next.docShortlistCandidateMax < next.docShortlistCandidateMin
  ) {
    next.docShortlistCandidateMin = next.docShortlistCandidateMax;
  }

  if (key === 'docShortlistDocMin' && next.docShortlistDocMin > next.docShortlistDocMax) {
    next.docShortlistDocMax = next.docShortlistDocMin;
  }
  if (key === 'docShortlistDocMax' && next.docShortlistDocMax < next.docShortlistDocMin) {
    next.docShortlistDocMin = next.docShortlistDocMax;
  }

  if (
    key === 'docSupportMultiHitRatioMin' &&
    next.docSupportMultiHitRatioMin > next.docSupportMultiHitRatioMax
  ) {
    next.docSupportMultiHitRatioMax = next.docSupportMultiHitRatioMin;
  }
  if (
    key === 'docSupportMultiHitRatioMax' &&
    next.docSupportMultiHitRatioMax < next.docSupportMultiHitRatioMin
  ) {
    next.docSupportMultiHitRatioMin = next.docSupportMultiHitRatioMax;
  }

  if (
    key === 'docSupportSingleHitRatioMin' &&
    next.docSupportSingleHitRatioMin > next.docSupportSingleHitRatioMax
  ) {
    next.docSupportSingleHitRatioMax = next.docSupportSingleHitRatioMin;
  }
  if (
    key === 'docSupportSingleHitRatioMax' &&
    next.docSupportSingleHitRatioMax < next.docSupportSingleHitRatioMin
  ) {
    next.docSupportSingleHitRatioMin = next.docSupportSingleHitRatioMax;
  }

  return next;
}

export function SearchTab() {
  const { data: settings, isPending, refetch } = useSettingsQuery();
  const { mutate: updateSettings } = useUpdateSettingsMutation();
  const [showAdvancedTuning, setShowAdvancedTuning] = useState(false);
  // A half-typed number is UI state, so each field may hold a draft while it
  // has focus. Everything else reads the query — the repository is the source
  // of truth and the mutation invalidates it, so there is nothing to roll back.
  const [drafts, setDrafts] = useState<Record<string, number>>({});

  const searchSettings = settings?.search ?? null;

  const setDraft = (key: string, value: number) => {
    setDrafts((previous) => ({ ...previous, [key]: value }));
  };

  const clearDrafts = (keys: string[]) => {
    setDrafts((previous) => {
      if (!keys.some((key) => key in previous)) {
        return previous;
      }
      const next = { ...previous };
      for (const key of keys) {
        delete next[key];
      }
      return next;
    });
  };

  const saveSearchUpdates = (updates: Partial<ApiSearchSettings>, draftKeys: string[] = []) => {
    updateSettings(
      { category: 'search', updates },
      {
        onError: (error) => {
          toast.error("Couldn't save search settings", { message: error.message });
        },
        // Drop the draft either way: on success the query already holds the
        // saved value, on failure the field must snap back to what is stored.
        onSettled: () => clearDrafts(draftKeys),
      }
    );
  };

  const fieldValue = (key: string, stored: number): number => drafts[key] ?? stored;

  const tuningValue = (key: keyof ApiRetrievalTuningSettings, stored: number): number =>
    drafts[`tuning.${String(key)}`] ?? stored;

  const saveTuningField = (key: keyof ApiRetrievalTuningSettings, value: number) => {
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
        searchSettings.retrievalTuning[candidate as keyof ApiRetrievalTuningSettings]
    );

    saveSearchUpdates(
      { retrievalTuning: normalizedTuning },
      [key, ...touched].map((candidate) => `tuning.${String(candidate)}`)
    );
  };

  const renderTuningField = (field: NumericFieldConfig) => {
    if (!searchSettings) {
      return null;
    }

    const stored = searchSettings.retrievalTuning[field.key];
    const value = tuningValue(field.key, stored);
    const inputId = `search-tuning-${String(field.key)}`;

    return (
      <SettingsRow key={field.key} label={field.label} htmlFor={inputId}>
        <input
          id={inputId}
          type="number"
          min={field.min}
          max={field.max}
          step={field.step}
          value={value}
          onChange={(event) => {
            setDraft(`tuning.${String(field.key)}`, toFinite(event.target.value, value));
          }}
          onBlur={(event) => {
            const normalized = normalizeFieldValue(field, toFinite(event.target.value, value));
            if (normalized === stored) {
              clearDrafts([`tuning.${String(field.key)}`]);
              return;
            }
            saveTuningField(field.key, normalized);
          }}
          className={NUMBER_FIELD_CLASS}
        />
      </SettingsRow>
    );
  };

  if (isPending) {
    return (
      <>
        <PageHeader title="Search" />
        <p className="text-sm text-text-muted">Loading…</p>
      </>
    );
  }

  if (!searchSettings) {
    return (
      <>
        <PageHeader title="Search" />
        <div className="flex items-center gap-2">
          <p className="text-sm text-text-muted">Couldn&apos;t read search settings.</p>
          <button
            type="button"
            onClick={() => void refetch()}
            className="text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary"
          >
            Retry
          </button>
        </div>
      </>
    );
  }

  const maxResults = fieldValue('maxResults', searchSettings.maxResults);
  const similarityThreshold = fieldValue(
    'similarityThreshold',
    searchSettings.similarityThreshold
  );
  const hybridSearchAlpha = fieldValue('hybridSearchAlpha', searchSettings.hybridSearchAlpha);

  return (
    <>
      <PageHeader title="Search" />

      <SettingsSection title="Retrieval">
        <SettingsRow label="Rerank results" htmlFor="search-rerank">
          <Switch
            id="search-rerank"
            checked={searchSettings.enableReranking}
            onCheckedChange={(checked) => saveSearchUpdates({ enableReranking: checked })}
            className={SWITCH_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Maximum results" htmlFor="search-max-results">
          <input
            id="search-max-results"
            type="number"
            min={1}
            max={128}
            step={1}
            value={maxResults}
            onChange={(event) => setDraft('maxResults', toFinite(event.target.value, maxResults))}
            onBlur={() =>
              saveSearchUpdates({ maxResults: Math.round(clamp(maxResults, 1, 128)) }, [
                'maxResults',
              ])
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Similarity threshold" htmlFor="search-similarity">
          <input
            id="search-similarity"
            type="number"
            min={0}
            max={1}
            step={0.01}
            value={similarityThreshold}
            onChange={(event) =>
              setDraft('similarityThreshold', toFinite(event.target.value, similarityThreshold))
            }
            onBlur={() =>
              saveSearchUpdates({ similarityThreshold: clamp(similarityThreshold, 0, 1) }, [
                'similarityThreshold',
              ])
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>

        <SettingsRow
          label="Hybrid alpha"
          hint="0 is keyword only, 1 is vector only"
          htmlFor="search-hybrid-alpha"
        >
          <input
            id="search-hybrid-alpha"
            type="number"
            min={0}
            max={1}
            step={0.01}
            value={hybridSearchAlpha}
            onChange={(event) =>
              setDraft('hybridSearchAlpha', toFinite(event.target.value, hybridSearchAlpha))
            }
            onBlur={() =>
              saveSearchUpdates({ hybridSearchAlpha: clamp(hybridSearchAlpha, 0, 1) }, [
                'hybridSearchAlpha',
              ])
            }
            className={NUMBER_FIELD_CLASS}
          />
        </SettingsRow>
      </SettingsSection>

      <div className="border-t border-border-subtle">
        <button
          type="button"
          onClick={() => setShowAdvancedTuning((previous) => !previous)}
          aria-expanded={showAdvancedTuning}
          className="flex w-full items-center gap-2 border-b border-border-subtle py-3 text-left text-sm text-text-primary transition-colors duration-fast hover:text-text-primary"
        >
          {showAdvancedTuning ? (
            <ChevronDown className="h-4 w-4 shrink-0 text-text-muted" />
          ) : (
            <ChevronRight className="h-4 w-4 shrink-0 text-text-muted" />
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
