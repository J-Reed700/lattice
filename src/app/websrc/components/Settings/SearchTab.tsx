import { useEffect, useState } from 'react';

import { ChevronDown, ChevronRight, Search } from 'lucide-react';

import { VaultAPI } from '../../lib/api';
import { toast } from '../../stores/toastStore';

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

const CANDIDATE_FIELDS: NumericFieldConfig[] = [
  { key: 'kbSearchMinLimit', label: 'KB Search Min Limit', min: 1, max: 256, step: 1 },
  { key: 'kbSearchMaxLimit', label: 'KB Search Max Limit', min: 1, max: 512, step: 1 },
  {
    key: 'docShortlistCandidateMin',
    label: 'Doc Shortlist Candidate Min',
    min: 1,
    max: 512,
    step: 1,
  },
  {
    key: 'docShortlistCandidateMax',
    label: 'Doc Shortlist Candidate Max',
    min: 1,
    max: 512,
    step: 1,
  },
  { key: 'docShortlistDocMin', label: 'Doc Shortlist Doc Min', min: 1, max: 128, step: 1 },
  { key: 'docShortlistDocMax', label: 'Doc Shortlist Doc Max', min: 1, max: 128, step: 1 },
  {
    key: 'shortlistGateMinCandidates',
    label: 'Shortlist Gate Min Candidates',
    min: 1,
    max: 128,
    step: 1,
  },
  { key: 'shortlistGateMinDocs', label: 'Shortlist Gate Min Docs', min: 1, max: 128, step: 1 },
];

const EXTERNAL_SOURCE_FIELDS: NumericFieldConfig[] = [
  { key: 'wikiSearchMaxResults', label: 'Wiki Search Max Results', min: 1, max: 50, step: 1 },
  { key: 'wikiSnippetMaxChars', label: 'Wiki Snippet Max Chars', min: 64, max: 4000, step: 1 },
  { key: 'wikiContextLimit', label: 'Wiki Context Limit', min: 1, max: 20, step: 1 },
  { key: 'webSearchMaxResults', label: 'Web Search Max Results', min: 1, max: 50, step: 1 },
  { key: 'webSnippetMaxChars', label: 'Web Snippet Max Chars', min: 64, max: 4000, step: 1 },
  {
    key: 'externalSearchMaxWikiTerms',
    label: 'External Max Wiki Terms',
    min: 1,
    max: 64,
    step: 1,
  },
  {
    key: 'externalSearchMaxWebTerms',
    label: 'External Max Web Terms',
    min: 1,
    max: 64,
    step: 1,
  },
  {
    key: 'externalSearchQueryMaxChars',
    label: 'External Search Query Max Chars',
    min: 32,
    max: 2000,
    step: 1,
  },
];

const DEEP_RESEARCH_FIELDS: NumericFieldConfig[] = [
  { key: 'deepResearchDepth', label: 'Deep Research Depth', min: 1, max: 4, step: 1 },
  {
    key: 'deepResearchBranchQueries',
    label: 'Deep Research Branch Queries',
    min: 1,
    max: 4,
    step: 1,
  },
];

const RERANK_FIELDS: NumericFieldConfig[] = [
  { key: 'rerankMaxCandidates', label: 'Rerank Max Candidates', min: 1, max: 256, step: 1 },
  { key: 'rerankQueryMaxChars', label: 'Rerank Query Max Chars', min: 32, max: 8000, step: 1 },
  {
    key: 'overlapMinHitsForMultiTerm',
    label: 'Overlap Min Hits (Multi-term)',
    min: 1,
    max: 8,
    step: 1,
  },
];

const DOC_SUPPORT_FIELDS: NumericFieldConfig[] = [
  { key: 'docSupportMultiHitRatioFactor', label: 'Doc Support Multi-hit Factor', min: 0, max: 1, step: 0.01 },
  {
    key: 'docSupportSingleHitRatioFactor',
    label: 'Doc Support Single-hit Factor',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportMultiHitRatioMin',
    label: 'Doc Support Multi-hit Min',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportMultiHitRatioMax',
    label: 'Doc Support Multi-hit Max',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportSingleHitRatioMin',
    label: 'Doc Support Single-hit Min',
    min: 0,
    max: 1,
    step: 0.01,
  },
  {
    key: 'docSupportSingleHitRatioMax',
    label: 'Doc Support Single-hit Max',
    min: 0,
    max: 1,
    step: 0.01,
  },
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
  const [searchSettings, setSearchSettings] = useState<ApiSearchSettings | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [showAdvancedTuning, setShowAdvancedTuning] = useState(false);

  useEffect(() => {
    let isActive = true;

    const loadSettings = async () => {
      const result = await VaultAPI.getSettings();
      if (!isActive) {
        return;
      }

      if (result.ok) {
        setSearchSettings(result.data.search);
      } else {
        toast.error('Failed to load search settings', {
          message: result.error,
        });
      }

      setIsLoading(false);
    };

    void loadSettings();

    return () => {
      isActive = false;
    };
  }, []);

  const saveSearchUpdates = async (updates: Partial<ApiSearchSettings>): Promise<boolean> => {
    if (!searchSettings) {
      return false;
    }

    const previous = searchSettings;
    const next = { ...searchSettings, ...updates };
    setSearchSettings(next);

    const result = await VaultAPI.updateSettings({
      category: 'search',
      updates,
    });

    if (!result.ok) {
      setSearchSettings(previous);
      toast.error('Failed to update search settings', {
        message: result.error,
      });
      return false;
    }

    return true;
  };

  const saveTuningField = async <K extends keyof ApiRetrievalTuningSettings>(
    key: K,
    value: ApiRetrievalTuningSettings[K]
  ) => {
    if (!searchSettings) {
      return;
    }

    const normalizedTuning = normalizeTuningPairBounds(key, {
      ...searchSettings.retrievalTuning,
      [key]: value,
    });

    setSearchSettings((previous) => {
      if (!previous) {
        return previous;
      }
      return {
        ...previous,
        retrievalTuning: normalizedTuning,
      };
    });

    await saveSearchUpdates({
      retrievalTuning: normalizedTuning,
    });
  };

  const updateDraftTuningField = <K extends keyof ApiRetrievalTuningSettings>(
    key: K,
    value: ApiRetrievalTuningSettings[K]
  ) => {
    setSearchSettings((previous) => {
      if (!previous) {
        return previous;
      }
      return {
        ...previous,
        retrievalTuning: {
          ...previous.retrievalTuning,
          [key]: value,
        },
      };
    });
  };

  const renderTuningField = (field: NumericFieldConfig) => {
    if (!searchSettings) {
      return null;
    }

    const value = searchSettings.retrievalTuning[field.key];
    const inputId = `search-tuning-${String(field.key)}`;

    return (
      <div key={field.key} className="space-y-1.5">
        <label htmlFor={inputId} className="text-xs font-medium text-[var(--text-secondary)]">
          {field.label}
        </label>
        <input
          id={inputId}
          type="number"
          min={field.min}
          max={field.max}
          step={field.step}
          value={value}
          onChange={(event) => {
            const next = toFinite(event.target.value, value);
            updateDraftTuningField(field.key, next);
          }}
          onBlur={(event) => {
            const currentValue = toFinite(event.target.value, value);
            const normalized = normalizeFieldValue(field, currentValue);
            updateDraftTuningField(field.key, normalized);
            void saveTuningField(field.key, normalized);
          }}
          className="w-full rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-[var(--accent-primary)] focus:outline-none"
        />
      </div>
    );
  };

  if (isLoading) {
    return (
      <div className="space-y-4">
        <div className="h-7 w-56 animate-pulse rounded bg-[var(--surface-elevated)]" />
        <div className="h-24 animate-pulse rounded bg-[var(--surface-elevated)]" />
      </div>
    );
  }

  if (!searchSettings) {
    return (
      <div className="rounded-lg border border-[var(--border-color)] bg-[var(--surface-elevated)] p-4 text-sm text-[var(--text-secondary)]">
        Search settings are unavailable right now.
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3 border-b border-[var(--border-color)] pb-4">
        <div className="rounded-lg bg-[var(--accent-light)] p-2">
          <Search className="h-5 w-5 text-[var(--accent-primary)]" />
        </div>
        <div>
          <h2 className="text-xl font-semibold text-[var(--text-primary)]">Search Settings</h2>
          <p className="text-sm text-[var(--text-secondary)]">
            Configure retrieval behavior, reranking, and pipeline tuning
          </p>
        </div>
      </div>

      <div className="space-y-4 rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)] p-4">
        <div className="text-sm font-semibold text-[var(--text-primary)]">Core Retrieval</div>
        <label className="inline-flex items-center gap-2 text-sm text-[var(--text-secondary)]">
          <input
            type="checkbox"
            checked={searchSettings.enableReranking}
            onChange={(event) => void saveSearchUpdates({ enableReranking: event.target.checked })}
            className="h-4 w-4 rounded border-[var(--border-color)] bg-[var(--bg-secondary)] text-[var(--accent-primary)] focus:ring-2 focus:ring-[var(--accent-primary)]"
          />
          Enable reranking
        </label>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-[var(--text-secondary)]">Max Results</label>
            <input
              type="number"
              min={1}
              max={128}
              step={1}
              value={searchSettings.maxResults}
              onChange={(event) =>
                setSearchSettings((previous) =>
                  previous
                    ? { ...previous, maxResults: Math.round(toFinite(event.target.value, previous.maxResults)) }
                    : previous
                )
              }
              onBlur={() =>
                void saveSearchUpdates({ maxResults: Math.round(clamp(searchSettings.maxResults, 1, 128)) })
              }
              className="w-full rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-[var(--accent-primary)] focus:outline-none"
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-[var(--text-secondary)]">
              Similarity Threshold
            </label>
            <input
              type="number"
              min={0}
              max={1}
              step={0.01}
              value={searchSettings.similarityThreshold}
              onChange={(event) =>
                setSearchSettings((previous) =>
                  previous
                    ? {
                        ...previous,
                        similarityThreshold: toFinite(
                          event.target.value,
                          previous.similarityThreshold
                        ),
                      }
                    : previous
                )
              }
              onBlur={() =>
                void saveSearchUpdates({
                  similarityThreshold: clamp(searchSettings.similarityThreshold, 0, 1),
                })
              }
              className="w-full rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-[var(--accent-primary)] focus:outline-none"
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-[var(--text-secondary)]">
              Hybrid Search Alpha
            </label>
            <input
              type="number"
              min={0}
              max={1}
              step={0.01}
              value={searchSettings.hybridSearchAlpha}
              onChange={(event) =>
                setSearchSettings((previous) =>
                  previous
                    ? {
                        ...previous,
                        hybridSearchAlpha: toFinite(event.target.value, previous.hybridSearchAlpha),
                      }
                    : previous
                )
              }
              onBlur={() =>
                void saveSearchUpdates({
                  hybridSearchAlpha: clamp(searchSettings.hybridSearchAlpha, 0, 1),
                })
              }
              className="w-full rounded-md border border-[var(--border-color)] bg-[var(--surface-elevated)] px-3 py-2 text-sm text-[var(--text-primary)] focus:border-[var(--accent-primary)] focus:outline-none"
            />
          </div>
        </div>
      </div>

      <div className="rounded-lg border border-[var(--border-color)] bg-[var(--bg-secondary)]">
        <button
          type="button"
          onClick={() => setShowAdvancedTuning((previous) => !previous)}
          className="flex w-full items-center justify-between px-4 py-3 text-left"
        >
          <div>
            <div className="text-sm font-semibold text-[var(--text-primary)]">
              Advanced Retrieval Tuning
            </div>
            <div className="text-xs text-[var(--text-secondary)]">
              Pipeline knobs for shortlist, external search shaping, overlap, and support filters
            </div>
          </div>
          {showAdvancedTuning ? (
            <ChevronDown className="h-4 w-4 text-[var(--text-secondary)]" />
          ) : (
            <ChevronRight className="h-4 w-4 text-[var(--text-secondary)]" />
          )}
        </button>

        {showAdvancedTuning && (
          <div className="space-y-5 border-t border-[var(--border-color)] px-4 py-4">
            <div className="space-y-3">
              <div className="text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                Candidate + Shortlist
              </div>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">{CANDIDATE_FIELDS.map(renderTuningField)}</div>
            </div>

            <div className="space-y-3">
              <div className="text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                External Sources
              </div>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                {EXTERNAL_SOURCE_FIELDS.map(renderTuningField)}
              </div>
            </div>

            <div className="space-y-3">
              <div className="text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                Deep Research
              </div>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                {DEEP_RESEARCH_FIELDS.map(renderTuningField)}
              </div>
            </div>

            <div className="space-y-3">
              <div className="text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                Rerank + Overlap
              </div>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">{RERANK_FIELDS.map(renderTuningField)}</div>
            </div>

            <div className="space-y-3">
              <div className="text-xs font-semibold uppercase tracking-wide text-[var(--text-tertiary)]">
                Document Support Ratios
              </div>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                {DOC_SUPPORT_FIELDS.map(renderTuningField)}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
