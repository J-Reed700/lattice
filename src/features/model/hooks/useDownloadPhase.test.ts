import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { DownloadStatus } from '@/types/downloads';

import { useDownloadPhase } from './useDownloadPhase';

const NOW = Date.parse('2026-10-01T01:00:00Z');
const completedAgo = (ms: number): DownloadStatus => ({
  id: 'a', state: 'Completed', completed_at: new Date(NOW - ms).toISOString(),
} as DownloadStatus);

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});
afterEach(() => vi.useRealTimers());

describe('useDownloadPhase', () => {
  it('polls every second while finishing, then goes idle when the window ends', () => {
    const poll = vi.fn();
    const { result } = renderHook(() => useDownloadPhase({ ready: false, download: completedAgo(50_000), poll }));
    expect(result.current).toBe('finishing');

    act(() => { vi.advanceTimersByTime(3000); });
    expect(poll).toHaveBeenCalledTimes(3);

    act(() => { vi.advanceTimersByTime(7000); });
    expect(result.current).toBe('idle');
    const calls = poll.mock.calls.length;
    act(() => { vi.advanceTimersByTime(5000); });
    expect(poll).toHaveBeenCalledTimes(calls);
  });

  it('goes idle when its timer fires a moment before the clock reaches the window end', () => {
    const { result } = renderHook(() => useDownloadPhase({ ready: false, download: completedAgo(58_000) }));
    expect(result.current).toBe('finishing');

    vi.setSystemTime(NOW - 1);
    act(() => { vi.advanceTimersByTime(2000); });
    act(() => { vi.advanceTimersByTime(5000); });

    expect(result.current).toBe('idle');
  });

  it('goes idle immediately when a row first sees a Completed download more than 60s after completion', () => {
    const poll = vi.fn();
    const { result, rerender } = renderHook(
      ({ download }: { download: DownloadStatus | null }) => useDownloadPhase({ ready: false, download, poll }),
      { initialProps: { download: null as DownloadStatus | null } }
    );
    expect(result.current).toBe('idle');

    // Two minutes later, the row receives a download that completed 70s ago.
    vi.setSystemTime(NOW + 120_000);
    const completed70sAgo: DownloadStatus = {
      id: 'a',
      state: 'Completed',
      completed_at: new Date(NOW + 50_000).toISOString(),
    } as DownloadStatus;
    rerender({ download: completed70sAgo });

    act(() => { vi.advanceTimersByTime(0); });
    expect(result.current).toBe('idle');
    act(() => { vi.advanceTimersByTime(5000); });
    expect(poll).not.toHaveBeenCalled();
  });

  it('does not poll when ready or never downloaded', () => {
    const poll = vi.fn();
    renderHook(() => useDownloadPhase({ ready: true, download: completedAgo(1000), poll }));
    renderHook(() => useDownloadPhase({ ready: false, download: null, poll }));
    act(() => { vi.advanceTimersByTime(5000); });
    expect(poll).not.toHaveBeenCalled();
  });
});
