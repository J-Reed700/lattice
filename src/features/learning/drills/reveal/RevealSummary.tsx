import { useEffect, useId, useRef, type ReactNode } from 'react';

import { Check, X } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { kindOf } from '@/features/learning/drills/drillMaterial';
import { MODE_LABEL, durationText, momentOf, percentText, plural } from '@/features/learning/drills/reveal/revealCopy';
import { leadingLine } from '@/features/learning/drills/reveal/revealPresentation';
import { eyebrow, panel } from '@/features/learning/drills/reveal/revealStyle';
import { severityLabel, severityTone } from '@/features/learning/drills/severity';
import type { CodeReviewResultDto, CodeReviewRevealDto, SeverityTallyDto } from '@/lib/bindings';

function Stat({ label, value, hint }: { label: string; value: ReactNode; hint?: string }) {
  return (
    <div className="rounded-xl bg-bg px-4 py-3">
      <dt className="text-xs text-text-muted">{label}</dt>
      <dd className="mt-0.5">
        <span className="text-2xl font-semibold tabular-nums text-text-primary">{value}</span>
        {hint && <span className="mt-0.5 block text-xs leading-5 text-text-muted">{hint}</span>}
      </dd>
    </div>
  );
}

function tallyText(tally: SeverityTallyDto): string {
  const found = `${tally.found} of ${tally.total} found`;
  return tally.halfFound > 0 ? `${found}, ${tally.halfFound} half found` : found;
}

/** What the review came to, as its parts, and which model graded it. */
export function RevealSummary({ result, reveal }: { result: CodeReviewResultDto; reveal: CodeReviewRevealDto }) {
  const headingId = useId();
  const headingRef = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    headingRef.current?.focus();
  }, []);
  const { score } = reveal;
  const issues = score.found + score.halfFound + score.missed;
  const graded = result.gradedAt === null ? null : momentOf(result.gradedAt);
  const leading = leadingLine(reveal);
  return (
    <section aria-labelledby={headingId} className={`${panel} overflow-hidden`}>
      <header className="border-b border-border bg-surface-raised px-5 py-5 sm:px-6">
        <p className={eyebrow}>Result · {MODE_LABEL[result.mode]} · {result.drill.material.kind === 'code' ? result.drill.material.language : 'Explanation'}</p>
        <h2 id={headingId} ref={headingRef} tabIndex={-1} className="mt-1 font-serif text-2xl leading-tight text-text-primary outline-hidden">
          {result.drill.title}
        </h2>
        <p className="mt-1.5 text-sm text-text-secondary">
          {result.graderModel
            ? <>Graded by <span className="font-medium text-text-primary">{result.graderModel}</span></>
            : 'Grader model not recorded'}
          {graded && <> · <time dateTime={graded.iso}>{graded.text}</time></>}
          {result.timeSpentMs !== null && <> · Time spent <span className="tabular-nums text-text-primary">{durationText(result.timeSpentMs)}</span></>}
        </p>
      </header>
      <div className="space-y-5 px-5 py-5 sm:px-6">
        <div className="grid gap-5 md:grid-cols-2">
          <div>
            <h3 className={eyebrow}>Of the {plural(issues, 'planted issue')}{kindOf(result.drill) === 'passage' ? ' in the explanation' : ''}</h3>
            <dl className="mt-2 grid grid-cols-3 gap-2">
              <Stat label="Found" value={score.found} />
              <Stat label="Half found" value={score.halfFound} />
              <Stat label="Missed" value={score.missed} />
            </dl>
            <ul aria-label="By severity" className="mt-3 space-y-1.5">
              {score.bySeverity.map((tally) => (
                <li key={tally.severity} className="flex flex-wrap items-center gap-2 text-sm text-text-secondary">
                  <Badge variant="outline" className={severityTone(tally.severity)}>{severityLabel(tally.severity)}</Badge>
                  <span className="tabular-nums">{tallyText(tally)}</span>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <h3 className={eyebrow}>Your review</h3>
            <dl className="mt-2 grid grid-cols-2 gap-2">
              <Stat label="Weighted recall" value={percentText(score.weightedRecall)} hint="Severity-weighted; half found counts half." />
              {score.precision !== null && (
                <Stat label="Precision" value={percentText(score.precision)} hint="Comments that matched an issue or were a valid extra." />
              )}
              <Stat label="Valid extras" value={score.validExtras} hint="Real problems beyond the planted ones." />
              <Stat label="False positives" value={score.falsePositives} />
            </dl>
          </div>
        </div>
        {leading && (
          <p className={`flex items-start gap-2 rounded-xl px-4 py-3 text-sm leading-6 ${leading.led ? 'bg-success-muted text-success-fg' : 'bg-warning-muted text-warning-fg'}`}>
            {leading.led
              ? <Check size={16} strokeWidth={2.5} aria-hidden="true" className="mt-1 shrink-0" />
              : <X size={16} strokeWidth={2.5} aria-hidden="true" className="mt-1 shrink-0" />}
            <span>{leading.text}</span>
          </p>
        )}
      </div>
    </section>
  );
}
