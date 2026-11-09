import { useId } from 'react';

import { ISSUE_OUTCOME } from '@/features/learning/drills/reveal/revealCopy';
import type { RankedIssue } from '@/features/learning/drills/reveal/revealPresentation';
import { sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import { CriteriaList, type CriterionView } from '@/features/learning/practice/shared/CriteriaList';
import type { IssueResultKind } from '@/lib/bindings';

const MAX_CREDIT = 2;

/** Found earns full credit, half found half of it, missed none. */
const CREDIT: Record<IssueResultKind, number> = { found: 2, half_found: 1, missed: 0 };

function criterionOf(entry: RankedIssue): CriterionView {
  const { issue } = entry;
  return {
    id: issue.id,
    title: issue.patternName,
    points: CREDIT[issue.outcome],
    maxPoints: MAX_CREDIT,
    evidence: [],
    feedback: ISSUE_OUTCOME[issue.outcome].label,
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
