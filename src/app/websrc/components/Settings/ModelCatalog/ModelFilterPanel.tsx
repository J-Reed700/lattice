/**
 * ModelFilterPanel
 *
 * One row of quiet controls above the results: category tabs, then selects
 * for sort, size, popularity, and speed. No card, no pills, no heading.
 */

import { cn } from '@/lib/utils';

import { CATALOG_TEXT_BUTTON_CLASS, clearedFilters, hasActiveFilters } from './catalogUtils';
import { useModelCatalogStore } from '../../../stores/modelCatalogStore';
import { SidebarTabs, settingsFieldClass } from '../../ui';

import type { ModelCategory, ModelSortBy } from '../../../types/modelCatalog';

type CategoryTab = ModelCategory | 'all';

const CATEGORY_TABS: ReadonlyArray<{ id: CategoryTab; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'LLM', label: 'Chat' },
  { id: 'Embedding', label: 'Embedding' },
  { id: 'OCR', label: 'OCR' },
  { id: 'Transcription', label: 'Transcription' },
];

const SIZE_OPTIONS: ReadonlyArray<{ value: number | null; label: string }> = [
  { value: null, label: 'Any size' },
  { value: 2, label: '≤ 2 GB' },
  { value: 4, label: '≤ 4 GB' },
  { value: 8, label: '≤ 8 GB' },
  { value: 20, label: '≤ 20 GB' },
];

const DOWNLOAD_OPTIONS: ReadonlyArray<{ value: number | null; label: string }> = [
  { value: null, label: 'Any popularity' },
  { value: 1_000, label: '1K+' },
  { value: 10_000, label: '10K+' },
  { value: 100_000, label: '100K+' },
  { value: 1_000_000, label: '1M+' },
];

const DIMENSION_OPTIONS: ReadonlyArray<{ value: number | null; label: string }> = [
  { value: null, label: 'Any dimensions' },
  { value: 384, label: '384' },
  { value: 768, label: '768' },
  { value: 1024, label: '1024' },
];

const TIER_OPTIONS: ReadonlyArray<{ value: string | null; label: string }> = [
  { value: null, label: 'Any speed' },
  { value: 'fast', label: 'Fast' },
  { value: 'balanced', label: 'Balanced' },
  { value: 'accurate', label: 'Accurate' },
];

const SELECT_CLASS = cn(settingsFieldClass, 'w-auto');

/** Selects carry `null` for "no filter"; the DOM only speaks strings. */
const NONE = '';
const toOptionValue = (value: number | string | null) => (value == null ? NONE : String(value));

export function ModelFilterPanel() {
  const filters = useModelCatalogStore((state) => state.filters);
  const sortBy = useModelCatalogStore((state) => state.sortBy);
  const setFilters = useModelCatalogStore((state) => state.setFilters);
  const setSortBy = useModelCatalogStore((state) => state.setSortBy);

  const showDimensions = filters.category === 'Embedding';
  const activeTier = filters.required_capabilities[0] ?? null;
  const isFiltered = hasActiveFilters(filters);

  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
      <SidebarTabs<CategoryTab>
        value={filters.category ?? 'all'}
        onChange={(id) =>
          setFilters({
            category: id === 'all' ? null : id,
            // Dimensions only mean something for embedding models.
            embedding_dimensions: id === 'Embedding' ? filters.embedding_dimensions : null,
          })
        }
        options={CATEGORY_TABS}
        className="mr-1 pt-1"
      />

      <select
        aria-label="Sort models"
        value={sortBy}
        onChange={(event) => setSortBy(event.target.value as ModelSortBy)}
        className={SELECT_CLASS}
      >
        <option value="popularity">Most downloaded</option>
        <option value="recommended">Recommended</option>
        <option value="size_asc">Smallest</option>
      </select>

      <select
        aria-label="Maximum size"
        value={toOptionValue(filters.max_size_gb)}
        onChange={(event) =>
          setFilters({ max_size_gb: event.target.value === NONE ? null : Number(event.target.value) })
        }
        className={SELECT_CLASS}
      >
        {SIZE_OPTIONS.map((option) => (
          <option key={option.label} value={toOptionValue(option.value)}>
            {option.label}
          </option>
        ))}
      </select>

      <select
        aria-label="Minimum downloads"
        value={toOptionValue(filters.min_downloads)}
        onChange={(event) =>
          setFilters({
            min_downloads: event.target.value === NONE ? null : Number(event.target.value),
          })
        }
        className={SELECT_CLASS}
      >
        {DOWNLOAD_OPTIONS.map((option) => (
          <option key={option.label} value={toOptionValue(option.value)}>
            {option.label}
          </option>
        ))}
      </select>

      {showDimensions ? (
        <select
          aria-label="Embedding dimensions"
          value={toOptionValue(filters.embedding_dimensions)}
          onChange={(event) =>
            setFilters({
              embedding_dimensions: event.target.value === NONE ? null : Number(event.target.value),
            })
          }
          className={SELECT_CLASS}
        >
          {DIMENSION_OPTIONS.map((option) => (
            <option key={option.label} value={toOptionValue(option.value)}>
              {option.label}
            </option>
          ))}
        </select>
      ) : null}

      <select
        aria-label="Speed"
        value={toOptionValue(activeTier)}
        onChange={(event) =>
          setFilters({
            required_capabilities: event.target.value === NONE ? [] : [event.target.value],
          })
        }
        className={SELECT_CLASS}
      >
        {TIER_OPTIONS.map((option) => (
          <option key={option.label} value={toOptionValue(option.value)}>
            {option.label}
          </option>
        ))}
      </select>

      {isFiltered ? (
        <button
          type="button"
          onClick={() => setFilters(clearedFilters(filters))}
          className={CATALOG_TEXT_BUTTON_CLASS}
        >
          Reset filters
        </button>
      ) : null}
    </div>
  );
}
