import { describe, expect, it } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { fitState, isCodingRound, MOCK_DIFFICULTY, mockRoundOptions, startAction, suggestsInterview, visibleCards } from './practiceChoices';

const profile = (patch: Partial<Wire.LearningPracticeProfileDto> = {}): Wire.LearningPracticeProfileDto => ({
  programId: 'course',
  details: '',
  language: null,
  interviewAt: null,
  company: null,
  role: null,
  revision: 0,
  ...patch,
});

describe('which cards the course gets', () => {
  it('offers Code only when the course teaches code', () => {
    expect(visibleCards(fitState({ code: 'fits', reason: null }))).toEqual(['explain', 'spot', 'code', 'mock', 'role']);
    expect(visibleCards(fitState({ code: 'does_not_fit', reason: null }))).toEqual(['explain', 'spot', 'mock', 'role']);
    expect(visibleCards(fitState({ code: 'checking', reason: null }))).not.toContain('code');
    expect(visibleCards(fitState(undefined))).not.toContain('code');
  });

  it('keeps coding rounds out of the mock interview unless code fits', () => {
    expect(mockRoundOptions('fits').map((option) => option.value)).toEqual(['live_coding', 'debugging', 'technical', 'system_design', 'behavioral']);
    expect(mockRoundOptions('does_not_fit').map((option) => option.value)).toEqual(['technical', 'behavioral']);
    expect(isCodingRound('live_coding')).toBe(true);
    expect(isCodingRound('behavioral')).toBe(false);
  });
});

describe('the Mock interview badge', () => {
  it('shows when the profile has an interview date or mentions an interview', () => {
    expect(suggestsInterview(profile({ interviewAt: 1_900_000_000_000 }))).toBe(true);
    expect(suggestsInterview(profile({ details: 'I have an Interview next week' }))).toBe(true);
    expect(suggestsInterview(profile({ details: 'easy problems' }))).toBe(false);
    expect(suggestsInterview(null)).toBe(false);
  });
});

describe('what Start does', () => {
  const context = { lessonId: 'lesson-1', profile: profile({ language: 'Go', company: 'Acme', role: 'Engineer' }) };

  it('starts a coached coding build for the Code card with the chosen sentence', () => {
    const action = startAction('code', { difficulty: 'hard', language: 'go', task: 'repair' }, context);
    expect(action).toMatchObject({
      kind: 'coding-build',
      choice: { lessonId: 'lesson-1', form: 'repair', setting: 'coached', language: 'python', difficulty: 'hard' },
    });
  });

  it('builds code in Python whatever the profile or the sentence says', () => {
    const js = { lessonId: 'lesson-1', profile: profile({ language: 'JavaScript' }) };
    expect(startAction('code', { task: 'build' }, js)).toMatchObject({ choice: { language: 'python' } });
    expect(startAction('code', { task: 'repair', language: 'javascript' }, js)).toMatchObject({ choice: { language: 'python' } });
  });

  it('builds a live coding round of the Mock card in Python', () => {
    const js = { lessonId: 'lesson-1', profile: profile({ language: 'JavaScript' }) };
    expect(startAction('mock', { round: 'live_coding' }, js)).toMatchObject({ kind: 'coding-build', choice: { language: 'python' } });
    expect(startAction('mock', { round: 'debugging' }, js)).toMatchObject({ kind: 'coding-build', choice: { language: 'python' } });
  });

  it('opens a code lab composer for a project', () => {
    expect(startAction('code', { task: 'project' }, context)).toMatchObject({ kind: 'activity', composer: { kind: 'code_lab' } });
  });

  it('turns a coding round of the Mock card into a timed build in the chosen setting', () => {
    const action = startAction('mock', { round: 'debugging', setting: 'watched', minutes: '30' }, context);
    expect(action).toMatchObject({ kind: 'coding-build', choice: { form: 'repair', setting: 'watched', budgetSeconds: 1800 } });
  });

  it('uses one difficulty for coding rounds of the Mock card', () => {
    const action = startAction('mock', { round: 'live_coding' }, context);
    expect(MOCK_DIFFICULTY).toBe('medium');
    expect(action).toMatchObject({ kind: 'coding-build', choice: { difficulty: MOCK_DIFFICULTY } });
  });

  it('turns a spoken round into an interview composer preset with the company and role', () => {
    const action = startAction('mock', { round: 'system_design' }, context);
    expect(action).toMatchObject({
      kind: 'activity',
      composer: { kind: 'interview', interview: { companyName: 'Acme', targetRole: 'Engineer', roundKind: 'system_design' } },
    });
  });

  it('maps Explain, Spot and Role-play', () => {
    expect(startAction('explain', { style: 'beginner' }, context)).toEqual({ kind: 'attempt', mode: 'feynman' });
    expect(startAction('explain', { style: 'own' }, context)).toEqual({ kind: 'attempt', mode: 'practice' });
    expect(startAction('spot', {}, context)).toEqual({ kind: 'drill' });
    expect(startAction('role', { scenario: 'design_review' }, context)).toMatchObject({ composer: { kind: 'system_design' } });
    expect(startAction('role', { scenario: 'incident' }, context)).toMatchObject({ composer: { kind: 'incident' } });
  });
});
