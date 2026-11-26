import { createElement, type ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import {
  LEARNING_PROGRAMS_KEY,
  learningClaimReviewPrefix,
  learningCoursePlanKey,
  learningLessonEvidencePrefix,
  learningOutlineEvidencePrefix,
  learningPlanKey,
  learningProgramKey,
  learningQuizOverviewKey,
} from '@/features/learning/api/learningQueryKeys';
import { registerLearningAffected } from '@/features/learning/edits/affected';
import { useResolveLearningClaim } from '@/features/learning/lessons/review/useLessonClaimReview';
import { refreshAffected } from '@/shared/edits/affected';
import { originLabel } from '@/shared/edits/origins';

const api = vi.hoisted(() => ({ resolveLearningClaim: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { resolveLearningClaim: api.resolveLearningClaim } }));

const PROGRAM_ID = 'program-1';
const LESSON_ID = 'lesson-1';
const SCOPE = { feature: 'learning', container: PROGRAM_ID };

const EXPECTED_KEYS: Record<string, readonly (readonly unknown[])[]> = {
  'learning.program': [learningProgramKey(PROGRAM_ID)],
  'learning.plan': [
    learningPlanKey(PROGRAM_ID),
    learningCoursePlanKey(PROGRAM_ID),
    learningOutlineEvidencePrefix(PROGRAM_ID),
    LEARNING_PROGRAMS_KEY,
  ],
  'learning.claims': [learningClaimReviewPrefix(PROGRAM_ID), learningLessonEvidencePrefix(PROGRAM_ID)],
  'learning.quizzes': [learningQuizOverviewKey(PROGRAM_ID)],
  'learning.recall': [['learning-recall-v2'], ['learning-memory'], ['learning-practice-due']],
  'study.decks': [['study']],
};

describe('Learning affected registrations and action labels', () => {
  beforeEach(() => {
    vi.resetAllMocks();
  });

  it.each(Object.entries(EXPECTED_KEYS))('invalidates %s with its exact query keys', async (name, keys) => {
    registerLearningAffected();
    const client = new QueryClient();
    const spy = vi.spyOn(client, 'invalidateQueries').mockResolvedValue();

    await refreshAffected([name], SCOPE, client);

    const actual = spy.mock.calls.map(([filters]) => filters?.queryKey);
    expect(actual).toHaveLength(keys.length);
    expect(actual).toEqual(expect.arrayContaining([...keys]));
  });

  it('registers human-readable origin labels for learning claim and teaching actions', () => {
    registerLearningAffected();
    expect(originLabel({ type: 'action', name: 'claim.edit' })).toBe('Edited in the claim review');
    expect(originLabel({ type: 'action', name: 'claim.remove' })).toBe('Removed in the claim review');
    expect(originLabel({ type: 'action', name: 'claim.put_back' })).toBe('Put back in the claim review');
    expect(originLabel({ type: 'action', name: 'claim.restore' })).toBe('Original restored in the claim review');
    expect(originLabel({ type: 'action', name: 'teaching.remove_sentence' })).toBe('Sentence removed from a teaching note');
  });

  it('invalidates learning.claims and learning.program when resolving a claim', async () => {
    registerLearningAffected();
    api.resolveLearningClaim.mockResolvedValue({ ok: true, data: { revision: 8, outcome: 'resolved' } });
    const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
    const spy = vi.spyOn(client, 'invalidateQueries').mockResolvedValue();
    const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children);

    const { result } = renderHook(() => useResolveLearningClaim(PROGRAM_ID, LESSON_ID), { wrapper });
    await act(async () => {
      result.current.mutate({ claimId: 'claim-1', action: { action: 'confirm', note: null, sourceVersionIds: [] }, expectedRevision: 7 });
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    const actual = spy.mock.calls.map(([filters]) => filters?.queryKey);
    const expected = [...EXPECTED_KEYS['learning.claims'], ...EXPECTED_KEYS['learning.program']];
    expect(actual).toHaveLength(expected.length);
    expect(actual).toEqual(expect.arrayContaining(expected));
  });
});
