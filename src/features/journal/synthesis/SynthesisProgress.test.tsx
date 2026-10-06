import { act, fireEvent, render, screen } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { SynthesisProgress } from './SynthesisProgress';
import { useSynthesisStore, type SynthesisJob } from './synthesisStore';

const running = (): SynthesisJob => ({
  id: 'synthesis', title: 'Research discussion', conversationIds: ['chat'], status: 'running', stage: 'reading',
  startedAt: Date.now(), entryCount: 3, chunkIndex: 1, chunkCount: 2,
});

function Location() {
  const location = useLocation();
  return <span data-testid="location">{location.pathname}{location.search}</span>;
}

function mount() {
  return render(<MemoryRouter initialEntries={['/chat']}><Location /><SynthesisProgress /></MemoryRouter>);
}

describe('synthesis progress', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useSynthesisStore.setState({ job: null, minimized: false });
  });
  afterEach(() => vi.useRealTimers());

  it('keeps an honest current stage during a long wait and keeps the timer out of live announcements', () => {
    useSynthesisStore.setState({ job: running() });
    mount();
    expect(screen.getByText('Reviewing part 1 of 2')).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(65_000));
    expect(screen.getByText('1m 05s elapsed · 3 entries')).toBeInTheDocument();
    expect(screen.getByText(/Still working/)).toBeInTheDocument();
    expect(screen.getByRole('status')).toHaveTextContent('Reviewing part 1 of 2 · Research discussion');
    expect(screen.getByRole('status')).not.toHaveTextContent('1m');
    expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
    act(() => useSynthesisStore.setState({ job: { ...running(), stage: 'writing' } }));
    expect(screen.getByRole('status')).toHaveTextContent('Writing your synthesis');
  });

  it('can be minimized and remounted, then opens the saved page only when requested', () => {
    const job = running();
    useSynthesisStore.setState({ job });
    const first = mount();
    fireEvent.click(screen.getByRole('button', { name: 'Keep working' }));
    expect(screen.getByRole('button', { name: /Show synthesis progress/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Show synthesis progress/ })).toHaveFocus();
    first.unmount();
    mount();
    act(() => useSynthesisStore.setState({ job: { ...job, stage: 'saving', status: 'completed', noteId: 'note 1', noteTitle: 'Research notes', finishedAt: Date.now() } }));
    expect(screen.getByTestId('location')).toHaveTextContent('/chat');
    fireEvent.click(screen.getByRole('button', { name: 'Show synthesis progress: Synthesis ready' }));
    expect(screen.getByRole('button', { name: 'Dismiss synthesis progress' })).toHaveFocus();
    expect(screen.getByText('Research notes')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Open journal page' }));
    expect(screen.getByTestId('location')).toHaveTextContent('/journals?noteId=note+1');
    expect(screen.queryByRole('complementary', { name: 'Conversation synthesis' })).not.toBeInTheDocument();
  });

  it('explains a save failure and offers recovery or dismissal', () => {
    const retry = vi.fn(async () => true);
    useSynthesisStore.setState({ job: { ...running(), status: 'failed', stage: 'saving', error: 'Disk full', retry } });
    mount();
    expect(screen.getByRole('alert')).toHaveTextContent('Disk full');
    expect(screen.getByRole('alert')).toHaveTextContent('kept here while you retry saving');
    fireEvent.click(screen.getByRole('button', { name: 'Retry saving' }));
    expect(retry).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('button', { name: 'Dismiss synthesis progress' }));
    expect(useSynthesisStore.getState().job).toBeNull();
  });
});
