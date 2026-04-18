import { type ReactNode } from 'react';

import {
  AlignCenter,
  AlignJustify,
  AlignLeft,
  FolderTree,
  Globe2,
  HardDrive,
  LayoutGrid,
  List,
  Loader2,
  RefreshCw,
  Search,
  X,
} from 'lucide-react';

import { PinnedSavedSearchRail, SavedSearchQuickControls } from './SavedSearchControls';
import {
  type Density,
  type SavedLibraryView,
  type SavedSearchPreset,
  type SourceFilter,
  type ViewMode,
} from '../../types/fileBrowser';
import Button from '../ui/Button/Button';
import { Input } from '../ui/input';

interface SourceCounts {
  all: number;
  local: number;
  web: number;
}

interface LibraryToolbarProps {
  filteredCount: number;
  totalCount: number;
  selectedCount: number;
  savedViews: SavedLibraryView[];
  activeSavedViewId: string | null;
  activeSavedViewName: string | null;
  onSavedViewChange: (_value: string) => void;
  onClearActiveSavedView: () => void;
  savedSearches: SavedSearchPreset[];
  activeSavedSearchId: string | null;
  activeSavedSearchName: string | null;
  onSavedSearchChange: (_value: string) => void;
  onClearActiveSavedSearch: () => void;
  onRenameActiveSearch: () => void;
  onDuplicateActiveSearch: () => void;
  onTogglePinActiveSearch: () => void;
  onDeleteActiveSearch: () => void;
  onSaveSearch: () => void;
  saveSearchDisabled: boolean;
  onSaveResultsAsCollection: () => void;
  saveResultsDisabled: boolean;
  searchQuery: string;
  onSearchQueryChange: (_value: string) => void;
  onClearSearch: () => void;
  isContentSearchLoading: boolean;
  hasNormalizedSearchQuery: boolean;
  sourceCounts: SourceCounts;
  filterBySource: SourceFilter;
  onFilterBySourceChange: (_source: SourceFilter) => void;
  groupByDate: boolean;
  onToggleGroupByDate: () => void;
  viewMode: ViewMode;
  onViewModeChange: (_mode: ViewMode) => void;
  density: Density;
  onDensityChange: (_density: Density) => void;
  onRefresh: () => void;
  normalizedSearchQuery: string;
  activeFilter: string | null;
  pinnedSearches: SavedSearchPreset[];
  draggedSearchId: string | null;
  onSetDraggedSearchId: (_searchId: string | null) => void;
  onReorderPinnedSearch: (_sourceId: string, _targetId: string) => void;
  onMovePinnedSearchByDirection: (_searchId: string, _direction: 'left' | 'right') => void;
  onRenamePinnedSearch: (_searchId: string, _name: string) => void;
  onApplySavedSearch: (_searchId: string) => void;
}

interface SourceFilterOption {
  key: SourceFilter;
  label: string;
  count: number;
  icon: ReactNode;
}

export function LibraryToolbar({
  filteredCount,
  totalCount,
  selectedCount,
  savedViews,
  activeSavedViewId,
  activeSavedViewName,
  onSavedViewChange,
  onClearActiveSavedView,
  savedSearches,
  activeSavedSearchId,
  activeSavedSearchName,
  onSavedSearchChange,
  onClearActiveSavedSearch,
  onRenameActiveSearch,
  onDuplicateActiveSearch,
  onTogglePinActiveSearch,
  onDeleteActiveSearch,
  onSaveSearch,
  saveSearchDisabled,
  onSaveResultsAsCollection,
  saveResultsDisabled,
  searchQuery,
  onSearchQueryChange,
  onClearSearch,
  isContentSearchLoading,
  hasNormalizedSearchQuery,
  sourceCounts,
  filterBySource,
  onFilterBySourceChange,
  groupByDate,
  onToggleGroupByDate,
  viewMode,
  onViewModeChange,
  density,
  onDensityChange,
  onRefresh,
  normalizedSearchQuery,
  activeFilter,
  pinnedSearches,
  draggedSearchId,
  onSetDraggedSearchId,
  onReorderPinnedSearch,
  onMovePinnedSearchByDirection,
  onRenamePinnedSearch,
  onApplySavedSearch,
}: LibraryToolbarProps) {
  const sourceFilterOptions: SourceFilterOption[] = [
    { key: 'all', label: 'All Sources', count: sourceCounts.all, icon: <FolderTree className="h-3.5 w-3.5" /> },
    { key: 'local', label: 'Local', count: sourceCounts.local, icon: <HardDrive className="h-3.5 w-3.5" /> },
    { key: 'web', label: 'Web', count: sourceCounts.web, icon: <Globe2 className="h-3.5 w-3.5" /> },
  ];

  return (
    <div className="border-b border-[hsl(var(--border-subtle))] bg-[radial-gradient(circle_at_top_right,_rgba(14,165,233,0.12),_transparent_45%),linear-gradient(180deg,hsl(var(--surface-raised)),hsl(var(--surface)))]">
      <div className="space-y-3 px-5 py-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-[220px]">
            <h2 className="text-xl font-semibold tracking-tight text-[hsl(var(--text-primary))]">Library</h2>
            <p className="text-sm text-[hsl(var(--text-secondary))]">
              Search by file name or matching file content.
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <StatChip label="Visible" value={filteredCount} />
            <StatChip label="Total" value={totalCount} />
            <StatChip label="Selected" value={selectedCount} />
            {savedViews.length > 0 && (
              <div className="inline-flex items-center gap-1 rounded-full border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-2 py-0.5">
                <select
                  value={activeSavedViewId ?? ''}
                  onChange={(event) => onSavedViewChange(event.target.value)}
                  className="h-8 max-w-[170px] rounded-full bg-transparent px-2 text-xs font-medium text-[hsl(var(--text-secondary))] outline-none"
                  aria-label="Toggle view preset"
                  title="Toggle view preset"
                >
                  <option value="">No preset</option>
                  {savedViews.map((view) => (
                    <option key={view.id} value={view.id}>
                      {view.name}
                    </option>
                  ))}
                </select>
                {activeSavedViewId && (
                  <button
                    type="button"
                    onClick={onClearActiveSavedView}
                    className="inline-flex h-7 w-7 items-center justify-center rounded-full text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]"
                    aria-label="Clear active view"
                    title="Clear active view"
                  >
                    <X className="h-4 w-4" />
                  </button>
                )}
              </div>
            )}
            <SavedSearchQuickControls
              savedSearches={savedSearches}
              activeSavedSearchId={activeSavedSearchId}
              onSavedSearchChange={onSavedSearchChange}
              onClearActiveSavedSearch={onClearActiveSavedSearch}
              onRenameActiveSearch={onRenameActiveSearch}
              onDuplicateActiveSearch={onDuplicateActiveSearch}
              onTogglePinActiveSearch={onTogglePinActiveSearch}
              onDeleteActiveSearch={onDeleteActiveSearch}
              onSaveSearch={onSaveSearch}
              saveSearchDisabled={saveSearchDisabled}
            />
            <Button
              variant="ghost"
              size="sm"
              onClick={onSaveResultsAsCollection}
              disabled={saveResultsDisabled}
              className="h-9 rounded-full border border-[hsl(var(--border-subtle))] px-3 text-sm text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))]"
            >
              <FolderTree className="mr-2 h-4 w-4" />
              Freeze Results
            </Button>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2 xl:gap-3">
          <div className="relative min-w-[260px] flex-1 xl:max-w-xl">
            <Search className="pointer-events-none absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-[hsl(var(--text-tertiary))]" />
            <Input
              type="text"
              placeholder="Search files and content..."
              value={searchQuery}
              onChange={(event) => onSearchQueryChange(event.target.value)}
              className="h-11 w-full rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] pl-10 pr-16 text-sm shadow-sm transition-colors duration-fast focus-visible:border-[hsl(var(--accent))] focus-visible:ring-[hsl(var(--accent))]"
            />
            {isContentSearchLoading && hasNormalizedSearchQuery && (
              <Loader2 className="pointer-events-none absolute right-10 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-[hsl(var(--text-tertiary))]" />
            )}
            {searchQuery.trim().length > 0 && (
              <button
                type="button"
                onClick={onClearSearch}
                className="absolute right-2 top-1/2 inline-flex h-7 w-7 -translate-y-1/2 items-center justify-center rounded-full text-[hsl(var(--text-tertiary))] transition-colors duration-fast hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]"
                aria-label="Clear search"
              >
                <X className="h-4 w-4" />
              </button>
            )}
          </div>

          <div className="flex items-center gap-1 rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] p-1 shadow-sm">
            {sourceFilterOptions.map((option) => {
              const isActive = filterBySource === option.key;
              return (
                <button
                  key={option.key}
                  type="button"
                  onClick={() => onFilterBySourceChange(option.key)}
                  className={`inline-flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-semibold transition-colors duration-fast ${
                    isActive
                      ? 'bg-[hsl(var(--accent-muted))]/60 text-[hsl(var(--accent))]'
                      : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]'
                  }`}
                  aria-pressed={isActive}
                >
                  {option.icon}
                  <span>{option.label}</span>
                  <span className="rounded-full bg-[hsl(var(--surface-raised))] px-1.5 py-0.5 text-[10px]">
                    {option.count}
                  </span>
                </button>
              );
            })}
          </div>

          <button
            onClick={onToggleGroupByDate}
            className={`h-11 rounded-xl border px-3 text-sm font-medium transition-colors duration-fast ${
              groupByDate
                ? 'border-[hsl(var(--accent))] bg-[hsl(var(--accent-muted))]/40 text-[hsl(var(--accent))]'
                : 'border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))] hover:border-[hsl(var(--border-default))]'
            }`}
            aria-pressed={groupByDate}
            title="Toggle date groups"
          >
            Group by Date
          </button>

          <div className="flex items-center gap-1 rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] p-1 shadow-sm">
            <ViewModeButton
              mode="tree"
              currentMode={viewMode}
              onClick={() => onViewModeChange('tree')}
              icon={<FolderTree className="h-4 w-4" />}
              label="Tree view"
            />
            <ViewModeButton
              mode="list"
              currentMode={viewMode}
              onClick={() => onViewModeChange('list')}
              icon={<List className="h-4 w-4" />}
              label="List view"
            />
            <ViewModeButton
              mode="grid"
              currentMode={viewMode}
              onClick={() => onViewModeChange('grid')}
              icon={<LayoutGrid className="h-4 w-4" />}
              label="Grid view"
            />
          </div>

          {viewMode === 'list' && (
            <div className="flex items-center gap-1 rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] p-1 shadow-sm">
              <DensityButton
                density="compact"
                currentDensity={density}
                onClick={() => onDensityChange('compact')}
                icon={<AlignJustify className="h-4 w-4" />}
                label="Compact density"
              />
              <DensityButton
                density="comfortable"
                currentDensity={density}
                onClick={() => onDensityChange('comfortable')}
                icon={<AlignLeft className="h-4 w-4" />}
                label="Comfortable density"
              />
              <DensityButton
                density="spacious"
                currentDensity={density}
                onClick={() => onDensityChange('spacious')}
                icon={<AlignCenter className="h-4 w-4" />}
                label="Spacious density"
              />
            </div>
          )}

          <Button
            variant="ghost"
            size="sm"
            onClick={onRefresh}
            aria-label="Refresh documents"
            className="h-11 rounded-xl border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))]"
          >
            <RefreshCw className="h-4 w-4" />
          </Button>
        </div>

        {(normalizedSearchQuery || activeFilter || filterBySource !== 'all' || activeSavedViewId || activeSavedSearchId || pinnedSearches.length > 0) && (
          <div className="space-y-2 text-xs text-[hsl(var(--text-secondary))]">
            <div className="flex flex-wrap items-center gap-2">
              {activeSavedViewId && (
                <span className="rounded-full bg-[hsl(var(--accent-muted))]/40 px-2.5 py-1 text-[hsl(var(--accent))]">
                  View: {activeSavedViewName ?? 'Saved'}
                </span>
              )}
              {activeSavedSearchId && (
                <span className="rounded-full bg-[hsl(var(--accent-muted))]/40 px-2.5 py-1 text-[hsl(var(--accent))]">
                  Search Preset: {activeSavedSearchName ?? 'Saved'}
                </span>
              )}
              {normalizedSearchQuery && (
                <span className="rounded-full bg-[hsl(var(--accent-muted))]/40 px-2.5 py-1 text-[hsl(var(--accent))]">
                  Search: “{searchQuery.trim()}”
                </span>
              )}
              {activeFilter && (
                <span className="rounded-full bg-[hsl(var(--surface-raised))] px-2.5 py-1">
                  Type: {activeFilter}
                </span>
              )}
              {filterBySource !== 'all' && (
                <span className="rounded-full bg-[hsl(var(--surface-raised))] px-2.5 py-1">
                  Source: {filterBySource}
                </span>
              )}
            </div>
            {pinnedSearches.length > 0 && (
              <PinnedSavedSearchRail
                pinnedSearches={pinnedSearches}
                activeSavedSearchId={activeSavedSearchId}
                draggedSearchId={draggedSearchId}
                onSetDraggedSearchId={onSetDraggedSearchId}
                onReorderPinnedSearch={onReorderPinnedSearch}
                onMovePinnedSearchByDirection={onMovePinnedSearchByDirection}
                onRenamePinnedSearch={onRenamePinnedSearch}
                onApplySavedSearch={onApplySavedSearch}
              />
            )}
          </div>
        )}
      </div>
    </div>
  );
}

interface ViewModeButtonProps {
  mode: ViewMode;
  currentMode: ViewMode;
  onClick: () => void;
  icon: ReactNode;
  label: string;
}

function ViewModeButton({ mode, currentMode, onClick, icon, label }: ViewModeButtonProps) {
  const isActive = mode === currentMode;

  return (
    <button
      onClick={onClick}
      className={`inline-flex h-9 w-9 items-center justify-center rounded-lg transition-colors duration-fast ${
        isActive
          ? 'bg-[hsl(var(--accent-muted))]/60 text-[hsl(var(--accent))]'
          : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]'
      }`}
      aria-label={label}
      aria-pressed={isActive}
      title={label}
    >
      {icon}
    </button>
  );
}

interface DensityButtonProps {
  density: Density;
  currentDensity: Density;
  onClick: () => void;
  icon: ReactNode;
  label: string;
}

function DensityButton({ density, currentDensity, onClick, icon, label }: DensityButtonProps) {
  const isActive = density === currentDensity;

  return (
    <button
      onClick={onClick}
      className={`inline-flex h-9 w-9 items-center justify-center rounded-lg transition-colors duration-fast ${
        isActive
          ? 'bg-[hsl(var(--accent-muted))]/60 text-[hsl(var(--accent))]'
          : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))]'
      }`}
      aria-label={label}
      aria-pressed={isActive}
      title={label}
    >
      {icon}
    </button>
  );
}

interface StatChipProps {
  label: string;
  value: number;
}

function StatChip({ label, value }: StatChipProps) {
  return (
    <div className="inline-flex items-center gap-2 rounded-full border border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] px-3 py-1.5 shadow-sm">
      <span className="text-xs font-medium uppercase tracking-wide text-[hsl(var(--text-tertiary))]">{label}</span>
      <span className="text-sm font-semibold text-[hsl(var(--text-primary))]">{value}</span>
    </div>
  );
}
