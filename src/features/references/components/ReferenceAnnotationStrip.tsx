import { useEffect, useState } from 'react';

import { Loader2, Pencil } from 'lucide-react';

import { Input, Textarea } from '@/components/ui';

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
 * Editorial inline-edit strip for a reference title + note. Kind-agnostic: it
 * annotates a message bookmark or a saved passage identically. Blur-to-save;
 * Enter commits title, ⌘Enter commits note.
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
    <div className="mt-6 space-y-3 rounded-xl border border-border-subtle bg-surface-sunken/50 p-4">
      {/* Title row */}
      <div className="flex items-start gap-4">
        {/* A span, not a label: the control it would name only exists in edit
            mode, and a label with no control is a lie to a screen reader. */}
        <span className="mt-1.5 w-12 shrink-0 text-xs font-medium text-text-muted">
          Title
        </span>
        <div className="min-w-0 flex-1">
          {editingTitle ? (
            <Input
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
              placeholder="Add a title…"
              className="text-sm font-medium"
            />
          ) : (
            <button
              type="button"
              onClick={() => {
                setTitleDraft(title ?? '');
                setEditingTitle(true);
              }}
              className="group/field flex w-full cursor-text items-center justify-between gap-2 rounded-lg border border-transparent px-2.5 py-1.5 text-left text-sm font-medium text-text-primary transition-[background-color,border-color] duration-fast hover:border-border-default hover:bg-surface focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            >
              <span className="min-w-0 flex-1 truncate">
                {title?.trim() ? (
                  title
                ) : (
                  <span className="font-normal italic text-text-muted">
                    Add a title…
                  </span>
                )}
              </span>
              <Pencil
                className="h-3.5 w-3.5 shrink-0 text-text-muted opacity-0 transition-opacity duration-fast group-hover/field:opacity-100 group-focus-visible/field:opacity-100"
                strokeWidth={1.75}
                aria-hidden="true"
              />
            </button>
          )}
        </div>
        <FieldStateLabel state={titleState} onRetry={retryTitle} />
      </div>

      <div className="h-px bg-border-subtle" aria-hidden="true" />

      {/* Note row */}
      <div className="flex items-start gap-4">
        <span className="mt-1.5 w-12 shrink-0 text-xs font-medium text-text-muted">
          Note
        </span>
        <div className="min-w-0 flex-1">
          {editingNote ? (
            <Textarea
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
              placeholder="Add context or a takeaway…"
              className="min-h-[76px] text-sm text-text-primary"
            />
          ) : (
            <button
              type="button"
              onClick={() => {
                setNoteDraft(note ?? '');
                setEditingNote(true);
              }}
              className="group/field flex w-full cursor-text items-start justify-between gap-2 rounded-lg border border-transparent px-2.5 py-1.5 text-left text-sm leading-relaxed text-text-secondary transition-[background-color,border-color] duration-fast hover:border-border-default hover:bg-surface focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
            >
              <span className="min-w-0 flex-1 whitespace-pre-wrap">
                {note?.trim() ? (
                  note
                ) : (
                  <span className="italic text-text-muted">
                    Add context or a takeaway…
                  </span>
                )}
              </span>
              <Pencil
                className="mt-0.5 h-3.5 w-3.5 shrink-0 text-text-muted opacity-0 transition-opacity duration-fast group-hover/field:opacity-100 group-focus-visible/field:opacity-100"
                strokeWidth={1.75}
                aria-hidden="true"
              />
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
      <span className="mt-1.5 inline-flex shrink-0 items-center gap-1.5 text-xs text-text-muted">
        <Loader2 className="h-3 w-3 animate-spin" aria-hidden="true" />
        Saving…
      </span>
    );
  }
  if (state === 'error') {
    return (
      <button
        type="button"
        onClick={onRetry}
        className="mt-1.5 shrink-0 text-xs text-danger-fg underline-offset-2 hover:underline"
      >
        Save failed — retry?
      </button>
    );
  }
  return null;
}
