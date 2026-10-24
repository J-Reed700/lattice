import { open } from '@tauri-apps/plugin-dialog';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { IndexingTab } from './IndexingTab';

type Callbacks = { onError?: (error: Error) => void };

const h = vi.hoisted(() => ({
  settings: { data: undefined as unknown, isLoading: false },
  mutation: { mutate: vi.fn(), isPending: false },
  addWatchFolder: vi.fn(),
  removeWatchFolder: vi.fn(),
  toast: { error: vi.fn() },
}));
vi.mock('../hooks/useSettingsQuery', () => ({
  useSettingsQuery: () => h.settings,
  useUpdateSettingsMutation: () => h.mutation,
  useWatchFolderMutations: () => ({ addWatchFolder: h.addWatchFolder, removeWatchFolder: h.removeWatchFolder }),
}));
vi.mock('@/stores/toastStore', () => ({ toast: h.toast }));
vi.mock('@/features/files/components/IndexingStatus/IndexingActivitySection', () => ({ IndexingActivitySection: () => null }));

const indexing = (overrides: Record<string, unknown> = {}) => ({
  data: { indexing: { batchSize: 16, indexedPaths: ['/Users/ada/Archive'], excludePatterns: ['*.tmp'], autoIndexNewFiles: true, ...overrides } },
  isLoading: false,
});
const lastCallbacks = (): Callbacks => h.mutation.mutate.mock.calls.at(-1)![1];

describe('IndexingTab controls', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    h.settings = indexing();
    h.mutation.isPending = false;
    h.addWatchFolder.mockResolvedValue({ ok: true, data: undefined });
    h.removeWatchFolder.mockResolvedValue({ ok: true, data: undefined });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('saves auto-indexing, reporting a failure', () => {
    render(<IndexingTab />);
    fireEvent.click(screen.getByRole('switch'));
    expect(h.mutation.mutate.mock.calls[0][0]).toEqual({ category: 'indexing', updates: { autoIndexNewFiles: false } });
    lastCallbacks().onError?.(new Error('read-only settings'));
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't save auto-index setting", { message: 'read-only settings' });
  });

  it('saves a batch size on Enter or blur, within bounds, and rolls back a failure', () => {
    render(<IndexingTab />);
    const field = screen.getByLabelText('Batch size');
    fireEvent.change(field, { target: { value: '500' } });
    expect(field).toHaveValue(128);
    fireEvent.change(field, { target: { value: 'abc' } });
    expect(field).toHaveValue(128);
    fireEvent.keyDown(field, { key: 'Enter' });
    expect(h.mutation.mutate.mock.calls[0][0]).toEqual({ category: 'indexing', updates: { batchSize: 128 } });
    act(() => lastCallbacks().onError?.(new Error('too large')));
    expect(field).toHaveValue(16);
    expect(h.toast.error).toHaveBeenCalledWith("Couldn't save batch size", { message: 'too large' });
    fireEvent.keyDown(field, { key: 'a' });
    fireEvent.blur(field);
    expect(h.mutation.mutate).toHaveBeenCalledTimes(1);
  });

  it('says when it is saving the batch size', () => {
    h.mutation.isPending = true;
    render(<IndexingTab />);
    expect(screen.getByText('Files indexed per pass · saving…')).toBeInTheDocument();
  });

  describe('watched folders', () => {
    it('adds a picked folder, skipping one already watched, and reports a refusal', async () => {
      vi.mocked(open).mockResolvedValueOnce('/Users/ada/Archive').mockResolvedValueOnce('/Volumes/Notes').mockResolvedValueOnce('/Volumes/Bad');
      h.addWatchFolder.mockResolvedValueOnce({ ok: true, data: undefined }).mockResolvedValueOnce({ ok: false, error: 'folder unreadable' });
      render(<IndexingTab />);
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Add folder' })); });
      expect(h.addWatchFolder).not.toHaveBeenCalled();
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Add folder' })); });
      expect(h.addWatchFolder).toHaveBeenCalledWith('/Volumes/Notes');
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Add folder' })); });
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't add watch folder", { message: 'folder unreadable' });
    });

    it('survives a picker that fails', async () => {
      const error = vi.spyOn(console, 'error').mockImplementation(() => {});
      vi.mocked(open).mockRejectedValueOnce(new Error('no dialog'));
      render(<IndexingTab />);
      await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Add folder' })); });
      expect(error).toHaveBeenCalled();
      expect(screen.getByRole('button', { name: 'Add folder' })).toBeEnabled();
    });

    it('removes a folder, reporting a refusal', async () => {
      h.removeWatchFolder.mockResolvedValueOnce({ ok: false, error: 'in use' });
      render(<IndexingTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Remove folder /Users/ada/Archive' }));
      await waitFor(() => expect(h.toast.error).toHaveBeenCalledWith("Couldn't remove watch folder", { message: 'in use' }));
    });

    it('says when nothing is watched', () => {
      h.settings = indexing({ indexedPaths: [], excludePatterns: [] });
      render(<IndexingTab />);
      expect(screen.getByText('No folders watched. Add one to start indexing.')).toBeInTheDocument();
      expect(screen.getByText('Nothing excluded.')).toBeInTheDocument();
    });
  });

  describe('excluded patterns', () => {
    it('adds a trimmed pattern on Enter, ignoring blanks and repeats', () => {
      render(<IndexingTab />);
      const field = screen.getByLabelText('Add a pattern');
      fireEvent.change(field, { target: { value: '  ' } });
      fireEvent.keyDown(field, { key: 'Enter' });
      fireEvent.change(field, { target: { value: '*.tmp' } });
      fireEvent.click(screen.getByRole('button', { name: 'Add' }));
      expect(h.mutation.mutate).not.toHaveBeenCalled();
      expect(field).toHaveValue('');
      fireEvent.change(field, { target: { value: ' .git ' } });
      fireEvent.keyDown(field, { key: 'Enter' });
      expect(h.mutation.mutate.mock.calls[0][0]).toEqual({ category: 'indexing', updates: { excludePatterns: ['*.tmp', '.git'] } });
      lastCallbacks().onError?.(new Error('bad glob'));
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't add exclude pattern", { message: 'bad glob' });
      fireEvent.keyDown(field, { key: 'a' });
    });

    it('removes a pattern, reporting a failure', () => {
      render(<IndexingTab />);
      fireEvent.click(screen.getByRole('button', { name: 'Remove pattern *.tmp' }));
      expect(h.mutation.mutate.mock.calls[0][0]).toEqual({ category: 'indexing', updates: { excludePatterns: [] } });
      lastCallbacks().onError?.(new Error('locked'));
      expect(h.toast.error).toHaveBeenCalledWith("Couldn't remove exclude pattern", { message: 'locked' });
    });
  });
});
