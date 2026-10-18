import type {
  LearningClaimTrustDto,
  LearningKindBarDto,
  LearningSourceTrustDto,
  LearningSourceTrustEntryDto,
  LearningTrustLevel,
} from '@/lib/bindings';

export function trustOf(level: LearningTrustLevel, extra: Partial<LearningSourceTrustDto> = {}): LearningSourceTrustDto {
  return {
    sourceVersionId: 'version-1',
    level,
    subject: null,
    reasons: [{ code: 'identified_publisher', text: 'Identified author: Jane Doe (JSON-LD author)' }],
    overrideScope: null,
    ...extra,
  };
}

export function entryOf(trust: LearningSourceTrustDto, extra: Partial<LearningSourceTrustEntryDto> = {}): LearningSourceTrustEntryDto {
  return {
    sourceId: 'source-1',
    sourceVersionId: trust.sourceVersionId,
    trust,
    standing: 'identified',
    standingReasons: [{ code: 'author_metadata', text: 'Byline names Jane Doe' }],
    provenance: {
      sourceVersionId: trust.sourceVersionId,
      originKey: 'web:example-dev.net',
      site: 'example-dev.net',
      publisher: null,
      author: 'Jane Doe',
      siteName: 'Example Dev',
      publishedAt: 1_743_897_600,
      modifiedAt: null,
      archivedFrom: null,
      notExtracted: null,
      relations: [],
    },
    trackRecord: { right: 0, wrong: 0 },
    overrides: [],
    impact: { source: { versions: 1, claims: 3, lessons: 2 }, site: { versions: 4, claims: 5, lessons: 3 } },
    ...extra,
  };
}

/** The static Specification bar: authoritative or primary alone. */
export const SPECIFICATION_BAR: LearningKindBarDto = {
  claimKind: 'specification',
  staticLowest: 'authoritative',
  lowestAccepted: 'authoritative',
  levels: (['primary', 'authoritative', 'established', 'community', 'unknown'] as const).map((level) => ({
    level,
    acceptedAlone: level === 'primary' || level === 'authoritative',
    basis: 'static' as const,
    held: 0,
    failed: 0,
    probabilityReliable: 0.5,
    since: null,
  })),
  reasons: [],
};

/** A specification claim about the OpenAI API supported only by a blog. */
export function blogOnlyClaim(): LearningClaimTrustDto {
  return {
    profile: {
      claimKind: 'specification',
      subject: { name: 'OpenAI API', kind: 'product', aliases: ['OpenAI platform'], classes: ['LLM APIs'] },
      measurementOrigin: null,
      reasons: [],
    },
    supports: [entryOf(trustOf('established', { subject: 'OpenAI API' }))],
    bar: SPECIFICATION_BAR,
  };
}
