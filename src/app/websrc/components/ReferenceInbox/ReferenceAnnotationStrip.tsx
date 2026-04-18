import { useEffect, useState } from 'react';

import type { ConversationMessageBookmarkDto } from '@/types';

type FieldState = 'idle' | 'saving' | 'error';

interface ReferenceAnnotationStripProps {
  bookmark: ConversationMessageBookmarkDto;
  onSave: (next: {
    title: string | null;
    note: string | null;
  }) => Promise<boolean>;
}

/**
 * Quiet inline-edit strip for reference title + note. Blur-to-save;
 * Enter commits title, ⌘Enter commits note. No Save button — save state
 * is a single quiet word per row.
 * Spec §5.5.
 */
export function ReferenceAnnotationStrip({
  bookmark,
  onSave,
}: ReferenceAnnotationStripProps) {
  const [titleDraft, setTitleDraft] = useState(bookmark.title ?? '');
  const [noteDraft, setNoteDraft] = useState(bookmark.note ?? '');
  const [editingTitle, setEditingTitle] = useState(false);
  const [editingNote, setEditingNote] = useState(false);
  const [titleState, setTitleState] = useState<FieldState>('idle');
  const [noteState, setNoteState] = useState<FieldState>('idle');

  // Reset drafts when switching references or when server values change.
  useEffect(() => {
    setTitleDraft(bookmark.title ?? '');
    setNoteDraft(bookmark.note ?? '');
    setEditingTitle(false);
    setEditingNote(false);
    setTitleState('idle');
    setNoteState('idle');
  }, [bookmark.id, bookmark.title, bookmark.note]);

  const commitTitle = async () => {
    setEditingTitle(false);
    const next = titleDraft.trim() ? titleDraft.trim() : null;
    const current = bookmark.title ?? null;
    if (next === current) {
      setTitleState('idle');
      return;
    }
    setTitleState('saving');
    const ok = await onSave({ title: next, note: bookmark.note ?? null });
    setTitleState(ok ? 'idle' : 'error');
  };

  const commitNote = async () => {
    setEditingNote(false);
    const next = noteDraft.trim() ? noteDraft.trim() : null;
    const current = bookmark.note ?? null;
    if (next === current) {
      setNoteState('idle');
      return;
    }
    setNoteState('saving');
    const ok = await onSave({ title: bookmark.title ?? null, note: next });
    setNoteState(ok ? 'idle' : 'error');
  };

  const retryTitle = () => {
    setEditingTitle(true);
  };

  const retryNote = () => {
    setEditingNote(true);
  };

  return (
    <div className="mt-8 border-t border-[hsl(var(--border-subtle))] pt-6">
      {/* Title row */}
      <div className="flex items-start gap-4">
        <label className="mt-1 w-12 shrink-0 text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
          Title
        </label>
        <div className="min-w-0 flex-1">
          {editingTitle ? (
            <input
              autoFocus
              value={titleDraft}
              onChange={(e) => setTitleDraft(e.target.value)}
              onBlur={() => void commitTitle()}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  void commitTitle();
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  setTitleDraft(bookmark.title ?? '');
                  setEditingTitle(false);
                }
              }}
              maxLength={200}
              className="w-full rounded-sm border border-[hsl(var(--border-default))] bg-[hsl(var(--surface))] px-2 py-1 text-base text-[hsl(var(--text-primary))] outline-none focus:border-[hsl(var(--accent))]"
            />
          ) : (
            <button
              type="button"
              onClick={() => {
                setTitleDraft(bookmark.title ?? '');
                setEditingTitle(true);
              }}
              className="block w-full cursor-text text-left text-base text-[hsl(var(--text-primary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            >
              {bookmark.title?.trim() ? (
                bookmark.title
              ) : (
                <span className="italic text-[hsl(var(--text-muted))]">
                  Add a title…
                </span>
              )}
            </button>
          )}
        </div>
        <FieldStateLabel state={titleState} onRetry={retryTitle} />
      </div>

      {/* Note row */}
      <div className="mt-4 flex items-start gap-4">
        <label className="mt-1 w-12 shrink-0 text-xxs font-medium uppercase tracking-[0.08em] text-[hsl(var(--text-muted))]">
          Note
        </label>
        <div className="min-w-0 flex-1">
          {editingNote ? (
            <textarea
              autoFocus
              value={noteDraft}
              onChange={(e) => setNoteDraft(e.target.value)}
              onBlur={() => void commitNote()}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                  e.preventDefault();
                  void commitNote();
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  setNoteDraft(bookmark.note ?? '');
                  setEditingNote(false);
                }
              }}
              rows={3}
              maxLength={2000}
              className="w-full resize-y rounded-sm border border-[hsl(var(--border-default))] bg-[hsl(var(--surface))] px-2 py-1 text-sm text-[hsl(var(--text-secondary))] outline-none focus:border-[hsl(var(--accent))]"
            />
          ) : (
            <button
              type="button"
              onClick={() => {
                setNoteDraft(bookmark.note ?? '');
                setEditingNote(true);
              }}
              className="block w-full cursor-text whitespace-pre-wrap text-left text-sm text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast"
            >
              {bookmark.note?.trim() ? (
                bookmark.note
              ) : (
                <span className="italic text-[hsl(var(--text-muted))]">
                  Add context or a takeaway…
                </span>
              )}
            </button>
          )}
        </div>
        <FieldStateLabel state={noteState} onRetry={retryNote} />
      </div>
    </div>
  );
}

function FieldStateLabel({
  state,
  onRetry,
}: {
  state: FieldState;
  onRetry: () => void;
}) {
  if (state === 'saving') {
    return (
      <span className="mt-1 shrink-0 text-xs text-[hsl(var(--text-muted))]">
        Saving…
      </span>
    );
  }
  if (state === 'error') {
    return (
      <button
        type="button"
        onClick={onRetry}
        className="mt-1 shrink-0 text-xs text-[hsl(var(--danger-fg))] underline-offset-2 hover:underline"
      >
        Save failed — retry?
      </button>
    );
  }
  return null;
}
