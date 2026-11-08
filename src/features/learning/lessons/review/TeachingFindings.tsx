import { useState } from 'react';

import { LoaderCircle } from 'lucide-react';

import { ReviewCheckbox, SelectionBar } from '@/features/learning/lessons/review/SelectionBar';
import { useReviewSelection } from '@/features/learning/lessons/review/useReviewSelection';
import { useResolveLearningClaims, useSettleTeachingFindings } from '@/features/learning/lessons/review/useLessonClaimReview';
import type { TeachingFindingDto, TeachingFindingKindDto } from '@/lib/bindings';

const KIND_LABELS: Record<TeachingFindingKindDto, string> = {
  teaching: 'Teaching review',
  answer_key: 'Answer key',
  structure: 'Lesson shape',
  quotation: 'Source quotation',
};

export const isTeachingWaiting = (finding: TeachingFindingDto) => finding.state === 'waiting';

export const teachingNotesNoun = (count: number) => `${count} teaching ${count === 1 ? 'note' : 'notes'}`;

function repairsLine(repairs: number) {
  if (repairs === 0) return 'No repair was tried on it.';
  return `${repairs === 1 ? 'One repair' : `${repairs} repairs`} tried and did not settle it.`;
}

/** What the passage under a note is, so it is not read as the lesson's own text when it is a source's. */
function passageLabel(finding: TeachingFindingDto) {
  if (finding.passageRole === 'quotation') return 'The quotation it cites';
  return finding.site === 'question' ? 'The question' : 'In the lesson';
}

const BATCH = '';
const NOT_ONE_SENTENCE = 'Some of these notes are not about one sentence. Remove them one at a time.';

const unique = (ids: string[]) => [...new Set(ids)];

/** What the last change said, so it can be taken back. */
type Settled =
  | { kind: 'keep'; keys: string[] }
  | { kind: 'reopen'; keys: string[] }
  | { kind: 'removed'; keys: string[]; claimIds: string[]; revision: number };

function statusLine(last: Settled) {
  const count = last.keys.length;
  if (last.kind === 'keep') return `Kept ${teachingNotesNoun(count)} as written.`;
  if (last.kind === 'removed') return count === 1 ? 'Removed the sentence from the lesson.' : `Removed ${count} sentences from the lesson.`;
  return `${teachingNotesNoun(count)} ${count === 1 ? 'waits' : 'wait'} for you again.`;
}

/**
 * Teaching-review findings the repairs did not settle. The lesson publishes
 * with them; each waits here with what is wrong and the part it is about
 * until the user keeps the text as written. Several can be kept at once, and
 * the last batch can be taken back. On a published lesson a note about one
 * sentence can also take the sentence out of the lesson, or hand it to Add a
 * source.
 */
export function TeachingFindings({ programId, lessonId, findings, lessonAnchor = null, published = false, revision, onAddSource }: {
  programId: string;
  lessonId: string;
  findings: TeachingFindingDto[];
  /** `lesson-<id>-section-` when the lesson's sections are on the page. */
  lessonAnchor?: string | null;
  /** The lesson is published: a note about a sentence can take the sentence out or add a source for it. */
  published?: boolean;
  /** The review's revision, which taking claims out or back checks. */
  revision?: number;
  /** Opens Add a source for the note's first claim. */
  onAddSource?: (claimId: string, findingKey: string) => void;
}) {
  const settle = useSettleTeachingFindings(programId, lessonId);
  const resolveMany = useResolveLearningClaims(programId, lessonId);
  const waitingKeys = findings.filter(isTeachingWaiting).map((finding) => finding.findingKey);
  const selection = useReviewSelection(waitingKeys);
  const [acting, setActing] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [last, setLast] = useState<Settled | null>(null);
  if (findings.length === 0) return null;
  const waiting = waitingKeys.length;
  const chosen = [...selection.selected];
  const byKey = new Map(findings.map((finding) => [finding.findingKey, finding]));
  const canAct = published && revision !== undefined;
  const aboutOneSentence = (finding: TeachingFindingDto | undefined) => Boolean(finding && isTeachingWaiting(finding) && finding.site === 'section' && finding.claimIds.length > 0);
  const actionable = (finding: TeachingFindingDto) => canAct && aboutOneSentence(finding);
  const everyChosenHasASentence = chosen.every((key) => aboutOneSentence(byKey.get(key)));

  /** Runs one change; what it returns is what the status line says, and null says nothing. */
  const act = async (origin: string, change: () => Promise<Settled | null>) => {
    setActing(origin);
    setErrors({});
    try {
      const done = await change();
      selection.clear();
      setLast(done);
    } catch (error) {
      setErrors({ [origin]: error instanceof Error ? error.message : 'The teaching notes could not be changed.' });
    } finally {
      setActing(null);
    }
  };

  const keep = (keys: string[], origin: string) => act(origin, async () => {
    await settle.mutateAsync({ findingKeys: keys, settle: 'keep' });
    return origin === BATCH ? { kind: 'keep', keys } : null;
  });

  /** Takes the claims on the notes' sentences out in one call, then settles the notes removed. */
  const removeSentences = (keys: string[], origin: string) => act(origin, async () => {
    const claimIds = unique(keys.flatMap((key) => byKey.get(key)?.claimIds ?? []));
    const result = await resolveMany.mutateAsync({ claimIds, action: { action: 'remove' }, expectedRevision: revision ?? 0 });
    await settle.mutateAsync({ findingKeys: keys, settle: 'removed' });
    return { kind: 'removed', keys, claimIds, revision: result.revision };
  });

  /** Takes back the last batch, or one resolved note: the claims first when they were taken out, then the notes. */
  const reopen = (keys: string[], origin: string, claims: { claimIds: string[]; revision: number } | null) => act(origin, async () => {
    if (claims) await resolveMany.mutateAsync({ claimIds: claims.claimIds, action: { action: 'undo' }, expectedRevision: claims.revision });
    await settle.mutateAsync({ findingKeys: keys, settle: 'reopen' });
    return origin === BATCH ? { kind: 'reopen', keys } : null;
  });

  const undoLast = (done: Settled) => reopen(done.keys, BATCH, done.kind === 'removed' ? done : null);
  const undoResolved = (finding: TeachingFindingDto) => reopen([finding.findingKey], finding.findingKey,
    finding.resolution === 'removed' && revision !== undefined ? { claimIds: finding.claimIds, revision } : null);

  const header = waiting > 0 ? `${waiting} waiting for you` : findings.every((finding) => finding.state === 'kept') ? 'All kept as written' : 'Nothing is waiting for you';
  const smallButton = 'min-h-9 rounded-full border px-3 text-xs font-medium disabled:opacity-50';

  return (
    <section aria-label="Teaching review" className="mt-5">
      <h4 className="flex items-center justify-between gap-2 py-1.5 text-xs font-semibold text-text-secondary">
        <span>Teaching review</span>
        <span className="shrink-0 tabular-nums text-text-muted">{header}</span>
      </h4>
      {waiting >= 2 && <div className="mb-2">
        {chosen.length === waiting
          ? <button type="button" disabled={acting !== null} onClick={selection.clear} className="min-h-9 px-1 text-xs font-medium text-accent disabled:opacity-50">Select none</button>
          : <button type="button" disabled={acting !== null} onClick={() => selection.setMany(waitingKeys, true)} className="min-h-9 px-1 text-xs font-medium text-accent disabled:opacity-50">{`Select all ${waiting}`}</button>}
      </div>}
      {last && <p role="status" className="mb-2 flex flex-wrap items-center gap-2 text-sm text-text-secondary">
        <span>{statusLine(last)}</span>
        {last.kind !== 'reopen' && <button type="button" disabled={acting !== null} onClick={() => void undoLast(last)}
          className={`${smallButton} border-accent/50 text-accent`}>Undo</button>}
      </p>}
      {errors[BATCH] && <p role="alert" className="mb-2 text-xs leading-5 text-rose-700">{errors[BATCH]}</p>}
      <ul role="list" className="space-y-2">
        {findings.map((finding) => (
          <li key={finding.findingKey}>
            <article aria-label={`${finding.unitTitle}: ${KIND_LABELS[finding.kind]}`} className="flex gap-3 rounded-xl border border-border bg-background p-4">
              {isTeachingWaiting(finding) && <ReviewCheckbox checked={selection.isSelected(finding.findingKey)} label={`Select the note on ${finding.unitTitle}`}
                disabled={acting !== null} onToggle={(shiftKey) => selection.toggle(finding.findingKey, { shiftKey })} />}
              <div className="min-w-0 flex-1">
                <p className="text-[11px] font-semibold uppercase tracking-[.12em] text-text-muted">{KIND_LABELS[finding.kind]}</p>
                <p className="mt-1 text-sm font-medium text-text-primary">
                  {lessonAnchor && finding.site === 'section' && finding.unitIndex != null
                    ? <a href={`#${lessonAnchor}${finding.unitIndex}`} className="underline decoration-border underline-offset-2 hover:decoration-accent">{finding.unitTitle}</a>
                    : finding.unitTitle}
                </p>
                <p className="mt-2 text-sm leading-6 whitespace-pre-line text-text-secondary">{finding.summary}</p>
                {finding.passage && <>
                  <p className="mt-2 text-[11px] font-semibold uppercase tracking-[.12em] text-text-muted">{passageLabel(finding)}</p>
                  <blockquote className="mt-1 border-l-2 border-accent pl-3 text-xs leading-5 whitespace-pre-wrap wrap-anywhere text-text-primary">{finding.passage}</blockquote>
                </>}
                <p className="mt-2 text-xs text-text-muted">{repairsLine(finding.repairs)}</p>
                <div className="mt-3 flex flex-wrap items-center gap-2">
                  {isTeachingWaiting(finding)
                    ? <>
                      <button type="button" disabled={acting !== null} onClick={() => void keep([finding.findingKey], finding.findingKey)}
                        className={`${smallButton} border-accent/50 text-accent`}>Keep as written</button>
                      {actionable(finding) && <>
                        <button type="button" disabled={acting !== null} onClick={() => void removeSentences([finding.findingKey], finding.findingKey)}
                          className={`${smallButton} border-border text-text-secondary`}>Remove this sentence</button>
                        <button type="button" disabled={acting !== null} onClick={() => onAddSource?.(finding.claimIds[0], finding.findingKey)}
                          className={`${smallButton} border-border text-text-secondary`}>Add a source</button>
                      </>}
                    </>
                    : <>
                      <span className="rounded-full bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-700">
                        {finding.state === 'kept' ? 'Kept as written' : finding.resolution === 'source_added' ? 'Source added' : 'Removed from the lesson'}
                      </span>
                      {!(finding.state === 'resolved' && finding.resolution === 'removed' && finding.claimIds.length === 0) && <button type="button"
                        disabled={acting !== null} onClick={() => void undoResolved(finding)} className={`${smallButton} border-border text-text-secondary`}>Undo</button>}
                    </>}
                  {acting === finding.findingKey && <span role="status" className="inline-flex items-center gap-1 text-xs text-text-muted"><LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />Saving…</span>}
                </div>
                {errors[finding.findingKey] && <p role="alert" className="mt-2 text-xs leading-5 text-rose-700">{errors[finding.findingKey]}</p>}
              </div>
            </article>
          </li>
        ))}
      </ul>
      <SelectionBar count={chosen.length} noun={['teaching note', 'teaching notes']} busy={acting !== null} onClear={selection.clear}
        actions={[
          { label: 'Keep as written', onClick: () => void keep(chosen, BATCH) },
          ...(canAct ? [{ label: 'Remove these sentences', onClick: () => void removeSentences(chosen, BATCH), disabled: !everyChosenHasASentence, hint: NOT_ONE_SENTENCE, tone: 'danger' as const }] : []),
        ]} />
    </section>
  );
}
