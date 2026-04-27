import { format } from 'date-fns';

import { EntryAutosaveIndicator } from './EntryAutosaveIndicator';

interface EntryHeaderProps {
  date: Date;
  journalName: string;
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
 * Editor pane header: large serif date title, year · journal name subline,
 * and metadata row (word count · edited · autosave state).
 * Spec §5.2.
 */
export function EntryHeader({
  date,
  journalName,
  wordCount,
  lastEditedAt,
  hasPendingChanges,
  isSaving,
  saveError,
  onRetrySave,
}: EntryHeaderProps) {
  const dateTitle = format(date, 'EEEE, MMMM d');
  const year = format(date, 'yyyy');
  const formattedWordCount = new Intl.NumberFormat().format(wordCount);

  return (
    <header className="mb-8">
      <h1 className="font-serif text-3xl font-semibold tracking-[-0.02em] text-[hsl(var(--text-primary))] mb-1">
        {dateTitle}
      </h1>
      <p className="text-sm text-[hsl(var(--text-tertiary))] mb-2">
        {year} <span aria-hidden="true">·</span> {journalName}
      </p>
      <div className="flex items-center gap-2 text-xs text-[hsl(var(--text-muted))]">
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
