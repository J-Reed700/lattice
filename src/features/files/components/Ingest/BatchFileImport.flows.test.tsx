import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TooltipProvider } from '@/components/ui/tooltip';

import { BatchFileImport } from './BatchFileImport';

type DragDropPayload = { type: 'over' | 'drop' | 'leave'; paths?: string[] };

const mocks = vi.hoisted(() => ({
  selectFiles: vi.fn(),
  startBatchImport: vi.fn(),
  cancelBatchImport: vi.fn(),
  getOperation: vi.fn(),
  operations: new Map(),
  historyError: undefined as string | undefined,
  collectionActions: { addDocuments: vi.fn() },
  collectionsQuery: { data: [] as unknown[], error: null as Error | null, refetch: vi.fn() },
  spaces: [] as unknown[],
  selectedSpaceId: null as string | null,
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
  dragHandler: null as null | ((event: { payload: DragDropPayload }) => void),
  onDragDropEvent: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => ({ onDragDropEvent: mocks.onDragDropEvent }) }));
vi.mock('@/features/files/hooks/useIndexing', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/files/hooks/useIndexing')>()),
  useIndexing: () => ({
    startBatchImport: mocks.startBatchImport,
    cancelBatchImport: mocks.cancelBatchImport,
    getOperation: mocks.getOperation,
    operations: mocks.operations,
    historyError: mocks.historyError,
  }),
}));
vi.mock('@/hooks/useToast', () => ({ useToast: () => ({ toast: mocks.toast }) }));
vi.mock('@/shared/conversations/conversationsStore', () => ({
  useConversationsStore: (select: (state: { selectedSpaceId: string | null }) => unknown) => select({ selectedSpaceId: mocks.selectedSpaceId }),
}));
vi.mock('@/features/files/hooks/useCustomCollectionsQuery', () => ({
  useCustomCollectionsQuery: () => mocks.collectionsQuery,
  useCustomCollectionActions: () => mocks.collectionActions,
}));
vi.mock('@/features/spaces/api/queries', () => ({ useSpacesQuery: () => ({ data: mocks.spaces, isLoading: false }) }));
vi.mock('@/lib/api', () => ({ default: { selectMultipleFiles: mocks.selectFiles } }));

const renderImport = (props: Parameters<typeof BatchFileImport>[0] = {}) =>
  render(<TooltipProvider><BatchFileImport {...props} /></TooltipProvider>);

/** Clicks and lets the async handler behind the button settle. */
const press = async (name: string) => {
  await act(async () => { fireEvent.click(screen.getByRole('button', { name })); });
};

const choose = async (paths: string[]) => {
  mocks.selectFiles.mockResolvedValueOnce(paths);
  await press('choose files');
};

const collection = (id: string, name: string, parentId: string | null = null, kind = 'manual') =>
  ({ id, name, kind, parentId, documentIds: [], createdAt: '', updatedAt: '' });

describe('BatchFileImport flows', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.operations.clear();
    mocks.getOperation.mockReset();
    mocks.historyError = undefined;
    mocks.collectionsQuery = { data: [], error: null, refetch: vi.fn() };
    mocks.spaces = [];
    mocks.selectedSpaceId = null;
    mocks.dragHandler = null;
    mocks.onDragDropEvent.mockImplementation(async (handler: (event: { payload: DragDropPayload }) => void) => {
      mocks.dragHandler = handler;
      return mocks.unlisten;
    });
    mocks.startBatchImport.mockResolvedValue({ ok: true, data: 'job-1' });
    mocks.cancelBatchImport.mockResolvedValue({ ok: true, data: 1 });
    mocks.collectionActions.addDocuments.mockResolvedValue(undefined);
    vi.spyOn(console, 'error').mockImplementation(() => {});
    vi.spyOn(console, 'log').mockImplementation(() => {});
  });

  it('warns about earlier results it could not load', () => {
    mocks.historyError = 'history table locked';
    renderImport();
    expect(screen.getByRole('alert')).toHaveTextContent('Previous import results could not be loaded: history table locked.');
  });

  describe('adding files', () => {
    it('lists supported files once, of every kind', async () => {
      renderImport();
      const names = ['a.pdf', 'b.docx', 'c.rtf', 'd.odt', 'e.xlsx', 'f.pptx', 'g.txt', 'h.md', 'i.markdown', 'j.html', 'k.htm', 'l.csv', 'm.tsv', 'n.json', 'o.xml', 'p.py'];
      await choose([...names.map((name) => `/field-notes/${name}`), '/field-notes/a.pdf']);
      expect(screen.getByRole('heading', { name: `${names.length} files` })).toBeInTheDocument();
      expect(screen.getByRole('button', { name: `Import ${names.length} files` })).toBeEnabled();
    });

    it('names the files it cannot import', async () => {
      renderImport();
      await choose(['/a/one.exe', '/a/two.exe', '/a/three.exe', '/a/four.exe', '/a/ok.pdf']);
      expect(mocks.toast.error).toHaveBeenCalledWith('Unsupported file type(s): one.exe, two.exe, three.exe and 1 more');
      await choose(['/a/five.bin']);
      expect(mocks.toast.error).toHaveBeenLastCalledWith('Unsupported file type(s): five.bin');
      expect(screen.getByRole('heading', { name: '1 file' })).toBeInTheDocument();
    });

    it('reports a picker that failed', async () => {
      mocks.selectFiles.mockRejectedValueOnce(new Error('dialog unavailable'));
      renderImport();
      await press('choose files');
      expect(mocks.toast.error).toHaveBeenCalledWith("Couldn't choose files: dialog unavailable");
    });

    it('removes a file before import', async () => {
      renderImport();
      await choose(['/a/one.pdf', '/a/two.pdf']);
      fireEvent.click(screen.getAllByRole('button', { name: 'Remove file' })[0]);
      expect(screen.queryByText('one.pdf')).not.toBeInTheDocument();
      expect(screen.getByRole('heading', { name: '1 file' })).toBeInTheDocument();
    });

    it('reverses the order of several files', async () => {
      renderImport();
      await choose(['/a/one.pdf', '/a/two.pdf']);
      fireEvent.click(screen.getByRole('button', { name: 'Reverse order' }));
      await press('Import 2 files');
      expect(mocks.startBatchImport).toHaveBeenCalledWith(['/a/two.pdf', '/a/one.pdf'], undefined);
    });

    it('refuses more than 100 files in one import', async () => {
      renderImport();
      await choose(Array.from({ length: 101 }, (_v, i) => `/a/page-${i}.pdf`));
      expect(screen.getByRole('alert')).toHaveTextContent('Select up to 100 files per import.');
      expect(screen.getByRole('button', { name: 'Import 101 files' })).toBeDisabled();
    });
  });

  describe('dropping files', () => {
    const dataTransfer = (files: Array<{ name: string; path?: string }>) => ({ files: files.map((file) => Object.assign(new File([''], file.name), file.path ? { path: file.path } : {})) });

    it('highlights while dragging and adds files with a path', () => {
      const { container } = renderImport();
      const zone = container.querySelector('.border-dashed') as HTMLElement;
      fireEvent.dragOver(zone);
      expect(zone.className).toContain('border-accent');
      fireEvent.dragLeave(zone);
      expect(zone.className).not.toContain('border-accent');
      fireEvent.drop(zone, { dataTransfer: dataTransfer([{ name: 'moss.pdf', path: '/bryophytes/moss.pdf' }]) });
      expect(screen.getByText('moss.pdf')).toBeInTheDocument();
    });

    it('explains files dropped without a path', () => {
      const { container } = renderImport();
      const zone = container.querySelector('.border-dashed') as HTMLElement;
      fireEvent.drop(zone, { dataTransfer: dataTransfer([{ name: 'moss.pdf' }, { name: 'tool.exe' }]) });
      expect(mocks.toast.error).toHaveBeenCalledWith('Dropped file "moss.pdf" is missing a filesystem path. Please use the file picker.');
      expect(mocks.toast.error).toHaveBeenCalledWith('Unsupported file type: tool.exe');
    });

    it('takes drops from the window and stops listening on unmount', async () => {
      const { container, unmount } = renderImport();
      await waitFor(() => expect(mocks.dragHandler).not.toBeNull());
      const zone = container.querySelector('.border-dashed') as HTMLElement;
      act(() => mocks.dragHandler!({ payload: { type: 'over' } }));
      expect(zone.className).toContain('border-accent');
      act(() => mocks.dragHandler!({ payload: { type: 'leave' } }));
      expect(zone.className).not.toContain('border-accent');
      act(() => mocks.dragHandler!({ payload: { type: 'drop', paths: ['/lichens/usnea.md'] } }));
      expect(screen.getByText('usnea.md')).toBeInTheDocument();
      act(() => mocks.dragHandler!({ payload: { type: 'drop' } }));
      unmount();
      expect(mocks.unlisten).toHaveBeenCalled();
    });

    it('ignores window events after unmount and survives a missing bridge', async () => {
      const first = renderImport();
      await waitFor(() => expect(mocks.dragHandler).not.toBeNull());
      const handler = mocks.dragHandler!;
      first.unmount();
      handler({ payload: { type: 'over' } });
      mocks.onDragDropEvent.mockRejectedValueOnce(new Error('no window'));
      renderImport();
      await waitFor(() => expect(console.error).toHaveBeenCalledWith('[BatchFileImport] Failed to listen for file drops:', expect.any(Error)));
    });
  });

  describe('destination', () => {
    it('defaults to the space open in Chat and lists spaces', () => {
      mocks.selectedSpaceId = 's-bio';
      mocks.spaces = [{ id: 's-bio', name: 'Biology', icon: '🧬', isArchived: false }, { id: 's-old', name: 'Old', icon: null, isArchived: true }];
      renderImport();
      const select = screen.getByLabelText('Add to space') as HTMLSelectElement;
      expect(select).toHaveValue('s-bio');
      expect(within(select).getByRole('option', { name: '🧬 Biology' })).toBeInTheDocument();
      expect(within(select).getByRole('option', { name: 'Old (archived)' })).toBeInTheDocument();
      fireEvent.change(select, { target: { value: '' } });
      expect(select).toHaveValue('');
    });

    it('labels nested manual collections by their path, skipping snapshots and loops', () => {
      mocks.collectionsQuery.data = [
        collection('k1', 'Kitchen'),
        collection('k2', 'Breads', 'k1'),
        collection('k3', 'Loop A', 'k4'),
        collection('k4', 'Loop B', 'k3'),
        collection('k5', 'Orphan', 'gone'),
        collection('k6', 'Snapshot', null, 'snapshot'),
      ];
      renderImport();
      const options = within(screen.getByLabelText('Add to collection')).getAllByRole('option').map((option) => option.textContent);
      expect(options).toEqual(['None', 'Kitchen', 'Kitchen / Breads', 'Loop A / Loop B', 'Loop B / Loop A', 'Orphan']);
    });

    it('says when there are no collections', () => {
      renderImport();
      expect(screen.getByText('No collections yet')).toBeInTheDocument();
    });

    it('files completed documents into the chosen collection', async () => {
      mocks.collectionsQuery.data = [collection('k1', 'Kitchen')];
      const onImportComplete = vi.fn();
      renderImport({ onImportComplete });
      fireEvent.change(screen.getByLabelText('Add to collection'), { target: { value: 'k1' } });
      await choose(['/a/one.pdf', '/a/two.pdf']);
      mocks.getOperation.mockReturnValue({
        id: 'job-1', status: 'completed', totalFiles: 2, processedFiles: 2, successfulFiles: 1, failedFiles: 1,
        items: [
          { target: '/a/one.pdf', status: 'completed', documentId: 'd1' },
          { target: '/a/two.pdf', status: 'failed', errorMessage: null },
        ],
      });
      await press('Import 2 files');
      await waitFor(() => expect(mocks.collectionActions.addDocuments).toHaveBeenCalledWith('k1', ['d1']));
      expect(onImportComplete).toHaveBeenCalledWith({ successful: 1, failed: 1 });
      expect(screen.getByText('Imported')).toBeInTheDocument();
      expect(screen.getByText('Failed — Import failed')).toBeInTheDocument();
    });

    it('reports documents that could not be filed into the collection', async () => {
      mocks.collectionsQuery.data = [collection('k1', 'Kitchen')];
      mocks.collectionActions.addDocuments.mockRejectedValueOnce(new Error('collection deleted'));
      renderImport();
      fireEvent.change(screen.getByLabelText('Add to collection'), { target: { value: 'k1' } });
      await choose(['/a/one.pdf']);
      mocks.getOperation.mockReturnValue({
        id: 'job-1', status: 'error', error: 'worker crashed', totalFiles: 1, processedFiles: 1, successfulFiles: 1, failedFiles: 0,
        items: [{ target: '/a/one.pdf', status: 'completed', documentId: 'd1' }],
      });
      await press('Import 1 file');
      await waitFor(() => expect(mocks.toast.error).toHaveBeenCalledWith('Imported documents could not be added to the collection', { message: 'collection deleted' }));
    });
  });

  describe('starting an import', () => {
    it('fails every file when no job id comes back', async () => {
      mocks.startBatchImport.mockResolvedValueOnce({ ok: true, data: '' });
      renderImport();
      await choose(['/a/one.pdf']);
      await press('Import 1 file');
      expect(await screen.findByText('Failed — Import did not return a job ID')).toBeInTheDocument();
    });

    it('prefers the detailed reason for a refused import', async () => {
      mocks.startBatchImport.mockResolvedValueOnce({ ok: false, error: 'refused', details: { details: 'Disk is full' } });
      renderImport();
      await choose(['/a/one.pdf']);
      await press('Import 1 file');
      expect(await screen.findByText('Failed — Disk is full')).toBeInTheDocument();
    });

    it('sends the chosen space', async () => {
      mocks.spaces = [{ id: 's-bio', name: 'Biology', icon: null, isArchived: false }];
      renderImport();
      fireEvent.change(screen.getByLabelText('Add to space'), { target: { value: 's-bio' } });
      await choose(['/a/one.pdf']);
      await press('Import 1 file');
      expect(mocks.startBatchImport).toHaveBeenCalledWith(['/a/one.pdf'], 's-bio');
    });
  });

  describe('following a job', () => {
    /** The job reports `operation` from the moment it starts. */
    const startJob = async (operation: unknown, props: Parameters<typeof BatchFileImport>[0] = {}) => {
      mocks.getOperation.mockReturnValue(operation);
      renderImport(props);
      await choose(['/a/one.pdf', '/a/two.pdf']);
      await press('Import 2 files');
    };

    it('shows preparing progress before the job reports', async () => {
      await startJob(undefined);
      expect(screen.getByRole('status')).toHaveTextContent('0 of 2 files processed · 0%');
      expect(screen.getByText('Preparing PDFs and building search indexes…')).toBeInTheDocument();
    });

    it('marks every file imported when a job without items fully succeeds', async () => {
      const onImportComplete = vi.fn();
      await startJob({ id: 'job-1', status: 'completed', totalFiles: 2, processedFiles: 2, successfulFiles: 2, failedFiles: 0 }, { onImportComplete });
      await waitFor(() => expect(screen.getAllByText('Imported')).toHaveLength(2));
      expect(onImportComplete).toHaveBeenCalledWith({ successful: 2, failed: 0 });
    });

    it('keeps files uncertain when a job without items only partly succeeds', async () => {
      await startJob({ id: 'job-1', status: 'processing', totalFiles: 2, processedFiles: 1, successfulFiles: 1, failedFiles: 1, error: 'one file unreadable' });
      expect(screen.getAllByText('Queued')).toHaveLength(2);
      expect(screen.getByText('1 failed')).toBeInTheDocument();
      expect(within(screen.getByRole('status')).getByRole('alert')).toHaveTextContent('one file unreadable');
    });

    it('marks queued files cancelled once a cancelled job settles', async () => {
      await startJob({
        id: 'job-1', status: 'cancelled', totalFiles: 2, processedFiles: 1, successfulFiles: 1, failedFiles: 0,
        items: [{ target: '/a/one.pdf', status: 'completed' }, { target: '/a/two.pdf', status: 'cancelled' }],
      });
      await waitFor(() => expect(screen.getByText('Cancelled')).toBeInTheDocument());
      expect(screen.getByRole('button', { name: 'Start a new import' })).toBeInTheDocument();
    });

    it('fails the unfinished files when the job errors, offering a review', async () => {
      const onImportComplete = vi.fn();
      const onReviewFailures = vi.fn();
      await startJob({ id: 'job-1', status: 'error', error: null, totalFiles: 2, processedFiles: 0, successfulFiles: 0, failedFiles: 2 }, { onImportComplete, onReviewFailures });
      expect(await screen.findAllByText('Failed — Import failed')).toHaveLength(2);
      expect(screen.getByText('2 files failed. Details are saved in History.')).toBeInTheDocument();
      fireEvent.click(screen.getByRole('button', { name: 'Retry or replace failed files' }));
      expect(onReviewFailures).toHaveBeenCalled();
      expect(onImportComplete).toHaveBeenCalledWith({ successful: 0, failed: 2 });
    });

    it('counts a single failure in the singular', async () => {
      await startJob({
        id: 'job-1', status: 'completed', totalFiles: 2, processedFiles: 2, successfulFiles: 1, failedFiles: 1,
        items: [{ target: '/a/one.pdf', status: 'completed' }, { target: '/a/two.pdf', status: 'failed', errorMessage: 'Encrypted PDF' }],
      });
      expect(await screen.findByText('1 file failed. Details are saved in History.')).toBeInTheDocument();
    });
  });

  describe('cancel', () => {
    const running = async () => {
      mocks.getOperation.mockReturnValue({ id: 'job-1', status: 'processing', totalFiles: 2, processedFiles: 0, successfulFiles: 0, failedFiles: 0 });
      renderImport();
      await choose(['/a/one.pdf', '/a/two.pdf']);
      await press('Import 2 files');
    };

    it('cancels a running job', async () => {
      await running();
      await press('Cancel');
      expect(mocks.cancelBatchImport).toHaveBeenCalledWith('job-1');
      expect(mocks.toast.info).toHaveBeenCalledWith('Import cancelled');
    });

    it('reports a refused or failed cancel', async () => {
      await running();
      mocks.cancelBatchImport.mockResolvedValueOnce({ ok: false, error: 'job already finished' });
      await press('Cancel');
      expect(mocks.toast.error).toHaveBeenCalledWith("Couldn't cancel import: job already finished");
      mocks.cancelBatchImport.mockRejectedValueOnce(new Error('ipc closed'));
      await press('Cancel');
      expect(mocks.toast.error).toHaveBeenLastCalledWith("Couldn't cancel import: ipc closed");
    });

    it('closes when idle, or clears the list without a close handler', async () => {
      const onClose = vi.fn();
      const first = renderImport({ onClose });
      await press('Cancel');
      expect(onClose).toHaveBeenCalled();
      first.unmount();
      renderImport();
      await choose(['/a/one.pdf']);
      await press('Cancel');
      expect(screen.queryByText('one.pdf')).not.toBeInTheDocument();
    });
  });
});
