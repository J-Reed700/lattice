import { useState } from 'react';

import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';

import { type ConversationSpaceDto } from '../../types';

interface BodyHeaderProps {
  totalCount: number;
  filteredCount: number;
  hasActiveFilters: boolean;
  selectedCount: number;
  spaces: ConversationSpaceDto[];
  isLoadingSpaces: boolean;
  isAssigningSpace: boolean;
  onAssignToSpace: (spaceId: string) => void;
  onDelete: () => void;
  onClearSelection: () => void;
}

/**
 * Body header — §6.1. Result count + inline bulk actions when selection > 0.
 * Replaces the tinted selection bar at FileBrowser.tsx:846-893.
 */
export function BodyHeader({
  totalCount,
  filteredCount,
  hasActiveFilters,
  selectedCount,
  spaces,
  isLoadingSpaces,
  isAssigningSpace,
  onAssignToSpace,
  onDelete,
  onClearSelection,
}: BodyHeaderProps) {
  return (
    <div className="flex min-h-8 items-center justify-between px-4 pb-2 pt-3">
      <p className="text-sm tabular-nums text-[hsl(var(--text-tertiary))]">
        {hasActiveFilters
          ? `Showing ${filteredCount.toLocaleString()} of ${totalCount.toLocaleString()} ${
              totalCount === 1 ? 'document' : 'documents'
            }`
          : `${totalCount.toLocaleString()} ${totalCount === 1 ? 'document' : 'documents'}`}
      </p>

      {selectedCount > 0 && (
        <div className="flex items-center gap-3">
          <span className="text-sm text-[hsl(var(--text-primary))]">
            {selectedCount} selected
          </span>
          <AssignSpacePopover
            spaces={spaces}
            isLoadingSpaces={isLoadingSpaces}
            isAssigningSpace={isAssigningSpace}
            onAssign={onAssignToSpace}
          />
          <button
            type="button"
            onClick={onDelete}
            className="text-sm text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--danger-fg))]"
          >
            Delete
          </button>
          <button
            type="button"
            onClick={onClearSelection}
            className="text-sm text-[hsl(var(--text-muted))] transition-colors duration-fast hover:text-[hsl(var(--text-secondary))]"
          >
            Clear
          </button>
        </div>
      )}
    </div>
  );
}

function AssignSpacePopover({
  spaces,
  isLoadingSpaces,
  isAssigningSpace,
  onAssign,
}: {
  spaces: ConversationSpaceDto[];
  isLoadingSpaces: boolean;
  isAssigningSpace: boolean;
  onAssign: (spaceId: string) => void;
}) {
  const [open, setOpen] = useState(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={isLoadingSpaces || isAssigningSpace}
          className="text-sm text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--text-primary))] disabled:opacity-60"
        >
          Assign to space…
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" sideOffset={6} className="w-64 p-1">
        {isLoadingSpaces ? (
          <p className="px-2 py-1 text-sm text-[hsl(var(--text-tertiary))]">Loading spaces…</p>
        ) : spaces.length === 0 ? (
          <p className="px-2 py-1 text-sm text-[hsl(var(--text-tertiary))]">No spaces found</p>
        ) : (
          <div className="flex flex-col">
            {spaces.map((space) => (
              <button
                key={space.id}
                type="button"
                onClick={() => {
                  onAssign(space.id);
                  setOpen(false);
                }}
                className="flex h-7 w-full items-center justify-between rounded-[var(--radius-sm)] px-2 text-sm text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:bg-[hsl(var(--surface))] hover:text-[hsl(var(--text-primary))]"
              >
                <span className="truncate">{space.name}</span>
                {space.isArchived && (
                  <span className="ml-2 text-xxs text-[hsl(var(--text-muted))]">Archived</span>
                )}
              </button>
            ))}
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
