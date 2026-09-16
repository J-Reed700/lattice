import { Loader2, PanelLeft, Search, X } from 'lucide-react';

import { type SortField, type SortOrder, type SourceFilter } from '../../types/fileBrowser';
import { IconButton } from '../ui/IconButton';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '../ui/select';

interface SourceCounts {
  all: number;
  local: number;
  web: number;
}

interface LibraryToolbarProps {
  isRailOpen: boolean;
  onToggleRail: () => void;
  searchQuery: string;
  onSearchQueryChange: (_value: string) => void;
  isContentSearchLoading: boolean;
  sourceCounts: SourceCounts;
  filterBySource: SourceFilter;
  onFilterBySourceChange: (_source: SourceFilter) => void;
  sortField: SortField;
  sortOrder: SortOrder;
  onSortChange: (_field: SortField, _order: SortOrder) => void;
  groupByDate: boolean;
  onToggleGroupByDate: () => void;
}

type SortKey = `${SortField}:${SortOrder}`;

const SOURCE_OPTIONS: Array<{ id: SourceFilter; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'local', label: 'Local' },
  { id: 'web', label: 'Web' },
];

const SORT_OPTIONS: Array<{ value: SortKey; label: string }> = [
  { value: 'modified:desc', label: 'Recently modified' },
  { value: 'modified:asc', label: 'Oldest first' },
  { value: 'name:asc', label: 'Name A–Z' },
  { value: 'name:desc', label: 'Name Z–A' },
  { value: 'size:desc', label: 'Longest first' },
  { value: 'type:asc', label: 'Type' },
];

/**
 * One row: search, source filter, sort, grouping. Nothing bordered around it.
 */
export function LibraryToolbar({
  isRailOpen,
  onToggleRail,
  searchQuery,
  onSearchQueryChange,
  isContentSearchLoading,
  sourceCounts,
  filterBySource,
  onFilterBySourceChange,
  sortField,
  sortOrder,
  onSortChange,
  groupByDate,
  onToggleGroupByDate,
}: LibraryToolbarProps) {
  const sortValue: SortKey = `${sortField}:${sortOrder}`;
  const hasQuery = searchQuery.trim().length > 0;

  return (
    <div className="mb-4 flex flex-wrap items-center gap-4">
      <IconButton
        label={isRailOpen ? 'Hide sidebar' : 'Show sidebar'}
        shortcut="⌘\"
        onClick={onToggleRail}
      >
        <PanelLeft strokeWidth={1.75} />
      </IconButton>

      <div className="relative h-9 w-full max-w-md">
        <Search
          className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-text-muted"
          strokeWidth={1.75}
        />
        <input
          type="text"
          value={searchQuery}
          onChange={(event) => onSearchQueryChange(event.target.value)}
          placeholder="Search files"
          aria-label="Search files"
          className="h-9 w-full rounded-md border border-border-default bg-surface pl-9 pr-16 text-sm text-text-primary outline-none transition-colors duration-fast placeholder:text-text-muted focus:border-accent"
        />
        {isContentSearchLoading && hasQuery ? (
          <Loader2 className="pointer-events-none absolute right-9 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-text-muted" />
        ) : null}
        {hasQuery ? (
          <button
            type="button"
            onClick={() => onSearchQueryChange('')}
            aria-label="Clear search"
            className="absolute right-2 top-1/2 flex h-6 w-6 -translate-y-1/2 items-center justify-center rounded-sm text-text-muted transition-colors duration-fast hover:text-text-primary"
          >
            <X className="h-4 w-4" strokeWidth={1.75} />
          </button>
        ) : null}
      </div>

      <div role="tablist" className="flex items-center gap-3 text-xs">
        {SOURCE_OPTIONS.map((option) => {
          const isActive = filterBySource === option.id;
          return (
            <button
              key={option.id}
              type="button"
              role="tab"
              aria-selected={isActive}
              onClick={() => onFilterBySourceChange(option.id)}
              className={`border-b-2 pb-1 transition-colors duration-fast ${
                isActive
                  ? 'border-accent text-text-primary'
                  : 'border-transparent text-text-tertiary hover:text-text-primary'
              }`}
            >
              {option.label}{' '}
              <span className="tabular-nums text-text-muted">
                {sourceCounts[option.id].toLocaleString()}
              </span>
            </button>
          );
        })}
      </div>

      <Select
        value={sortValue}
        onValueChange={(value) => {
          const [field, order] = value.split(':') as [SortField, SortOrder];
          onSortChange(field, order);
        }}
      >
        <SelectTrigger
          className="h-9 w-auto gap-1.5 border-0 px-2 text-sm text-text-secondary"
          aria-label="Sort files"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {SORT_OPTIONS.map((option) => (
            <SelectItem key={option.value} value={option.value}>
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <button
        type="button"
        onClick={onToggleGroupByDate}
        aria-pressed={groupByDate}
        className={`h-9 rounded-md px-2 text-sm transition-colors duration-fast ${
          groupByDate
            ? 'text-accent'
            : 'text-text-secondary hover:text-text-primary'
        }`}
      >
        Group by date
      </button>
    </div>
  );
}
