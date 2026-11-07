import { UNVERIFIED_REASONS } from '@/features/learning/lessons/review/claimReviewCopy';
import type { LearningLessonEvidenceDto, UnverifiedReasonDto } from '@/lib/bindings';

export type UnverifiedStatement = { key: string; statement: string; reason: UnverifiedReasonDto };

/** The statements of one section that the lesson published without verifying them. */
export function unverifiedStatements(report: LearningLessonEvidenceDto | null | undefined, sectionIndex: number): UnverifiedStatement[] {
  return (report?.teachingClaims ?? []).flatMap((claim, index) => (
    claim.resolution === 'not_verified' && claim.sectionIndex === sectionIndex
      ? [{ key: `${index}:${claim.claim}`, statement: claim.claim, reason: claim.unverifiedReason ?? 'unchecked' }]
      : []
  ));
}

/** Always visible under its section, with citations shown or not. */
export function UnverifiedStatements({ statements }: { statements: UnverifiedStatement[] }) {
  if (statements.length === 0) return null;
  return (
    <div role="note" className="mt-4 rounded-lg border border-amber-600/30 p-3 text-xs leading-5 text-text-primary">
      <p className="font-medium">Not verified:</p>
      <ul className="mt-1 space-y-1">
        {statements.map((item) => (
          <li key={item.key}>
            <span>“{item.statement}”</span>{' '}
            <span className="text-text-secondary">{UNVERIFIED_REASONS[item.reason]}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
