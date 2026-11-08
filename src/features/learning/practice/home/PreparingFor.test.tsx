import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { type LearningPracticeProfileDto, practiceHomeApi } from './practiceHomeApi';
import { PreparingFor } from './PreparingFor';

describe('PreparingFor', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('displays the course goal, learner details, and interview date', () => {
    const profile: LearningPracticeProfileDto = {
      programId: 'program-1',
      details: 'Easy problems in Python, live, someone watching and guiding.',
      language: 'Python',
      interviewAt: new Date('2026-11-20T12:00:00Z').getTime(),
      company: null,
      role: null,
      revision: 1,
    };

    render(
      <PreparingFor
        programId="program-1"
        courseGoal="Master algorithmic problem solving and coding under pressure"
        profile={profile}
      />,
    );

    expect(screen.getByText(/preparing for/i)).toBeInTheDocument();
    expect(screen.getByText('Master algorithmic problem solving and coding under pressure')).toBeInTheDocument();
    expect(screen.getByText('Easy problems in Python, live, someone watching and guiding.')).toBeInTheDocument();
    expect(screen.getByText(/interview/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^edit$/i })).toBeInTheDocument();
  });

  it('opens the edit form on clicking Edit', async () => {
    const user = userEvent.setup();
    const profile: LearningPracticeProfileDto = {
      programId: 'program-1',
      details: 'Initial preparation notes',
      language: 'Python',
      interviewAt: null,
      company: null,
      role: null,
      revision: 1,
    };

    render(
      <PreparingFor
        programId="program-1"
        courseGoal="Master algorithmic problem solving"
        profile={profile}
      />,
    );

    const editBtn = screen.getByRole('button', { name: /^edit$/i });
    await user.click(editBtn);

    expect(screen.getByRole('textbox', { name: /details/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^save$/i })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^cancel$/i })).toBeInTheDocument();
  });

  it('calls saveLearningPracticeProfile via practiceHomeApi on save', async () => {
    const user = userEvent.setup();
    const profile: LearningPracticeProfileDto = {
      programId: 'program-1',
      details: 'Initial notes',
      language: 'Python',
      interviewAt: null,
      company: null,
      role: null,
      revision: 1,
    };

    const saveSpy = vi.spyOn(practiceHomeApi, 'saveProfile').mockResolvedValue({
      ...profile,
      details: 'Updated preparation notes for mock rounds',
      revision: 2,
    });

    const onProfileChange = vi.fn();

    render(
      <PreparingFor
        programId="program-1"
        courseGoal="Master algorithmic problem solving"
        profile={profile}
        onProfileChange={onProfileChange}
      />,
    );

    await user.click(screen.getByRole('button', { name: /^edit$/i }));

    const detailsBox = screen.getByRole('textbox', { name: /details/i });
    await user.clear(detailsBox);
    await user.type(detailsBox, 'Updated preparation notes for mock rounds');

    await user.click(screen.getByRole('button', { name: /^save$/i }));

    expect(saveSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        programId: 'program-1',
        details: 'Updated preparation notes for mock rounds',
      }),
    );

    await waitFor(() => {
      expect(onProfileChange).toHaveBeenCalledWith(
        expect.objectContaining({
          details: 'Updated preparation notes for mock rounds',
          revision: 2,
        }),
      );
    });
  });
});
