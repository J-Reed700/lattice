import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { useDownloadStore } from '@/features/model/stores/downloadStore';
import type { AggregatedDownload, DownloadState, DownloadStatus } from '@/types/downloads';

import { InlineDownloadStatus } from './InlineDownloadStatus';

const actions = vi.hoisted(() => ({ resumeDownload: vi.fn() }));
vi.mock('@/features/model/hooks/useDownloads', () => ({ useDownloadActions: () => actions }));

const row = (id: string, state: DownloadState, percentage = 0): DownloadStatus => ({
  id, state, percentage, destination: `/m/${id}`,
} as DownloadStatus);

function show(download: DownloadStatus | AggregatedDownload, finishing = false) {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <InlineDownloadStatus download={download} finishing={finishing} label="Downloading the thing" />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  actions.resumeDownload.mockResolvedValue(undefined);
  useDownloadStore.setState({ isDrawerOpen: false });
});

describe('InlineDownloadStatus', () => {
  it('says Queued for a pending download', () => {
    show(row('a', 'Pending'));
    expect(screen.getByRole('status', { name: 'Downloading the thing' })).toHaveTextContent('Queued');
  });

  it('shows the percentage while downloading', () => {
    show(row('a', 'Downloading', 41.6));
    expect(screen.getByRole('status')).toHaveTextContent('Downloading… 42%');
    expect((screen.getByRole('progressbar') as HTMLProgressElement).value).toBe(42);
    expect(screen.queryByRole('button', { name: 'Resume' })).toBeNull();
  });

  it('shows Finishing… at full progress', () => {
    show(row('a', 'Completed', 100), true);
    expect(screen.getByRole('status')).toHaveTextContent('Finishing…');
    expect((screen.getByRole('progressbar') as HTMLProgressElement).value).toBe(100);
  });

  it('resumes a paused download', async () => {
    show(row('a', 'Paused', 30));
    expect(screen.getByRole('status')).toHaveTextContent('Paused');
    await userEvent.click(screen.getByRole('button', { name: 'Resume' }));
    expect(actions.resumeDownload).toHaveBeenCalledWith('a');
  });

  it('resumes each paused file of a group', async () => {
    const group = {
      model_id: 'm', model_name: 'M', state: 'Paused', percentage: 10,
      downloads: [row('a', 'Paused'), row('b', 'Completed'), row('c', 'Paused')],
    } as AggregatedDownload;
    show(group);
    await userEvent.click(screen.getByRole('button', { name: 'Resume' }));
    expect(actions.resumeDownload.mock.calls).toEqual([['a'], ['c']]);
  });

  it('shows Queued, without Resume, once a resumed download waits for a slot', () => {
    const client = new QueryClient();
    const tree = (download: DownloadStatus) => (
      <QueryClientProvider client={client}>
        <InlineDownloadStatus download={download} label="Downloading the thing" />
      </QueryClientProvider>
    );
    const view = render(tree(row('a', 'Paused', 30)));
    expect(screen.getByRole('button', { name: 'Resume' })).toBeInTheDocument();
    view.rerender(tree(row('a', 'Pending', 30)));
    expect(screen.getByRole('status')).toHaveTextContent('Queued');
    expect(screen.queryByRole('button', { name: 'Resume' })).toBeNull();
  });

  it('opens the Downloads drawer', async () => {
    show(row('a', 'Downloading', 5));
    await userEvent.click(screen.getByRole('button', { name: 'Show in Downloads' }));
    expect(useDownloadStore.getState().isDrawerOpen).toBe(true);
  });
});
