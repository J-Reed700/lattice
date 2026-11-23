import { useCallback, useMemo, useState } from 'react';

import { Columns3 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import {
  selectSelectedDocumentIds,
  useFileBrowserStore,
} from '@/features/files/stores/fileBrowserStore';
import { useRegisterPaletteCommands } from '@/features/palette/hooks/useRegisterPaletteCommands';
import type { PaletteCommand } from '@/features/palette/stores/paletteCommandsStore';
import { useAppNavigate } from '@/hooks/useAppNavigate';
import { cn } from '@/lib/utils';
import { type ConversationSpaceDto } from '@/types';

interface SelectionBarProps {
  selectedCount: number;
  spaces: ConversationSpaceDto[];
  isLoadingSpaces: boolean;
  isAssigningSpace: boolean;
  onAssignToSpace: (_spaceId: string) => void;
  onAddToCollection: () => void;
  onRemoveFromCollection?: () => void;
  onSnapshot: () => void;
  onDelete: () => void;
  onSelectAll: () => void;
  onClear: () => void;
}

/**
 * Shown only while something is selected. Unified h-8 rounded-lg controls with
 * `--action` primary CTA and calm secondary/ghost actions.
 */
export function SelectionBar({
  selectedCount,
  spaces,
  isLoadingSpaces,
  isAssigningSpace,
  onAssignToSpace,
  onAddToCollection,
  onRemoveFromCollection,
  onSnapshot,
  onDelete,
  onSelectAll,
  onClear,
}: SelectionBarProps) {
  const navigate = useAppNavigate();
  // The selection already lives in the store, so this reads it directly rather
  // than adding a prop to FileBrowser.
  const selectedDocumentIds = useFileBrowserStore(selectSelectedDocumentIds);

  const openCompare = useCallback(() => {
    navigate(`/compare?ids=${Array.from(selectedDocumentIds).join(',')}`);
  }, [navigate, selectedDocumentIds]);

  const compareCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'library.compare',
        label: 'Compare selected documents',
        group: 'Library',
        icon: Columns3,
        enabled: selectedCount >= 2,
        run: openCompare,
      },
    ],
    [openCompare, selectedCount],
  );
  useRegisterPaletteCommands(compareCommands);

  return (
    <div
      className="mb-3 flex min-h-11 flex-wrap items-center gap-1.5 rounded-xl border border-border-subtle bg-surface px-3 py-1.5 shadow-control"
      aria-label="Document selection actions"
    >
      <span className="mr-1.5 text-ui font-medium tabular-nums text-text-primary">
        {selectedCount} selected
      </span>
      <Button type="button" onClick={onAddToCollection}>
        Add to collection
      </Button>
      {onRemoveFromCollection ? (
        <BarAction label="Remove from collection" onClick={onRemoveFromCollection} />
      ) : null}
      <AddToSpace
        spaces={spaces}
        isLoadingSpaces={isLoadingSpaces}
        isAssigningSpace={isAssigningSpace}
        onAssign={onAssignToSpace}
      />
      <BarAction label="Compare" onClick={openCompare} disabled={selectedCount < 2} />
      <BarAction label="Snapshot" onClick={onSnapshot} />
      <BarAction label="Delete" onClick={onDelete} danger />
      <div className="ml-auto flex items-center gap-1">
        <BarAction label="Select all" onClick={onSelectAll} />
        <BarAction label="Clear" onClick={onClear} muted />
      </div>
    </div>
  );
}

function BarAction({
  label,
  onClick,
  danger,
  muted,
  disabled,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  muted?: boolean;
  disabled?: boolean;
}) {
  return (
    <Button
      type="button"
      variant="ghost"
      onClick={onClick}
      disabled={disabled}
      className={cn(
        'h-8 rounded-lg px-2.5 text-ui',
        danger
          ? 'text-text-secondary hover:bg-danger-muted hover:text-danger-fg'
          : muted
            ? 'text-text-muted hover:text-text-primary'
            : 'text-text-secondary hover:text-text-primary',
      )}
    >
      {label}
    </Button>
  );
}

function AddToSpace({
  spaces,
  isLoadingSpaces,
  isAssigningSpace,
  onAssign,
}: {
  spaces: ConversationSpaceDto[];
  isLoadingSpaces: boolean;
  isAssigningSpace: boolean;
  onAssign: (_spaceId: string) => void;
}) {
  const [open, setOpen] = useState(false);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          disabled={isLoadingSpaces || isAssigningSpace}
          className="h-8 rounded-lg px-2.5 text-ui text-text-secondary hover:text-text-primary"
        >
          Add to space
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" sideOffset={6} className="w-56 p-1">
        {spaces.length === 0 ? (
          <p className="px-2 py-1 text-sm text-text-muted">No spaces.</p>
        ) : (
          spaces.map((space) => (
            <button
              key={space.id}
              type="button"
              onClick={() => {
                onAssign(space.id);
                setOpen(false);
              }}
              className="flex h-8 w-full items-center justify-between rounded-md px-2 text-left text-ui text-text-secondary transition-colors duration-fast hover:bg-[hsl(var(--text-primary)/0.06)] hover:text-text-primary"
            >
              <span className="truncate">{space.name}</span>
              {space.isArchived ? (
                <span className="ml-2 shrink-0 text-xs text-text-muted">Archived</span>
              ) : null}
            </button>
          ))
        )}
      </PopoverContent>
    </Popover>
  );
}
