import { useState } from 'react';

import { Button, PageHeader, SettingsRow, SettingsSection, Switch } from '@/components/ui';
import { toast } from '@/stores/toastStore';
import type { SearchSettings as ApiSearchSettings } from '@/types/api/settings';

import { RerankerRow } from './RerankerRow';
import { RetrievalTuningSection } from './RetrievalTuningSection';
import {
  clamp,
  COMPRESSION_MODES,
  QUANTIZATIONS,
  type CompressionMode,
  type Quantization,
  type VectorCompression,
} from './searchSettingsOptions';
import {
  RangeStepperControl,
  SettingsNumberField,
  SettingsSelect,
} from './SettingControls';
import { SWITCH_CLASS } from './settingsStyles';
import { useSettingsQuery, useUpdateSettingsMutation } from '../hooks/useSettingsQuery';

export { describeReranker, formatDownloadSize } from './rerankerStatus';

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
      },
    );
  };

  const fieldValue = (key: string, stored: number): number => drafts[key] ?? stored;

  if (isPending) {
    return (
      <>
        <PageHeader
          title="Search"
          description="Shape how Lattice finds and ranks information in your library."
        />
        <p className="text-sm text-text-muted">Loading…</p>
      </>
    );
  }

  if (!searchSettings) {
    return (
      <>
        <PageHeader
          title="Search"
          description="Shape how Lattice finds and ranks information in your library."
        />
        <div className="flex items-center gap-3">
          <p className="text-sm text-text-muted">Couldn&apos;t read search settings.</p>
          <Button type="button" variant="outline" size="sm" onClick={() => void refetch()}>
            Retry
          </Button>
        </div>
      </>
    );
  }

  const compression = searchSettings.vectorIndexCompression;
  const compressionDims = fieldValue('vectorCompressionDims', compression.dims);
  const saveCompression = (
    updates: Partial<VectorCompression>,
    draftKeys: string[] = [],
  ) =>
    saveSearchUpdates(
      { vectorIndexCompression: { ...compression, ...updates } },
      draftKeys,
    );

  const maxResults = fieldValue('maxResults', searchSettings.maxResults);
  const similarityThreshold = fieldValue(
    'similarityThreshold',
    searchSettings.similarityThreshold,
  );
  const hybridSearchAlpha = fieldValue('hybridSearchAlpha', searchSettings.hybridSearchAlpha);

  return (
    <>
      <PageHeader
        title="Search"
        description="Shape how Lattice finds and ranks information in your library."
      />

      <SettingsSection title="Retrieval">
        <RerankerRow />

        <SettingsRow
          label="Read web pages"
          hint="Open web search results and read the pages; off, answers use the search snippets only"
          htmlFor="search-read-web-pages"
        >
          <Switch
            id="search-read-web-pages"
            checked={searchSettings.retrievalTuning.readWebPages}
            onCheckedChange={(checked) =>
              saveSearchUpdates({
                retrievalTuning: {
                  ...searchSettings.retrievalTuning,
                  readWebPages: checked,
                },
              })
            }
            className={SWITCH_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Corrective retry" htmlFor="search-corrective-retry">
          <Switch
            id="search-corrective-retry"
            checked={searchSettings.retrievalTuning.sufficiencyRetryEnabled}
            onCheckedChange={(checked) =>
              saveSearchUpdates({
                retrievalTuning: {
                  ...searchSettings.retrievalTuning,
                  sufficiencyRetryEnabled: checked,
                },
              })
            }
            className={SWITCH_CLASS}
          />
        </SettingsRow>

        <SettingsRow
          label="Vector storage"
          hint="Truncation needs a model trained for it (Qwen3 Embedding); it is ignored for models that are not"
          htmlFor="search-vector-compression-mode"
        >
          <SettingsSelect
            id="search-vector-compression-mode"
            value={compression.mode}
            onValueChange={(next) => saveCompression({ mode: next as CompressionMode })}
            options={COMPRESSION_MODES.map((option) => ({
              value: option.value,
              label: option.label,
            }))}
          />
        </SettingsRow>

        {compression.mode === 'truncated' && (
          <>
            <SettingsRow label="Stored dimensions" htmlFor="search-vector-compression-dims">
              <SettingsNumberField
                id="search-vector-compression-dims"
                min={1}
                step={1}
                integer
                value={compressionDims}
                onDraftChange={(next) => setDraft('vectorCompressionDims', next)}
                onCommit={(next) =>
                  saveCompression(
                    { dims: Math.max(1, Math.round(next)) },
                    ['vectorCompressionDims'],
                  )
                }
              />
            </SettingsRow>

            <SettingsRow
              label="Stored precision"
              htmlFor="search-vector-compression-quantization"
            >
              <SettingsSelect
                id="search-vector-compression-quantization"
                value={compression.quantization}
                onValueChange={(next) =>
                  saveCompression({ quantization: next as Quantization })
                }
                options={QUANTIZATIONS.map((option) => ({
                  value: option.value,
                  label: option.label,
                }))}
              />
            </SettingsRow>
          </>
        )}

        <SettingsRow
          label="Document summaries"
          hint="Summarize each document and section with the utility model so whole-document questions start from the right place. Requires a utility model and takes effect after restart."
          htmlFor="search-summary-index"
        >
          <Switch
            id="search-summary-index"
            checked={searchSettings.summaryIndexEnabled}
            onCheckedChange={(checked) => saveSearchUpdates({ summaryIndexEnabled: checked })}
            className={SWITCH_CLASS}
          />
        </SettingsRow>

        <SettingsRow label="Maximum results" htmlFor="search-max-results">
          <SettingsNumberField
            id="search-max-results"
            min={1}
            step={1}
            integer
            value={maxResults}
            onDraftChange={(next) => setDraft('maxResults', next)}
            onCommit={(next) =>
              saveSearchUpdates({ maxResults: Math.max(1, Math.round(next)) }, ['maxResults'])
            }
          />
        </SettingsRow>

        <SettingsRow label="Similarity threshold" htmlFor="search-similarity">
          <RangeStepperControl
            id="search-similarity"
            label="Similarity threshold"
            min={0}
            max={1}
            step={0.01}
            value={similarityThreshold}
            onDraftChange={(next) => setDraft('similarityThreshold', next)}
            onCommit={(next) =>
              saveSearchUpdates(
                { similarityThreshold: clamp(next, 0, 1) },
                ['similarityThreshold'],
              )
            }
          />
        </SettingsRow>

        <SettingsRow
          label="Hybrid alpha"
          hint="0 is keyword only, 1 is vector only"
          htmlFor="search-hybrid-alpha"
        >
          <RangeStepperControl
            id="search-hybrid-alpha"
            label="Hybrid alpha"
            min={0}
            max={1}
            step={0.01}
            minLabel="Keyword"
            maxLabel="Vector"
            value={hybridSearchAlpha}
            onDraftChange={(next) => setDraft('hybridSearchAlpha', next)}
            onCommit={(next) =>
              saveSearchUpdates(
                { hybridSearchAlpha: clamp(next, 0, 1) },
                ['hybridSearchAlpha'],
              )
            }
          />
        </SettingsRow>
      </SettingsSection>

      <RetrievalTuningSection
        searchSettings={searchSettings}
        drafts={drafts}
        setDraft={setDraft}
        clearDrafts={clearDrafts}
        saveSearchUpdates={saveSearchUpdates}
        showAdvancedTuning={showAdvancedTuning}
        setShowAdvancedTuning={setShowAdvancedTuning}
      />
    </>
  );
}
