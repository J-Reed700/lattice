import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { emptyPlan, entry, ok, planView, topics } from '@/features/learning/curriculum/coursePlan/coursePlanFixtures';
import { CoursePlanStatus, LessonTopics } from '@/features/learning/curriculum/coursePlan/PlannedTopics';

const api = vi.hoisted(() => ({ get: vi.fn(), plan: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { getLearningCoursePlan: api.get, planLearningCourse: api.plan } }));

function show(ui: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

describe("a lesson's planned topics", () => {
  beforeEach(() => vi.resetAllMocks());

  it('shows the ideas as one line and keeps terms, actions and contrast behind a disclosure', async () => {
    const planned = topics('lesson-1', { ideas: ['The cardiac cycle', 'Valves'], terms: ['Systole'], actions: ['Trace a beat'], contrast: 'Pulse is not pressure' });
    api.get.mockResolvedValue(ok(planView([entry('lesson-1', { topics: planned })])));
    show(<LessonTopics programId="program-1" lessonId="lesson-1" />);
    expect(await screen.findByText('The cardiac cycle · Valves')).toBeVisible();
    const disclosure = screen.getByRole('group');
    expect(disclosure.tagName).toBe('DETAILS');
    expect(within(disclosure).getByText('Systole')).toBeInTheDocument();
    expect(within(disclosure).getByText('Trace a beat')).toBeInTheDocument();
    expect(within(disclosure).getByText('Pulse is not pressure')).toBeInTheDocument();
    expect(disclosure).not.toHaveAttribute('open');
  });

  it('shows nothing for a lesson the plan does not list', async () => {
    api.get.mockResolvedValue(ok(planView([entry('lesson-1')])));
    const { container } = show(<LessonTopics programId="program-1" lessonId="lesson-9" />);
    await waitFor(() => expect(api.get).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });
});

describe('the course plan status', () => {
  beforeEach(() => vi.resetAllMocks());

  it('says the topics are being planned', async () => {
    api.get.mockResolvedValue(ok(emptyPlan('planning')));
    show(<CoursePlanStatus programId="program-1" />);
    expect(await screen.findByText('Planning topics…')).toBeVisible();
  });

  it('says why planning failed and plans again on Try again', async () => {
    const user = userEvent.setup();
    api.get.mockResolvedValue(ok(emptyPlan('failed', 'The model stopped.')));
    api.plan.mockResolvedValue(ok(emptyPlan('planning')));
    show(<CoursePlanStatus programId="program-1" />);
    expect(await screen.findByText("Couldn't plan topics: The model stopped.")).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Try again' }));
    await waitFor(() => expect(api.plan).toHaveBeenCalledWith({ programId: 'program-1' }));
  });

  it('offers to plan topics when there is no plan, and plans on click', async () => {
    const user = userEvent.setup();
    api.get.mockResolvedValue(ok(emptyPlan('none')));
    api.plan.mockResolvedValue(ok(emptyPlan('planning')));
    show(<CoursePlanStatus programId="program-1" />);
    await user.click(await screen.findByRole('button', { name: 'Plan topics' }));
    await waitFor(() => expect(api.plan).toHaveBeenCalledWith({ programId: 'program-1' }));
  });

  it('says nothing once the topics are planned', async () => {
    api.get.mockResolvedValue(ok(planView([entry('lesson-1')])));
    const { container } = show(<CoursePlanStatus programId="program-1" />);
    await waitFor(() => expect(api.get).toHaveBeenCalled());
    expect(container).toBeEmptyDOMElement();
  });
});
