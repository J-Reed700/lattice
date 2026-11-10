import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { InterviewDrill } from '@/features/learning/practice/practical/InterviewDrill';
import type { LearningPracticalActivityDto } from '@/lib/bindings';

const mocks = vi.hoisted(() => ({ create: vi.fn(), profile: vi.fn() }));
vi.mock('@/lib/api', () => ({ default: { createInterviewStudyDeck: mocks.create } }));
vi.mock('@/features/learning/practice/home/practiceHomeApi', () => ({ practiceHomeApi: { getProfile: mocks.profile } }));
vi.mock('@/features/learning/recall/flashcards/FlashcardSession', () => ({
  FlashcardSession: ({ title, onClose }: { title: string; onClose: () => void }) => (
    <div>
      <p>{`Reviewing ${title}`}</p>
      <button type="button" onClick={onClose}>End the review</button>
    </div>
  ),
}));

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

function show() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { mutations: { retry: false } } })}>
    <InterviewDrill programId="program" activity={activity} />
  </QueryClientProvider>);
}

describe('InterviewDrill', () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.profile.mockResolvedValue({ interviewAt: null }); });

  it('makes a deck with the interview date as its target and opens it in a drawer', async () => {
    const user = userEvent.setup();
    mocks.create.mockResolvedValue({ ok: true, data: { id: 'drill-deck', title: 'Acme drill', focus: '', modelName: 'test', createdAt: 0, cards: [] } });
    show();
    expect(screen.getByRole('heading', { name: 'Drill these 1 question until they stick' })).toBeVisible();
    const when = screen.getByLabelText('Interview date and time');
    await user.clear(when);
    await user.type(when, '2099-03-04T10:15');
    await user.click(screen.getByRole('button', { name: 'Make drill deck' }));
    const drawer = await screen.findByRole('dialog', { name: 'Acme drill' });
    expect(drawer).toBeVisible();
    expect(screen.getByText('Reviewing Acme drill')).toBeVisible();
    expect(mocks.create).toHaveBeenCalledWith({ programId: 'program', activityId: 'activity', targetAt: new Date('2099-03-04T10:15').getTime() });

    await user.click(screen.getByRole('button', { name: 'End the review' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('starts the date from the interview date in Preparing for, not a guess', async () => {
    const at = new Date('2099-05-06T14:30').getTime();
    mocks.profile.mockResolvedValue({ interviewAt: at });
    show();
    await waitFor(() => expect(screen.getByLabelText('Interview date and time')).toHaveValue('2099-05-06T14:30'));
  });

  it('leaves the date empty and asks for it when the course has none', async () => {
    show();
    await waitFor(() => expect(mocks.profile).toHaveBeenCalled());
    expect(screen.getByLabelText('Interview date and time')).toHaveValue('');
    expect(screen.getByText(/Add your interview date/)).toBeVisible();
  });

  it('shows the message the backend gave when it refuses', async () => {
    const user = userEvent.setup();
    mocks.create.mockResolvedValue({ ok: false, error: 'failed', details: { details: 'The interview has no questions yet.' } });
    show();
    await user.click(screen.getByRole('button', { name: 'Make drill deck' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('The interview has no questions yet.'));
  });

  it('shows why a deck could not be made and opens no drawer', async () => {
    const user = userEvent.setup();
    mocks.create.mockResolvedValue({ ok: false, error: 'This interview is not ready yet.' });
    show();
    await user.click(screen.getByRole('button', { name: 'Make drill deck' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('This interview is not ready yet.'));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
});
