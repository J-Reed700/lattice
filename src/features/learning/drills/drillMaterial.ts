import type { CodeReviewDrillDto } from '@/lib/bindings';

import { languageLabel, type ReviewKind } from './drillOptions';

export function kindOf(drill: CodeReviewDrillDto): ReviewKind {
  return drill.material.kind;
}

/** The language of a drill on code; none for a passage. */
export function languageOf(drill: CodeReviewDrillDto): string | null {
  return drill.material.kind === 'code' ? drill.material.language : null;
}

/** What a drill is made of, for a heading: "Python", or "Explanation of Spacing and memory". */
export function materialLabel(drill: CodeReviewDrillDto): string {
  return drill.material.kind === 'code'
    ? languageLabel(drill.material.language)
    : `Explanation of ${drill.material.sourceTitle}`;
}
