import { NotebookPen, PanelLeft, Plus } from 'lucide-react';

import type { Notice } from '@/features/journal/hooks/useJournalNotice';

/** Shown when the journal list could not be loaded or the first journal not created. */
export function JournalLoadError({ error, onRetry }: { error: string; onRetry: () => void }) {
  return (
    <div className="flex h-full items-center justify-center bg-[hsl(var(--bg))] p-6">
      <div className="max-w-sm text-center">
        <NotebookPen className="mx-auto mb-5 h-8 w-8 text-text-tertiary" strokeWidth={1.5} />
        <h1 className="font-serif text-2xl text-text-primary">Your journal couldn’t load</h1>
        <p className="mt-3 text-sm leading-relaxed text-text-tertiary">Try connecting again to return to your pages.</p>
        <button type="button" onClick={onRetry} className="mt-6 rounded-lg bg-accent px-5 py-2.5 text-sm font-medium text-accent-fg transition-colors hover:bg-accent-hover">Try again</button>
        <details className="mt-5 text-xs text-text-tertiary">
          <summary className="cursor-pointer">Error details</summary>
          <p className="mt-2 wrap-break-word text-left">{error}</p>
        </details>
      </div>
    </div>
  );
}

/** First-run empty state (no journal in URL, auto-redirect in flight). */
export function JournalEmptyState({
  isCreatingJournal,
  onCreateJournal,
  notice,
}: {
  isCreatingJournal: boolean;
  onCreateJournal: () => void;
  notice: Notice | null;
}) {
  return (
    <div className="flex h-full items-center justify-center bg-[hsl(var(--bg))] p-6">
      <div className="flex max-w-md flex-col items-center gap-4 text-center">
        <NotebookPen
          className="h-10 w-10 text-[hsl(var(--text-muted))]"
          strokeWidth={1.5}
        />
        <h1 className="font-serif text-xl text-[hsl(var(--text-primary))]">
          Start a journal.
        </h1>
        <p className="text-sm text-[hsl(var(--text-tertiary))]">
          A journal is a notebook of your own thinking. Conversations attached to it
          become entries you can return to.
        </p>
        <button
          type="button"
          onClick={onCreateJournal}
          disabled={isCreatingJournal}
          className="inline-flex h-9 items-center gap-1.5 rounded-md bg-[hsl(var(--accent))] px-4 text-sm font-medium text-[hsl(var(--accent-fg))] hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-[hsl(var(--ring))] focus-visible:ring-offset-2 focus-visible:ring-offset-[hsl(var(--bg))]"
        >
          <Plus className="h-3.5 w-3.5" strokeWidth={1.75} />
          {isCreatingJournal ? 'Creating journal…' : 'New journal'}
        </button>
        {notice?.tone === 'error' && (
          <p role="alert" className="max-w-sm text-sm text-[hsl(var(--danger-fg))]">
            {notice.message}
          </p>
        )}
      </div>
    </div>
  );
}

/** The sidebar folded down to its expand and new-page buttons. */
export function CollapsedJournalSidebar({
  onExpand,
  onNewPage,
}: {
  onExpand: () => void;
  onNewPage: () => void;
}) {
  return (
    <aside className="flex h-full w-12 shrink-0 flex-col items-center gap-1 border-r border-[hsl(var(--border-subtle))] py-2.5">
      <button
        type="button"
        onClick={onExpand}
        className="row-hover pressable rounded-md p-2 text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))]"
        aria-label="Expand sidebar"
        title="Expand sidebar (⌘\\)"
      >
        <PanelLeft className="h-4 w-4" strokeWidth={1.75} />
      </button>
      <button
        type="button"
        onClick={onNewPage}
        className="row-hover pressable rounded-md p-2 text-[hsl(var(--text-tertiary))] hover:text-[hsl(var(--text-primary))]"
        aria-label="New page"
        title="New page (⌘N)"
      >
        <Plus className="h-4 w-4" strokeWidth={1.75} />
      </button>
    </aside>
  );
}

/** The workspace's transient status line, with its one optional action. */
export function JournalNoticeToast({ notice, onDismiss }: { notice: Notice; onDismiss: () => void }) {
  return (
    <div
      role="status"
      className={`absolute bottom-6 left-1/2 z-20 flex -translate-x-1/2 items-center gap-3 rounded-lg border px-3.5 py-2 text-xs shadow-lg animate-in fade-in-0 slide-in-from-bottom-2 duration-base ${
        notice.action ? '' : 'pointer-events-none '
      }${
        notice.tone === 'error'
          ? 'border-[hsl(var(--danger-muted))] bg-[hsl(var(--danger-muted))] text-[hsl(var(--danger-fg))]'
          : notice.tone === 'success'
            ? 'border-[hsl(var(--success-muted))] bg-[hsl(var(--success-muted))] text-[hsl(var(--success-fg))]'
            : 'border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface-raised))] text-[hsl(var(--text-secondary))]'
      }`}
    >
      <span>{notice.message}</span>
      {notice.action && (
        <button
          type="button"
          onClick={() => {
            notice.action?.run();
            onDismiss();
          }}
          className="shrink-0 font-medium underline-offset-2 hover:underline"
        >
          {notice.action.label}
        </button>
      )}
    </div>
  );
}
