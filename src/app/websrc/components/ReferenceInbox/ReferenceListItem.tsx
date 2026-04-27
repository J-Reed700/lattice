import { useState } from 'react';

import { format, isToday, isYesterday } from 'date-fns';
import { motion, useReducedMotion } from 'framer-motion';
import { ArrowUpRight, Check, Copy, Trash2 } from 'lucide-react';

import type { ConversationMessageBookmarkDto } from '@/types';

interface ReferenceListItemProps {
  bookmark: ConversationMessageBookmarkDto;
  isActive: boolean;
  isCaptured: boolean;
  isJournalOrigin: boolean;
  onSelect: () => void;
  onOpenInOrigin: () => void;
  onCopy: () => void;
  onDelete: () => void;
}

function formatDatePrefix(iso: string, origin: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return origin;
  if (isToday(date)) return `${origin} · TODAY`;
  if (isYesterday(date)) return `${origin} · YESTERDAY`;
  return `${origin} · ${format(date, 'EEE · MMM d').toUpperCase()}`;
}

function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

/**
 * Single reference row with origin+date prefix, title, preview line, time,
 * captured dot, hover actions, active state with layoutId bar.
 * Spec §4.2.
 */
export function ReferenceListItem({
  bookmark,
  isActive,
  isCaptured,
  isJournalOrigin,
  onSelect,
  onOpenInOrigin,
  onCopy,
  onDelete,
}: ReferenceListItemProps) {
  const prefersReducedMotion = useReducedMotion();
  const [isHovering, setIsHovering] = useState(false);

  const originLabel = isJournalOrigin ? 'JOURNAL' : 'CHAT';
  const displayTitle =
    bookmark.title?.trim() || bookmark.conversationTitle?.trim() || 'Untitled reference';
  const hasTitle = Boolean(bookmark.title?.trim() || bookmark.conversationTitle?.trim());

  return (
    <div
      role="button"
      tabIndex={0}
      aria-current={isActive ? 'page' : undefined}
      onMouseEnter={() => setIsHovering(true)}
      onMouseLeave={() => setIsHovering(false)}
      onClick={onSelect}
      onKeyDown={(e) => {
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
            layoutId="references-sidebar-active-bar"
            className="absolute inset-y-0 left-0 w-0.5 bg-[hsl(var(--accent))]"
            aria-hidden="true"
            transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          />
        ))}

      <p className="mb-1 text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
        {formatDatePrefix(bookmark.createdAt, originLabel)}
      </p>
      <div className="flex items-baseline justify-between gap-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5">
          {isCaptured && (
            <Check
              className="h-3 w-3 shrink-0 text-[hsl(var(--accent))]"
              strokeWidth={2}
              aria-label="Captured"
            />
          )}
          <span
            className={`min-w-0 flex-1 truncate text-sm ${
              hasTitle
                ? isActive
                  ? 'font-medium text-[hsl(var(--text-primary))]'
                  : 'text-[hsl(var(--text-primary))]'
                : 'italic text-[hsl(var(--text-muted))]'
            }`}
          >
            {displayTitle}
          </span>
        </div>
        <time className="shrink-0 text-xxs text-[hsl(var(--text-muted))]">
          {formatTime(bookmark.createdAt)}
        </time>
      </div>
      {bookmark.messagePreview && (
        <p className="mt-0.5 truncate text-xs text-[hsl(var(--text-tertiary))]">
          {bookmark.messagePreview}
        </p>
      )}

      <div
        className={`absolute right-2 top-2 flex items-center gap-0.5 transition-opacity duration-fast ${
          isHovering ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          onClick={onOpenInOrigin}
          className="rounded-sm p-1 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
          title="Open in origin"
          aria-label="Open in origin"
        >
          <ArrowUpRight className="h-3.5 w-3.5" strokeWidth={1.75} />
        </button>
        <button
          type="button"
          onClick={onCopy}
          className="rounded-sm p-1 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--surface-raised))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
          title="Copy reference"
          aria-label="Copy reference"
        >
          <Copy className="h-3.5 w-3.5" strokeWidth={1.75} />
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="rounded-sm p-1 text-[hsl(var(--text-tertiary))] hover:bg-[hsl(var(--danger-muted))] hover:text-[hsl(var(--danger-fg))] transition-colors duration-fast"
          title="Delete reference"
          aria-label="Delete reference"
        >
          <Trash2 className="h-3.5 w-3.5" strokeWidth={1.75} />
        </button>
      </div>
    </div>
  );
}
