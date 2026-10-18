import type { ClaimEventDto, ClaimReviewItemDto, LessonClaimReviewDto } from '@/lib/bindings';

/** Shared by the claim review tests: one card per verdict, overridable. */
export const STOPPED_AT = 1_760_000_010_000;

const passage = {
  sourceVersionId: 'v1', title: 'Thermodynamics, ch. 4', url: null,
  text: 'At one atmosphere, pure water boils at 99.97 °C.', startByte: 120, endByte: 184, retrievalKind: 'hybrid',
};

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
    counts: { verified: 12, confirmed: 0, generalKnowledge: 0, queued: claims.length, open: 0, removed: 0 },
    claims,
    ...overrides,
  };
}
