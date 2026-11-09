import { useState } from 'react';
import { AlertCircle, Check } from 'lucide-react';

import { useSaveLearningCardDraft } from '@/features/learning/memory/useLearningMemory';

export interface SaveAsReviewCardProps {
  programId: string;
  lessonId: string;
  promptText?: string;
  question?: string;
  promptLabel?: string;
  answerLabel?: string;
  explanation?: string;
  initialAnswer?: string;
  answer?: string;
}

/**
 * Shared card allowing learners to correct a misconception in their own words
 * and save it as an active recall study card draft.
 */
export function SaveAsReviewCard({
  programId,
  lessonId,
  promptText,
  question,
  promptLabel,
  answerLabel,
  explanation = 'Drafted from a misconception found in practice',
  initialAnswer,
  answer: defaultAnswer,
}: SaveAsReviewCardProps): JSX.Element {
  const saveDraft = useSaveLearningCardDraft();
  const frontText = question ?? promptText ?? '';
  const labelText = answerLabel ?? promptLabel ?? 'What is actually right, in your own words?';
  const [answer, setAnswer] = useState(defaultAnswer ?? initialAnswer ?? '');

  if (saveDraft.isSuccess) {
    return (
      <p role="status" className="mt-3 flex items-center gap-1.5 text-xs text-text-secondary">
        <Check className="h-3.5 w-3.5 text-accent" aria-hidden="true" />
        <span>Saved as a recall card draft. It comes back in Recall.</span>
      </p>
    );
  }

  const handleSave = () => {
    if (!answer.trim() || saveDraft.isPending) return;
    saveDraft.mutate({
      draftId: null,
      programId,
      lessonId,
      question: frontText,
      answer: answer.trim(),
      explanation,
      sourceIds: [],
    });
  };

  return (
    <div className="mt-3 rounded-xl border border-border-default bg-surface p-4">
      <div className="text-xs font-semibold text-text-primary">
        {frontText}
      </div>
      <label className="mt-2 block text-xs font-medium text-text-secondary">
        {labelText}
        <textarea
          rows={3}
          value={answer}
          onChange={(event) => setAnswer(event.target.value)}
          placeholder="Write what is true instead, explaining why…"
          className="mt-1.5 w-full rounded-lg border border-border-default bg-surface-sunken px-3 py-2 text-sm leading-6 text-text-primary placeholder:text-text-muted outline-hidden focus:border-accent focus:ring-1 focus:ring-ring"
        />
      </label>

      <div className="mt-3 flex items-center gap-3">
        <button
          type="button"
          disabled={!answer.trim() || saveDraft.isPending}
          onClick={handleSave}
          className="pressable rounded-full bg-accent px-4 py-2 text-xs font-semibold text-accent-fg disabled:cursor-not-allowed disabled:opacity-50"
        >
          {saveDraft.isPending ? 'Saving…' : 'Make a recall card'}
        </button>
      </div>

      {saveDraft.isError && (
        <p role="alert" className="mt-2 flex items-center gap-1.5 text-xs text-danger-fg">
          <AlertCircle className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span>{saveDraft.error?.message ?? 'Failed to save card draft.'}</span>
        </p>
      )}
    </div>
  );
}
