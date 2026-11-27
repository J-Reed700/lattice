import { useId, useState, type ReactNode } from 'react';

import { ChevronDown, SlidersHorizontal } from 'lucide-react';

import { Button, SidebarTabs } from '@/components/ui';
import { useModelCatalogStore } from '@/features/model/stores/modelCatalogStore';
import type { ModelCategory, ModelSortBy } from '@/types/modelCatalog';

import { CATALOG_TEXT_BUTTON_CLASS, hasActiveFilters } from './catalogUtils';
import { SettingsSelect } from '../SettingControls';

type CategoryTab = ModelCategory | 'all';
const CATEGORY_TABS: ReadonlyArray<{ id: CategoryTab; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'LLM', label: 'Chat' },
  { id: 'Embedding', label: 'Embedding' },
  { id: 'OCR', label: 'OCR' },
  { id: 'Transcription', label: 'Transcription' },
];

const SORT_OPTIONS = [
  { value: 'popularity', label: 'Most downloaded' },
  { value: 'recommended', label: 'Recommended' },
  { value: 'size_asc', label: 'Smallest listed file' },
  { value: 'size_desc', label: 'Largest listed file' },
  { value: 'likes', label: 'Most liked' },
  { value: 'name', label: 'Name A–Z' },
];

const FIT_OPTIONS = [
  { value: 'all', label: 'Any computer fit' },
  { value: 'fits', label: 'Fits comfortably' },
  { value: 'fits-or-tight', label: 'Fits or tight' },
];

const SIZE_OPTIONS = [
  { value: '', label: 'Any download size' },
  ...[2, 4, 8, 16, 32, 64].map((size) => ({ value: String(size), label: `Up to ${size} GB` })),
];

const DOWNLOAD_OPTIONS = [
  { value: '', label: 'Any popularity' },
  ...[1000, 10000, 100000, 1000000].map((count) => ({
    value: String(count),
    label: `${new Intl.NumberFormat(undefined, { notation: 'compact' }).format(count)}+`,
  })),
];

const DIMENSION_OPTIONS = [
  { value: '', label: 'Any dimensions' },
  ...[384, 768, 1024].map((count) => ({ value: String(count), label: String(count) })),
];

const CAPABILITY_OPTIONS = [
  { value: '', label: 'Any capability' },
  { value: 'code', label: 'Code' },
  { value: 'reasoning', label: 'Reasoning' },
  { value: 'multilingual', label: 'Multilingual' },
];

function Field({ id, label, children }: { id: string; label: string; children: ReactNode }) {
  return (
    <div className="min-w-0 space-y-1">
      <label htmlFor={id} className="block text-xs text-text-secondary">
        {label}
      </label>
      {children}
    </div>
  );
}

export function ModelFilterPanel({ quantizations = [] }: { quantizations?: string[] }) {
  const {
    filters,
    sortBy,
    setFilters,
    setSortBy,
    quantizationFilter,
    setQuantizationFilter,
    fitFilter,
    setFitFilter,
    resetFilters,
  } = useModelCatalogStore();
  const isFiltered =
    hasActiveFilters(filters) || Boolean(quantizationFilter) || fitFilter !== 'all';
  const quantizationOptions = [
    { value: '', label: 'All precisions' },
    ...[...new Set([...quantizations, quantizationFilter].filter(Boolean))]
      .sort()
      .map((value) => ({ value, label: value })),
  ];
  const advancedCount = [
    filters.max_size_gb,
    filters.min_downloads,
    quantizationFilter,
    filters.required_capabilities.length,
    filters.embedding_dimensions,
  ].filter(Boolean).length;
  const [expanded, setExpanded] = useState(advancedCount > 0);
  const baseId = useId();
  const filtersId = `${baseId}-filters`;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <SidebarTabs<CategoryTab>
          value={filters.category ?? 'all'}
          onChange={(id) => {
            setFilters({
              category: id === 'all' ? null : id,
              embedding_dimensions: id === 'Embedding' ? filters.embedding_dimensions : null,
            });
            setQuantizationFilter('');
          }}
          options={CATEGORY_TABS}
          className="catalog-category-tabs max-w-full flex-wrap"
        />
        {isFiltered ? (
          <button type="button" onClick={resetFilters} className={CATALOG_TEXT_BUTTON_CLASS}>
            Reset filters
          </button>
        ) : null}
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <div className="min-w-[140px] flex-1">
          <Field id={`${baseId}-sort`} label="Sort models">
            <SettingsSelect
              id={`${baseId}-sort`}
              value={sortBy}
              onValueChange={(next) => setSortBy(next as ModelSortBy)}
              options={SORT_OPTIONS}
            />
          </Field>
        </div>
        <div className="min-w-[140px] flex-1">
          <Field id={`${baseId}-fit`} label="Estimated fit">
            <SettingsSelect
              id={`${baseId}-fit`}
              value={fitFilter}
              onValueChange={(next) => setFitFilter(next as typeof fitFilter)}
              options={FIT_OPTIONS}
            />
          </Field>
        </div>
        <Button
          type="button"
          variant="outline"
          onClick={() => setExpanded((value) => !value)}
          aria-expanded={expanded}
          aria-controls={filtersId}
        >
          <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
          Filters{advancedCount > 0 ? ` · ${advancedCount}` : ''}
          <ChevronDown
            className={`h-3.5 w-3.5 transition-transform duration-fast ${expanded ? 'rotate-180' : ''}`}
            aria-hidden="true"
          />
        </Button>
      </div>
      {expanded ? (
        <div id={filtersId} className="space-y-3 border-t border-border-subtle pt-4">
          <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,10rem),1fr))] gap-3">
            <Field id={`${baseId}-size`} label="Maximum size">
              <SettingsSelect
                id={`${baseId}-size`}
                value={filters.max_size_gb ? String(filters.max_size_gb) : ''}
                onValueChange={(next) => setFilters({ max_size_gb: next ? Number(next) : null })}
                options={SIZE_OPTIONS}
              />
            </Field>
            {filters.category === null || filters.category === 'LLM' ? (
              <Field id={`${baseId}-quant`} label="Listed quantization">
                <SettingsSelect
                  id={`${baseId}-quant`}
                  value={quantizationFilter}
                  onValueChange={setQuantizationFilter}
                  options={quantizationOptions}
                />
              </Field>
            ) : null}
            <Field id={`${baseId}-downloads`} label="Minimum downloads">
              <SettingsSelect
                id={`${baseId}-downloads`}
                value={filters.min_downloads ? String(filters.min_downloads) : ''}
                onValueChange={(next) => setFilters({ min_downloads: next ? Number(next) : null })}
                options={DOWNLOAD_OPTIONS}
              />
            </Field>
            {filters.category === 'Embedding' ? (
              <Field id={`${baseId}-dims`} label="Embedding dimensions">
                <SettingsSelect
                  id={`${baseId}-dims`}
                  value={filters.embedding_dimensions ? String(filters.embedding_dimensions) : ''}
                  onValueChange={(next) =>
                    setFilters({ embedding_dimensions: next ? Number(next) : null })
                  }
                  options={DIMENSION_OPTIONS}
                />
              </Field>
            ) : (
              <Field id={`${baseId}-cap`} label="Capability">
                <SettingsSelect
                  id={`${baseId}-cap`}
                  value={filters.required_capabilities[0] ?? ''}
                  onValueChange={(next) =>
                    setFilters({ required_capabilities: next ? [next] : [] })
                  }
                  options={CAPABILITY_OPTIONS}
                />
              </Field>
            )}
          </div>
          <p className="max-w-[75ch] text-xs leading-relaxed text-text-muted">
            Filters apply to the listed version. Open Versions to compare files. Unknown sizes
            remain visible unless you filter by fit.
          </p>
        </div>
      ) : null}
    </div>
  );
}
