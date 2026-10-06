import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { SynthesizePopover } from '@/features/journal/components/SynthesizePopover';
import { useSynthesisStore } from '@/features/journal/synthesis/synthesisStore';

async function openPopover(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole('button', { name: /synthesize…/i }));
  await waitFor(() => expect(screen.getByText('Past week')).toBeInTheDocument());
}

describe('SynthesizePopover', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSynthesisStore.setState({ job: null, minimized: false });
  });

  it('shows four scopes and defaults to "current" without a conversation', async () => {
    const user = userEvent.setup();
    render(
      <SynthesizePopover
        selectedEntryId="entry_1"
        pinnedCount={2}
        deckCount={5}
        onSynthesize={vi.fn().mockResolvedValue(true)}
      />,
    );
    await openPopover(user);

    expect(screen.getByText('This entry only')).toBeInTheDocument();
    expect(screen.getByText('All pinned entries (2)')).toBeInTheDocument();
    expect(screen.getByText('Recent entries (5)')).toBeInTheDocument();
    expect(screen.getByText('Past week')).toBeInTheDocument();
    expect(screen.queryByText('This conversation')).not.toBeInTheDocument();

    const radios = screen.getAllByRole('radio');
    expect(radios).toHaveLength(4);
    expect(radios[0]).toBeChecked();
  });

  it('shows five scopes and defaults to "This conversation" when given one', async () => {
    const user = userEvent.setup();
    render(
      <SynthesizePopover
        selectedEntryId="entry_1"
        pinnedCount={0}
        deckCount={0}
        conversationId="conv_1"
        conversationTitle="Sleep study"
        onSynthesize={vi.fn().mockResolvedValue(true)}
      />,
    );
    await openPopover(user);

    expect(screen.getByText('This conversation')).toBeInTheDocument();
    expect(screen.getByText('Sleep study')).toBeInTheDocument();
    expect(screen.getByText('Writes a summary onto a journal page.')).toBeInTheDocument();

    const radios = screen.getAllByRole('radio');
    expect(radios).toHaveLength(5);
    expect(radios[4]).toBeChecked();
  });

  it('disables "Past week" and says so when nothing is there', async () => {
    const user = userEvent.setup();
    render(
      <SynthesizePopover
        selectedEntryId="entry_1"
        pinnedCount={0}
        deckCount={0}
        weekCandidates={{ conversations: 0, references: 0, notes: 0, total: 0 }}
        onSynthesize={vi.fn().mockResolvedValue(true)}
      />,
    );
    await openPopover(user);

    expect(screen.getByText('Nothing from the past week')).toBeInTheDocument();
    expect(screen.getAllByRole('radio')[3]).toBeDisabled();
  });

  it('renders the real-count helper and omits zero parts', async () => {
    const user = userEvent.setup();
    render(
      <SynthesizePopover
        selectedEntryId="entry_1"
        pinnedCount={0}
        deckCount={0}
        weekCandidates={{ conversations: 6, references: 0, notes: 2, total: 8 }}
        onSynthesize={vi.fn().mockResolvedValue(true)}
      />,
    );
    await openPopover(user);

    expect(screen.getByText('6 conversations · 2 journal pages')).toBeInTheDocument();
  });

  it('submits the selected week scope', async () => {
    const user = userEvent.setup();
    const onSynthesize = vi.fn().mockResolvedValue(true);
    render(
      <SynthesizePopover
        selectedEntryId="entry_1"
        pinnedCount={0}
        deckCount={0}
        weekCandidates={{ conversations: 3, references: 1, notes: 1, total: 5 }}
        onSynthesize={onSynthesize}
      />,
    );
    await openPopover(user);

    await user.click(screen.getAllByRole('radio')[3]);
    await user.click(screen.getByRole('button', { name: 'Synthesize' }));

    await waitFor(() => expect(onSynthesize).toHaveBeenCalledWith('week'));
  });

  it('shows progress instead of starting a second synthesis after a remount', async () => {
    const user = userEvent.setup();
    const onSynthesize = vi.fn();
    useSynthesisStore.setState({ minimized: true, job: {
      id: 'running', title: 'Research', conversationIds: ['entry_1'], status: 'running', stage: 'writing', startedAt: Date.now(),
    } });
    render(<SynthesizePopover selectedEntryId="entry_1" pinnedCount={1} deckCount={2} onSynthesize={onSynthesize} />);
    await user.click(screen.getByRole('button', { name: 'Synthesizing…' }));
    expect(useSynthesisStore.getState().minimized).toBe(false);
    expect(onSynthesize).not.toHaveBeenCalled();
    expect(screen.queryByRole('radio')).not.toBeInTheDocument();
  });

  it('recovers the submit button after an unexpected rejection', async () => {
    const user = userEvent.setup();
    const onSynthesize = vi.fn().mockRejectedValueOnce(new Error('Model unavailable')).mockResolvedValueOnce(true);
    render(<SynthesizePopover selectedEntryId="entry_1" pinnedCount={1} deckCount={2} onSynthesize={onSynthesize} />);
    await openPopover(user);
    await user.click(screen.getByRole('button', { name: 'Synthesize' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Model unavailable');
    expect(screen.getByRole('button', { name: 'Synthesize' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: 'Synthesize' }));
    expect(onSynthesize).toHaveBeenCalledTimes(2);
  });
});
