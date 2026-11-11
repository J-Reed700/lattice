import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { usePracticeView } from './usePracticeView';

describe('usePracticeView', () => {
  it('starts at home with nothing visited', () => {
    const { result } = renderHook(() => usePracticeView());
    expect(result.current.isHome).toBe(true);
    expect(result.current.visited.size).toBe(0);
  });

  it('keeps a retained screen visited after the learner goes home', () => {
    const { result } = renderHook(() => usePracticeView());
    act(() => result.current.openDrill());
    act(() => result.current.openAttempt('feynman'));
    act(() => result.current.openHome());
    expect(result.current.isHome).toBe(true);
    expect([...result.current.visited].sort()).toEqual(['attempt', 'drill']);
    expect(result.current.attemptMode).toBe('feynman');
  });

  it('opens a written attempt and a drill on the ids it was given and keeps them across tabs', () => {
    const { result } = renderHook(() => usePracticeView());
    act(() => result.current.openAttempt('practice', { sessionId: 's-1', lessonId: 'l-1' }));
    expect(result.current.view).toEqual({ kind: 'attempt', mode: 'practice', attempt: { sessionId: 's-1', lessonId: 'l-1' } });
    act(() => result.current.openDrill('d-1'));
    act(() => result.current.openActivity());
    expect(result.current.attempt).toEqual({ sessionId: 's-1', lessonId: 'l-1' });
    expect(result.current.drillAttemptId).toBe('d-1');
  });

  it('opens a fresh attempt and drill when no id is given', () => {
    const { result } = renderHook(() => usePracticeView());
    act(() => result.current.openAttempt('practice', { sessionId: 's-1', lessonId: 'l-1' }));
    act(() => result.current.openAttempt());
    expect(result.current.attempt).toBeNull();
    act(() => result.current.openDrill('d-1'));
    act(() => result.current.openDrill());
    expect(result.current.drillAttemptId).toBeNull();
  });

  it('does not retain the coding screens', () => {
    const { result } = renderHook(() => usePracticeView());
    act(() => result.current.openCoding('session-1'));
    expect(result.current.view).toEqual({ kind: 'coding', sessionId: 'session-1' });
    expect(result.current.visited.size).toBe(0);
  });

  it('remembers the activity and composer request of the practical screen', () => {
    const { result } = renderHook(() => usePracticeView());
    const composer = { id: 'request-1', kind: 'interview' as const };
    act(() => result.current.openActivity('activity-1', composer));
    expect(result.current.activity).toEqual({ activityId: 'activity-1', composer });
    act(() => result.current.openActivity());
    expect(result.current.activity).toEqual({ activityId: null, composer: null });
  });

  it('opens a coding build with the choice it was given', () => {
    const { result } = renderHook(() => usePracticeView());
    const choice = { task: 'problem' as const, setting: 'coached' as const, language: 'python', difficulty: 'easy' as const };
    act(() => result.current.openCodingBuild(choice));
    expect(result.current.view).toEqual({ kind: 'coding-build', choice });
  });

  it('discards every retained screen on reset', () => {
    const { result } = renderHook(() => usePracticeView());
    act(() => result.current.openActivity('activity-1'));
    act(() => result.current.reset());
    expect(result.current.isHome).toBe(true);
    expect(result.current.visited.size).toBe(0);
    expect(result.current.activity.activityId).toBeNull();
  });
});
