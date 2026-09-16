import { useCallback, useMemo, useState } from 'react';

import { Columns3 } from 'lucide-react';
import { useNavigate } from 'react-router';

import { useRegisterPaletteCommands } from '@/hooks/useRegisterPaletteCommands';
import {
  selectSelectedDocumentIds,
  useFileBrowserStore,
} from '@/stores/fileBrowserStore';
import type { PaletteCommand } from '@/stores/paletteCommandsStore';

import { type ConversationSpaceDto } from '../../types';
import { Popover, PopoverContent, PopoverTrigger } from '../ui/popover';

interface SelectionBarProps {
  selectedCount: number;
  spaces: ConversationSpaceDto[];
  isLoadingSpaces: boolean;
  isAssigningSpace: boolean;
  onAssignToSpace: (_spaceId: string) => void;
  onSnapshot: () => void;
  onDelete: () => void;
  onSelectAll: () => void;
  onClear: () => void;
}

/**
 * Shown only while something is selected. A row, not a floating card, not a
 * tinted panel: a count and four verbs.
 */
export function SelectionBar({
  selectedCount,
  spaces,
  isLoadingSpaces,
  isAssigningSpace,
  onAssignToSpace,
  onSnapshot,
  onDelete,
  onSelectAll,
  onClear,
}: SelectionBarProps) {
  const navigate = useNavigate();
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
    <div className="flex h-10 items-center gap-4">
      <span className="text-sm tabular-nums text-text-primary">{selectedCount} selected</span>
      <AddToSpace
        spaces={spaces}
        isLoadingSpaces={isLoadingSpaces}
        isAssigningSpace={isAssigningSpace}
        onAssign={onAssignToSpace}
      />
      <TextAction label="Compare" onClick={openCompare} disabled={selectedCount < 2} />
      <TextAction label="Snapshot" onClick={onSnapshot} />
      <TextAction label="Delete" onClick={onDelete} danger />
      <TextAction label="Select all" onClick={onSelectAll} />
      <TextAction label="Clear" onClick={onClear} muted />
    </div>
  );
}

function TextAction({
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
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`text-sm transition-colors duration-fast disabled:opacity-50 ${
        danger
          ? 'text-text-secondary hover:text-[hsl(var(--danger-fg))]'
          : muted
            ? 'text-text-muted hover:text-text-secondary'
            : 'text-text-secondary hover:text-text-primary'
      }`}
    >
      {label}
    </button>
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
        <button
          type="button"
          disabled={isLoadingSpaces || isAssigningSpace}
          className="text-sm text-text-secondary transition-colors duration-fast hover:text-text-primary disabled:opacity-50"
        >
          Add to space
        </button>
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
              className="flex h-8 w-full items-center justify-between rounded-sm px-2 text-left text-sm text-text-secondary transition-colors duration-fast hover:bg-surface hover:text-text-primary"
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
