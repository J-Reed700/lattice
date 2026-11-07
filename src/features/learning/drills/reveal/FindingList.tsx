import { useId } from 'react';

import { CommentCard } from '@/features/learning/drills/reveal/CommentCard';
import { FINDING_OUTCOME } from '@/features/learning/drills/reveal/revealCopy';
import { matchLabel, type RankedIssue } from '@/features/learning/drills/reveal/revealPresentation';
import { sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import type { RevealedFindingDto } from '@/lib/bindings';

/** The learner's comments in the order they made them, each with what the grader made of it. */
export function FindingList({ findings, ranked }: {
  findings: readonly RevealedFindingDto[];
  ranked: readonly RankedIssue[];
}) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <h2 id={headingId} className={sectionHeading}>Your comments</h2>
      {findings.length === 0 ? (
        <p className="text-sm text-text-secondary">You left no comments.</p>
      ) : (
        <ol className="space-y-3">
          {findings.map((finding, index) => {
            const words = FINDING_OUTCOME[finding.outcome];
            return (
              <li key={index}>
                <CommentCard
                  number={index + 1}
                  span={finding.span}
                  text={finding.text}
                  severity={finding.severity}
                  verdict={{
                    label: finding.outcome === 'matches' ? matchLabel(finding, ranked) : words.label,
                    tone: words.tone,
                    reason: finding.reason,
                  }}
                />
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}
