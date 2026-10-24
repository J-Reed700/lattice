import type { ClaimEventDto, ClaimReviewItemDto, ClaimSupportDto, LessonClaimReviewDto, SourceRefDto, TeachingFindingDto } from '@/lib/bindings';

/** Shared by the claim review tests: one card per verdict, overridable. */
export const STOPPED_AT = 1_760_000_010_000;

const passage = {
  sourceVersionId: 'v1', title: 'Thermodynamics, ch. 4', url: null,
  text: 'At one atmosphere, pure water boils at 99.97 °C.', startByte: 120, endByte: 184, retrievalKind: 'hybrid', trust: null,
};

/** The review screen's default claim (UI design §5.4): a field guide that states it, below the bar a measurement needs. */
const GUIDE_TEXT = 'Each autumn, monarch butterflies migrate up to 3,000 miles to wintering sites in central Mexico.';
const GUIDE_QUOTE = 'monarch butterflies migrate up to 3,000 miles';
const guideSource: SourceRefDto = {
  sourceId: 'source-field-guide', sourceVersionId: 'v-field-guide', title: 'Field Guide to North American Butterflies, ch. 7',
  url: 'https://example.org/field-guide/monarch', addedBy: 'research',
};

export function fieldGuideSupport(overrides: Partial<ClaimSupportDto> = {}): ClaimSupportDto {
  const at = GUIDE_TEXT.indexOf(GUIDE_QUOTE);
  return {
    role: 'supports',
    source: guideSource,
    site: 'example.org',
    kind: 'online_book',
    standing: 'established',
    counts: { outcome: 'not_enough' },
    quote: GUIDE_QUOTE,
    quoteSpan: [at, at + GUIDE_QUOTE.length],
    passage: {
      sourceVersionId: guideSource.sourceVersionId, title: guideSource.title, url: guideSource.url,
      text: GUIDE_TEXT, startByte: 2048, endByte: 2048 + GUIDE_TEXT.length, retrievalKind: 'hybrid', trust: null,
    },
    ...overrides,
  };
}

export function claimItem(overrides: Partial<ClaimReviewItemDto> = {}): ClaimReviewItemDto {
  return {
    claimId: 'claim-1',
    location: { unitIndex: 0, unitKind: 'block', unitTitle: 'Phase changes', field: 'body', quote: 'Water boils at 100 °C.', span: [42, 65] },
    statement: 'Water boils at 100 °C at sea level.',
    state: { state: 'queued' },
    verdictKind: { kind: 'contradicted' },
    reason: 'The passage gives 99.97 °C at one atmosphere.',
    evidence: [{ passage, addedBy: 'user' }],
    supportingQuote: null,
    research: [{ queries: ['boiling point of water at sea level'], sourcesAdded: [], passagesFound: 0, at: 1_760_000_002_000 }],
    repairs: [{ attempt: 1, escalated: false, before: 'Water boils at 100 °C.', after: 'Water boils at about 100 °C.', outcome: { outcome: 'rejected', reason: 'The rewrite still contradicts the passage.' }, at: 1_760_000_003_000 }],
    executions: [],
    human: null,
    availableActions: ['confirm', 'edit', 'remove', 'attach_evidence', 'research_with_hint'],
    history: [],
    localCheck: null,
    verifierCheck: null,
    sufficiency: null,
    presentation: {
      statement: 'Monarch butterflies migrate up to 3,000 miles.',
      sentence: 'Each autumn, monarch butterflies migrate up to 3,000 miles.',
      contextBefore: 'Monarchs breed across the northern United States and southern Canada.',
      contextAfter: 'They winter in the fir forests of central Mexico.',
      place: 'Section 2 · Migration',
    },
    support: fieldGuideSupport(),
    alsoSupported: [],
    tried: { searches: 1, pagesRead: 1, pagesMentioning: 1, stated: false, relevant: [guideSource] },
    ...overrides,
  };
}

export function unsupportedItem(index: number, unitIndex = index): ClaimReviewItemDto {
  return claimItem({
    claimId: `claim-${index}`,
    location: { unitIndex, unitKind: 'block', unitTitle: `Section title ${unitIndex + 1}`, field: 'body', quote: `Sentence number ${index}.`, span: null },
    statement: `Statement number ${index}.`,
    verdictKind: { kind: 'unsupported' },
    reason: `No passage supports statement ${index}.`,
    availableActions: ['confirm', 'edit', 'remove', 'general_knowledge', 'attach_evidence', 'research_with_hint'],
    research: [],
    repairs: [],
  });
}

export function userActionAt(at: number): ClaimEventDto {
  return { ordinal: 9, actor: 'user', at, jobId: null, appVersion: 'test', checkerModel: null, event: { kind: 'user_action', action: { action: 'remove' }, note: null } };
}

export function review(claims: ClaimReviewItemDto[], overrides: Partial<LessonClaimReviewDto> = {}): LessonClaimReviewDto {
  return {
    lessonId: 'lesson-1', programId: 'program-1', state: 'needs_review', revision: 7, pass: 3, stopReason: null, changedSinceStop: false,
    counts: { verified: 12, confirmed: 0, generalKnowledge: 0, queued: claims.length, open: 0, removedByYou: 0, editedByYou: 0, rewritten: 0 },
    claims,
    teachingFindings: [],
    ...overrides,
  };
}

/** The finding that used to fail a two-hour lesson job, set aside instead. */
export function teachingFinding(overrides: Partial<TeachingFindingDto> = {}): TeachingFindingDto {
  return {
    findingKey: 'teaching:section-2',
    kind: 'teaching',
    site: 'section',
    unitIndex: 2,
    unitTitle: 'Section 3: Returning results',
    note: 'Section 3 (candidate blocks-2): the example shows "result": "4" where the code returns the number 4.',
    passage: 'The function returns the sum, so the response reads {"result": "4"}.',
    repairs: 2,
    state: 'waiting',
    ...overrides,
  };
}
