import { useState } from 'react';

import { LoaderCircle } from 'lucide-react';

import { ReviewCheckbox, SelectionBar } from '@/features/learning/lessons/review/SelectionBar';
import { useReviewSelection } from '@/features/learning/lessons/review/useReviewSelection';
import { useSettleTeachingFindings } from '@/features/learning/lessons/review/useLessonClaimReview';
import type { TeachingFindingDto, TeachingFindingKindDto, TeachingSettleDto } from '@/lib/bindings';

const KIND_LABELS: Record<TeachingFindingKindDto, string> = {
  teaching: 'Teaching review',
  answer_key: 'Answer key',
  structure: 'Lesson shape',
  quotation: 'Source quotation',
};

export const isTeachingWaiting = (finding: TeachingFindingDto) => finding.state === 'waiting';

export const teachingNotesNoun = (count: number) => `${count} teaching ${count === 1 ? 'note' : 'notes'}`;

function repairsLine(repairs: number) {
  if (repairs === 0) return 'The repairs stopped before reaching it.';
  return `${repairs === 1 ? 'One repair' : `${repairs} repairs`} tried and did not settle it.`;
}

/** What the passage under a note is, so it is not read as the lesson's own text when it is a source's. */
function passageLabel(finding: TeachingFindingDto) {
  if (finding.passageRole === 'quotation') return 'The quotation it cites';
  return finding.site === 'question' ? 'The question' : 'In the lesson';
}

const BATCH = '';

type Settled = { settle: TeachingSettleDto; keys: string[] };

/**
 * Teaching-review findings the repairs did not settle. The lesson publishes
 * with them; each waits here with what is wrong and the part it is about
 * until the user keeps the text as written. Several can be kept at once, and
 * the last batch can be taken back.
 */
export function TeachingFindings({ programId, lessonId, findings, lessonAnchor = null }: {
  programId: string;
  lessonId: string;
  findings: TeachingFindingDto[];
  /** `lesson-<id>-section-` when the lesson's sections are on the page. */
  lessonAnchor?: string | null;
}) {
  const settle = useSettleTeachingFindings(programId, lessonId);
  const waitingKeys = findings.filter(isTeachingWaiting).map((finding) => finding.findingKey);
  const selection = useReviewSelection(waitingKeys);
  const [acting, setActing] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [last, setLast] = useState<Settled | null>(null);
  if (findings.length === 0) return null;
  const waiting = waitingKeys.length;
  const chosen = [...selection.selected];

  const run = async (keys: string[], to: TeachingSettleDto, origin: string) => {
    setActing(origin);
    setErrors({});
    try {
      await settle.mutateAsync({ findingKeys: keys, settle: to });
      selection.clear();
      setLast(origin === BATCH ? { settle: to, keys } : null);
    } catch (error) {
      setErrors({ [origin]: error instanceof Error ? error.message : 'The teaching notes could not be changed.' });
    } finally {
      setActing(null);
    }
  };

  const status = last && (last.settle === 'keep'
    ? `Kept ${teachingNotesNoun(last.keys.length)} as written.`
    : `${teachingNotesNoun(last.keys.length)} ${last.keys.length === 1 ? 'waits' : 'wait'} for you again.`);

  return (
    <section aria-label="Teaching review" className="mt-5">
      <h4 className="flex items-center justify-between gap-2 py-1.5 text-xs font-semibold text-text-secondary">
        <span>Teaching review</span>
        <span className="shrink-0 tabular-nums text-text-muted">{waiting > 0 ? `${waiting} waiting for you` : 'All kept as written'}</span>
      </h4>
      {waiting >= 2 && <div className="mb-2">
        {chosen.length === waiting
          ? <button type="button" disabled={acting !== null} onClick={selection.clear} className="min-h-9 px-1 text-xs font-medium text-accent disabled:opacity-50">Select none</button>
          : <button type="button" disabled={acting !== null} onClick={() => selection.setMany(waitingKeys, true)} className="min-h-9 px-1 text-xs font-medium text-accent disabled:opacity-50">{`Select all ${waiting}`}</button>}
      </div>}
      {status && last && <p role="status" className="mb-2 flex flex-wrap items-center gap-2 text-sm text-text-secondary">
        <span>{status}</span>
        {last.settle === 'keep' && <button type="button" disabled={acting !== null} onClick={() => void run(last.keys, 'reopen', BATCH)}
          className="min-h-9 rounded-full border border-accent/50 px-3 text-xs font-medium text-accent disabled:opacity-50">Undo</button>}
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
                {finding.partChanged && <p className="mt-1 text-xs text-text-muted">You changed this part after this note was written.</p>}
                {finding.passage && <>
                  <p className="mt-2 text-[11px] font-semibold uppercase tracking-[.12em] text-text-muted">{passageLabel(finding)}</p>
                  <blockquote className="mt-1 border-l-2 border-accent pl-3 text-xs leading-5 whitespace-pre-wrap wrap-anywhere text-text-primary">{finding.passage}</blockquote>
                </>}
                <p className="mt-2 text-xs text-text-muted">{repairsLine(finding.repairs)}</p>
                <div className="mt-3 flex flex-wrap items-center gap-2">
                  {isTeachingWaiting(finding)
                    ? <button type="button" disabled={acting !== null} onClick={() => void run([finding.findingKey], 'keep', finding.findingKey)}
                      className="min-h-9 rounded-full border border-accent/50 px-3 text-xs font-medium text-accent disabled:opacity-50">Keep as written</button>
                    : <>
                      <span className="rounded-full bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-700">Kept as written</span>
                      <button type="button" disabled={acting !== null} onClick={() => void run([finding.findingKey], 'reopen', finding.findingKey)}
                        className="min-h-9 rounded-full border border-border px-3 text-xs font-medium text-text-secondary disabled:opacity-50">Undo</button>
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
        actions={[{ label: 'Keep as written', onClick: () => void run(chosen, 'keep', BATCH) }]} />
    </section>
  );
}
