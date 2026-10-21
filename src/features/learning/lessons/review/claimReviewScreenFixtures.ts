import { claimItem } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimReviewItemDto, ClaimSupportDto, LearningSufficiencyDto, ResearchSummaryDto } from '@/lib/bindings';

/*
 * The review screen's test shapes, built on the shared `claimItem()`: a
 * support on a named source, research sums and a sufficiency with what it
 * lacks. Biology and history, so no copy assumes a software course.
 */

const FIELD_GUIDE_TEXT = 'Monarchs breed across the northern range in summer. Each autumn the eastern population migrates up to 3,000 miles to wintering sites in central Mexico. The return trip takes several generations.';

export function support(overrides: Partial<ClaimSupportDto> = {}): ClaimSupportDto {
  const quote = 'Each autumn the eastern population migrates up to 3,000 miles to wintering sites in central Mexico.';
  const start = new TextEncoder().encode(FIELD_GUIDE_TEXT.slice(0, FIELD_GUIDE_TEXT.indexOf(quote))).length;
  return {
    role: 'supports',
    source: { sourceId: 'source-guide', sourceVersionId: 'version-guide', title: 'Field Guide to North American Butterflies', url: 'https://butterflies.example.org/monarch', addedBy: 'research' },
    site: 'butterflies.example.org',
    kind: 'online_book',
    standing: 'unknown',
    counts: { outcome: 'not_enough' },
    quote,
    quoteSpan: [start, start + new TextEncoder().encode(quote).length],
    passage: { sourceVersionId: 'version-guide', title: 'Field Guide to North American Butterflies', url: 'https://butterflies.example.org/monarch', text: FIELD_GUIDE_TEXT, startByte: 0, endByte: FIELD_GUIDE_TEXT.length, retrievalKind: 'hybrid', trust: null },
    ...overrides,
  };
}

export const tried = (overrides: Partial<ResearchSummaryDto> = {}): ResearchSummaryDto => ({
  searches: 9, pagesRead: 214, pagesMentioning: 0, stated: false, relevant: [], ...overrides,
});

export const sufficiency = (missing: string | null, overrides: Partial<LearningSufficiencyDto> = {}): LearningSufficiencyDto => ({
  claimKind: 'general', subject: 'Monarch migration', sufficient: false, bar: 'established', label: null, missing, ceiling: null, reasons: [], ...overrides,
});

/** A claim the course has only an unidentified author's page for. */
export function waitingOnSource(index: number, sourceId = 'source-guide', title = 'Field Guide to North American Butterflies'): ClaimReviewItemDto {
  const statement = `Monarch fact number ${index} is stated in the guide.`;
  return claimItem({
    claimId: `claim-${index}`,
    location: { unitIndex: index % 3, unitKind: 'block', unitTitle: `Migration ${index % 3 + 1}`, field: 'body', quote: `**Monarch** fact number ${index} [5] is stated in the guide.`, span: null },
    statement,
    state: { state: 'queued' },
    verdictKind: { kind: 'insufficient_standing', bar: 'established', have: [] },
    reason: null,
    presentation: { statement, sentence: statement, contextBefore: 'Monarchs are large orange butterflies.', contextAfter: 'They winter in Mexico.', place: `Section ${index % 3 + 1} · Migration ${index % 3 + 1}` },
    support: support({ source: { ...support().source, sourceId, sourceVersionId: `version-${sourceId}`, title } }),
    alsoSupported: [],
    tried: tried(),
    sufficiency: sufficiency('no_identified_source'),
    availableActions: ['confirm', 'edit', 'remove', 'attach_evidence', 'research_with_hint'],
    research: [],
    repairs: [],
  });
}

/** 33 claims, 15 of them on one field guide, the rest spread over a history text and none. */
export function thirtyThreeClaims(): ClaimReviewItemDto[] {
  return Array.from({ length: 33 }, (_, index) => {
    if (index < 15) return waitingOnSource(index + 1);
    if (index < 21) return waitingOnSource(index + 1, 'source-chronicle', 'Chronicle of the Monarch Reserves');
    return { ...waitingOnSource(index + 1), support: null, verdictKind: { kind: 'unsupported' }, sufficiency: null };
  });
}
