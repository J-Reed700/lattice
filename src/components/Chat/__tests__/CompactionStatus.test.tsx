import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useCompactionStore } from '../../../stores/compactionStore';
import { CompactionStatus, formatElapsed } from '../CompactionStatus';

import type { CompactionRecord } from '../../../types/conversation';

const record: CompactionRecord = {
  id: 'c1',
  conversationId: 'conv-1',
  summaryText: 'The user is learning the boot path; lessons 1 and 2 are done.',
  upToMessageId: 'm6',
  originalMessageCount: 6,
  originalTokens: 2400,
  summaryTokens: 200,
  compressionRatio: 0.083,
  createdAt: '2026-10-04T00:00:00.000Z',
};

describe('compaction store', () => {
  beforeEach(() => useCompactionStore.setState({ runs: {} }));

  it('runs one compaction per conversation at a time and keeps how it ended', () => {
    const { start, finish, fail, dismiss } = useCompactionStore.getState();
    expect(start('conv-1')).toBe(true);
    expect(start('conv-1')).toBe(false);
    expect(start('conv-2')).toBe(true);

    // Still running: nothing to dismiss yet.
    dismiss('conv-1');
    expect(useCompactionStore.getState().runs['conv-1']?.state).toBe('running');

    finish('conv-1', record);
    fail('conv-2', 'Nothing to compact');
    expect(useCompactionStore.getState().runs['conv-1']).toMatchObject({ state: 'done', record });
    expect(useCompactionStore.getState().runs['conv-2']).toMatchObject({ state: 'failed', error: 'Nothing to compact' });

    dismiss('conv-1');
    expect(useCompactionStore.getState().runs['conv-1']).toBeUndefined();
    // A finished run can start again.
    expect(start('conv-2')).toBe(true);
  });
});

describe('CompactionStatus', () => {
  afterEach(() => vi.useRealTimers());

  it('says it is compacting, and for how long, while it runs', () => {
    vi.useFakeTimers();
    const startedAt = Date.now();
    render(<CompactionStatus run={{ state: 'running', startedAt }} onDismiss={() => {}} />);

    expect(screen.getByRole('status')).toHaveTextContent('Compacting context…');
    expect(screen.getByText(/Sending waits until it’s done/)).toBeInTheDocument();
    expect(screen.getByText('0s')).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(65_000);
    });
    expect(screen.getByText('1m 05s')).toBeInTheDocument();
    // Nothing to dismiss while it runs.
    expect(screen.queryByRole('button', { name: 'Dismiss' })).not.toBeInTheDocument();
  });

  it('says what it did, shows the summary on request, and can be dismissed', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    render(
      <CompactionStatus
        run={{ state: 'done', startedAt: 0, finishedAt: 72_000, record }}
        onDismiss={onDismiss}
      />
    );

    expect(screen.getByText('Context compacted')).toBeInTheDocument();
    expect(screen.getByText(/6 older messages summarized in 1m 12s/)).toBeInTheDocument();
    expect(screen.queryByText(record.summaryText)).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Show summary' }));
    expect(screen.getByText(record.summaryText)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Dismiss' }));
    expect(onDismiss).toHaveBeenCalledOnce();
  });

  it('says why when it could not compact', () => {
    render(
      <CompactionStatus
        run={{
          state: 'failed',
          startedAt: 0,
          finishedAt: 1_000,
          error: 'Nothing to compact: this conversation has no older messages that are not already covered.',
        }}
        onDismiss={() => {}}
      />
    );
    expect(screen.getByRole('alert')).toHaveTextContent('Couldn’t compact the context');
    expect(screen.getByRole('alert')).toHaveTextContent('Nothing to compact');
  });

  it('formats elapsed time', () => {
    expect(formatElapsed(0)).toBe('0s');
    expect(formatElapsed(59_999)).toBe('59s');
    expect(formatElapsed(60_000)).toBe('1m 00s');
    expect(formatElapsed(605_000)).toBe('10m 05s');
  });
});
