import { format } from 'date-fns';

import { EntryAutosaveIndicator } from './EntryAutosaveIndicator';

interface EntryHeaderProps {
  date: Date;
  journalName: string;
  /** The page's own title, when it has one beyond this journal's default. */
  pageTitle?: string | null;
  wordCount: number;
  lastEditedAt: string;
  hasPendingChanges: boolean;
  isSaving: boolean;
  saveError: string | null;
  onRetrySave: () => void;
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
 * Editor pane header: large serif title, subline, and metadata row (word count
 * · edited · autosave state).
 *
 * The title is the date for a journal's own page, and the page's name for any
 * other page — a synthesis written to "Week of Sep 1" must not open under
 * today's date.
 * Spec §5.2.
 */
export function EntryHeader({
  date,
  journalName,
  pageTitle = null,
  wordCount,
  lastEditedAt,
  hasPendingChanges,
  isSaving,
  saveError,
  onRetrySave,
}: EntryHeaderProps) {
  const dateTitle = format(date, 'EEEE, MMMM d');
  const named = pageTitle?.trim() ?? '';
  const title = named || format(date, 'MMMM');
  const subline = named
    ? `${format(date, 'MMM d, yyyy')} · ${journalName}`
    : `${format(date, 'yyyy')} · ${journalName}`;
  const formattedWordCount = new Intl.NumberFormat().format(wordCount);

  return (
    <header className="journal-page-header mb-9 border-b border-border-subtle pb-6">
      <div className="mb-8 flex items-center justify-between gap-3 text-[9px] font-medium uppercase tracking-[0.18em] text-text-tertiary">
        <span>Your journal / {journalName}</span><span className="journal-page-tag">{named ? 'Notes & ideas' : format(date, 'yyyy')}</span>
      </div>
      <div className="flex items-center gap-6">
        {!named && <span aria-hidden="true" className="journal-date-number font-serif text-[88px] leading-none tracking-[-0.07em] lg:text-[112px]">{format(date, 'dd')}</span>}
        <div className="min-w-0">
      {!named && <p className="mb-1 text-[11px] uppercase tracking-[0.2em] text-text-tertiary">{format(date, 'EEEE')}</p>}
      <h1 aria-label={named || dateTitle} className="font-serif text-[clamp(32px,3.5vw,48px)] leading-tight font-medium tracking-[-0.035em] text-[hsl(var(--text-primary))] mb-3">
        {title}
      </h1>
      <p className="text-xs text-[hsl(var(--text-tertiary))] mb-2">{subline}</p>
        </div>
      </div>
      <div className="mt-6 flex flex-wrap items-center gap-2 text-xs text-[hsl(var(--text-muted))]">
        <span>
          {formattedWordCount} word{wordCount === 1 ? '' : 's'}
        </span>
        <span aria-hidden="true">·</span>
        <span>{formatRelativeEdit(lastEditedAt)}</span>
        <span aria-hidden="true">·</span>
        <EntryAutosaveIndicator
          hasPendingChanges={hasPendingChanges}
          isSaving={isSaving}
          saveError={saveError}
          onRetry={onRetrySave}
        />
      </div>
    </header>
  );
}
