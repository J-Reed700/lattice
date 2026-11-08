import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { useApi } from '@/contexts/ApiContext';
import { useEditLearningLessonSection } from '@/features/learning/lessons/review/useLessonClaimReview';
import { MarkdownViewer } from '@/features/reading/components/viewers/MarkdownViewer';

const DRY_RUN_DELAY_MS = 400;

const NO_CHANGES = 'No changes yet.';
const NEEDS_TITLE = 'A section needs a title.';
const NEEDS_TEXT = 'A section needs some text.';
const NOT_CHECKED = 'What you write is marked as written by you, not checked';

/** What saving does, from the dry run's count of checked statements it would change. */
function savingLine(checkedAffected: number): string {
  if (checkedAffected <= 0) return `${NOT_CHECKED} against the sources.`;
  const statements = checkedAffected === 1 ? '1 statement that was' : `${checkedAffected} statements that were`;
  return `Saving changes ${statements} checked against the sources. ${NOT_CHECKED}.`;
}

type Props = {
  programId: string;
  lessonId: string;
  /** Zero-based. */
  sectionIndex: number;
  /** The section's title and text as they were when the editor opened; the save sends them back as the text it started from. */
  title: string;
  body: string;
  open: boolean;
  onClose: () => void;
  /** Opens with the first occurrence of this text selected, so the review lands on a sentence. */
  focusText?: string;
  /** Called after a save, before the editor closes. */
  onSaved?: () => void;
};

function EditorBody({ programId, lessonId, sectionIndex, title: titleProp, body: bodyProp, onClose, focusText, onSaved }: Props) {
  // The text the editor opened with stays what the save is measured against, even if the course reloads under it.
  const [opened] = useState({ title: titleProp, body: bodyProp });
  const { title: openedTitle, body: openedBody } = opened;
  const api = useApi();
  const ids = useId();
  const save = useEditLearningLessonSection(programId, lessonId);
  const [title, setTitle] = useState(openedTitle);
  const [body, setBody] = useState(openedBody);
  const [preview, setPreview] = useState(false);
  const [asking, setAsking] = useState(false);
  const [checked, setChecked] = useState<number | null>(null);
  const [dryError, setDryError] = useState<string | null>(null);
  const textRef = useRef<HTMLTextAreaElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);

  const changed = title !== openedTitle || body !== openedBody;
  const blankTitle = title.trim() === '';
  const blankBody = body.trim() === '';
  const saving = save.isPending;

  useEffect(() => {
    if (!changed || blankTitle || blankBody) return;
    let current = true;
    const timer = setTimeout(() => {
      void (async () => {
        const result = await api.editLearningLessonSection({
          programId, lessonId, sectionIndex, beforeTitle: openedTitle, beforeBody: openedBody, title, body, dryRun: true,
        });
        if (!current) return;
        if (result.ok) {
          setChecked(result.data.checkedStatementsAffected);
          setDryError(null);
        } else {
          setDryError(result.error);
        }
      })();
    }, DRY_RUN_DELAY_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [api, programId, lessonId, sectionIndex, openedTitle, openedBody, title, body, changed, blankTitle, blankBody]);

  useLayoutEffect(() => {
    const box = textRef.current;
    if (!box || preview) return;
    box.style.height = 'auto';
    box.style.height = `${box.scrollHeight}px`;
  }, [body, preview]);

  const requestClose = () => {
    if (saving) return;
    if (changed) setAsking(true);
    else onClose();
  };

  const submit = async () => {
    try {
      await save.mutateAsync({ sectionIndex, beforeTitle: openedTitle, beforeBody: openedBody, title, body });
    } catch {
      return;
    }
    onSaved?.();
    onClose();
  };

  const line = blankTitle ? NEEDS_TITLE
    : blankBody ? NEEDS_TEXT
    : !changed ? NO_CHANGES
    : dryError ?? (checked === null ? '' : savingLine(checked));

  const landOnSentence = (event: Event) => {
    const box = textRef.current;
    const at = focusText ? openedBody.indexOf(focusText) : -1;
    if (!box || !focusText || at < 0) return;
    event.preventDefault();
    box.focus({ preventScroll: true });
    box.setSelectionRange(at, at + focusText.length);
    const scroller = contentRef.current;
    if (scroller && openedBody.length > 0) {
      scroller.scrollTop = Math.max(0, box.offsetTop + (at / openedBody.length) * box.offsetHeight - scroller.clientHeight / 3);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => { if (!open) requestClose(); }}>
      <DialogContent ref={contentRef} hideClose aria-describedby={undefined} onOpenAutoFocus={landOnSentence}
        className="max-h-[90vh] max-w-3xl overflow-y-auto rounded-2xl border border-border bg-surface p-5 shadow-sheet">
        <DialogTitle className="font-serif text-xl text-text-primary">Edit section {sectionIndex + 1}</DialogTitle>
        <div>
          <label htmlFor={`${ids}-title`} className="block text-xs font-medium text-text-secondary">Title</label>
          <input id={`${ids}-title`} value={title} disabled={saving} onChange={(event) => setTitle(event.target.value)}
            className="mt-1 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-text-primary" />
        </div>
        <div>
          <div className="flex items-center justify-between gap-2">
            <label htmlFor={`${ids}-text`} className="block text-xs font-medium text-text-secondary">Text</label>
            <Button type="button" size="sm" variant="ghost" onClick={() => setPreview((value) => !value)}>{preview ? 'Back to editing' : 'Preview'}</Button>
          </div>
          <textarea id={`${ids}-text`} ref={textRef} hidden={preview} value={body} disabled={saving} onChange={(event) => setBody(event.target.value)} rows={6}
            className="mt-1 block w-full resize-none overflow-hidden rounded-lg border border-border bg-background px-3 py-2 font-mono text-sm leading-6 text-text-primary" />
          {preview && <div className="studio-lesson-prose mt-1 rounded-lg border border-border bg-background px-3 py-2"><MarkdownViewer content={body} /></div>}
        </div>
        <p aria-live="polite" className="min-h-5 text-sm leading-6 text-text-secondary">{line}</p>
        {save.error && <p role="alert" className="text-sm text-danger-fg">{save.error.message}</p>}
        {asking ? (
          <div role="group" aria-label="Discard your changes?" className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-sm font-medium text-text-primary">Discard your changes?</p>
            <div className="flex gap-2">
              <Button type="button" variant="ghost" onClick={() => setAsking(false)}>Keep editing</Button>
              <Button type="button" variant="destructive" onClick={onClose}>Discard</Button>
            </div>
          </div>
        ) : (
          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" disabled={saving} onClick={requestClose}>Cancel</Button>
            <Button type="button" disabled={saving || !changed || blankTitle || blankBody} onClick={() => void submit()}>{saving ? 'Saving…' : 'Save'}</Button>
          </div>
        )}
        <p className="text-xs text-text-muted">Saved changes can&apos;t be undone. You can edit the section again at any time.</p>
      </DialogContent>
    </Dialog>
  );
}

/**
 * Edits one section of a published lesson, its title and its Markdown text.
 * It shows from a dry run what saving does to the statements the review
 * holds, asks before throwing changes away, and says what the server refused
 * without losing the text.
 */
export function LessonSectionEditor(props: Props) {
  return props.open ? <EditorBody key={`${props.lessonId}:${props.sectionIndex}`} {...props} /> : null;
}
