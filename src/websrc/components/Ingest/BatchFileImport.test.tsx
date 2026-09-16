import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';

import { BatchFileImport } from './BatchFileImport';

const mocks = vi.hoisted(() => ({
  selectFiles: vi.fn(),
  indexFile: vi.fn(),
  startBatchImport: vi.fn(),
  getOperation: vi.fn(),
  operations: new Map(),
  fileBrowser: { customCollections: [], addDocumentsToCustomCollection: vi.fn() },
}));

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ onDragDropEvent: async () => () => {} }),
}));
vi.mock('@/hooks/useIndexing', () => ({
  useIndexing: () => ({ startBatchImport: mocks.startBatchImport, getOperation: mocks.getOperation, operations: mocks.operations }),
}));
vi.mock('@/hooks/useToast', () => ({ useToast: () => ({ toast: { error: vi.fn(), success: vi.fn() } }) }));
vi.mock('@/stores/conversationsStore', () => ({
  useConversationsStore: (select: (state: { selectedSpaceId: null }) => unknown) => select({ selectedSpaceId: null }),
}));
vi.mock('@/stores/fileBrowserStore', () => ({
  useFileBrowserStore: (select: (state: typeof mocks.fileBrowser) => unknown) => select(mocks.fileBrowser),
}));
vi.mock('@/lib/api', () => ({
  default: {
    selectMultipleFiles: mocks.selectFiles,
    indexFile: mocks.indexFile,
    listConversationSpaces: async () => ({ ok: true, data: [] }),
  },
}));

describe('BatchFileImport', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.operations.clear();
    mocks.getOperation.mockReset();
    mocks.startBatchImport.mockResolvedValue({ ok: false, error: 'Embedding model files are missing. Download MiniLM again.' });
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  it('shows the backend failure for each PDF and lets the user retry', async () => {
    const user = userEvent.setup();
    mocks.selectFiles.mockResolvedValue(['/Downloads/first.pdf', '/Downloads/second.pdf']);
    render(<TooltipProvider><BatchFileImport /></TooltipProvider>);
    await user.click(screen.getByRole('button', { name: 'choose files' }));
    await user.click(screen.getByRole('button', { name: 'Import 2 files' }));

    expect(await screen.findAllByText('Failed — Embedding model files are missing. Download MiniLM again.')).toHaveLength(2);
    expect(screen.getByRole('button', { name: 'Retry 2 files' })).toBeEnabled();
    mocks.startBatchImport.mockResolvedValue({ ok: true, data: 'retry-job' });
    await user.click(screen.getByRole('button', { name: 'Retry 2 files' }));
    await waitFor(() => expect(mocks.startBatchImport).toHaveBeenCalledTimes(2));
    expect(screen.queryByText(/Failed —/)).not.toBeInTheDocument();
  });

  it('uses durable jobs even for a single PDF', async () => {
    const user = userEvent.setup();
    mocks.selectFiles.mockResolvedValue(['/Downloads/first.pdf']);
    mocks.startBatchImport.mockResolvedValue({ ok: true, data: 'single-job' });
    render(<TooltipProvider><BatchFileImport /></TooltipProvider>);
    await user.click(screen.getByRole('button', { name: 'choose files' }));
    await user.click(screen.getByRole('button', { name: 'Import 1 file' }));
    expect(mocks.startBatchImport).toHaveBeenCalledWith(['/Downloads/first.pdf'], undefined);
    expect(mocks.indexFile).not.toHaveBeenCalled();
    expect(await screen.findByText('Queued')).toBeInTheDocument();
  });
  it('restores a running 45-PDF batch with the current file and truthful queued states', async () => {
    const paths = Array.from({ length: 45 }, (_, i) => `/Downloads/chapter-${i}.pdf`);
    const operation = {
      id: 'existing-job', status: 'processing', totalFiles: 45,
      processedFiles: 3, successfulFiles: 2, failedFiles: 1, currentFile: paths[3],
      items: paths.map((target, i) => ({
        target, status: i < 2 ? 'completed' : i === 2 ? 'failed' : i === 3 ? 'running' : 'pending',
        errorMessage: i === 2 ? 'Unreadable PDF' : null,
      })),
    };
    mocks.operations.set(operation.id, operation);
    mocks.getOperation.mockReturnValue(operation);
    render(<TooltipProvider><BatchFileImport /></TooltipProvider>);
    expect(await screen.findByText('3 of 45 files processed · 7%')).toBeInTheDocument();
    expect(screen.getByText('Processing chapter-3.pdf — extracting text and building search index')).toBeInTheDocument();
    expect(screen.getAllByText('Queued')).toHaveLength(41);
    expect(screen.getAllByText('Imported')).toHaveLength(2);
    expect(screen.getByText('Failed — Unreadable PDF')).toBeInTheDocument();
    expect(mocks.startBatchImport).not.toHaveBeenCalled();
  });

  it('restores a finished failed PDF and offers retry after reopening Import', async () => {
    const user = userEvent.setup();
    const operation = {
      id: 'failed-job', status: 'error', totalFiles: 2,
      processedFiles: 2, successfulFiles: 1, failedFiles: 1,
      items: [
        { target: '/Downloads/readable.pdf', status: 'completed' },
        { target: '/Downloads/mpep-2100.pdf', status: 'failed', errorMessage: 'PDF extraction timed out' },
      ],
    };
    mocks.operations.set(operation.id, operation);
    mocks.getOperation.mockReturnValue(operation);
    render(<TooltipProvider><BatchFileImport /></TooltipProvider>);
    expect(await screen.findByText('Failed — PDF extraction timed out')).toBeInTheDocument();
    expect(screen.getByText('Imported')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Retry or replace failed files' })).toBeEnabled();
    expect(screen.getByRole('button', { name: 'Import' })).toBeDisabled();
    // A later import that cannot start must not overwrite the saved PDF error.
    mocks.selectFiles.mockResolvedValue(['/Downloads/new.pdf']);
    await user.click(screen.getByRole('button', { name: 'choose files' }));
    await user.click(screen.getByRole('button', { name: 'Import 1 file' }));
    expect(await screen.findByText('Failed — Embedding model files are missing. Download MiniLM again.')).toBeVisible();
    expect(screen.getByText('Failed — PDF extraction timed out')).toBeVisible();
  });

});

describe('related source import', () => {
  it('starts a new related source after restoring failed work without restoring it again', async () => {
    vi.clearAllMocks(); mocks.operations.clear(); mocks.getOperation.mockReset();
    const operation = {
      id: 'old-book', status: 'error', totalFiles: 1,
      processedFiles: 1, successfulFiles: 0, failedFiles: 1,
      items: [{ target: '/Downloads/old.pdf', status: 'failed', errorMessage: 'Unreadable PDF' }],
    };
    mocks.operations.set(operation.id, operation);
    mocks.getOperation.mockReturnValue(operation);
    const user = userEvent.setup();
    render(<TooltipProvider><BatchFileImport /></TooltipProvider>);
    expect(await screen.findByText('Failed — Unreadable PDF')).toBeVisible();
    expect(screen.getByRole('checkbox', { name: /same book/ })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: 'Start a new import' }));
    expect(screen.queryByText('old.pdf')).not.toBeInTheDocument();
    expect(mocks.operations.get('old-book')).toBe(operation);
    await user.click(screen.getByRole('checkbox', { name: /same book/ }));
    await user.type(screen.getByRole('textbox', { name: 'Source title' }), 'New book');
    mocks.selectFiles.mockResolvedValue(['C:\\Downloads\\chapter2.pdf', 'C:\\Downloads\\chapter2.pdf']);
    mocks.startBatchImport.mockResolvedValue({ ok: true, data: 'new-book' });
    await user.click(screen.getByRole('button', { name: 'choose files' }));
    expect(screen.getByText('chapter2.pdf')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Import 1 file' }));
    expect(mocks.startBatchImport).toHaveBeenCalledWith(
      ['C:\\Downloads\\chapter2.pdf'], undefined,
      { sourceGroup: expect.objectContaining({ title: 'New book' }), rebuildExisting: false }
    );
  });

  it('requires a shared title, sends the displayed reading order and contextual indexing options', async () => {
    vi.clearAllMocks(); mocks.operations.clear(); mocks.getOperation.mockReset();
    const user = userEvent.setup();
    mocks.selectFiles.mockResolvedValue(['/Downloads/chapter10.pdf', '/Downloads/chapter2.pdf']);
    mocks.startBatchImport.mockResolvedValue({ ok: true, data: 'ordered-book' });
    render(<TooltipProvider><BatchFileImport /></TooltipProvider>);
    await user.click(screen.getByRole('button', { name: 'choose files' }));
    await user.click(screen.getByRole('checkbox', { name: /same book/ }));
    expect(screen.getByRole('button', { name: 'Import 2 files' })).toBeDisabled();
    await user.type(screen.getByRole('textbox', { name: 'Source title' }), 'MPEP');
    await user.type(screen.getByRole('textbox', { name: 'Edition or version' }), '2024');
    await user.click(screen.getByRole('button', { name: 'Sort by filename' }));
    await user.click(screen.getByRole('button', { name: 'Import 2 files' }));
    expect(mocks.startBatchImport).toHaveBeenCalledWith(
      ['/Downloads/chapter2.pdf', '/Downloads/chapter10.pdf'], undefined,
      { sourceGroup: expect.objectContaining({ title: 'MPEP', edition: '2024', ordered: true, structure: 'sections' }), rebuildExisting: false }
    );
  });
});
