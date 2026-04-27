import { useState } from 'react';

import { format, isToday, isYesterday } from 'date-fns';
import { motion, useReducedMotion } from 'framer-motion';
import { Pencil, Pin, PinOff, Trash2 } from 'lucide-react';

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

function formatDatePrefix(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  if (isToday(date)) return 'TODAY';
  if (isYesterday(date)) return 'YESTERDAY';
  return format(date, 'EEE · MMM d').toUpperCase();
}

function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

/**
 * Single entry row in the journal sidebar. Date prefix on its own line above
 * the title; hover reveals rename/pin/delete actions.
 * Spec §4.2.
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
        isActive ? 'bg-[hsl(var(--surface))]' : 'hover:bg-[hsl(var(--surface))]'
      }`}
    >
      {isActive &&
        (prefersReducedMotion ? (
          <span
            className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
            aria-hidden="true"
          />
        ) : (
          <motion.span
            layoutId="journal-sidebar-active-bar"
            className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
            aria-hidden="true"
            transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          />
        ))}

      <p className="mb-1 text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
        {formatDatePrefix(entry.updatedAt)}
      </p>
      <div className="flex items-baseline justify-between gap-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5">
          {isPinned && (
            <Pin
              className="h-3 w-3 shrink-0 text-[hsl(var(--accent))]"
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
              className="min-w-0 flex-1 rounded-sm border border-[hsl(var(--border-default))] bg-[hsl(var(--bg))] px-1.5 py-0.5 text-sm text-[hsl(var(--text-primary))] outline-none focus:border-[hsl(var(--accent))]"
            />
          ) : (
            <span
              className={`min-w-0 flex-1 truncate text-sm ${
                isActive
                  ? 'font-medium text-[hsl(var(--text-primary))]'
                  : 'text-[hsl(var(--text-primary))]'
              }`}
            >
              {entry.title}
            </span>
          )}
        </div>
        {!isRenaming && (
          <time className="shrink-0 text-xxs text-[hsl(var(--text-muted))]">
            {formatTime(entry.updatedAt)}
          </time>
        )}
      </div>

      {/* Hover-revealed action row */}
      {!isRenaming && (
        <div
          className={`absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-0.5 transition-opacity duration-fast ${
            isHovering ? 'opacity-100' : 'opacity-0 pointer-events-none'
          }`}
          onClick={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            onClick={onStartRename}
            className="rounded-sm p-1 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            title="Rename entry"
            aria-label="Rename entry"
          >
            <Pencil className="h-3.5 w-3.5" strokeWidth={1.75} />
          </button>
          <button
            type="button"
            onClick={onTogglePinned}
            className="rounded-sm p-1 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            title={isPinned ? 'Unpin entry' : 'Pin entry'}
            aria-label={isPinned ? 'Unpin entry' : 'Pin entry'}
          >
            {isPinned ? (
              <PinOff className="h-3.5 w-3.5" strokeWidth={1.75} />
            ) : (
              <Pin className="h-3.5 w-3.5" strokeWidth={1.75} />
            )}
          </button>
          <button
            type="button"
            onClick={onDelete}
            className="rounded-sm p-1 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--danger-muted))] hover:text-[hsl(var(--danger-fg))] transition-colors duration-fast"
            title="Delete entry"
            aria-label="Delete entry"
          >
            <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
          </button>
        </div>
      )}
    </div>
  );
}
