import { useId } from 'react';

import { Badge } from '@/components/ui/badge';
import { RATING } from '@/features/learning/drills/reveal/revealCopy';
import type { RankedIssue } from '@/features/learning/drills/reveal/revealPresentation';
import { sectionHeading } from '@/features/learning/drills/reveal/revealStyle';
import type { RatedPatternDto } from '@/lib/bindings';

/** Each pattern the drill's issues belong to, and how this attempt rated it. */
export function PatternList({ ratings, ranked }: {
  ratings: readonly RatedPatternDto[];
  ranked: readonly RankedIssue[];
}) {
  const headingId = useId();
  if (ratings.length === 0) return null;
  const numberOfPattern = (pattern: string) =>
    Math.min(
      Number.MAX_SAFE_INTEGER,
      ...ranked.filter((entry) => entry.issue.pattern === pattern).map((entry) => entry.number),
    );
  const rows = ratings
    .map((rated) => ({ rated, number: numberOfPattern(rated.pattern) }))
    .sort((a, b) => a.number - b.number);
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <div>
        <h2 id={headingId} className={sectionHeading}>Patterns this drill practised</h2>
        <p className="mt-1 text-sm text-text-secondary">
          A pattern’s rating sets when it comes back: found by your first comment is Easy, found is Good, half found is Hard, missed is Again. An also-valid issue you did not raise gives no rating.
        </p>
      </div>
      <ul className="grid gap-2 sm:grid-cols-2">
        {rows.map(({ rated }, index) => (
          <li key={index} className="flex items-center justify-between gap-3 rounded-xl border border-border bg-surface px-4 py-2.5">
            <span className="min-w-0 text-sm text-text-primary">{rated.patternName}</span>
            <Badge variant="outline" className={RATING[rated.rating].tone}>{RATING[rated.rating].label}</Badge>
          </li>
        ))}
      </ul>
    </section>
  );
}
