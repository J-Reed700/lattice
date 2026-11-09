import { AlertCircle, CheckCircle2, Layers } from 'lucide-react';
import { useState } from 'react';

import { useSaveLearningCardDraft } from '@/features/learning/memory/useLearningMemory';
import { useLearningProgram } from '@/features/learning/workspace/useLearningStudio';
import { getErrorMessage } from '@/lib/errorUtils';

/** Offers one question and answer as a recall card draft, with the answer editable. */
export function SaveAsReviewCard({
  programId,
  front,
  back,
  sourceLabel,
  lessonId,
  onSaved,
}: {
  programId: string;
  front: string;
  back: string;
  sourceLabel?: string;
  lessonId?: string;
  onSaved?: () => void;
}) {
  const [answer, setAnswer] = useState(back);
  const save = useSaveLearningCardDraft();
  const program = useLearningProgram(lessonId ? null : programId);
  const fallback = program.data?.summary.currentLessonId ?? program.data?.modules[0]?.lessons[0]?.id ?? null;
  const target = lessonId ?? fallback;

  const submit = () => {
    if (!target) return;
    save.mutate(
      {
        draftId: null,
        programId,
        lessonId: target,
        question: front,
        answer,
        explanation: sourceLabel ?? '',
        sourceIds: [],
      },
      { onSuccess: () => onSaved?.() },
    );
  };

  return (
    <section className="space-y-3 rounded-2xl border border-border-default bg-surface p-4">
      <p className="text-sm font-medium text-text-primary">{front}</p>
      <label className="block space-y-1">
        <span className="text-xs font-medium text-text-secondary">Answer on the card</span>
        <textarea
          value={answer}
          onChange={(event) => setAnswer(event.target.value)}
          rows={3}
          className="w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm text-text-primary focus:outline-hidden focus:ring-2 focus:ring-ring"
        />
      </label>
      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          onClick={submit}
          disabled={!target || save.isPending || save.isSuccess}
          className="inline-flex items-center gap-1.5 rounded-lg bg-accent px-3 py-1.5 text-xs font-medium text-accent-fg focus:outline-hidden focus:ring-2 focus:ring-ring disabled:opacity-60"
        >
          <Layers size={14} aria-hidden="true" />
          Make a recall card
        </button>
        {save.isSuccess && (
          <p role="status" className="inline-flex items-center gap-1.5 text-xs text-success-fg">
            <CheckCircle2 size={14} aria-hidden="true" />
            Saved as a recall card draft. Review it in Memory.
          </p>
        )}
      </div>
      {!target && (
        <p className="text-xs text-text-secondary">Open a lesson in this course first; the card is filed under it.</p>
      )}
      {save.isError && (
        <div role="alert" className="flex flex-wrap items-center gap-2 text-xs text-danger-fg">
          <AlertCircle size={14} aria-hidden="true" />
          <span>{getErrorMessage(save.error)}</span>
          <button
            type="button"
            onClick={submit}
            className="font-medium underline focus:outline-hidden focus:ring-2 focus:ring-ring"
          >
            Try again
          </button>
        </div>
      )}
    </section>
  );
}
