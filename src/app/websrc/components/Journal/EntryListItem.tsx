import { useState } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import { Pencil, Pin, PinOff, Trash2 } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';

import type { JournalEntrySummary } from './useJournalEntries';

interface EntryListItemProps {
  entry: JournalEntrySummary;
  isActive: boolean;
  isPinned: boolean;
  isRenaming: boolean;
  renameDraft: string;
  onSelect: () => void;
  onTogglePinned: () => void;
  onStartRename: () => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onRenameDraftChange: (value: string) => void;
  onDelete: () => void;
}

function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

/**
 * Single entry row in the journal sidebar. Title and time share one line;
 * hover reveals rename/pin/delete as an overlay that reserves no width.
 */
export function EntryListItem({
  entry,
  isActive,
  isPinned,
  isRenaming,
  renameDraft,
  onSelect,
  onTogglePinned,
  onStartRename,
  onCommitRename,
  onCancelRename,
  onRenameDraftChange,
  onDelete,
}: EntryListItemProps) {
  const prefersReducedMotion = useReducedMotion();
  const [isHovering, setIsHovering] = useState(false);

  return (
    <div
      role="button"
      tabIndex={0}
      aria-current={isActive ? 'page' : undefined}
      onMouseEnter={() => setIsHovering(true)}
      onMouseLeave={() => setIsHovering(false)}
      onClick={() => {
        if (!isRenaming) onSelect();
      }}
      onKeyDown={(e) => {
        if (isRenaming) return;
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onSelect();
        }
      }}
      className={`group relative w-full cursor-pointer px-4 py-2.5 transition-colors duration-fast ${
        isActive ? 'bg-surface-raised' : 'hover:bg-surface-raised'
      }`}
    >
      {isActive &&
        (prefersReducedMotion ? (
          <span
            className="absolute inset-y-0 left-0 w-0.5 bg-accent"
            aria-hidden="true"
          />
        ) : (
          <motion.span
            layoutId="journal-sidebar-active-bar"
            className="absolute inset-y-0 left-0 w-0.5 bg-accent"
            aria-hidden="true"
            transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          />
        ))}

      <div className="flex items-center gap-2">
        {isPinned && (
          <Pin
            className="h-3 w-3 shrink-0 text-accent"
            strokeWidth={1.75}
            aria-label="Pinned"
          />
        )}
        {isRenaming ? (
          <input
            autoFocus
            value={renameDraft}
            onChange={(e) => onRenameDraftChange(e.target.value)}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                onCommitRename();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                onCancelRename();
              }
            }}
            onBlur={onCommitRename}
            maxLength={120}
            className="h-6 min-w-0 flex-1 rounded-sm border border-border-default bg-bg px-1.5 text-sm text-text-primary outline-none transition-colors duration-fast focus:border-accent"
          />
        ) : (
          <>
            <span
              className={`min-w-0 flex-1 truncate text-sm text-text-primary ${
                isActive ? 'font-medium' : ''
              }`}
              title={entry.title}
            >
              {entry.title}
            </span>
            <time className="shrink-0 text-xs tabular-nums text-text-muted">
              {formatTime(entry.updatedAt)}
            </time>
          </>
        )}
      </div>

      {/* Hover-revealed actions — overlay, reserves no width */}
      {!isRenaming && (
        <div
          className={`absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-0.5 rounded-sm bg-surface-raised pl-2 transition-opacity duration-fast ${
            isHovering ? 'opacity-100' : 'pointer-events-none opacity-0'
          }`}
          onClick={(e) => e.stopPropagation()}
        >
          <IconButton label="Rename entry" onClick={onStartRename}>
            <Pencil />
          </IconButton>
          <IconButton
            label={isPinned ? 'Unpin entry' : 'Pin entry'}
            onClick={onTogglePinned}
          >
            {isPinned ? <PinOff /> : <Pin />}
          </IconButton>
          <IconButton
            label="Delete entry"
            className="hover:text-[hsl(var(--danger-fg))]"
            onClick={onDelete}
          >
            <Trash2 />
          </IconButton>
        </div>
      )}
    </div>
  );
}
