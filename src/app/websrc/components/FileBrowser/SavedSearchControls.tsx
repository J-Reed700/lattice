import { BookmarkPlus, Copy, Edit3, Pin, PinOff, Trash2, X } from 'lucide-react';

import { type SavedSearchPreset } from '../../types/fileBrowser';
import Button from '../ui/Button/Button';

interface SavedSearchQuickControlsProps {
  savedSearches: SavedSearchPreset[];
  activeSavedSearchId: string | null;
  onSavedSearchChange: (_value: string) => void;
  onClearActiveSavedSearch: () => void;
  onRenameActiveSearch: () => void;
  onDuplicateActiveSearch: () => void;
  onTogglePinActiveSearch: () => void;
  onDeleteActiveSearch: () => void;
  onSaveSearch: () => void;
  saveSearchDisabled: boolean;
}

export function SavedSearchQuickControls({
  savedSearches,
  activeSavedSearchId,
  onSavedSearchChange,
  onClearActiveSavedSearch,
  onRenameActiveSearch,
  onDuplicateActiveSearch,
  onTogglePinActiveSearch,
  onDeleteActiveSearch,
  onSaveSearch,
  saveSearchDisabled,
}: SavedSearchQuickControlsProps) {
  const activeSavedSearch = activeSavedSearchId
    ? savedSearches.find((search) => search.id === activeSavedSearchId) ?? null
    : null;

  return (
    <>
      {savedSearches.length > 0 && (
        <div className="inline-flex items-center gap-1 rounded-full border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2 py-0.5">
          <select
            value={activeSavedSearchId ?? ''}
            onChange={(event) => onSavedSearchChange(event.target.value)}
            className="h-8 max-w-[170px] rounded-full bg-transparent px-2 text-xs font-medium text-[var(--text-secondary)] outline-none"
            aria-label="Toggle saved search"
            title="Toggle saved search"
          >
            <option value="">Ad hoc search</option>
            {savedSearches.map((search) => (
              <option key={search.id} value={search.id}>
                {search.name}
              </option>
            ))}
          </select>
          {activeSavedSearchId && (
            <button
              type="button"
              onClick={onClearActiveSavedSearch}
              className="inline-flex h-7 w-7 items-center justify-center rounded-full text-[var(--text-tertiary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
              aria-label="Clear active search"
              title="Clear active search"
            >
              <X className="h-4 w-4" />
            </button>
          )}
        </div>
      )}
      {activeSavedSearch && (
        <div className="inline-flex items-center gap-1 rounded-full border border-[var(--border-color)] bg-[var(--surface-elevated)] px-2 py-0.5">
          <button
            type="button"
            onClick={onRenameActiveSearch}
            className="inline-flex h-7 w-7 items-center justify-center rounded-full text-[var(--text-tertiary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
            aria-label="Rename active saved search"
            title="Rename saved search"
          >
            <Edit3 className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={onDuplicateActiveSearch}
            className="inline-flex h-7 w-7 items-center justify-center rounded-full text-[var(--text-tertiary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
            aria-label="Duplicate active saved search"
            title="Duplicate saved search"
          >
            <Copy className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={onTogglePinActiveSearch}
            className="inline-flex h-7 w-7 items-center justify-center rounded-full text-[var(--text-tertiary)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]"
            aria-label={activeSavedSearch.pinned ? 'Unpin active saved search' : 'Pin active saved search'}
            title={activeSavedSearch.pinned ? 'Unpin saved search' : 'Pin saved search'}
          >
            {activeSavedSearch.pinned ? (
              <PinOff className="h-3.5 w-3.5" />
            ) : (
              <Pin className="h-3.5 w-3.5" />
            )}
          </button>
          <button
            type="button"
            onClick={onDeleteActiveSearch}
            className="inline-flex h-7 w-7 items-center justify-center rounded-full text-[var(--error)] transition-colors hover:bg-[var(--surface-hover)]"
            aria-label="Delete active saved search"
            title="Delete saved search"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>
      )}
      <Button
        variant="ghost"
        size="sm"
        onClick={onSaveSearch}
        disabled={saveSearchDisabled}
        className="h-9 rounded-full border border-[var(--border-color)] px-3 text-sm text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
      >
        <BookmarkPlus className="mr-2 h-4 w-4" />
        Save Search
      </Button>
    </>
  );
}

interface PinnedSavedSearchRailProps {
  pinnedSearches: SavedSearchPreset[];
  activeSavedSearchId: string | null;
  draggedSearchId: string | null;
  onSetDraggedSearchId: (_searchId: string | null) => void;
  onReorderPinnedSearch: (_sourceId: string, _targetId: string) => void;
  onMovePinnedSearchByDirection: (_searchId: string, _direction: 'left' | 'right') => void;
  onRenamePinnedSearch: (_searchId: string, _name: string) => void;
  onApplySavedSearch: (_searchId: string) => void;
}

export function PinnedSavedSearchRail({
  pinnedSearches,
  activeSavedSearchId,
  draggedSearchId,
  onSetDraggedSearchId,
  onReorderPinnedSearch,
  onMovePinnedSearchByDirection,
  onRenamePinnedSearch,
  onApplySavedSearch,
}: PinnedSavedSearchRailProps) {
  if (pinnedSearches.length === 0) {
    return null;
  }

  return (
    <div className="flex max-w-full items-center gap-2 overflow-x-auto pb-0.5">
      <span
        className="shrink-0 text-[10px] font-semibold uppercase tracking-[0.16em] text-[var(--text-tertiary)]"
        title="Drag to reorder or use Alt+Left / Alt+Right while focused"
      >
        Pinned
      </span>
      {pinnedSearches.map((search) => {
        const isActive = activeSavedSearchId === search.id;
        return (
          <button
            key={search.id}
            type="button"
            draggable
            onDragStart={() => onSetDraggedSearchId(search.id)}
            onDragEnd={() => onSetDraggedSearchId(null)}
            onDragOver={(event) => {
              event.preventDefault();
            }}
            onDrop={() => {
              if (draggedSearchId) {
                onReorderPinnedSearch(draggedSearchId, search.id);
              }
              onSetDraggedSearchId(null);
            }}
            onKeyDown={(event) => {
              if (!event.altKey) {
                return;
              }
              if (event.key === 'ArrowLeft') {
                event.preventDefault();
                onMovePinnedSearchByDirection(search.id, 'left');
                return;
              }
              if (event.key === 'ArrowRight') {
                event.preventDefault();
                onMovePinnedSearchByDirection(search.id, 'right');
              }
            }}
            onDoubleClick={() => onRenamePinnedSearch(search.id, search.name)}
            onClick={() => onApplySavedSearch(search.id)}
            className={`inline-flex shrink-0 items-center gap-1.5 rounded-full border px-2.5 py-1 transition-colors ${
              isActive
                ? 'border-[var(--accent-primary)] bg-[var(--accent-light)]/50 text-[var(--accent-primary)]'
                : 'border-[var(--border-color)] bg-[var(--surface-elevated)] text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-primary)]'
            }`}
          >
            <Pin className="h-3 w-3" />
            <span>{search.name}</span>
          </button>
        );
      })}
    </div>
  );
}
