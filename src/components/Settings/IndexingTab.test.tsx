import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { IndexingTab } from './IndexingTab';


const {
  mockDialogOpen,
  mockGetSettings,
  mockUpdateSettings,
  mockAddWatchFolder,
  mockRemoveWatchFolder,
} = vi.hoisted(() => ({
  mockDialogOpen: vi.fn(),
  mockGetSettings: vi.fn(),
  mockUpdateSettings: vi.fn(),
  mockAddWatchFolder: vi.fn(),
  mockRemoveWatchFolder: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: mockDialogOpen,
}));

vi.mock('@/lib/api', () => ({
  __esModule: true,
  default: {
    getSettings: mockGetSettings,
    updateSettings: mockUpdateSettings,
    addWatchFolder: mockAddWatchFolder,
    removeWatchFolder: mockRemoveWatchFolder,
  },
}));

const baseSettings = {
  indexing: {
    chunkSize: 800,
    chunkOverlap: 120,
    batchSize: 32,
    autoIndexNewFiles: true,
    fileTypes: ['txt', 'md'],
    indexedPaths: ['/home/user/documents', '/home/user/downloads'],
    excludePatterns: ['*.git', 'node_modules', '*.tmp'],
  },
};

function renderTab() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  return render(<IndexingTab />, { wrapper });
}

describe('IndexingTab', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSettings.mockResolvedValue({ ok: true, data: baseSettings });
    mockUpdateSettings.mockResolvedValue({ ok: true, data: baseSettings });
    mockAddWatchFolder.mockResolvedValue({ ok: true, data: undefined });
    mockRemoveWatchFolder.mockResolvedValue({ ok: true, data: undefined });
  });

  it('renders the page header and sections', async () => {
    renderTab();
    expect(screen.getByRole('heading', { level: 1, name: 'Indexing' })).toBeInTheDocument();
    expect(await screen.findByText('Behavior')).toBeInTheDocument();
    expect(screen.getByText('Watched folders')).toBeInTheDocument();
    expect(screen.getByText('Excluded patterns')).toBeInTheDocument();
  });

  it('displays auto-index checkbox with current state', async () => {
    renderTab();
    const checkbox = (await screen.findByLabelText('Auto-index new files')) as HTMLInputElement;
    await waitFor(() => expect(checkbox).toBeChecked());
  });

  it('toggles auto-index by calling updateSettings', async () => {
    const user = userEvent.setup();
    renderTab();
    const checkbox = await screen.findByLabelText('Auto-index new files');
    await waitFor(() => expect(checkbox).not.toBeDisabled());

    await user.click(checkbox);

    await waitFor(() => {
      expect(mockUpdateSettings).toHaveBeenCalledWith(
        expect.objectContaining({
          category: 'indexing',
          updates: { autoIndexNewFiles: false },
        })
      );
    });
  });

  it('displays current batch size value from backend', async () => {
    renderTab();
    expect(await screen.findByLabelText('Batch size')).toHaveValue(32);
  });

  describe('Watch Folders', () => {
    it('displays watch folders list', async () => {
      renderTab();
      expect(await screen.findByText('/home/user/documents')).toBeInTheDocument();
      expect(screen.getByText('/home/user/downloads')).toBeInTheDocument();
    });

    it('shows empty state when no folders watched', async () => {
      mockGetSettings.mockResolvedValue({
        ok: true,
        data: {
          indexing: { ...baseSettings.indexing, indexedPaths: [] },
        },
      });
      renderTab();
      expect(
        await screen.findByText('No folders watched. Add one to start indexing.')
      ).toBeInTheDocument();
    });

    it('adds folder via Tauri dialog + addWatchFolder command', async () => {
      const user = userEvent.setup();
      mockDialogOpen.mockResolvedValue('/home/user/projects');
      renderTab();

      const addButton = await screen.findByRole('button', { name: /add folder/i });
      await waitFor(() => expect(addButton).not.toBeDisabled());
      await user.click(addButton);

      await waitFor(() => {
        expect(mockAddWatchFolder).toHaveBeenCalledWith('/home/user/projects');
      });
    });

    it('does not add folder when dialog cancelled', async () => {
      const user = userEvent.setup();
      mockDialogOpen.mockResolvedValue(null);
      renderTab();

      const addButton = await screen.findByRole('button', { name: /add folder/i });
      await waitFor(() => expect(addButton).not.toBeDisabled());
      await user.click(addButton);

      await waitFor(() => {
        expect(mockDialogOpen).toHaveBeenCalled();
        expect(mockAddWatchFolder).not.toHaveBeenCalled();
      });
    });

    it('removes folder via removeWatchFolder command', async () => {
      const user = userEvent.setup();
      renderTab();

      const removeButtons = await screen.findAllByTitle('Remove folder');
      await user.click(removeButtons[0]);

      await waitFor(() => {
        expect(mockRemoveWatchFolder).toHaveBeenCalledWith('/home/user/documents');
      });
    });
  });

  describe('Exclude Patterns', () => {
    it('displays exclude patterns list', async () => {
      renderTab();
      expect(await screen.findByText('*.git')).toBeInTheDocument();
      expect(screen.getByText('node_modules')).toBeInTheDocument();
      expect(screen.getByText('*.tmp')).toBeInTheDocument();
    });

    it('adds pattern via updateSettings', async () => {
      const user = userEvent.setup();
      mockGetSettings.mockResolvedValue({
        ok: true,
        data: {
          indexing: { ...baseSettings.indexing, excludePatterns: [] },
        },
      });
      renderTab();

      const input = await screen.findByPlaceholderText('*.tmp, node_modules, .git');
      await user.type(input, '*.log');

      const addButton = screen.getByRole('button', { name: /^add$/i });
      await waitFor(() => expect(addButton).not.toBeDisabled());
      await user.click(addButton);

      await waitFor(() => {
        expect(mockUpdateSettings).toHaveBeenCalledWith(
          expect.objectContaining({
            category: 'indexing',
            updates: { excludePatterns: ['*.log'] },
          })
        );
      });
    });

    it('does not add empty pattern (button disabled)', async () => {
      mockGetSettings.mockResolvedValue({
        ok: true,
        data: {
          indexing: { ...baseSettings.indexing, excludePatterns: [] },
        },
      });
      const user = userEvent.setup();
      renderTab();

      const input = await screen.findByPlaceholderText('*.tmp, node_modules, .git');
      await user.type(input, '   ');

      const addButton = screen.getByRole('button', { name: /^add$/i });
      expect(addButton).toBeDisabled();
    });

    it('removes pattern via updateSettings', async () => {
      const user = userEvent.setup();
      renderTab();

      const removeButtons = await screen.findAllByTitle('Remove pattern');
      await user.click(removeButtons[0]);

      await waitFor(() => {
        expect(mockUpdateSettings).toHaveBeenCalledWith(
          expect.objectContaining({
            category: 'indexing',
            updates: { excludePatterns: ['node_modules', '*.tmp'] },
          })
        );
      });
    });
  });
});
