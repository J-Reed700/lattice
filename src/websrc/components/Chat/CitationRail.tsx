import { Bookmark, ChevronDown, ChevronUp, MessageSquare, NotebookPen } from 'lucide-react';

import { IconButton } from '@/components/ui';
import type { PassageLocator, SourceWithMetadata } from '@/types/conversation';
import { renderHighlightedText } from '@/utils/sourcePreview';

import { formatSourceLocation } from '../Reading/passageLocator';

/**
 * The pinned excerpt beside an open file (BRIEF rank 1).
 *
 * Keeps the cited sentence visible while the reader looks at the document
 * around it, and offers the same three capture verbs as the selection toolbar,
 * acting on the excerpt itself.
 *
 * How confidently the passage was found is *not* said here: this rail is
 * `lg:flex`, and below 1024px it would take that admission with it. The
 * match-tier line lives in the viewer column, which always renders.
 */

export interface CitationRailProps {
  source: SourceWithMetadata;
  locator: PassageLocator;
  resolvedLabel?: string;
  index?: number;
  total?: number;
  onPrevious?: () => void;
  onNext?: () => void;
  onReference: () => void | Promise<void>;
  onAddToJournal: () => void | Promise<void>;
  onAskAbout: () => void;
}

const VERB_CLASS =
  'inline-flex items-center gap-2 text-left text-xs text-[hsl(var(--text-secondary))] transition-colors duration-fast hover:text-[hsl(var(--accent))]';

export function CitationRail({
  source,
  locator,
  resolvedLabel,
  index,
  total,
  onPrevious,
  onNext,
  onReference,
  onAddToJournal,
  onAskAbout,
}: CitationRailProps) {
  const showNav = typeof total === 'number' && total > 1 && typeof index === 'number';
  const locationLabel = resolvedLabel ?? formatSourceLocation(source) ?? 'Location unknown';
  const excerpt = locator.text?.trim() ?? '';

  return (
    <aside className="hidden w-[320px] shrink-0 flex-col gap-4 overflow-y-auto border-l border-subtle bg-surface px-5 py-6 lg:flex">
      {showNav && (
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs tabular-nums text-[hsl(var(--text-muted))]">
            Citation {index + 1} of {total}
          </span>
          <div className="flex items-center gap-1">
            <IconButton
              label="Previous citation"
              shortcut="["
              onClick={onPrevious}
              disabled={index === 0}
            >
              <ChevronUp />
            </IconButton>
            <IconButton
              label="Next citation"
              shortcut="]"
              onClick={onNext}
              disabled={index === total - 1}
            >
              <ChevronDown />
            </IconButton>
          </div>
        </div>
      )}

      <p className="text-xs text-[hsl(var(--text-muted))]">{locationLabel}</p>

      {excerpt && (
        <blockquote className="border-l-2 border-border-default pl-4 font-serif text-sm leading-relaxed text-[hsl(var(--text-secondary))]">
          {renderHighlightedText(excerpt, locator.highlights)}
        </blockquote>
      )}

      <div className="flex flex-col items-start gap-3 border-t border-subtle pt-4">
        <button type="button" className={VERB_CLASS} onClick={() => void onReference()}>
          <Bookmark className="h-3 w-3" strokeWidth={1.75} />
          Reference
        </button>
        <button type="button" className={VERB_CLASS} onClick={() => void onAddToJournal()}>
          <NotebookPen className="h-3 w-3" strokeWidth={1.75} />
          Add to journal
        </button>
        <button type="button" className={VERB_CLASS} onClick={onAskAbout}>
          <MessageSquare className="h-3 w-3" strokeWidth={1.75} />
          Ask about this
        </button>
      </div>
    </aside>
  );
}
