import { useId, useState } from 'react';

import { Button } from '@/components/ui/button';
import { UNVERIFIED_REASONS } from '@/features/learning/lessons/review/claimReviewCopy';
import { useLessonClaimReview, useResolveLearningClaim } from '@/features/learning/lessons/review/useLessonClaimReview';
import type { ClaimReviewItemDto, LearningLessonEvidenceDto, UnverifiedReasonDto } from '@/lib/bindings';

export type UnverifiedStatement = { key: string; statement: string; reason: UnverifiedReasonDto; claimId: string | null };

/** The statements of one section that the lesson published without verifying them. */
export function unverifiedStatements(report: LearningLessonEvidenceDto | null | undefined, sectionIndex: number): UnverifiedStatement[] {
  return (report?.teachingClaims ?? []).flatMap((claim, index) => (
    claim.resolution === 'not_verified' && claim.sectionIndex === sectionIndex
      ? [{ key: `${index}:${claim.claim}`, statement: claim.claim, reason: claim.unverifiedReason ?? 'unchecked', claimId: claim.claimId ?? null }]
      : []
  ));
}

type Form = 'edit' | 'reason' | null;

function Statement({ item, programId, lessonId, revision, claim, onReview }: {
  item: UnverifiedStatement;
  programId: string;
  lessonId: string;
  revision: number | null;
  claim: ClaimReviewItemDto | null;
  onReview?: (programId: string, lessonId: string, claimId?: string) => void;
}) {
  const id = useId();
  const resolve = useResolveLearningClaim(programId, lessonId);
  const [form, setForm] = useState<Form>(null);
  const [text, setText] = useState('');
  const [error, setError] = useState('');
  const available = new Set(claim?.availableActions ?? []);
  const claimId = claim && revision != null ? item.claimId : null;

  const act = async (action: Parameters<typeof resolve.mutateAsync>[0]['action']) => {
    if (!claimId || revision == null) return;
    setError('');
    try {
      await resolve.mutateAsync({ claimId, action, expectedRevision: revision });
      setForm(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'The change could not be saved.');
    }
  };
  const confirm = () => {
    if (item.reason === 'contradicted' && form !== 'reason') { setText(''); setForm('reason'); return; }
    const note = text.trim();
    if (item.reason === 'contradicted' && !note) return;
    void act({ action: 'confirm', note: item.reason === 'contradicted' ? note : null, sourceVersionIds: [] });
  };
  const save = () => {
    const after = text.trim();
    if (!after || !claim) return;
    void act({ action: 'edit', before: claim.location.quote, after });
  };
  const busy = resolve.isPending;

  return (
    <li>
      <span>“{item.statement}”</span>{' '}
      <span className="text-text-secondary">{UNVERIFIED_REASONS[item.reason]}</span>
      {claimId && (
        <div className="mt-1 flex flex-wrap gap-2">
          {available.has('confirm') && <Button type="button" size="sm" variant="outline" disabled={busy} onClick={confirm}>This is right</Button>}
          {available.has('edit') && <Button type="button" size="sm" variant="outline" disabled={busy} onClick={() => { setText(claim?.location.quote ?? ''); setForm(form === 'edit' ? null : 'edit'); }}>Edit</Button>}
          {onReview && <Button type="button" size="sm" variant="ghost" onClick={() => onReview(programId, lessonId, claimId)}>Check the sources</Button>}
        </div>
      )}
      {form === 'reason' && (
        <div className="mt-2">
          <label htmlFor={`${id}-reason`} className="block text-xs font-medium text-text-secondary">A source disagrees with this. Why is it right?</label>
          <textarea id={`${id}-reason`} autoFocus rows={2} value={text} onChange={(event) => setText(event.target.value)} className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
          <div className="mt-2 flex gap-2">
            <Button type="button" size="sm" disabled={busy} onClick={confirm}>Save</Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setForm(null)}>Cancel</Button>
          </div>
        </div>
      )}
      {form === 'edit' && (
        <div className="mt-2">
          <label htmlFor={`${id}-edit`} className="block text-xs font-medium text-text-secondary">Your version</label>
          <textarea id={`${id}-edit`} autoFocus rows={3} value={text} onChange={(event) => setText(event.target.value)} className="mt-1 w-full rounded-lg border border-border bg-surface px-3 py-2 text-sm" />
          <div className="mt-2 flex gap-2">
            <Button type="button" size="sm" disabled={busy} onClick={save}>Save</Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setForm(null)}>Cancel</Button>
          </div>
        </div>
      )}
      {error && <p role="alert" className="mt-1 text-danger-fg">{error}</p>}
    </li>
  );
}

/** Always visible under its section, with citations shown or not. */
export function UnverifiedStatements({ programId, lessonId, statements, onReview }: {
  programId: string;
  lessonId: string;
  statements: UnverifiedStatement[];
  onReview?: (programId: string, lessonId: string, claimId?: string) => void;
}) {
  const review = useLessonClaimReview(programId, lessonId, statements.length > 0).data ?? null;
  if (statements.length === 0) return null;
  return (
    <div role="note" className="mt-4 rounded-lg border border-amber-600/30 p-3 text-xs leading-5 text-text-primary">
      <p className="font-medium">Not verified:</p>
      <ul className="mt-1 space-y-2">
        {statements.map((item) => (
          <Statement key={item.key} item={item} programId={programId} lessonId={lessonId} revision={review?.revision ?? null}
            claim={review?.claims.find((claim) => claim.claimId === item.claimId) ?? null} onReview={onReview} />
        ))}
      </ul>
    </div>
  );
}
