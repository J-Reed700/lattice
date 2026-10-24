import { useState } from 'react';

import { LoaderCircle } from 'lucide-react';

import { useKeepTeachingFinding } from '@/features/learning/lessons/review/useLessonClaimReview';
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
  if (repairs === 0) return 'The repairs stopped before reaching it.';
  return `${repairs === 1 ? 'One repair' : `${repairs} repairs`} tried and did not settle it.`;
}

/**
 * Teaching-review findings the repairs did not settle. The lesson publishes
 * with them; each waits here with the reviewer's note and the part it is
 * about until the user keeps the text as written.
 */
export function TeachingFindings({ programId, lessonId, findings, lessonAnchor = null, onKept }: {
  programId: string;
  lessonId: string;
  findings: TeachingFindingDto[];
  /** `lesson-<id>-section-` when the lesson's sections are on the page. */
  lessonAnchor?: string | null;
  onKept?: (remaining: number) => void;
}) {
  const keep = useKeepTeachingFinding(programId, lessonId);
  const [acting, setActing] = useState<string | null>(null);
  const [errors, setErrors] = useState<Record<string, string>>({});
  if (findings.length === 0) return null;
  const waiting = findings.filter(isTeachingWaiting).length;

  const keepAsWritten = async (finding: TeachingFindingDto) => {
    setActing(finding.findingKey);
    setErrors(({ [finding.findingKey]: _dropped, ...rest }) => rest);
    try {
      await keep.mutateAsync({ findingKey: finding.findingKey });
      onKept?.(waiting - 1);
    } catch (error) {
      setErrors((current) => ({ ...current, [finding.findingKey]: error instanceof Error ? error.message : 'The finding could not be kept.' }));
    } finally {
      setActing(null);
    }
  };

  return (
    <section aria-label="Teaching review" className="mt-5">
      <h4 className="flex items-center justify-between gap-2 py-1.5 text-xs font-semibold text-text-secondary">
        <span>Teaching review</span>
        <span className="shrink-0 tabular-nums text-text-muted">{waiting > 0 ? `${waiting} waiting for you` : 'All kept as written'}</span>
      </h4>
      <ul role="list" className="space-y-2">
        {findings.map((finding) => (
          <li key={finding.findingKey}>
            <article aria-label={`${finding.unitTitle}: ${KIND_LABELS[finding.kind]}`} className="rounded-xl border border-border-default bg-surface-sunken/40 p-4">
              <p className="text-[11px] font-semibold uppercase tracking-[.12em] text-text-muted">{KIND_LABELS[finding.kind]}</p>
              <p className="mt-1 text-sm font-medium text-text-primary">
                {lessonAnchor && finding.site === 'section' && finding.unitIndex != null
                  ? <a href={`#${lessonAnchor}${finding.unitIndex}`} className="underline decoration-border underline-offset-2 hover:decoration-accent">{finding.unitTitle}</a>
                  : finding.unitTitle}
              </p>
              <p className="mt-2 text-sm leading-6 text-text-secondary">{finding.note}</p>
              {finding.passage && <blockquote className="mt-2 border-l-2 border-accent pl-3 text-xs leading-5 whitespace-pre-wrap wrap-anywhere text-text-primary">{finding.passage}</blockquote>}
              <p className="mt-2 text-xs text-text-muted">{repairsLine(finding.repairs)}</p>
              <div className="mt-3 flex flex-wrap items-center gap-2">
                {isTeachingWaiting(finding)
                  ? <button type="button" disabled={acting !== null} onClick={() => void keepAsWritten(finding)}
                    className="min-h-9 rounded-full border border-border-default bg-surface px-3 text-xs font-medium text-accent hover:bg-surface-sunken disabled:opacity-50">Keep as written</button>
                  : <span className="rounded-full bg-success-muted px-2 py-0.5 text-xs font-medium text-success-fg">Kept as written</span>}
                {acting === finding.findingKey && <span role="status" className="inline-flex items-center gap-1 text-xs text-text-muted"><LoaderCircle size={13} className="animate-spin motion-reduce:animate-none" aria-hidden="true" />Saving…</span>}
              </div>
              {errors[finding.findingKey] && <p role="alert" className="mt-2 text-xs leading-5 text-danger-fg">{errors[finding.findingKey]}</p>}
            </article>
          </li>
        ))}
      </ul>
    </section>
  );
}
