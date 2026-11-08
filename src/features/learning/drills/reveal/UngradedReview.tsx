import { useEffect, useId, useRef } from 'react';

import { ArrowRight, LoaderCircle, RotateCcw } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { drillMeta, kindOf } from '@/features/learning/drills/drillMaterial';
import { CommentCard } from '@/features/learning/drills/reveal/CommentCard';
import { eyebrow, panel, sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import type { CodeReviewResultDto } from '@/lib/bindings';

const NO_ASSESSMENT = 'The grader gave no assessment.';

/** A review the grader gave no assessment of: why, a way to try again, and the learner's own comments. */
export function UngradedReview({ result, onRegrade, onNextDrill, regrading }: {
  result: CodeReviewResultDto;
  onRegrade: () => void;
  onNextDrill: () => void;
  regrading: boolean;
}) {
  const headingId = useId();
  const commentsId = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    headingRef.current?.focus();
  }, []);
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6">
      <section aria-labelledby={headingId} className={`${panel} px-5 py-5 sm:px-6`}>
        <p className={eyebrow}>Result · {drillMeta(result.drill, result.mode)}</p>
        <h2 id={headingId} ref={headingRef} tabIndex={-1} className="mt-1 font-serif text-2xl leading-tight text-text-primary outline-hidden">
          This review could not be graded
        </h2>
        <p role="alert" className="mt-3 rounded-xl bg-danger-muted px-4 py-3 text-sm leading-6 text-text-primary">
          {result.error ?? NO_ASSESSMENT}
        </p>
        {result.graderModel && (
          <p className="mt-2 text-sm text-text-secondary">
            Tried with <span className="font-medium text-text-primary">{result.graderModel}</span>
          </p>
        )}
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <Button type="button" size="lg" disabled={regrading} onClick={onRegrade}>
            {regrading
              ? <LoaderCircle className="animate-spin motion-reduce:animate-none" aria-hidden="true" />
              : <RotateCcw aria-hidden="true" />}
            Grade again
          </Button>
          <Button type="button" size="lg" variant="outline" onClick={onNextDrill}>
            Next drill
            <ArrowRight aria-hidden="true" />
          </Button>
          {regrading && <p role="status" className="text-xs text-text-muted">Grading again…</p>}
        </div>
      </section>
      <section aria-labelledby={commentsId} className="space-y-3">
        <h2 id={commentsId} className={sectionHeading}>Your comments</h2>
        {result.findings.length === 0 ? (
          <p className="text-sm text-text-secondary">You left no comments.</p>
        ) : (
          <ol className="space-y-3">
            {result.findings.map((finding, index) => (
              <li key={index}>
                <CommentCard number={index + 1} span={finding.span} text={finding.text} severity={finding.severity} material={kindOf(result.drill)} />
              </li>
            ))}
          </ol>
        )}
      </section>
    </div>
  );
}
