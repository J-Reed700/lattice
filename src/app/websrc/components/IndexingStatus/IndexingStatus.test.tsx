import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import type { IndexingFailure, IndexingSnapshot } from '@/types/api/indexing';

import { FailedItemsList } from './FailedItemsList';
import { IndexingStatusRail } from './IndexingStatusRail';
import {
  INDEXING_STATUS_QUERY_KEY,
  useIndexingStatusQuery,
} from '../../hooks/queries/useIndexingStatusQuery';


const {
  mockIndexFile,
  mockReindexFile,
  mockRemoveIndexedFile,
  mockClearIndexingFailure,
  mockGetIndexProgress,
  mockGetIndexingActivities,
  mockPauseIndexing,
  mockResumeIndexing,
  mockCancelIndexing,
  mockListen,
} = vi.hoisted(() => ({
  mockIndexFile: vi.fn(),
  mockReindexFile: vi.fn(),
  mockRemoveIndexedFile: vi.fn(),
  mockClearIndexingFailure: vi.fn(),
  mockGetIndexProgress: vi.fn(),
  mockGetIndexingActivities: vi.fn(),
  mockPauseIndexing: vi.fn(),
  mockResumeIndexing: vi.fn(),
  mockCancelIndexing: vi.fn(),
  mockListen: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
  __esModule: true,
  default: {
    indexFile: mockIndexFile,
    reindexFile: mockReindexFile,
    removeIndexedFile: mockRemoveIndexedFile,
    clearIndexingFailure: mockClearIndexingFailure,
    getIndexProgress: mockGetIndexProgress,
    getIndexingActivities: mockGetIndexingActivities,
    pauseIndexing: mockPauseIndexing,
    resumeIndexing: mockResumeIndexing,
    cancelIndexing: mockCancelIndexing,
  },
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: mockListen,
}));

function failure(overrides: Partial<IndexingFailure> = {}): IndexingFailure {
  return {
    path: '/vault/q3-summary.pdf',
    fileName: 'q3-summary.pdf',
    reason: "Couldn't extract text from this PDF.",
    failedAt: '2026-09-06T12:00:00.000Z',
    ...overrides,
  };
}

function snapshot(overrides: Partial<IndexingSnapshot> = {}): IndexingSnapshot {
  return {
    totalFiles: 0,
    processed: 0,
    failed: 0,
    status: 'idle',
    percentage: 0,
    paused: false,
    failures: [],
    ...overrides,
  };
}

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

function wrapperFor(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>
        <MemoryRouter>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  };
}

describe('FailedItemsList', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIndexFile.mockResolvedValue({ ok: true, data: {} });
    mockReindexFile.mockResolvedValue({ ok: true, data: undefined });
    mockRemoveIndexedFile.mockResolvedValue({ ok: true, data: undefined });
    mockClearIndexingFailure.mockResolvedValue({ ok: true, data: undefined });
  });

  it('renders the file name, the reason, and both verbs per row', () => {
    render(<FailedItemsList failures={[failure()]} />, { wrapper: wrapperFor(makeClient()) });

    expect(screen.getByText('q3-summary.pdf')).toBeInTheDocument();
    expect(screen.getByText("Couldn't extract text from this PDF.")).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Remove' })).toBeInTheDocument();
  });

  it('retries through indexFile with the failure path', async () => {
    const user = userEvent.setup();
    render(<FailedItemsList failures={[failure()]} />, { wrapper: wrapperFor(makeClient()) });

    await user.click(screen.getByRole('button', { name: 'Retry' }));

    await waitFor(() => expect(mockIndexFile).toHaveBeenCalledWith('/vault/q3-summary.pdf'));
  });

  it('removes through removeIndexedFile with the failure path', async () => {
    const user = userEvent.setup();
    render(<FailedItemsList failures={[failure()]} />, { wrapper: wrapperFor(makeClient()) });

    await user.click(screen.getByRole('button', { name: 'Remove' }));

    await waitFor(() =>
      expect(mockRemoveIndexedFile).toHaveBeenCalledWith('/vault/q3-summary.pdf'),
    );
  });

  it('clears the failure in the engine so a dismissal survives a view change', async () => {
    const user = userEvent.setup();
    render(<FailedItemsList failures={[failure()]} />, { wrapper: wrapperFor(makeClient()) });

    await user.click(screen.getByRole('button', { name: 'Remove' }));

    // Local `useState` dismissal alone would let the row come back the moment
    // another surface read the snapshot.
    await waitFor(() =>
      expect(mockClearIndexingFailure).toHaveBeenCalledWith('/vault/q3-summary.pdf'),
    );
  });

  it('clears the failure after a successful retry too', async () => {
    const user = userEvent.setup();
    render(<FailedItemsList failures={[failure()]} />, { wrapper: wrapperFor(makeClient()) });

    await user.click(screen.getByRole('button', { name: 'Retry' }));

    await waitFor(() =>
      expect(mockClearIndexingFailure).toHaveBeenCalledWith('/vault/q3-summary.pdf'),
    );
  });

  it('still reports success when clearing the failure is unavailable', async () => {
    const user = userEvent.setup();
    mockClearIndexingFailure.mockRejectedValue(new Error('Command not found'));
    render(<FailedItemsList failures={[failure()]} />, { wrapper: wrapperFor(makeClient()) });

    await user.click(screen.getByRole('button', { name: 'Remove' }));

    await waitFor(() =>
      expect(mockRemoveIndexedFile).toHaveBeenCalledWith('/vault/q3-summary.pdf'),
    );
    expect(screen.queryByText('q3-summary.pdf')).not.toBeInTheDocument();
  });

  it('collapses everything past the limit into one muted line', () => {
    const failures = Array.from({ length: 12 }, (_, i) =>
      failure({ path: `/vault/${i}.pdf`, fileName: `${i}.pdf` }),
    );
    render(<FailedItemsList failures={failures} limit={5} />, {
      wrapper: wrapperFor(makeClient()),
    });

    expect(screen.getByText('and 7 more')).toBeInTheDocument();
  });

  it('hides dismissed rows without calling the backend', () => {
    render(
      <FailedItemsList
        failures={[failure()]}
        dismissed={new Set(['/vault/q3-summary.pdf'])}
      />,
      { wrapper: wrapperFor(makeClient()) },
    );

    expect(screen.queryByText('q3-summary.pdf')).not.toBeInTheDocument();
    expect(mockRemoveIndexedFile).not.toHaveBeenCalled();
    expect(mockIndexFile).not.toHaveBeenCalled();
  });
});

describe('IndexingStatusRail', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockListen.mockResolvedValue(() => {});
    mockGetIndexingActivities.mockResolvedValue({ ok: true, data: [] });
  });

  it('renders nothing when idle with no failures', async () => {
    mockGetIndexProgress.mockResolvedValue({ ok: true, data: snapshot() });
    const { container } = render(<IndexingStatusRail />, { wrapper: wrapperFor(makeClient()) });

    await waitFor(() => expect(mockGetIndexProgress).toHaveBeenCalled());
    expect(container.querySelector('button')).toBeNull();
  });

  it('labels the button with the run and the failures', async () => {
    mockGetIndexProgress.mockResolvedValue({
      ok: true,
      data: snapshot({
        status: 'processing',
        processed: 12,
        totalFiles: 47,
        failed: 3,
        failures: [failure()],
      }),
    });
    render(<IndexingStatusRail />, { wrapper: wrapperFor(makeClient()) });

    expect(
      await screen.findByRole('button', { name: 'Indexing 12 of 47, 3 failed' }),
    ).toBeInTheDocument();
  });
});

describe('indexing://progress merge', () => {
  it('keeps the cached failures when the event payload omits them', async () => {
    let handler: ((event: { payload: Partial<IndexingSnapshot> }) => void) | undefined;
    mockListen.mockImplementation((_name: string, cb: typeof handler) => {
      handler = cb;
      return Promise.resolve(() => {});
    });
    mockGetIndexProgress.mockResolvedValue({
      ok: true,
      data: snapshot({
        status: 'processing',
        processed: 1,
        totalFiles: 10,
        failed: 1,
        failures: [failure()],
      }),
    });

    const client = makeClient();
    function Probe() {
      const { data } = useIndexingStatusQuery();
      return <span data-testid="count">{data ? data.failures.length : -1}</span>;
    }
    render(<Probe />, { wrapper: wrapperFor(client) });

    await waitFor(() => expect(screen.getByTestId('count')).toHaveTextContent('1'));
    await waitFor(() => expect(handler).toBeDefined());

    // The engine's event payload carries counters only — no `failures` field.
    handler?.({
      payload: {
        totalFiles: 10,
        processed: 2,
        failed: 1,
        status: 'processing',
        percentage: 20,
      } as Partial<IndexingSnapshot>,
    });

    await waitFor(() => {
      const cached = client.getQueryData<IndexingSnapshot>(INDEXING_STATUS_QUERY_KEY);
      expect(cached?.processed).toBe(2);
      expect(cached?.failures).toHaveLength(1);
    });
  });
});
