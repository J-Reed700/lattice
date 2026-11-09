import type { ComponentProps } from 'react';

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type * as Wire from '@/lib/bindings';

import { practiceHomeApi } from './practiceHomeApi';
import { PreparingFor } from './PreparingFor';

vi.mock('./practiceHomeApi', () => ({ practiceHomeApi: { saveProfile: vi.fn() } }));

const profile: Wire.LearningPracticeProfileDto = {
  programId: 'course-1',
  details: 'Easy problems while someone watches',
  language: 'python',
  interviewAt: null,
  company: null,
  role: null,
  revision: 3,
};

function show(overrides: Partial<ComponentProps<typeof PreparingFor>> = {}) {
  const props = { programId: 'course-1', goal: 'Learn to code', profile, onSaved: vi.fn(), onReload: vi.fn(), ...overrides };
  const view = render(<PreparingFor {...props} />);
  return { props, view };
}

beforeEach(() => vi.resetAllMocks());

describe('PreparingFor', () => {
  it('shows what the learner wrote, or the course goal before they write anything', () => {
    const { view } = show();
    expect(screen.getByText('Easy problems while someone watches')).toBeVisible();
    view.unmount();
    show({ profile: undefined });
    expect(screen.getByText('Learn to code')).toBeVisible();
  });

  it('saves against the revision it loaded', async () => {
    const user = userEvent.setup();
    const saved = { ...profile, revision: 4, company: 'Acme' };
    vi.mocked(practiceHomeApi.saveProfile).mockResolvedValue(saved);
    const { props } = show();
    await user.click(screen.getByRole('button', { name: /Edit/ }));
    await user.type(screen.getByLabelText('Company'), 'Acme');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(practiceHomeApi.saveProfile).toHaveBeenCalledWith(expect.objectContaining({ programId: 'course-1', company: 'Acme', expectedRevision: 3 }));
    await waitFor(() => expect(props.onSaved).toHaveBeenCalledWith(saved));
  });

  it('says it changed elsewhere on a conflict and keeps Save off until the latest is loaded', async () => {
    const user = userEvent.setup();
    vi.mocked(practiceHomeApi.saveProfile).mockRejectedValue(Object.assign(new Error('conflict'), { details: { code: 'CONFLICT' } }));
    const { props } = show({ onReload: vi.fn().mockResolvedValue({ ...profile, revision: 5, details: 'Newer words' }) });
    await user.click(screen.getByRole('button', { name: /Edit/ }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('changed somewhere else');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();

    await user.click(screen.getByRole('button', { name: 'Load the latest' }));
    expect(props.onReload).toHaveBeenCalled();
    await waitFor(() => expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled());
    expect(screen.getByLabelText('What you are preparing for')).toHaveValue('Newer words');
  });

  it('offers a retry when the save fails for another reason', async () => {
    const user = userEvent.setup();
    vi.mocked(practiceHomeApi.saveProfile).mockRejectedValueOnce(new Error('disk full')).mockResolvedValueOnce(profile);
    show();
    await user.click(screen.getByRole('button', { name: /Edit/ }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText(/disk full/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: /Retry|Try again/ }));
    expect(practiceHomeApi.saveProfile).toHaveBeenCalledTimes(2);
  });
});
