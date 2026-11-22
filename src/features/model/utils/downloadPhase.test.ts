import { describe, expect, it } from 'vitest';

import type { AggregatedDownload, DownloadState, DownloadStatus } from '@/types/downloads';

import { completedAtMs, downloadPhase, FINISHING_WINDOW_MS, latestDownloadFor } from './downloadPhase';

const row = (id: string, state: DownloadState, overrides: Partial<DownloadStatus> = {}): DownloadStatus => ({
  id, url: `https://example.org/${id}`, destination: '/models/rerank.gguf', state, bytes_downloaded: 0, total_bytes: 100,
  bytes_per_second: 0, percentage: 0, eta_seconds: null, error_message: null, retry_count: 0,
  created_at: '2026-10-01T00:00:00Z', started_at: null, completed_at: null, model_name: null, model_id: null, ...overrides,
});

describe('latestDownloadFor', () => {
  it('matches by destination, ignoring separator style', () => {
    const match = row('a', 'Failed', { destination: 'C:\\models\\rerank.gguf' });
    expect(latestDownloadFor([row('b', 'Failed', { destination: '/other.gguf' }), match], 'C:/models/rerank.gguf')).toBe(match);
  });

  it('returns the newest row when none is live', () => {
    const older = row('old', 'Failed', { created_at: '2026-10-01T00:00:00Z' });
    const newer = row('new', 'Cancelled', { created_at: '2026-10-02T00:00:00Z' });
    expect(latestDownloadFor([newer, older], '/models/rerank.gguf')).toBe(newer);
  });

  it('prefers a live row over a newer finished one', () => {
    const live = row('live', 'Paused', { created_at: '2026-10-01T00:00:00Z' });
    const newer = row('new', 'Cancelled', { created_at: '2026-10-02T00:00:00Z' });
    expect(latestDownloadFor([newer, live], '/models/rerank.gguf')).toBe(live);
  });

  it('returns null for no match or no path', () => {
    expect(latestDownloadFor([row('a', 'Pending')], '/nowhere.gguf')).toBeNull();
    expect(latestDownloadFor([row('a', 'Pending')], undefined)).toBeNull();
  });
});

describe('downloadPhase', () => {
  const now = Date.parse('2026-10-01T01:00:00Z');
  const completed = (secondsAgo: number) => row('c', 'Completed', { completed_at: new Date(now - secondsAgo * 1000).toISOString() });

  it('is ready whenever the status says so', () => {
    expect(downloadPhase({ ready: true, download: row('a', 'Downloading'), now })).toBe('ready');
  });

  it('is live while pending, downloading or paused', () => {
    for (const state of ['Pending', 'Downloading', 'Paused'] as const) {
      expect(downloadPhase({ ready: false, download: row('a', state), now })).toBe('live');
    }
  });

  it('is finishing for a minute after completion, then idle', () => {
    expect(downloadPhase({ ready: false, download: completed(5), now })).toBe('finishing');
    expect(downloadPhase({ ready: false, download: completed(FINISHING_WINDOW_MS / 1000 - 1), now })).toBe('finishing');
    expect(downloadPhase({ ready: false, download: completed(FINISHING_WINDOW_MS / 1000), now })).toBe('idle');
  });

  it('is idle with no download, a failed one, or a completed one with no timestamp', () => {
    expect(downloadPhase({ ready: false, download: null, now })).toBe('idle');
    expect(downloadPhase({ ready: false, download: row('f', 'Failed'), now })).toBe('idle');
    expect(downloadPhase({ ready: false, download: row('c', 'Completed'), now })).toBe('idle');
  });

  it('reads an aggregated download from its latest file', () => {
    const files = [
      row('a', 'Completed', { completed_at: new Date(now - 120_000).toISOString() }),
      row('b', 'Completed', { completed_at: new Date(now - 10_000).toISOString() }),
    ];
    const group = { model_id: 'm', model_name: 'M', downloads: files, state: 'Completed' } as AggregatedDownload;
    expect(completedAtMs(group)).toBe(now - 10_000);
    expect(downloadPhase({ ready: false, download: group, now })).toBe('finishing');
    expect(downloadPhase({ ready: false, download: { ...group, state: 'Paused' }, now })).toBe('live');
  });
});
