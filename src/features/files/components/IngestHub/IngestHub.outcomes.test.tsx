import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useFileBrowserStore } from '@/features/files/stores/fileBrowserStore';

import { IngestHub } from './IngestHub';

const h = vi.hoisted(() => ({
  toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn(), info: vi.fn() },
  fetchCorpusShape: vi.fn(),
}));

vi.mock('@/hooks/useToast', () => ({ useToast: () => ({ toast: h.toast }) }));
vi.mock('@/features/corpus-shape/hooks/useCorpusShapeQuery', () => ({ fetchCorpusShape: h.fetchCorpusShape }));
vi.mock('@/features/files/components/Ingest/UrlImport', () => ({
  UrlImport: ({ onImportComplete }: { onImportComplete: (success: boolean, url: string) => void }) => (
    <button type="button" onClick={() => onImportComplete(false, 'https://example.org/paywalled')}>Fail URL</button>
  ),
}));
vi.mock('@/features/files/components/Ingest/BatchUrlImport', () => ({
  BatchUrlImport: ({ onImportComplete }: { onImportComplete: (results: { successful: number; failed: number }) => void }) => (
    <>
      <button type="button" onClick={() => onImportComplete({ successful: 3, failed: 0 })}>All URLs</button>
      <button type="button" onClick={() => onImportComplete({ successful: 1, failed: 2 })}>Some URLs</button>
    </>
  ),
}));
vi.mock('@/features/files/components/Ingest/BatchFileImport', () => ({
  BatchFileImport: ({ onImportComplete, onReviewFailures }: { onImportComplete: (results: { successful: number; failed: number }) => void; onReviewFailures: () => void }) => (
    <>
      <button type="button" onClick={() => onImportComplete({ successful: 0, failed: 4 })}>Failed files</button>
      <button type="button" onClick={onReviewFailures}>Review failures</button>
    </>
  ),
}));
vi.mock('@/features/files/components/Ingest/ImportHistory', () => ({ ImportHistory: () => <p>History panel</p> }));

describe('IngestHub outcomes', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.removeItem('ingestHub.lastTab');
    window.history.replaceState({}, '', '/ingest');
  });

  afterEach(() => {
    window.history.replaceState({}, '', '/');
  });

  it('opens the tab named in the address, then the last one used', () => {
    window.history.replaceState({}, '', '/ingest?tab=history');
    const first = render(<IngestHub />);
    expect(screen.getByText('History panel')).toBeInTheDocument();
    expect(localStorage.getItem('ingestHub.lastTab')).toBe('history');
    first.unmount();
    window.history.replaceState({}, '', '/ingest?tab=nonsense');
    localStorage.setItem('ingestHub.lastTab', 'files');
    render(<IngestHub />);
    expect(screen.getByRole('button', { name: 'Failed files' })).toBeInTheDocument();
  });

  it('reports a link that could not be imported', () => {
    const onImportComplete = vi.fn();
    render(<IngestHub onImportComplete={onImportComplete} />);
    fireEvent.click(screen.getByRole('button', { name: 'Fail URL' }));
    expect(onImportComplete).toHaveBeenCalledWith({ type: 'single-url', success: false, count: 1, message: 'https://example.org/paywalled' });
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't import https://example.org/paywalled");
  });

  it('summarises a batch of links with the library’s shape, offering to open it', async () => {
    h.fetchCorpusShape.mockResolvedValueOnce({ total: 40, byType: [{ type: 'pdf', count: 38 }], grownLast7Days: 3 });
    useFileBrowserStore.getState().setSearchQuery('stale search');
    render(<IngestHub defaultTab="bulk-url" />);
    fireEvent.click(screen.getByRole('button', { name: 'All URLs' }));
    await waitFor(() => expect(h.toast.success).toHaveBeenCalled());
    const [, options] = h.toast.success.mock.calls[0];
    const popstate = vi.fn();
    window.addEventListener('popstate', popstate);
    act(() => options.action.onClick());
    expect(window.location.pathname).toBe('/files');
    expect(popstate).toHaveBeenCalled();
    expect(useFileBrowserStore.getState().searchQuery).toBe('');
    window.removeEventListener('popstate', popstate);
  });

  it('still summarises when the shape cannot be read', async () => {
    h.fetchCorpusShape.mockRejectedValueOnce(new Error('stats offline'));
    render(<IngestHub defaultTab="bulk-url" />);
    fireEvent.click(screen.getByRole('button', { name: 'All URLs' }));
    await waitFor(() => expect(h.toast.success).toHaveBeenCalled());
  });

  it('counts failed links and files in the error', () => {
    const first = render(<IngestHub defaultTab="bulk-url" />);
    fireEvent.click(screen.getByRole('button', { name: 'Some URLs' }));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't import 2 URLs");
    first.unmount();
    localStorage.removeItem('ingestHub.lastTab');
    render(<IngestHub defaultTab="files" />);
    fireEvent.click(screen.getByRole('button', { name: 'Failed files' }));
    expect(h.toast.error).toHaveBeenLastCalledWith("Couldn't import 4 files");
  });

  it('switches to History to review failed files', () => {
    render(<IngestHub defaultTab="files" />);
    fireEvent.click(screen.getByRole('button', { name: 'Review failures' }));
    expect(screen.getByText('History panel')).toBeInTheDocument();
  });
});
