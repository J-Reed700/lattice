import { useState } from 'react';

import { PageHeader, SettingsRow, SettingsSection, settingsFieldClass, Switch } from '@/components/ui';
import { toast } from '@/stores/toastStore';
import type { SearchSettings as ApiSearchSettings } from '@/types/api/settings';

import { describeReranker, formatDownloadSize } from './rerankerStatus';
import { RetrievalTuningSection } from './RetrievalTuningSection';
import {
  clamp,
  COMPRESSION_MODES,
  EMBEDDING_STRATEGIES,
  MAX_COMPRESSION_DIMS,
  MIN_COMPRESSION_DIMS,
  QUANTIZATIONS,
  toFinite,
  type CompressionMode,
  type EmbeddingStrategy,
  type Quantization,
  type VectorCompression,
} from './searchSettingsOptions';
import { NUMBER_FIELD_CLASS, SWITCH_CLASS } from './settingsStyles';
import {
  useDownloadRerankerMutation,
  useRerankerStatusQuery,
} from '../hooks/useRerankerQuery';
import { useSettingsQuery, useUpdateSettingsMutation } from '../hooks/useSettingsQuery';

export { describeReranker, formatDownloadSize } from './rerankerStatus';

export function SearchTab() {
  const { data: settings, isPending, refetch } = useSettingsQuery();
  const { mutate: updateSettings } = useUpdateSettingsMutation();
  const { data: rerankerStatus } = useRerankerStatusQuery();
  const downloadReranker = useDownloadRerankerMutation();
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

  if (isPending) {
    return (
      <>
        <PageHeader title="Search" description="Shape how Lattice finds and ranks information in your library." />
        <p className="text-sm text-text-muted">Loading…</p>
      </>
    );
  }

  if (!searchSettings) {
    return (
      <>
        <PageHeader title="Search" description="Shape how Lattice finds and ranks information in your library." />
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

  const compression = searchSettings.vectorIndexCompression;
  const compressionDims = fieldValue('vectorCompressionDims', compression.dims);
  const saveCompression = (
    updates: Partial<VectorCompression>,
    draftKeys: string[] = []
  ) =>
    saveSearchUpdates(
      { vectorIndexCompression: { ...compression, ...updates } },
      draftKeys
    );

  const maxResults = fieldValue('maxResults', searchSettings.maxResults);
  const similarityThreshold = fieldValue(
    'similarityThreshold',
    searchSettings.similarityThreshold
  );
  const hybridSearchAlpha = fieldValue('hybridSearchAlpha', searchSettings.hybridSearchAlpha);

  return (
    <>
      <PageHeader title="Search" description="Shape how Lattice finds and ranks information in your library." />

      <SettingsSection title="Retrieval">
        <SettingsRow
          label="Rerank results"
          hint={describeReranker(rerankerStatus, downloadReranker.error)}
          htmlFor="search-rerank"
        >
          <div className="flex items-center gap-3">
            {rerankerStatus && !rerankerStatus.installed ? (
              <button
                type="button"
                onClick={() => downloadReranker.mutate()}
                disabled={downloadReranker.isPending}
                className="h-8 shrink-0 rounded-sm border border-border-default px-2.5 text-sm text-text-primary transition-colors duration-fast hover:border-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                {downloadReranker.isPending
                  ? 'Downloading…'
                  : `Download (${formatDownloadSize(rerankerStatus.downloadBytes)})`}
              </button>
            ) : null}
            <Switch
              id="search-rerank"
              checked={searchSettings.enableReranking}
              onCheckedChange={(checked) => saveSearchUpdates({ enableReranking: checked })}
              className={SWITCH_CLASS}
            />
          </div>
        </SettingsRow>

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
          <select
            id="search-vector-compression-mode"
            value={compression.mode}
            onChange={(event) =>
              saveCompression({ mode: event.target.value as CompressionMode })
            }
            className={settingsFieldClass}
          >
            {COMPRESSION_MODES.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </SettingsRow>

        {compression.mode === 'truncated' && (
          <>
            <SettingsRow label="Stored dimensions" htmlFor="search-vector-compression-dims">
              <input
                id="search-vector-compression-dims"
                type="number"
                min={MIN_COMPRESSION_DIMS}
                max={MAX_COMPRESSION_DIMS}
                step={1}
                value={compressionDims}
                onChange={(event) =>
                  setDraft(
                    'vectorCompressionDims',
                    toFinite(event.target.value, compressionDims)
                  )
                }
                onBlur={() =>
                  saveCompression(
                    {
                      dims: Math.round(
                        clamp(compressionDims, MIN_COMPRESSION_DIMS, MAX_COMPRESSION_DIMS)
                      ),
                    },
                    ['vectorCompressionDims']
                  )
                }
                className={NUMBER_FIELD_CLASS}
              />
            </SettingsRow>

            <SettingsRow label="Stored precision" htmlFor="search-vector-compression-quantization">
              <select
                id="search-vector-compression-quantization"
                value={compression.quantization}
                onChange={(event) =>
                  saveCompression({ quantization: event.target.value as Quantization })
                }
                className={settingsFieldClass}
              >
                {QUANTIZATIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
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

        <SettingsRow
          label="Embedding strategy"
          hint="Late chunking conditions each chunk on its whole section. Changing it re-embeds your library."
          htmlFor="search-embedding-strategy"
        >
          <select
            id="search-embedding-strategy"
            value={searchSettings.embeddingStrategy}
            onChange={(event) =>
              saveSearchUpdates({
                embeddingStrategy: event.target.value as EmbeddingStrategy,
              })
            }
            className={settingsFieldClass}
          >
            {EMBEDDING_STRATEGIES.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
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
