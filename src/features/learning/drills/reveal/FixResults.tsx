import { useId } from 'react';

import { Badge } from '@/components/ui/badge';
import { DECIDED_BY, FIX_OUTCOME } from '@/features/learning/drills/reveal/revealCopy';
import {
  issuesNamed,
  numberOf,
  suiteSummary,
  type RankedIssue,
  type SuiteSummary,
} from '@/features/learning/drills/reveal/revealPresentation';
import { panel, sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import type { IssueFix } from '@/lib/bindings';

const SUITE_TONE: Record<SuiteSummary['verdict'], string> = {
  passed: 'bg-success-muted text-success-fg',
  failed: 'bg-danger-muted text-danger-fg',
  not_run: 'bg-surface-raised text-text-secondary',
};

function Output({ label, output }: { label: string; output: string }) {
  return (
    <details className="mt-3">
      <summary className="cursor-pointer text-xs font-medium text-text-secondary hover:text-text-primary">{label}</summary>
      <pre className="mt-2 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg border border-border bg-[color:var(--code-bg)] p-3 font-mono text-xs leading-5 text-[color:var(--code-fg)]">
        {output}
      </pre>
    </details>
  );
}

function SuiteVerdictBox({ summary }: { summary: SuiteSummary }) {
  return (
    <div className={`rounded-2xl px-5 py-4 text-sm leading-6 ${SUITE_TONE[summary.verdict]}`}>
      <p className="font-medium">{summary.headline}</p>
      {summary.detail && <p className="mt-0.5">{summary.detail}</p>}
      {summary.output && <Output label="Suite output" output={summary.output} />}
    </div>
  );
}

function FixRow({ fix, ranked }: { fix: IssueFix; ranked: readonly RankedIssue[] }) {
  const titleId = useId();
  const entry = ranked.find((candidate) => candidate.position === fix.issue);
  const outcome = FIX_OUTCOME[fix.outcome];
  return (
    <article aria-labelledby={titleId} className={`${panel} p-4 sm:p-5`}>
      <header className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
        <h3 id={titleId} className="font-serif text-base leading-snug text-text-primary">
          {entry ? `Issue ${entry.number}: ${entry.issue.patternName}` : `Issue ${fix.issue + 1}`}
        </h3>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="outline" className={outcome.tone}>{outcome.label}</Badge>
          <span className="text-xs text-text-muted">Decided by {DECIDED_BY[fix.decidedBy]}</span>
        </div>
      </header>
      <p className="mt-2 text-sm leading-6 text-text-secondary">{fix.reason}</p>
      {fix.fallback && (
        <p className="mt-1 text-xs leading-5 text-text-muted">
          The issue’s test could not decide it, so a judge did: {fix.fallback}
        </p>
      )}
      {fix.gradedBeside.length > 0 && (
        <p className="mt-1 text-xs leading-5 text-text-muted">
          Its test ran with the fixes of the issues you left: {issuesNamed(fix.gradedBeside, ranked)}.
        </p>
      )}
      {fix.output && <Output label={fix.decidedBy === 'suite' ? 'Suite output' : 'Test output'} output={fix.output} />}
    </article>
  );
}

/** What came of the learner's own fix: the hidden suite's verdict, then each issue's result. */
export function FixResults({ fixes, ranked }: { fixes: readonly IssueFix[]; ranked: readonly RankedIssue[] }) {
  const headingId = useId();
  const rows = [...fixes].sort((a, b) => numberOf(a.issue, ranked) - numberOf(b.issue, ranked));
  if (rows.length === 0) return null;
  const summary = suiteSummary(rows[0].suite, ranked);
  // An issue the suite decided already carries the suite's output: it is shown once.
  const output = rows.some((fix) => fix.output === summary.output) ? null : summary.output;
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <h2 id={headingId} className={sectionHeading}>Your fix</h2>
      <SuiteVerdictBox summary={{ ...summary, output }} />
      <ul className="space-y-3">
        {rows.map((fix) => (
          <li key={fix.issue}>
            <FixRow fix={fix} ranked={ranked} />
          </li>
        ))}
      </ul>
    </section>
  );
}
