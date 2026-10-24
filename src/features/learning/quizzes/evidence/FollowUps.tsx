import { useState } from 'react';

import { TiptapViewer } from '@/components/TiptapEditor';
import { useAcceptFollowUp, useDismissFollowUp, useOperationIds } from '@/features/learning/quizzes/useQuizzes';
import type { LearningFollowUpActionKind, LearningFollowUpReasonCode, LearningFollowUpRecommendationDto, LearningOutcomeDefinitionDto } from '@/lib/bindings';

const reasonText: Record<LearningFollowUpReasonCode, string> = {
  missed_outcome: 'Missed outcome', assisted_success: 'Assisted success', low_transfer: 'Low transfer', stale_evidence: 'Stale evidence', uncertain_grade: 'Uncertain grade',
};
const actionText: Record<LearningFollowUpActionKind, string> = {
  lesson: 'Review the lesson', practice: 'Try practice', assessment: 'Take a quiz', recall: 'Recall review', practical: 'Apply it',
};
const pill = 'min-h-11 rounded-full px-4 text-sm disabled:opacity-50';

/** Evidence-based next steps; accepting or dismissing one is idempotent and retryable. */
export function FollowUps({ programId, outcomes, followUps }: { programId: string; outcomes: LearningOutcomeDefinitionDto[]; followUps: LearningFollowUpRecommendationDto[] }) {
  const accept = useAcceptFollowUp();
  const dismiss = useDismissFollowUp();
  const operations = useOperationIds();
  const [failed, setFailed] = useState<{ item: LearningFollowUpRecommendationDto; kind: 'accept' | 'dismiss' } | null>(null);
  const pending = followUps.filter((item) => item.status === 'pending');
  const busyId = accept.isPending ? accept.variables?.followUpId : dismiss.isPending ? dismiss.variables?.followUpId : undefined;
  const decide = async (item: LearningFollowUpRecommendationDto, kind: 'accept' | 'dismiss') => {
    const key = `${kind}:${item.id}`;
    try {
      await (kind === 'accept' ? accept : dismiss).mutateAsync({ operationId: operations.take(key), programId, followUpId: item.id });
      operations.done(key);
      setFailed(null);
    } catch { setFailed({ item, kind }); }
  };
  return <section aria-labelledby="follow-ups-heading" className="rounded-2xl border border-border bg-surface p-5">
    <div className="flex flex-wrap items-end justify-between gap-2">
      <div><div className="text-[10px] font-semibold uppercase tracking-[.15em] text-accent">Next steps</div>
        <h3 id="follow-ups-heading" className="mt-1 font-serif text-xl text-text-primary">From your evidence</h3></div>
      <span className="rounded-full bg-background px-3 py-1 text-xs text-text-muted">{pending.length} waiting</span>
    </div>
    {pending.length === 0 ? <p className="mt-4 rounded-xl bg-background px-4 py-4 text-sm text-text-muted">Nothing to follow up yet. New results may suggest a next step.</p>
      : <ul className="mt-4 space-y-3">{pending.map((item) => <li key={item.id} className="rounded-xl border border-border bg-background/40 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <span className="rounded-full bg-accent/10 px-2.5 py-1 text-[11px] font-semibold text-accent">{reasonText[item.reasonCode]}</span>
          {outcomes.find((outcome) => outcome.id === item.outcomeId) && <span className="text-xs text-text-muted">{outcomes.find((outcome) => outcome.id === item.outcomeId)?.title}</span>}
        </div>
        <div className="mt-2 text-sm leading-6 text-text-secondary"><TiptapViewer content={item.explanation} /></div>
        <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
          <span className="text-xs font-medium text-text-primary">Suggested: {actionText[item.actionKind]}</span>
          <div className="flex gap-2">
            <button type="button" disabled={Boolean(busyId)} onClick={() => void decide(item, 'dismiss')} className={`${pill} border border-border text-text-secondary`}>Dismiss</button>
            <button type="button" disabled={Boolean(busyId)} onClick={() => void decide(item, 'accept')} className={`${pill} bg-accent font-semibold text-accent-fg`}>{busyId === item.id ? 'Saving…' : 'Accept'}</button>
          </div>
        </div>
      </li>)}</ul>}
    {failed && <p role="alert" className="mt-3 flex flex-wrap items-center gap-2 text-sm text-rose-700 dark:text-rose-300">That was not saved.
      <button type="button" onClick={() => void decide(failed.item, failed.kind)} className="min-h-11 font-semibold underline">Try again</button></p>}
  </section>;
}
