import { useId } from 'react';

import { ISSUE_OUTCOME } from '@/features/learning/drills/reveal/revealCopy';
import type { RankedIssue } from '@/features/learning/drills/reveal/revealPresentation';
import { sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import { CriteriaList, type CriterionLevel, type CriterionView } from '@/features/learning/practice/shared/CriteriaList';
import type { IssueResultKind } from '@/lib/bindings';

const LEVEL: Record<IssueResultKind, CriterionLevel> = { found: 'met', half_found: 'partly', missed: 'not_met' };

function criterionOf(entry: RankedIssue): CriterionView {
  const { issue } = entry;
  return {
    id: issue.id,
    title: issue.patternName,
    outcome: { label: ISSUE_OUTCOME[issue.outcome].label, level: LEVEL[issue.outcome] },
    evidence: [],
  };
}

/** How each planted issue went, as criteria, in the order of the issues below. */
export function IssueOutcomes({ planted }: { planted: readonly RankedIssue[] }) {
  const headingId = useId();
  if (planted.length === 0) return null;
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <h2 id={headingId} className={sectionHeading}>How each issue went</h2>
      <CriteriaList criteria={planted.map(criterionOf)} />
    </section>
  );
}
