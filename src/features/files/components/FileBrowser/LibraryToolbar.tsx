import { Loader2, PanelLeft, Search, X } from 'lucide-react';

import { Tabs, TabsList, TabsTrigger } from '@/components/ui';
import { IconButton } from '@/components/ui/IconButton';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { cn } from '@/lib/utils';
import { type SortField, type SortOrder, type SourceFilter } from '@/types/fileBrowser';

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
 * Unified compact desktop toolbar: h-8 rounded-lg controls across search,
 * source Tabs, Radix Sort Select, and date grouping toggle.
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
    <div className="mb-4 flex flex-wrap items-center gap-2.5">
      <IconButton
        label={isRailOpen ? 'Hide sidebar' : 'Show sidebar'}
        shortcut="⌘\"
        onClick={onToggleRail}
        className="h-8 w-8 rounded-lg"
      >
        <PanelLeft strokeWidth={1.75} />
      </IconButton>

      <div className="relative h-8 min-w-[200px] flex-1 max-w-sm">
        <Search
          className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted"
          strokeWidth={1.75}
          aria-hidden="true"
        />
        <input
          type="text"
          value={searchQuery}
          onChange={(event) => onSearchQueryChange(event.target.value)}
          placeholder="Search files"
          aria-label="Search files"
          className="h-8 w-full rounded-lg border border-border-default bg-surface pl-8 pr-14 text-ui text-text-primary shadow-control outline-hidden transition-[border-color,box-shadow] duration-fast placeholder:text-text-muted hover:border-border-strong focus-visible:border-accent/60 focus-visible:ring-2 focus-visible:ring-ring"
        />
        {isContentSearchLoading && hasQuery ? (
          <Loader2
            className="pointer-events-none absolute right-8 top-1/2 h-3.5 w-3.5 -translate-y-1/2 animate-spin text-text-muted"
            aria-hidden="true"
          />
        ) : null}
        {hasQuery ? (
          <button
            type="button"
            onClick={() => onSearchQueryChange('')}
            aria-label="Clear search"
            className="absolute right-1.5 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-md text-text-muted transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
          >
            <X className="h-3.5 w-3.5" strokeWidth={1.75} aria-hidden="true" />
          </button>
        ) : null}
      </div>

      <Tabs
        variant="pill"
        value={filterBySource}
        onValueChange={(value) => onFilterBySourceChange(value as SourceFilter)}
      >
        <TabsList aria-label="Filter by source">
          {SOURCE_OPTIONS.map((option) => (
            <TabsTrigger
              key={option.id}
              value={option.id}
              onClick={() => onFilterBySourceChange(option.id)}
              className="gap-1.5 px-2.5 text-xs"
            >
              {option.label}{' '}
              <span className="tabular-nums text-text-muted">
                {sourceCounts[option.id].toLocaleString()}
              </span>
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <div className="ml-auto flex items-center gap-2">
        <Select
          value={sortValue}
          onValueChange={(value) => {
            const [field, order] = value.split(':') as [SortField, SortOrder];
            onSortChange(field, order);
          }}
        >
          <SelectTrigger
            className="h-8 w-auto min-w-[152px] rounded-lg"
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
          className={cn(
            'inline-flex h-8 items-center rounded-lg border px-2.5 text-ui font-medium transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring',
            groupByDate
              ? 'border-accent/40 bg-accent-muted text-text-primary'
              : 'border-border-default bg-surface text-text-secondary shadow-control hover:border-border-strong hover:text-text-primary',
          )}
        >
          Group by date
        </button>
      </div>
    </div>
  );
}
