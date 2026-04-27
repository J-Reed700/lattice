import { useEffect, useMemo, useState } from 'react';

import { Check, Loader2, MoreHorizontal, RefreshCw, Search, X } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

import { CorpusDisplayMenu } from './CorpusDisplayMenu';
import { type DensityMode, type GroupByMode } from './hooks/useCorpusBrowser';
import { type CustomCollection, type SortField, type SortOrder, type SourceFilter } from '../../types/fileBrowser';

type TypeChipValue = 'all' | 'pdf' | 'web' | 'notes' | 'code' | 'media' | 'other';

const TYPE_CHIPS: { value: TypeChipValue; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'pdf', label: 'PDFs' },
  { value: 'web', label: 'Web' },
  { value: 'notes', label: 'Notes' },
  { value: 'code', label: 'Code' },
  { value: 'media', label: 'Media' },
  { value: 'other', label: 'Other' },
];

const SOURCE_OPTIONS: { value: SourceFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'local', label: 'Local' },
  { value: 'web', label: 'Web' },
];

interface CorpusCommandBarProps {
  searchQuery: string;
  onSearchQueryChange: (value: string) => void;
  isContentSearchLoading: boolean;
  activeTypeFilter: string | null;
  onTypeFilterChange: (value: string | null) => void;
  sourceFilter: SourceFilter;
  onSourceFilterChange: (value: SourceFilter) => void;
  collections: CustomCollection[];
  activeCollectionId: string | null;
  onActiveCollectionChange: (id: string | null) => void;
  groupBy: GroupByMode;
  onGroupByChange: (value: GroupByMode) => void;
  sortField: SortField;
  sortOrder: SortOrder;
  onSortChange: (field: SortField, order: SortOrder) => void;
  density: DensityMode;
  onDensityChange: (value: DensityMode) => void;
  onRefresh: () => void;
  hasActiveFilters: boolean;
  onClearAllFilters: () => void;
}

/**
 * Command bar — §5 + §15.4 collapsed to 5 groups:
 * 1. Search input
 * 2. Type chips
 * 3. Source segmented
 * 4. Collection filter dropdown
 * 5. Display dropdown (combined group-by / sort / density)
 * 6. Overflow menu (Refresh, Manage sources)
 */
export function CorpusCommandBar(props: CorpusCommandBarProps) {
  const {
    searchQuery,
    onSearchQueryChange,
    isContentSearchLoading,
    activeTypeFilter,
    onTypeFilterChange,
    sourceFilter,
    onSourceFilterChange,
    collections,
    activeCollectionId,
    onActiveCollectionChange,
    groupBy,
    onGroupByChange,
    sortField,
    sortOrder,
    onSortChange,
    density,
    onDensityChange,
    onRefresh,
    hasActiveFilters,
    onClearAllFilters,
  } = props;

  return (
    <div className="flex min-h-12 flex-wrap items-center gap-x-3 gap-y-2 border-b border-[hsl(var(--border-subtle))] bg-[hsl(var(--bg))] px-8 py-2">
      <SearchInput
        value={searchQuery}
        onChange={onSearchQueryChange}
        isLoading={isContentSearchLoading}
      />

      <TypeChips value={activeTypeFilter} onChange={onTypeFilterChange} />

      <div className="h-6 w-px bg-[hsl(var(--border-subtle))]" aria-hidden="true" />

      <SourceSegmented value={sourceFilter} onChange={onSourceFilterChange} />

      <div className="h-6 w-px bg-[hsl(var(--border-subtle))]" aria-hidden="true" />

      <CollectionsDropdown
        collections={collections}
        activeCollectionId={activeCollectionId}
        onChange={onActiveCollectionChange}
      />

      <CorpusDisplayMenu
        groupBy={groupBy}
        onGroupByChange={onGroupByChange}
        sortField={sortField}
        sortOrder={sortOrder}
        onSortChange={onSortChange}
        density={density}
        onDensityChange={onDensityChange}
      />

      <OverflowMenu onRefresh={onRefresh} />

      {hasActiveFilters && (
        <button
          type="button"
          onClick={onClearAllFilters}
          className="ml-auto text-xs text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))]"
        >
          Clear filters
        </button>
      )}
    </div>
  );
}

function SearchInput({
  value,
  onChange,
  isLoading,
}: {
  value: string;
  onChange: (value: string) => void;
  isLoading: boolean;
}) {
  return (
    <div className="relative flex h-8 w-full max-w-[360px] flex-1 items-center">
      <Search
        className="pointer-events-none absolute left-2.5 h-3.5 w-3.5 text-[hsl(var(--text-tertiary))]"
        strokeWidth={1.75}
      />
      <input
        type="text"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder="Search documents or content…"
        className="h-full w-full rounded-[var(--radius-sm)] border border-[hsl(var(--border-default))] bg-[hsl(var(--surface))] pl-8 pr-8 text-sm text-[hsl(var(--text-primary))] placeholder:text-[hsl(var(--text-muted))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
      />
      <div className="absolute right-2 flex h-full items-center">
        {isLoading && !value && (
          <Loader2 className="h-3.5 w-3.5 animate-spin text-[hsl(var(--text-tertiary))]" strokeWidth={1.75} />
        )}
        {value && (
          <button
            type="button"
            onClick={() => onChange('')}
            aria-label="Clear search"
            className="flex h-5 w-5 items-center justify-center rounded-[var(--radius-sm)] text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]"
          >
            <X className="h-3 w-3" strokeWidth={1.75} />
          </button>
        )}
      </div>
      {isLoading && value && (
        <Loader2
          className="absolute right-8 h-3.5 w-3.5 animate-spin text-[hsl(var(--text-tertiary))]"
          strokeWidth={1.75}
        />
      )}
    </div>
  );
}

function TypeChips({
  value,
  onChange,
}: {
  value: string | null;
  onChange: (value: string | null) => void;
}) {
  const activeValue: TypeChipValue = (() => {
    if (!value) return 'all';
    const match = TYPE_CHIPS.find((chip) => chip.value === value);
    return match ? match.value : 'all';
  })();

  return (
    <div className="flex flex-wrap items-center gap-0.5" role="group" aria-label="Filter by type">
      {TYPE_CHIPS.map((chip) => {
        const isActive = chip.value === activeValue;
        return (
          <button
            key={chip.value}
            type="button"
            onClick={() => onChange(chip.value === 'all' ? null : chip.value)}
            aria-pressed={isActive}
            className={`relative px-2 py-1 text-xs transition-colors duration-fast ${
              isActive
                ? 'text-[hsl(var(--text-primary))]'
                : 'text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))]'
            }`}
          >
            {chip.label}
            {isActive && (
              <span
                aria-hidden="true"
                className="absolute inset-x-2 bottom-0 h-0.5 bg-[hsl(var(--accent))]"
              />
            )}
          </button>
        );
      })}
    </div>
  );
}

function SourceSegmented({
  value,
  onChange,
}: {
  value: SourceFilter;
  onChange: (value: SourceFilter) => void;
}) {
  return (
    <div
      role="tablist"
      aria-label="Filter by source"
      className="flex items-center rounded-[var(--radius-sm)] border border-[hsl(var(--border-subtle))] p-0.5"
    >
      {SOURCE_OPTIONS.map((option) => {
        const isActive = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="tab"
            aria-selected={isActive}
            onClick={() => onChange(option.value)}
            className={`rounded-[var(--radius-sm)] px-2 py-0.5 text-xs transition-colors duration-fast ${
              isActive
                ? 'bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))]'
                : 'text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-secondary))]'
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

interface CollectionNode {
  collection: CustomCollection;
  depth: number;
}

function flattenCollections(
  collections: CustomCollection[],
  parentId: string | null,
  depth: number,
  out: CollectionNode[],
): void {
  const children = collections
    .filter((c) => (c.parentId ?? null) === parentId)
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
  for (const child of children) {
    out.push({ collection: child, depth });
    flattenCollections(collections, child.id, depth + 1, out);
  }
}

function CollectionsDropdown({
  collections,
  activeCollectionId,
  onChange,
}: {
  collections: CustomCollection[];
  activeCollectionId: string | null;
  onChange: (id: string | null) => void;
}) {
  const [open, setOpen] = useState(false);
  const flattened = useMemo(() => {
    const out: CollectionNode[] = [];
    flattenCollections(collections, null, 0, out);
    return out;
  }, [collections]);

  const activeName = useMemo(() => {
    if (!activeCollectionId) return null;
    return collections.find((c) => c.id === activeCollectionId)?.name ?? null;
  }, [collections, activeCollectionId]);

  if (collections.length === 0) {
    return null;
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={`inline-flex h-8 items-center gap-1.5 rounded-[var(--radius-sm)] px-2.5 text-xs transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))] ${
            activeName
              ? 'bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))]'
              : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))]'
          }`}
        >
          <span>{activeName ?? 'Collections'}</span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" sideOffset={6} className="w-64 p-1">
        <button
          type="button"
          onClick={() => {
            onChange(null);
            setOpen(false);
          }}
          className={`flex h-7 w-full items-center justify-between rounded-[var(--radius-sm)] px-2 text-sm transition-colors duration-fast ${
            !activeCollectionId
              ? 'bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))]'
              : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))]'
          }`}
        >
          <span>All documents</span>
          {!activeCollectionId && (
            <Check className="h-3.5 w-3.5 text-[hsl(var(--accent))]" strokeWidth={1.75} />
          )}
        </button>
        <div className="my-1 h-px bg-[hsl(var(--border-subtle))]" aria-hidden="true" />
        <div className="flex max-h-72 flex-col overflow-y-auto">
          {flattened.map(({ collection, depth }) => {
            const isActive = collection.id === activeCollectionId;
            return (
              <button
                key={collection.id}
                type="button"
                onClick={() => {
                  onChange(collection.id);
                  setOpen(false);
                }}
                style={{ paddingLeft: `${8 + depth * 14}px` }}
                className={`flex h-7 w-full items-center justify-between rounded-[var(--radius-sm)] pr-2 text-sm transition-colors duration-fast ${
                  isActive
                    ? 'bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))]'
                    : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))]'
                }`}
              >
                <span className="truncate">{collection.name}</span>
                <span className="ml-2 flex items-center gap-2">
                  <span className="text-xxs tabular-nums text-[hsl(var(--text-muted))]">
                    {collection.documentIds.length}
                  </span>
                  {isActive && (
                    <Check className="h-3.5 w-3.5 text-[hsl(var(--accent))]" strokeWidth={1.75} />
                  )}
                </span>
              </button>
            );
          })}
        </div>
      </PopoverContent>
    </Popover>
  );
}

function OverflowMenu({ onRefresh }: { onRefresh: () => void }) {
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();

  // Close on Escape — Popover handles this, but guard against stale open state when route changes.
  useEffect(() => () => setOpen(false), []);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="More actions"
          className="flex h-8 w-8 items-center justify-center rounded-[var(--radius-sm)] text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
        >
          <MoreHorizontal className="h-4 w-4" strokeWidth={1.75} />
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" sideOffset={6} className="w-48 p-1">
        <button
          type="button"
          onClick={() => {
            onRefresh();
            setOpen(false);
          }}
          className="flex h-7 w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-sm text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))]"
        >
          <RefreshCw className="h-3.5 w-3.5" strokeWidth={1.75} />
          <span>Refresh</span>
        </button>
        <button
          type="button"
          onClick={() => {
            navigate('/settings');
            setOpen(false);
          }}
          className="flex h-7 w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-sm text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))]"
        >
          <span>Manage sources</span>
        </button>
      </PopoverContent>
    </Popover>
  );
}
