import { type ReactNode, useEffect, useRef, useState } from 'react';

import { format } from 'date-fns';

import { EntryAutosaveIndicator } from './EntryAutosaveIndicator';

interface EntryHeaderProps {
  date: Date;
  journalName: string;
  /** The page's own title, when it has one beyond this journal's default. */
  pageTitle?: string | null;
  /** Identity of the page, so a draft title never leaks onto the next page. */
  pageId: string;
  /** Put the caret in the title: a page that was just made wants a name. */
  autoFocusTitle?: boolean;
  onRename: (title: string) => void;
  wordCount: number;
  lastEditedAt: string;
  hasPendingChanges: boolean;
  isSaving: boolean;
  saveError: string | null;
  onRetrySave: () => void;
  /** Controls that belong to the page as a whole, e.g. the side-panel toggle. */
  actions?: ReactNode;
}

function formatRelativeEdit(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return 'edited just now';
  const now = Date.now();
  const diffMs = now - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  if (diffMin < 1) return 'edited just now';
  if (diffMin < 60) return `edited ${diffMin}m ago`;
  const diffHours = Math.floor(diffMin / 60);
  if (diffHours < 24) return `edited ${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) return `edited ${diffDays}d ago`;
  return `edited ${format(date, 'MMM d')}`;
}

/**
 * Page header: one quiet line of facts, then the title — which is the page's
 * name and is edited right here. Every page is the same kind of thing, so every
 * page gets the same header; an unnamed one simply reads "Untitled page".
 * Spec §5.2.
 */
export function EntryHeader({
  date,
  journalName,
  pageTitle = null,
  pageId,
  autoFocusTitle = false,
  onRename,
  wordCount,
  lastEditedAt,
  hasPendingChanges,
  isSaving,
  saveError,
  onRetrySave,
  actions,
}: EntryHeaderProps) {
  const named = pageTitle?.trim() ?? '';
  // The journal's name already heads the index beside this page.
  const subline = format(date, 'EEEE, MMM d, yyyy');
  const formattedWordCount = new Intl.NumberFormat().format(wordCount);

  const [draft, setDraft] = useState(named);
  const titleRef = useRef<HTMLTextAreaElement | null>(null);

  // Another page, or a rename made elsewhere (the list): take its title.
  const [seen, setSeen] = useState({ pageId, named });
  if (seen.pageId !== pageId || seen.named !== named) {
    setSeen({ pageId, named });
    setDraft(named);
  }

  // A title can run to two or three lines; the field grows to hold it.
  useEffect(() => {
    const field = titleRef.current;
    if (!field) return;
    field.style.height = 'auto';
    field.style.height = `${field.scrollHeight}px`;
  }, [draft, pageId]);

  useEffect(() => {
    if (!autoFocusTitle) return;
    titleRef.current?.focus();
    titleRef.current?.select();
  }, [autoFocusTitle, pageId]);

  const commit = () => {
    const next = draft.trim();
    if (next === named) return;
    if (!next) {
      setDraft(named);
      return;
    }
    onRename(next);
  };

  return (
    <header className="journal-page-header mb-8">
      {/* One quiet line of facts; the title gets the rest of the room. */}
      <div className="mb-6 flex min-h-7 items-center justify-between gap-3 text-xs text-text-muted">
        <span className="min-w-0 truncate" title={journalName}>{subline}</span>
        <div className="flex shrink-0 items-center gap-2" title={formatRelativeEdit(lastEditedAt)}>
          <span className="tabular-nums">
            {formattedWordCount} word{wordCount === 1 ? '' : 's'}
          </span>
          <span aria-hidden="true">·</span>
          <EntryAutosaveIndicator
            hasPendingChanges={hasPendingChanges}
            isSaving={isSaving}
            saveError={saveError}
            onRetry={onRetrySave}
          />
          {actions ? <span className="-mr-1.5 ml-1 flex items-center">{actions}</span> : null}
        </div>
      </div>
      <h1 className="m-0">
        <textarea
          ref={titleRef}
          rows={1}
          value={draft}
          onChange={(event) => setDraft(event.target.value.replace(/\n/g, ' '))}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              event.currentTarget.blur();
            } else if (event.key === 'Escape') {
              event.preventDefault();
              setDraft(named);
              event.currentTarget.blur();
            }
          }}
          maxLength={160}
          placeholder="Untitled page"
          aria-label="Page title"
          spellCheck={false}
          className="-mx-1.5 block w-[calc(100%+12px)] resize-none overflow-hidden rounded-md bg-transparent px-1.5 font-serif text-[clamp(30px,3vw,40px)] font-normal leading-[1.12] tracking-[-0.03em] text-[hsl(var(--text-primary))] outline-none transition-colors duration-fast placeholder:text-[hsl(var(--text-disabled))] hover:bg-[hsl(var(--text-primary)/0.035)] focus:bg-[hsl(var(--text-primary)/0.035)]"
        />
      </h1>
    </header>
  );
}
