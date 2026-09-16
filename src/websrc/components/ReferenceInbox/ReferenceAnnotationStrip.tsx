import { useEffect, useState } from 'react';

type FieldState = 'idle' | 'saving' | 'error';

interface ReferenceAnnotationStripProps {
  /** Identity of the annotated thing — drafts reset when it changes. */
  id: string;
  title: string | null;
  note: string | null;
  onSave: (next: {
    title: string | null;
    note: string | null;
  }) => Promise<boolean>;
}

/**
 * Quiet inline-edit strip for a reference title + note. Kind-agnostic: it
 * annotates a message bookmark or a saved passage identically. Blur-to-save;
 * Enter commits title, ⌘Enter commits note. No Save button — save state
 * is a single quiet word per row.
 * Spec §5.5.
 */
export function ReferenceAnnotationStrip({
  id,
  title,
  note,
  onSave,
}: ReferenceAnnotationStripProps) {
  const [titleDraft, setTitleDraft] = useState(title ?? '');
  const [noteDraft, setNoteDraft] = useState(note ?? '');
  const [editingTitle, setEditingTitle] = useState(false);
  const [editingNote, setEditingNote] = useState(false);
  const [titleState, setTitleState] = useState<FieldState>('idle');
  const [noteState, setNoteState] = useState<FieldState>('idle');

  // Reset drafts when switching references or when server values change.
  useEffect(() => {
    setTitleDraft(title ?? '');
    setNoteDraft(note ?? '');
    setEditingTitle(false);
    setEditingNote(false);
    setTitleState('idle');
    setNoteState('idle');
  }, [id, title, note]);

  const commitTitle = async () => {
    setEditingTitle(false);
    const next = titleDraft.trim() ? titleDraft.trim() : null;
    const current = title ?? null;
    if (next === current) {
      setTitleState('idle');
      return;
    }
    setTitleState('saving');
    const ok = await onSave({ title: next, note: note ?? null });
    setTitleState(ok ? 'idle' : 'error');
  };

  const commitNote = async () => {
    setEditingNote(false);
    const next = noteDraft.trim() ? noteDraft.trim() : null;
    const current = note ?? null;
    if (next === current) {
      setNoteState('idle');
      return;
    }
    setNoteState('saving');
    const ok = await onSave({ title: title ?? null, note: next });
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
        {/* A span, not a label: the control it would name only exists in edit
            mode, and a label with no control is a lie to a screen reader. */}
        <span className="mt-1 w-12 shrink-0 text-xs text-[hsl(var(--text-muted))]">
          Title
        </span>
        <div className="min-w-0 flex-1">
          {editingTitle ? (
            <input
              autoFocus
              aria-label="Reference title"
              value={titleDraft}
              onChange={(e) => setTitleDraft(e.target.value)}
              onBlur={() => void commitTitle()}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  void commitTitle();
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  setTitleDraft(title ?? '');
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
                setTitleDraft(title ?? '');
                setEditingTitle(true);
              }}
              className="block w-full cursor-text text-left text-base text-[hsl(var(--text-primary))] hover:text-[hsl(var(--text-primary))] transition-colors duration-fast"
            >
              {title?.trim() ? (
                title
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
        <span className="mt-1 w-12 shrink-0 text-xs text-[hsl(var(--text-muted))]">
          Note
        </span>
        <div className="min-w-0 flex-1">
          {editingNote ? (
            <textarea
              autoFocus
              aria-label="Reference note"
              value={noteDraft}
              onChange={(e) => setNoteDraft(e.target.value)}
              onBlur={() => void commitNote()}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                  e.preventDefault();
                  void commitNote();
                } else if (e.key === 'Escape') {
                  e.preventDefault();
                  setNoteDraft(note ?? '');
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
                setNoteDraft(note ?? '');
                setEditingNote(true);
              }}
              className="block w-full cursor-text whitespace-pre-wrap text-left text-sm text-[hsl(var(--text-secondary))] hover:text-[hsl(var(--text-secondary))] transition-colors duration-fast"
            >
              {note?.trim() ? (
                note
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
