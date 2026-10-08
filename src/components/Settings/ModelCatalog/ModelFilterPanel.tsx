import { useId, useState, type ReactNode } from 'react';

import { ChevronDown, SlidersHorizontal } from 'lucide-react';

import { CATALOG_TEXT_BUTTON_CLASS, hasActiveFilters } from './catalogUtils';
import { useModelCatalogStore } from '../../../stores/modelCatalogStore';
import { SidebarTabs, settingsFieldClass } from '../../ui';
import { SECONDARY_BUTTON_CLASS } from '../settingsStyles';

import type { ModelCategory, ModelSortBy } from '../../../types/modelCatalog';

type CategoryTab = ModelCategory | 'all';
const CATEGORY_TABS: ReadonlyArray<{ id: CategoryTab; label: string }> = [
  { id: 'all', label: 'All' }, { id: 'LLM', label: 'Chat' },
  { id: 'Embedding', label: 'Embedding' }, { id: 'OCR', label: 'OCR' },
  { id: 'Transcription', label: 'Transcription' },
];

function Field({ label, children }: { label: string; children: ReactNode }) {
  return <label className="min-w-0 space-y-1 text-xs text-text-secondary"><span className="block">{label}</span>{children}</label>;
}

export function ModelFilterPanel({ quantizations = [] }: { quantizations?: string[] }) {
  const { filters, sortBy, setFilters, setSortBy, quantizationFilter, setQuantizationFilter, fitFilter, setFitFilter, resetFilters } = useModelCatalogStore();
  const isFiltered = hasActiveFilters(filters) || Boolean(quantizationFilter) || fitFilter !== 'all';
  const quantizationOptions = [...new Set([...quantizations, quantizationFilter].filter(Boolean))].sort();
  const advancedCount = [filters.max_size_gb, filters.min_downloads, quantizationFilter, filters.required_capabilities.length, filters.embedding_dimensions].filter(Boolean).length;
  const [expanded, setExpanded] = useState(advancedCount > 0);
  const filtersId = useId();
  return <div className="space-y-4">
    <div className="flex flex-wrap items-center justify-between gap-2">
      <SidebarTabs<CategoryTab> value={filters.category ?? 'all'} onChange={id => {
        setFilters({ category: id === 'all' ? null : id, embedding_dimensions: id === 'Embedding' ? filters.embedding_dimensions : null });
        setQuantizationFilter('');
      }} options={CATEGORY_TABS} className="catalog-category-tabs max-w-full flex-wrap" />
      {isFiltered ? <button type="button" onClick={resetFilters} className={CATALOG_TEXT_BUTTON_CLASS}>Reset filters</button> : null}
    </div>
    <div className="flex flex-wrap items-end gap-3">
      <div className="min-w-[140px] flex-1">
      <Field label="Sort models"><select value={sortBy} onChange={event => setSortBy(event.target.value as ModelSortBy)} className={settingsFieldClass}>
        <option value="popularity">Most downloaded</option><option value="recommended">Recommended</option>
        <option value="size_asc">Smallest listed file</option><option value="size_desc">Largest listed file</option>
        <option value="likes">Most liked</option><option value="name">Name A–Z</option>
      </select></Field>
      </div>
      <div className="min-w-[140px] flex-1">
      <Field label="Estimated fit"><select value={fitFilter} onChange={event => setFitFilter(event.target.value as typeof fitFilter)} className={settingsFieldClass}>
        <option value="all">Any computer fit</option><option value="fits">Fits comfortably</option><option value="fits-or-tight">Fits or tight</option>
      </select></Field>
      </div>
      <button type="button" onClick={() => setExpanded(value => !value)} aria-expanded={expanded} aria-controls={filtersId} className={`${SECONDARY_BUTTON_CLASS} gap-2`}>
        <SlidersHorizontal className="h-3.5 w-3.5" aria-hidden="true" />
        Filters{advancedCount > 0 ? ` · ${advancedCount}` : ''}
        <ChevronDown className={`h-3.5 w-3.5 ${expanded ? 'rotate-180' : ''}`} aria-hidden="true" />
      </button>
    </div>
    {expanded ? <div id={filtersId} className="space-y-3 border-t border-border-subtle pt-4">
    <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,10rem),1fr))] gap-3">
      <Field label="Maximum size"><select value={filters.max_size_gb ?? ''} onChange={event => setFilters({ max_size_gb: event.target.value ? Number(event.target.value) : null })} className={settingsFieldClass}>
        <option value="">Any download size</option>{[2, 4, 8, 16, 32, 64].map(size => <option key={size} value={size}>Up to {size} GB</option>)}
      </select></Field>
      {filters.category === null || filters.category === 'LLM' ? <Field label="Listed quantization"><select value={quantizationFilter} onChange={event => setQuantizationFilter(event.target.value)} className={settingsFieldClass}>
        <option value="">All precisions</option>{quantizationOptions.map(value => <option key={value}>{value}</option>)}
      </select></Field> : null}
      <Field label="Minimum downloads"><select value={filters.min_downloads ?? ''} onChange={event => setFilters({ min_downloads: event.target.value ? Number(event.target.value) : null })} className={settingsFieldClass}>
        <option value="">Any popularity</option>{[1000, 10000, 100000, 1000000].map(count => <option key={count} value={count}>{new Intl.NumberFormat(undefined, { notation: 'compact' }).format(count)}+</option>)}
      </select></Field>
      {filters.category === 'Embedding' ? <Field label="Embedding dimensions"><select value={filters.embedding_dimensions ?? ''} onChange={event => setFilters({ embedding_dimensions: event.target.value ? Number(event.target.value) : null })} className={settingsFieldClass}>
        <option value="">Any dimensions</option>{[384, 768, 1024].map(count => <option key={count}>{count}</option>)}
      </select></Field> : <Field label="Capability"><select value={filters.required_capabilities[0] ?? ''} onChange={event => setFilters({ required_capabilities: event.target.value ? [event.target.value] : [] })} className={settingsFieldClass}>
        <option value="">Any capability</option><option value="code">Code</option><option value="reasoning">Reasoning</option><option value="multilingual">Multilingual</option>
      </select></Field>}
    </div>
    <p className="max-w-[75ch] text-xs leading-relaxed text-text-muted">Filters apply to the listed version. Open Versions to compare files. Unknown sizes remain visible unless you filter by fit.</p>
    </div> : null}
  </div>;
}
