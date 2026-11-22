import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { PROGRAM_KIND_ID } from '@/features/learning/practice/kinds';
import { resetCodingBuildStore, useCodingBuildStore } from '@/features/learning/practice/kinds/program/codingBuildStore';
import type * as Wire from '@/lib/bindings';

import { PracticeHome, type PracticeHomeProps } from './PracticeHome';

import type { HistoryItem } from './historyItems';

const data = vi.hoisted(() => ({
  profile: undefined as unknown,
  fit: undefined as unknown,
  due: [] as unknown[],
  history: { items: [] as unknown[], continueItem: null as unknown },
  buildJob: null as Wire.JobDto | null,
}));

vi.mock('./usePracticeData', () => ({
  usePracticeProfile: () => ({ profile: data.profile, loading: false, error: null, retry: vi.fn(), reload: vi.fn(), set: vi.fn() }),
  usePracticeFit: () => ({ fit: data.fit, checking: false, error: null, retry: vi.fn() }),
  usePracticeDue: () => ({ items: data.due, loading: false, error: null, retry: vi.fn() }),
  usePracticeHistory: () => ({ ...data.history, loading: false, error: null, retry: vi.fn() }),
  usePracticeBuildJob: () => data.buildJob,
}));

const fitOf = (fit: Wire.KindFit): Wire.PracticeFitDto => ({ kinds: [{ kind: PROGRAM_KIND_ID, fit, reason: null }] });

const profile = (patch: Partial<Wire.LearningPracticeProfileDto> = {}): Wire.LearningPracticeProfileDto => ({
  programId: 'course-1',
  details: '',
  language: null,
  interviewAt: null,
  company: null,
  role: null,
  revision: 1,
  ...patch,
});

function homeProps(overrides: Partial<PracticeHomeProps> = {}): PracticeHomeProps {
  return {
    programId: 'course-1',
    goal: 'Learn to code',
    lessonId: 'lesson-1',
    onStart: vi.fn(),
    onOpenTarget: vi.fn(),
    onStartDue: vi.fn(),
    onOpenWorkbench: vi.fn(),
    ...overrides,
  };
}

function show(overrides: Partial<PracticeHomeProps> = {}) {
  const props = homeProps(overrides);
  render(<PracticeHome {...props} />);
  return props;
}

const cardTitles = () => screen.getAllByRole('heading', { level: 3 }).map((heading) => heading.textContent?.trim());

beforeEach(() => {
  resetCodingBuildStore();
  data.profile = profile();
  data.fit = fitOf('fits');
  data.due = [];
  data.history = { items: [], continueItem: null };
  data.buildJob = null;
});

afterEach(() => localStorage.clear());

describe('PracticeHome', () => {
  it('shows the five cards in a fixed order when the course teaches code', () => {
    show();
    expect(cardTitles()).toEqual(['Explain', 'Spot the mistakes', 'Code', 'Mock interview', 'Role-play']);
    expect(screen.getAllByRole('button', { name: /^Start (Explain|Spot|Code|Mock|Role)/ })).toHaveLength(5);
  });

  it('leaves out Code when the course does not teach code', () => {
    data.fit = fitOf('does_not_fit');
    show();
    expect(cardTitles()).toEqual(['Explain', 'Spot the mistakes', 'Mock interview', 'Role-play']);
  });

  it('names code only when the course teaches it', () => {
    data.fit = fitOf('does_not_fit');
    const { unmount } = render(<PracticeHome {...homeProps()} />);
    for (const card of screen.getAllByRole('article')) expect(card.textContent).not.toMatch(/code|coding|debugging|system design/i);
    unmount();

    data.fit = fitOf('fits');
    show();
    expect(screen.getByTestId('card-spot')).toHaveTextContent('code or an explanation');
    expect(screen.getByTestId('card-mock')).toHaveTextContent('live coding, debugging, technical questions, system design or behavioral');
  });

  it('badges Mock interview when there is an interview date, without moving a card', () => {
    data.profile = profile({ interviewAt: Date.now() + 10 * 86_400_000 });
    show();
    const mock = screen.getByTestId('card-mock');
    expect(within(mock).getByText("Fits what you're preparing for")).toBeVisible();
    expect(screen.getAllByText("Fits what you're preparing for")).toHaveLength(1);
    expect(cardTitles()).toEqual(['Explain', 'Spot the mistakes', 'Code', 'Mock interview', 'Role-play']);
  });

  it('badges Mock interview when the details mention an interview, in any case', () => {
    data.profile = profile({ details: 'Ready for the INTERVIEW loop' });
    show();
    expect(within(screen.getByTestId('card-mock')).getByText("Fits what you're preparing for")).toBeVisible();
  });

  it('shows no badge when the profile mentions nothing of the kind', () => {
    show();
    expect(screen.queryByText("Fits what you're preparing for")).not.toBeInTheDocument();
  });

  it('reads the Code card as a sentence and starts a coached build from it', async () => {
    const user = userEvent.setup();
    const props = show();
    const card = screen.getByTestId('card-code');
    expect(within(card).getByRole('combobox', { name: 'Difficulty' })).toHaveValue('easy');
    await user.click(within(card).getByRole('button', { name: /^Start/ }));
    expect(props.onStart).toHaveBeenCalledWith(
      expect.objectContaining({ kind: 'coding-build', choice: expect.objectContaining({ lessonId: 'lesson-1', setting: 'coached', difficulty: 'easy', form: 'build' }) }),
    );
  });

  it('offers Python only for the Code card and starts in it with another profile language', async () => {
    data.profile = profile({ language: 'JavaScript' });
    const user = userEvent.setup();
    const props = show();
    const card = screen.getByTestId('card-code');
    expect(within(card).queryByRole('combobox', { name: 'Language' })).not.toBeInTheDocument();
    expect(card).toHaveTextContent('Python');
    await user.click(within(card).getByRole('button', { name: /^Start/ }));
    expect(props.onStart).toHaveBeenCalledWith(
      expect.objectContaining({ kind: 'coding-build', choice: expect.objectContaining({ language: 'python' }) }),
    );
  });

  it('continues the work left open', async () => {
    const user = userEvent.setup();
    const open: HistoryItem = {
      id: 'coding:s1',
      card: 'code',
      title: 'Two sum',
      when: 1,
      detail: 'Coached · Problem',
      result: 'In progress',
      resumable: true,
      stoppedAt: 'At the code stage',
      target: { kind: 'coding', sessionId: 's1' },
    };
    data.history = { items: [open], continueItem: open };
    const props = show();
    await user.click(screen.getByRole('button', { name: /^Continue/ }));
    expect(props.onOpenTarget).toHaveBeenCalledWith({ kind: 'coding', sessionId: 's1' });
  });

  it('starts the most overdue item that is due', async () => {
    const user = userEvent.setup();
    const item: Wire.DueItemDto = { kind: 'drill_lesson', label: 'Strings', count: 1, dueAt: 1, lessonId: 'lesson-1', material: null };
    data.due = [item];
    const props = show();
    await user.click(screen.getByRole('button', { name: 'Start the most overdue' }));
    expect(props.onStartDue).toHaveBeenCalledWith(item);
  });

  it('opens the practical workbench from History', async () => {
    const user = userEvent.setup();
    const props = show();
    await user.click(screen.getByRole('button', { name: 'Code labs and simulations' }));
    expect(props.onOpenWorkbench).toHaveBeenCalled();
  });

  it('surfaces an active backend coding build job and reattaches to it on Open build', async () => {
    const user = userEvent.setup();
    data.buildJob = {
      id: 'job-99',
      kind: 'learning.practice_build',
      subjectId: 'course-1:coding',
      status: 'running',
      progressCurrent: 1,
      progressTotal: 3,
      progressMessage: '',
      activity: null,
      resultRef: null,
      errorCode: null,
      error: null,
      retryOfJobId: null,
      retryCount: 0,
      retryNotBefore: null,
      createdAt: 1,
      startedAt: 1,
      finishedAt: null,
      revision: 1,
    };
    const onResumeBuild = vi.fn();
    show({ onResumeBuild });
    const banner = screen.getByRole('region', { name: 'Coding build status' });
    expect(banner).toHaveTextContent('Building your coding problem…');
    await user.click(within(banner).getByRole('button', { name: 'Open build' }));
    expect(useCodingBuildStore.getState().handles['course-1']?.jobId).toBe('job-99');
    expect(onResumeBuild).toHaveBeenCalled();
  });

  it('surfaces a failed coding build banner and lets the learner dismiss it', async () => {
    const user = userEvent.setup();
    useCodingBuildStore.setState({
      handles: {
        'course-1': {
          programId: 'course-1',
          choice: { form: 'build', setting: 'coached', language: 'python', difficulty: 'easy' },
          operationId: 'op-1',
          startOperationId: 'sop-1',
          jobId: 'job-1',
          step: 1,
          phase: { kind: 'failed', message: 'Tests did not pass.', stage: 'build' },
          activityId: null,
          seenJobs: {},
        },
      },
    });
    show();
    const banner = screen.getByRole('region', { name: 'Coding build status' });
    expect(banner).toHaveTextContent('Tests did not pass.');
    await user.click(within(banner).getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByRole('region', { name: 'Coding build status' })).not.toBeInTheDocument();
  });

  it('lets the learner dismiss a ready coding build banner', async () => {
    const user = userEvent.setup();
    useCodingBuildStore.setState({
      handles: {
        'course-1': {
          programId: 'course-1',
          choice: { form: 'build', setting: 'coached', language: 'python', difficulty: 'easy' },
          operationId: 'op-1',
          startOperationId: 'sop-1',
          jobId: 'job-1',
          step: 3,
          phase: { kind: 'ready', sessionId: 'session-old' },
          activityId: 'activity-1',
          seenJobs: {},
        },
      },
    });
    show();
    const banner = screen.getByRole('region', { name: 'Coding build status' });
    expect(banner).toHaveTextContent('Your coding problem is ready.');
    expect(within(banner).getByRole('button', { name: 'Open problem' })).toBeVisible();
    await user.click(within(banner).getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByRole('region', { name: 'Coding build status' })).not.toBeInTheDocument();
    expect(useCodingBuildStore.getState().handles['course-1']).toBeUndefined();
  });
});
