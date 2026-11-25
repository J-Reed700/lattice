import { useState } from 'react';

import { ArrowUpRight, Copy, Quote, Trash2 } from 'lucide-react';

import { IconButton } from '@/components/ui/IconButton';
import { PASSAGE_ORIGIN, passagePreview, passageTitle } from '@/features/references/model/inboxItems';
import type { PassageReferenceDto } from '@/types/api/references';


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
 * "Origin · where · time" meta line, one-line preview, Option A raised active card.
 */
export function PassageListItem({
  passage,
  isActive,
  onSelect,
  onOpenSource,
  onCopy,
  onDelete,
}: PassageListItemProps) {
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
      className={`group relative w-full cursor-pointer rounded-xl px-3 py-2.5 transition-[background-color,box-shadow] duration-fast ${
        isActive
          ? 'bg-surface shadow-sm ring-1 ring-border-subtle'
          : 'hover:bg-surface/65'
      }`}
    >
      <div className="flex items-center gap-1.5">
        <Quote className={`h-3 w-3 shrink-0 ${isActive ? 'text-accent' : 'text-text-muted'}`} strokeWidth={1.75} aria-hidden="true" />
        <span
          className={`min-w-0 flex-1 truncate font-sans text-ui tracking-[-0.01em] text-text-primary${
            isActive ? ' font-semibold' : ' font-medium'
          }`}
          title={displayTitle}
        >
          {displayTitle}
        </span>
      </div>

      <p className="mt-1 truncate text-[11.5px] text-text-muted">{metaLine}</p>

      {preview && <p className="mt-0.5 truncate text-xs text-text-tertiary">{preview}</p>}

      <div
        className={`absolute right-1.5 top-1.5 flex items-center gap-0.5 rounded-lg pl-1.5 transition-opacity duration-fast ${
          isActive ? 'bg-surface/95' : 'bg-surface-raised/95'
        } ${
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
          className="hover:text-danger-fg"
          onClick={onDelete}
        >
          <Trash2 />
        </IconButton>
      </div>
    </div>
  );
}
