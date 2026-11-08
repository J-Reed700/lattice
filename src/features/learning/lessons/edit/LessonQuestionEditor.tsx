import { useEffect, useId, useState } from 'react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogTitle } from '@/components/ui/dialog';
import { useApi } from '@/contexts/ApiContext';
import { useEditLearningLessonQuestion, useLearningLessonQuestion } from '@/features/learning/lessons/review/useLessonClaimReview';
import type { LessonQuestionDto } from '@/lib/bindings';

const DRY_RUN_DELAY_MS = 400;
const MIN_CHOICES = 2;
const MAX_CHOICES = 8;

const NO_CHANGES = 'No changes yet.';
const NEEDS_PROMPT = 'A question needs a prompt.';
const NEEDS_CHOICE_TEXT = 'Each choice needs some text.';
const NEEDS_RIGHT = 'Choose the right answer.';
const NOT_CHECKED = 'What you write is marked as written by you, not checked';

/** What saving does, from the dry run's count of checked statements it would change. */
function savingLine(checkedAffected: number): string {
  if (checkedAffected <= 0) return `${NOT_CHECKED} against the sources.`;
  const statements = checkedAffected === 1 ? '1 statement that was' : `${checkedAffected} statements that were`;
  return `Saving changes ${statements} checked against the sources. ${NOT_CHECKED}.`;
}

const sameQuestion = (a: LessonQuestionDto, b: LessonQuestionDto) =>
  a.prompt === b.prompt && a.explanation === b.explanation && a.right === b.right
  && a.choices.length === b.choices.length && a.choices.every((choice, index) => choice === b.choices[index]);

type Props = {
  programId: string;
  lessonId: string;
  /** Zero-based, among the lesson's questions. */
  questionIndex: number;
  open: boolean;
  onClose: () => void;
  /** Called after a save, before the editor closes. */
  onSaved?: () => void;
};

function EditorBody({ programId, lessonId, questionIndex, opened, onClose, onSaved }: Omit<Props, 'open'> & { opened: LessonQuestionDto }) {
  const api = useApi();
  const ids = useId();
  const save = useEditLearningLessonQuestion(programId, lessonId);
  const [draft, setDraft] = useState<LessonQuestionDto>(opened);
  const [asking, setAsking] = useState(false);
  const [checked, setChecked] = useState<number | null>(null);
  const [dryError, setDryError] = useState<string | null>(null);

  const hasChoices = opened.kind === 'single_choice';
  const trueFalse = opened.kind === 'true_false';
  const changed = !sameQuestion(draft, opened);
  const blankPrompt = draft.prompt.trim() === '';
  const blankChoice = hasChoices && draft.choices.some((choice) => choice.trim() === '');
  const noRight = (hasChoices || trueFalse) && draft.right === null;
  const invalid = blankPrompt || blankChoice || noRight;
  const saving = save.isPending;

  useEffect(() => {
    if (!changed || invalid) return;
    let current = true;
    const timer = setTimeout(() => {
      void (async () => {
        const result = await api.editLearningLessonQuestion({
          programId, lessonId, questionIndex, before: opened, after: draft, dryRun: true,
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
  }, [api, programId, lessonId, questionIndex, opened, draft, changed, invalid]);

  const requestClose = () => {
    if (saving) return;
    if (changed) setAsking(true);
    else onClose();
  };

  const submit = async () => {
    try {
      await save.mutateAsync({ questionIndex, before: opened, after: draft });
    } catch {
      return;
    }
    onSaved?.();
    onClose();
  };

  const setChoice = (index: number, text: string) =>
    setDraft((value) => ({ ...value, choices: value.choices.map((choice, at) => (at === index ? text : choice)) }));
  const removeChoice = (index: number) =>
    setDraft((value) => ({
      ...value,
      choices: value.choices.filter((_, at) => at !== index),
      right: value.right === null || value.right === index ? null : value.right > index ? value.right - 1 : value.right,
    }));

  const line = blankPrompt ? NEEDS_PROMPT
    : blankChoice ? NEEDS_CHOICE_TEXT
    : noRight ? NEEDS_RIGHT
    : !changed ? NO_CHANGES
    : dryError ?? (checked === null ? '' : savingLine(checked));

  const field = 'mt-1 w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-text-primary';

  return (
    <Dialog open onOpenChange={(open) => { if (!open) requestClose(); }}>
      <DialogContent hideClose aria-describedby={undefined}
        className="max-h-[90vh] max-w-3xl overflow-y-auto rounded-2xl border border-border bg-surface p-5 shadow-sheet">
        <DialogTitle className="font-serif text-xl text-text-primary">Edit question {questionIndex + 1}</DialogTitle>
        <div>
          <label htmlFor={`${ids}-prompt`} className="block text-xs font-medium text-text-secondary">Question</label>
          <textarea id={`${ids}-prompt`} value={draft.prompt} disabled={saving} rows={3}
            onChange={(event) => setDraft((value) => ({ ...value, prompt: event.target.value }))} className={field} />
        </div>
        {hasChoices && (
          <div className="space-y-2">
            {draft.choices.map((choice, index) => (
              <div key={index} className="flex items-center gap-2">
                <input type="radio" name={`${ids}-right`} aria-label="Right answer" checked={draft.right === index} disabled={saving}
                  onChange={() => setDraft((value) => ({ ...value, right: index }))} />
                <label htmlFor={`${ids}-choice-${index}`} className="sr-only">Choice {index + 1}</label>
                <input id={`${ids}-choice-${index}`} value={choice} disabled={saving} onChange={(event) => setChoice(index, event.target.value)}
                  className="w-full rounded-lg border border-border bg-background px-3 py-2 text-sm text-text-primary" />
                <Button type="button" variant="ghost" size="sm" disabled={saving || draft.choices.length <= MIN_CHOICES}
                  onClick={() => removeChoice(index)}>Remove</Button>
              </div>
            ))}
            <Button type="button" variant="ghost" size="sm" disabled={saving || draft.choices.length >= MAX_CHOICES}
              onClick={() => setDraft((value) => ({ ...value, choices: [...value.choices, ''] }))}>Add a choice</Button>
          </div>
        )}
        {trueFalse && (
          <div role="radiogroup" aria-label="Right answer" className="flex gap-4">
            {['True', 'False'].map((label, index) => (
              <label key={label} className="flex items-center gap-2 text-sm text-text-primary">
                <input type="radio" name={`${ids}-truth`} checked={draft.right === index} disabled={saving}
                  onChange={() => setDraft((value) => ({ ...value, right: index }))} />
                {label}
              </label>
            ))}
          </div>
        )}
        <div>
          <label htmlFor={`${ids}-explanation`} className="block text-xs font-medium text-text-secondary">Explanation</label>
          <textarea id={`${ids}-explanation`} value={draft.explanation} disabled={saving} rows={3}
            onChange={(event) => setDraft((value) => ({ ...value, explanation: event.target.value }))} className={field} />
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
            <Button type="button" disabled={saving || !changed || invalid} onClick={() => void submit()}>{saving ? 'Saving…' : 'Save'}</Button>
          </div>
        )}
        <p className="text-xs text-text-muted">Saved changes can&apos;t be undone. You can edit the question again at any time.</p>
      </DialogContent>
    </Dialog>
  );
}

function Loading({ questionIndex, error, onClose }: { questionIndex: number; error: string | null; onClose: () => void }) {
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose(); }}>
      <DialogContent hideClose aria-describedby={undefined} className="max-w-3xl rounded-2xl border border-border bg-surface p-5 shadow-sheet">
        <DialogTitle className="font-serif text-xl text-text-primary">Edit question {questionIndex + 1}</DialogTitle>
        {error ? <p role="alert" className="text-sm text-danger-fg">{error}</p> : <p role="status" className="text-sm text-text-secondary">Loading…</p>}
        <div className="flex justify-end"><Button type="button" variant="ghost" onClick={onClose}>Close</Button></div>
      </DialogContent>
    </Dialog>
  );
}

function QuestionEditorLoader(props: Props) {
  const { programId, lessonId, questionIndex, onClose } = props;
  const question = useLearningLessonQuestion(programId, lessonId, questionIndex);
  if (!question.data) return <Loading questionIndex={questionIndex} error={question.error?.message ?? null} onClose={onClose} />;
  return <EditorBody programId={programId} lessonId={lessonId} questionIndex={questionIndex} opened={question.data} onClose={onClose} onSaved={props.onSaved} />;
}

/**
 * Edits one question of a published lesson: its prompt, choices, right answer
 * and explanation. It shows from a dry run what saving does to the statements
 * the review holds, asks before throwing changes away, and says what the
 * server refused without losing the text.
 */
export function LessonQuestionEditor(props: Props) {
  return props.open ? <QuestionEditorLoader key={`${props.lessonId}:${props.questionIndex}`} {...props} /> : null;
}
