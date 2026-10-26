import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes, useSearchParams } from 'react-router';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { InterviewDrill } from '@/features/learning/practice/practical/InterviewDrill';
import type { LearningPracticalActivityDto } from '@/lib/bindings';

const mocks = vi.hoisted(() => ({ create: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { createInterviewStudyDeck: mocks.create } }));

const activity = {
  id: 'activity', programId: 'program', lessonId: 'lesson', predecessorId: null, kind: 'interview', title: 'Acme interview',
  brief: '', status: 'ready', practiceMode: 'practice', allowedAids: [], outcomeIds: [], sourceVersionIds: [], rubric: [],
  runtimeKind: 'none', runtimeProfileId: null, runtimeEngine: null, runtimeImageId: null, runtimeCommand: null,
  runtimeLimits: null, runtimeAvailable: false, runtimeUnavailableReason: null, generatorModel: 'test', files: [],
  revision: 0, createdAt: 0, updatedAt: 0,
  interviewPlan: {
    companyName: 'Acme', targetRole: 'Engineer', roundKind: 'behavioral', companySummary: '', learnerRole: 'Candidate', counterpartRole: 'Interviewer',
    questions: [{ id: 'q', round: 'behavioral', question: 'Tell me about a conflict.', companyRationale: '', followUpProbes: [], evaluationSignals: [] }],
  },
} satisfies LearningPracticalActivityDto;

function OpenedDeck() {
  const [params] = useSearchParams();
  return <p>Opened deck {params.get('deck')}</p>;
}

function show() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { mutations: { retry: false } } })}>
    <MemoryRouter initialEntries={['/practice']}><Routes>
      <Route path="/practice" element={<InterviewDrill programId="program" activity={activity} />} />
      <Route path="/studio" element={<OpenedDeck />} />
    </Routes></MemoryRouter>
  </QueryClientProvider>);
}

describe('InterviewDrill', () => {
  beforeEach(() => { vi.resetAllMocks(); });

  it('makes a deck with the interview date as its target and opens it', async () => {
    const user = userEvent.setup();
    mocks.create.mockResolvedValue({ ok: true, data: { id: 'drill-deck', title: 'Drill', focus: '', modelName: 'test', createdAt: 0, cards: [] } });
    show();
    expect(screen.getByRole('heading', { name: 'Drill these 1 question until they stick' })).toBeVisible();
    const when = screen.getByLabelText('Interview date and time');
    await user.clear(when);
    await user.type(when, '2099-03-04T10:15');
    await user.click(screen.getByRole('button', { name: 'Make drill deck' }));
    expect(await screen.findByText('Opened deck drill-deck')).toBeVisible();
    expect(mocks.create).toHaveBeenCalledWith({ programId: 'program', activityId: 'activity', targetAt: new Date('2099-03-04T10:15').getTime() });
  });

  it('shows why a deck could not be made and stays put', async () => {
    const user = userEvent.setup();
    mocks.create.mockResolvedValue({ ok: false, error: 'This interview is not ready yet.' });
    show();
    await user.click(screen.getByRole('button', { name: 'Make drill deck' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('This interview is not ready yet.'));
    expect(screen.queryByText(/Opened deck/)).not.toBeInTheDocument();
  });

  it('renders nothing outside the app router', () => {
    const { container } = render(<QueryClientProvider client={new QueryClient()}><InterviewDrill programId="program" activity={activity} /></QueryClientProvider>);
    expect(container).toBeEmptyDOMElement();
  });
});
