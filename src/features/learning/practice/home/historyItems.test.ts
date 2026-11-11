import { describe, expect, it } from 'vitest';

import { outcomeText } from '@/features/learning/drills/DrillChoose';
import type * as Wire from '@/lib/bindings';


import { buildHistory, cardOfActivity, pickContinue, type HistorySources } from './historyItems';

const empty: HistorySources = { coding: [], activities: [], simulations: [], attempts: [], drills: [] };

const activity = (patch: Partial<Wire.LearningPracticalActivityDto>): Wire.LearningPracticalActivityDto =>
  ({ id: 'act', title: 'An activity', kind: 'code_lab', practiceMode: 'practice', updatedAt: 100, ...patch }) as Wire.LearningPracticalActivityDto;

const session = (patch: Partial<Wire.CodingSessionSummaryDto>): Wire.CodingSessionSummaryDto => ({
  sessionId: 's1',
  activityId: 'coding-act',
  title: 'Two sum',
  task: 'build',
  setting: 'coached',
  stage: 'make',
  finished: false,
  passed: null,
  total: null,
  createdAt: 50,
  finishedAt: null,
  ...patch,
});

describe('history', () => {
  it('lists a coding session once, newest first, beside the other work', () => {
    const items = buildHistory({
      ...empty,
      coding: [session({ finished: true, passed: 3, total: 4, finishedAt: 300 })],
      activities: [activity({ id: 'coding-act', kind: 'exercise', updatedAt: 40 }), activity({ id: 'inc', kind: 'incident', updatedAt: 200 })],
    });
    expect(items.map((entry) => entry.id)).toEqual(['coding:s1', 'activity:inc']);
    expect(items[0]).toMatchObject({ card: 'code', result: '3 of 4 passed', target: { kind: 'coding', sessionId: 's1' } });
  });

  it('opens a coding problem that has no session as a coding activity', () => {
    const [entry] = buildHistory({ ...empty, activities: [activity({ id: 'old', kind: 'exercise' })] });
    expect(entry).toMatchObject({ card: 'code', target: { kind: 'coding-activity', activityId: 'old' } });
  });

  it('keeps every other practical kind, opening the activity selected', () => {
    const [entry] = buildHistory({ ...empty, activities: [activity({ id: 'lab', kind: 'code_lab' })] });
    expect(entry).toMatchObject({ target: { kind: 'activity', activityId: 'lab' } });
  });

  it('puts the interview and role-play kinds under their cards', () => {
    expect(cardOfActivity('interview')).toBe('mock');
    expect(cardOfActivity('incident')).toBe('role');
    expect(cardOfActivity('conversation')).toBe('role');
    expect(cardOfActivity('code_review')).toBe('spot');
  });

  it('names the stage a session stopped at, not its identifier', () => {
    const [entry] = buildHistory({ ...empty, coding: [session({ stage: 'reproduce' })] });
    expect(entry?.stoppedAt).toBe('At the Reproduce stage');
  });

  it('opens a written attempt on its own session and lesson', () => {
    const attempt: Wire.LearningPracticeSessionSummaryDto = {
      id: 'sess-9',
      lessonId: 'lesson-4',
      lessonTitle: 'Graphs',
      status: 'active',
      mode: 'practice',
      revision: 1,
      artifactRevision: 1,
      sourceVersionIds: [],
      createdAt: 5,
      updatedAt: 10,
      submittedAt: null,
      gradeStatus: null,
    };
    const [entry] = buildHistory({ ...empty, attempts: [attempt] });
    expect(entry?.target).toEqual({ kind: 'attempt', sessionId: 'sess-9', lessonId: 'lesson-4', mode: 'practice' });
  });

  it('opens a drill on its own attempt and words its result as the drill does', () => {
    const graded = {
      attemptId: 'att-3', title: 'Review a parser', mode: 'practice', status: 'graded', startedAt: 20,
      score: { found: 2, halfFound: 1, missed: 1 },
    } as Wire.CourseDrillAttemptDto;
    const [entry] = buildHistory({ ...empty, drills: [graded] });
    expect(entry?.target).toEqual({ kind: 'drill', attemptId: 'att-3' });
    expect(entry?.result).toBe(outcomeText(graded));
    expect(entry?.result).toBe('Found 2 of 4');
  });

  it('picks the most recent work left open to continue', () => {
    const items = buildHistory({
      ...empty,
      coding: [session({ sessionId: 'done', finished: true, finishedAt: 500 }), session({ sessionId: 'open', createdAt: 400 })],
    });
    expect(pickContinue(items)?.id).toBe('coding:open');
    expect(pickContinue([])).toBeNull();
  });
});
