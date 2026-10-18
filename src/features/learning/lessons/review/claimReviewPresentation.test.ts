import { describe, expect, it } from 'vitest';

import {
  applyFilter,
  changedSince,
  checkerColumns,
  claimFilterOf,
  confirmNeedsNote,
  countsLine,
  describeEvent,
  filterCounts,
  groupBySection,
  heldLessonIds,
  initialFilter,
  jobHoldingLesson,
  unitLabel,
  verdictLabel,
} from '@/features/learning/lessons/review/claimReviewPresentation';
import { claimItem, review, STOPPED_AT, unsupportedItem, userActionAt } from '@/features/learning/lessons/review/claimReviewTestData';
import type { ClaimEventDto, LearningGenerationJob } from '@/lib/bindings';

const job = (overrides: Partial<LearningGenerationJob>): LearningGenerationJob => ({
  id: 'job-1', programId: 'program-1', operationId: 'op', kind: 'lesson_preparation', payloadSha256: 'hash', baseRevisionNumber: 1,
  status: 'completed', progressCompleted: 1, progressTotal: 1, progressMessage: 'Needs your review: 2 claims', activity: null,
  resultId: 'needs_review:lesson-1,lesson-2', error: null, retryOfJobId: null, createdAt: 1, startedAt: 1, finishedAt: STOPPED_AT,
  ...overrides,
});

describe('claim review presentation', () => {
  it('names every verdict honestly, a dispute with the Verifier’s side', () => {
    expect(verdictLabel({ kind: 'unsupported' })).toBe('Unsupported');
    expect(verdictLabel({ kind: 'contradicted' })).toBe('Contradicted');
    expect(verdictLabel({ kind: 'no_source' })).toBe('No source found');
    expect(verdictLabel({ kind: 'could_not_judge' })).toBe('Could not judge');
    expect(verdictLabel({ kind: 'general_knowledge_proposed' })).toBe('General knowledge proposed');
    expect(verdictLabel({ kind: 'disputed', verifier: { kind: 'contradicted' } })).toBe('Disputed by the Verifier · contradicted');
    expect(verdictLabel(null)).toBe('Not checked yet');
  });

  it('requires a note to confirm a contradiction, local or by the Verifier, and only then', () => {
    expect(confirmNeedsNote(claimItem())).toBe(true);
    expect(confirmNeedsNote(claimItem({ verdictKind: { kind: 'disputed', verifier: { kind: 'contradicted' } } }))).toBe(true);
    expect(confirmNeedsNote(claimItem({ verdictKind: { kind: 'disputed', verifier: { kind: 'unsupported' } } }))).toBe(false);
    expect(confirmNeedsNote(unsupportedItem(2))).toBe(false);
  });

  it('files each state under one filter and counts them', () => {
    const claims = [
      claimItem(),
      claimItem({ claimId: 'reopened', state: { state: 'reopened', cause: { cause: 'verifier_dispute' }, previous: { state: 'confirmed', at: 1 } } }),
      claimItem({ claimId: 'open', state: { state: 'open', machineAttemptsLeft: 1 } }),
      claimItem({ claimId: 'verified', state: { state: 'resolved_by_evidence' } }),
      claimItem({ claimId: 'machine-removed', state: { state: 'removed', at: 1, by: 'machine' } }),
      claimItem({ claimId: 'user-removed', state: { state: 'removed', at: 1, by: 'user' } }),
      claimItem({ claimId: 'confirmed', state: { state: 'confirmed', at: 1 } }),
      claimItem({ claimId: 'general', state: { state: 'general_knowledge', at: 1, by: 'user' } }),
    ];
    expect(claims.map(claimFilterOf)).toEqual(['needs_you', 'needs_you', 'repairing', 'machine', 'machine', 'changed', 'confirmed', 'general']);
    expect(filterCounts(claims)).toMatchObject({ needs_you: 2, repairing: 1, machine: 2, confirmed: 1, general: 1, changed: 1, all: 8 });
    expect(applyFilter(claims, 'machine').map((claim) => claim.claimId)).toEqual(['verified', 'machine-removed']);
    expect(initialFilter(claims)).toBe('needs_you');
    expect(initialFilter([claims[3]])).toBe('all');
  });

  it('groups claims by unit in lesson order with the unit in words', () => {
    const sections = groupBySection([unsupportedItem(1, 4), unsupportedItem(2, 0), unsupportedItem(3, 4)]);
    expect(sections.map((section) => [section.label, section.claims.length])).toEqual([['Section 1 · Section title 1', 1], ['Section 5 · Section title 5', 2]]);
    expect(unitLabel({ unitIndex: 6, unitKind: 'answer_key', unitTitle: 'Boiling', field: 'explanation', quote: '', span: null })).toBe('Question · Boiling · answer key');
  });

  it('finds the lessons a completed job holds, ignoring superseded and unfinished attempts', () => {
    expect(heldLessonIds(job({}))).toEqual(['lesson-1', 'lesson-2']);
    expect(heldLessonIds(job({ status: 'running' }))).toEqual([]);
    expect(heldLessonIds(job({ resultId: 'lesson-1' }))).toEqual([]);
    const retry = job({ id: 'job-2', status: 'pending', resultId: null, retryOfJobId: 'job-1' });
    expect(jobHoldingLesson([job({}), retry], 'lesson-1')).toBeNull();
    expect(jobHoldingLesson([job({})], 'lesson-2')?.id).toBe('job-1');
  });

  it('allows Resume only after a user event since the job stopped', () => {
    expect(changedSince(review([claimItem()]), STOPPED_AT)).toBe(false);
    expect(changedSince(review([claimItem({ history: [userActionAt(STOPPED_AT + 1)] })]), STOPPED_AT)).toBe(true);
    expect(changedSince(review([claimItem()]), null)).toBe(true);
  });

  it('puts the local checker and the Verifier side by side from the history', () => {
    const checked = (role: 'local' | 'verifier', verdict: 'supported' | 'contradicted', ordinal: number): ClaimEventDto => ({
      ordinal, actor: 'machine', at: ordinal, jobId: null, appVersion: 'test', checkerModel: null,
      event: { kind: 'checked', check: { checker: { role, model: role === 'local' ? 'qwen' : 'claude' }, windowTokens: 8192, fingerprint: 'f', evidence: [],
        reason: role === 'local' ? 'Entailed.' : 'The passage says 99.97 °C.',
        result: verdict === 'supported' ? { verdict, sourceVersionId: 'v1', supportingQuote: 'boils at 100' } : { verdict: 'failed', kind: { kind: 'contradicted' } } } },
    });
    const item = claimItem({ history: [checked('local', 'supported', 1), checked('verifier', 'contradicted', 2)] });
    expect(checkerColumns(item)).toMatchObject({ local: { verdict: 'Supported', model: 'qwen' }, verifier: { verdict: 'Contradicted', reason: 'The passage says 99.97 °C.' } });
    expect(checkerColumns(claimItem({ history: [checked('local', 'supported', 1)] }))).toBeNull();
    expect(describeEvent(item.history[1].event)).toBe('Verifier (claude): contradicted · The passage says 99.97 °C.');
  });

  it('summarises the counts without hiding confirmations', () => {
    expect(countsLine(review([claimItem()], { counts: { verified: 12, confirmed: 3, generalKnowledge: 2, queued: 1, open: 0, removed: 0 } })))
      .toBe('12 verified · 3 confirmed by you · 2 general knowledge · 1 waiting for you');
  });
});
