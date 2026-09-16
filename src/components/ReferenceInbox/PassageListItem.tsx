import { useState } from 'react';

import { motion, useReducedMotion } from 'framer-motion';
import { ArrowUpRight, Copy, Trash2 } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import type { PassageReferenceDto } from '@/types/api/references';

import { PASSAGE_ORIGIN, passagePreview, passageTitle } from './inboxItems';

interface PassageListItemProps {
  passage: PassageReferenceDto;
  isActive: boolean;
  onSelect: () => void;
  onOpenSource: () => void;
  onCopy: () => void;
  onDelete: () => void;
}

function formatTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

/**
 * Single saved-passage row. Same anatomy as `ReferenceListItem` — title line,
 * "Origin · where · time" meta line, one-line preview, hover actions that
 * reserve no width — differing only in what it says.
 */
export function PassageListItem({
  passage,
  isActive,
  onSelect,
  onOpenSource,
  onCopy,
  onDelete,
}: PassageListItemProps) {
  const prefersReducedMotion = useReducedMotion();
  const [isHovering, setIsHovering] = useState(false);
  const [isFocusWithin, setIsFocusWithin] = useState(false);
  // Hidden actions must not be tab stops, but a keyboard user still has to be
  // able to reach them: focusing the row reveals them, exactly like hovering.
  const showActions = isHovering || isFocusWithin;

  const displayTitle = passageTitle(passage);
  const preview = passagePreview(passage);
  // Say it once: when the row's title is already the file name, the meta line
  // does not repeat it.
  const place = passage.locator?.trim() || (displayTitle === passage.fileName ? '' : passage.fileName);
  const metaLine = [PASSAGE_ORIGIN, place, formatTime(passage.createdAt)]
    .filter(Boolean)
    .join(' · ');

  return (
    <div
      role="button"
      tabIndex={0}
      aria-current={isActive ? 'page' : undefined}
      onMouseEnter={() => setIsHovering(true)}
      onMouseLeave={() => setIsHovering(false)}
      onFocus={() => setIsFocusWithin(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
          setIsFocusWithin(false);
        }
      }}
      onClick={onSelect}
      onKeyDown={(e) => {
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
          <span className="absolute inset-y-0 left-0 w-0.5 bg-accent" aria-hidden="true" />
        ) : (
          <motion.span
            layoutId="references-sidebar-active-bar"
            className="absolute inset-y-0 left-0 w-0.5 bg-accent"
            aria-hidden="true"
            transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
          />
        ))}

      <div className="flex items-center gap-2">
        <span
          className={`min-w-0 flex-1 truncate text-sm text-text-primary${
            isActive ? ' font-medium' : ''
          }`}
          title={displayTitle}
        >
          {displayTitle}
        </span>
      </div>

      <p className="mt-0.5 truncate text-xs text-text-muted">{metaLine}</p>

      {preview && <p className="truncate text-xs text-text-tertiary">{preview}</p>}

      <div
        className={`absolute right-2 top-1.5 flex items-center gap-0.5 rounded-sm bg-surface-raised pl-2 transition-opacity duration-fast ${
          showActions ? 'opacity-100' : 'pointer-events-none opacity-0'
        }`}
        onClick={(e) => e.stopPropagation()}
      >
        <IconButton label="Open source" tabIndex={showActions ? 0 : -1} onClick={onOpenSource}>
          <ArrowUpRight />
        </IconButton>
        <IconButton label="Copy reference" tabIndex={showActions ? 0 : -1} onClick={onCopy}>
          <Copy />
        </IconButton>
        <IconButton
          label="Delete reference"
          tabIndex={showActions ? 0 : -1}
          className="hover:text-[hsl(var(--danger-fg))]"
          onClick={onDelete}
        >
          <Trash2 />
        </IconButton>
      </div>
    </div>
  );
}
