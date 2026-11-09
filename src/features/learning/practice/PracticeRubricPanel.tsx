import { CriteriaList, type CriterionView } from '@/features/learning/practice/shared/CriteriaList';
import type { LearningPracticeSessionDto } from '@/lib/bindings';

function dimensionName(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1);
}

export function PreviousFeedback({ session }: { session: LearningPracticeSessionDto }) {
  const inherited = session.assistance
    .map((event) => event.details as Record<string, unknown> | null)
    .find((details) => details?.revisesSessionId);
  const feedback = inherited?.previousFeedback;
  if (!Array.isArray(feedback)) return null;

  return (
    <details open className="rounded-xl border border-accent bg-accent-muted p-4">
      <summary className="cursor-pointer text-sm font-semibold text-text-primary">
        Feedback to use in this revision
      </summary>
      <ul className="mt-3 space-y-3">
        {feedback.map((item: { criterionId?: string; observation?: string; evidenceQuote?: string }, index) => (
          <li key={item.criterionId ?? index} className="text-sm leading-6 text-text-primary">
            <p>{item.observation}</p>
            {item.evidenceQuote && (
              <blockquote className="mt-1 border-l-2 border-accent pl-3 text-xs text-text-muted">
                {item.evidenceQuote}
              </blockquote>
            )}
          </li>
        ))}
      </ul>
      <p className="mt-3 text-xs text-text-muted">
        Your original submission and feedback stay in the attempt history.
      </p>
    </details>
  );
}

export function RubricPanel({
  rubric,
  result,
}: {
  rubric: LearningPracticeSessionDto['rubric'];
  result: LearningPracticeSessionDto['result'];
}) {
  const criteria: CriterionView[] = rubric.map((criterion) => {
    const feedback = result?.criteria.find((item) => item.criterionId === criterion.id);
    return {
      id: criterion.id,
      title: `${criterion.title} · ${dimensionName(criterion.dimension)}`,
      description: criterion.description,
      maxPoints: criterion.maxPoints,
      evidence: feedback?.evidenceQuote ? [{ kind: 'quote', text: feedback.evidenceQuote }] : [],
      ...(feedback ? { points: feedback.score, feedback: feedback.observation } : {}),
    };
  });

  return (
    <div className="space-y-3">
      <CriteriaList heading="Visible rubric" criteria={criteria} />
      {result && (
        <p className="rounded-lg bg-surface-sunken p-3 text-xs leading-5 text-text-secondary">
          {result.gradeStatus === 'uncertain'
            ? 'Feedback is uncertain; the response may not support a reliable interpretation.'
            : 'Feedback is provisional and criterion-specific; it is not a mastery claim.'}{' '}
          Model: {result.graderModel}.
        </p>
      )}
    </div>
  );
}

export function ResultPanel({ result }: { result: NonNullable<LearningPracticeSessionDto['result']> }) {
  return (
    <section aria-labelledby="workbench-result-heading" className="rounded-2xl border border-success-muted bg-surface p-5 sm:p-6">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="text-xs font-semibold uppercase tracking-[.15em] text-success-fg">Response evidence</div>
          <h4 id="workbench-result-heading" className="mt-1 font-serif text-xl text-text-primary">
            Feedback on revision {result.artifactRevision}
          </h4>
        </div>
        <span className="rounded-full bg-surface-sunken px-3 py-1.5 text-xs font-semibold uppercase tracking-wider text-text-muted">
          {result.gradeStatus}
        </span>
      </div>
      <p className="mt-3 whitespace-pre-wrap rounded-xl bg-surface-sunken p-4 text-xs leading-6 text-text-secondary">
        {result.artifactText}
      </p>
      <div className="mt-4 grid gap-2 sm:grid-cols-2">
        {result.evidence.map((item, i) => (
          <article key={`${item.dimension}-${i}`} className="rounded-xl border border-border-default bg-surface p-3">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-semibold text-text-primary">{dimensionName(item.dimension)}</span>
              <span className={`rounded-full px-2 py-1 text-xs uppercase tracking-wider ${item.observed ? 'bg-success-muted text-success-fg' : 'bg-surface-sunken text-text-muted'}`}>
                {item.observed ? 'Observed' : 'Not observed'}
              </span>
            </div>
            <p className="mt-2 text-xs leading-5 text-text-secondary">{item.observation}</p>
            {item.evidenceQuote && (
              <blockquote className="mt-2 border-l border-accent pl-2 text-xs italic leading-4 text-text-muted">
                “{item.evidenceQuote}”
              </blockquote>
            )}
          </article>
        ))}
      </div>
      <p className="mt-3 text-xs leading-5 text-text-muted">
        Evidence interpretation is {result.gradeStatus}; assistance and submission mode are part of the record. This is not a mastery score.
      </p>
    </section>
  );
}

import { useRef } from 'react';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';

export function ConfirmDialog({
  title,
  description,
  confirmLabel,
  pending,
  onCancel,
  onConfirm,
}: {
  title: string;
  description: string;
  confirmLabel: string;
  pending: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onCancel(); }}>
      <DialogContent
        hideClose
        overlayClassName="fixed inset-0 z-50 bg-bg/80"
        className="block max-w-md rounded-2xl border border-border-default bg-surface p-6 shadow-2xl"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          cancelRef.current?.focus();
        }}
        onEscapeKeyDown={(event) => {
          if (pending) event.preventDefault();
        }}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <DialogTitle asChild>
          <h3 className="font-serif text-2xl text-text-primary">{title}</h3>
        </DialogTitle>
        <DialogDescription asChild>
          <p className="mt-3 text-sm leading-6 text-text-secondary">{description}</p>
        </DialogDescription>
        <div className="mt-5 flex flex-wrap justify-end gap-2">
          <button
            ref={cancelRef}
            type="button"
            disabled={pending}
            onClick={onCancel}
            className="rounded-full border border-border-default px-4 py-2.5 text-xs font-medium text-text-secondary disabled:opacity-50"
          >
            Keep working
          </button>
          <button
            type="button"
            disabled={pending}
            onClick={onConfirm}
            className="rounded-full bg-accent px-4 py-2.5 text-xs font-semibold text-accent-fg disabled:opacity-50"
          >
            {pending ? 'Saving…' : confirmLabel}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
