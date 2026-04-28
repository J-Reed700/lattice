import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { IndexingTab } from './IndexingTab';

import type { ReactNode } from 'react';

const { mockDialogOpen, mockGetConfig, mockGetSettings, mockSaveConfig, mockAddWatchFolder, mockRemoveWatchFolder, mockUpdateSettings } =
  vi.hoisted(() => ({
    mockDialogOpen: vi.fn(),
    mockGetConfig: vi.fn(),
    mockGetSettings: vi.fn(),
    mockSaveConfig: vi.fn(),
    mockAddWatchFolder: vi.fn(),
    mockRemoveWatchFolder: vi.fn(),
    mockUpdateSettings: vi.fn(),
  }));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: mockDialogOpen,
}));

vi.mock('@/lib/api', () => ({
  __esModule: true,
  default: {
    getConfig: mockGetConfig,
    getSettings: mockGetSettings,
    saveConfig: mockSaveConfig,
    addWatchFolder: mockAddWatchFolder,
    removeWatchFolder: mockRemoveWatchFolder,
    updateSettings: mockUpdateSettings,
  },
}));

const baseConfig = {
  indexedPaths: ['/home/user/documents', '/home/user/downloads'],
  excludePatterns: ['*.git', 'node_modules', '*.tmp'],
  autoIndex: true,
  ollamaEndpoint: 'http://localhost:11434',
  ollamaModel: '',
};

const baseSettings = {
  indexing: {
    chunkSize: 800,
    chunkOverlap: 120,
    batchSize: 32,
    autoIndexNewFiles: true,
    fileTypes: ['txt', 'md'],
    excludedPaths: [],
  },
};

function renderTab() {
  // Each test gets a fresh client so cached query data doesn't leak between tests.
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
    mockGetConfig.mockResolvedValue({ ok: true, data: baseConfig });
    mockGetSettings.mockResolvedValue({ ok: true, data: baseSettings });
    mockSaveConfig.mockResolvedValue({ ok: true, data: undefined });
    mockAddWatchFolder.mockResolvedValue({ ok: true, data: undefined });
    mockRemoveWatchFolder.mockResolvedValue({ ok: true, data: undefined });
    mockUpdateSettings.mockResolvedValue({ ok: true, data: baseSettings });
  });

  it('renders indexing settings header', async () => {
    renderTab();
    expect(screen.getByText('Indexing Settings')).toBeInTheDocument();
    expect(await screen.findByText('Manage file watching and indexing behavior')).toBeInTheDocument();
  });

  it('displays auto-index checkbox with current state', async () => {
    renderTab();
    const checkbox = await screen.findByLabelText('Auto-index New Files') as HTMLInputElement;
    await waitFor(() => expect(checkbox).toBeChecked());
  });

  it('toggles auto-index when checkbox clicked', async () => {
    const user = userEvent.setup();
    renderTab();
    const checkbox = await screen.findByLabelText('Auto-index New Files');
    await waitFor(() => expect(checkbox).not.toBeDisabled());

    await user.click(checkbox);

    await waitFor(() => {
      expect(mockSaveConfig).toHaveBeenCalledWith({ ...baseConfig, autoIndex: false });
    });
  });

  it('displays current batch size value from backend', async () => {
    renderTab();
    expect(await screen.findByText('Batch Size: 32')).toBeInTheDocument();
  });

  describe('Watch Folders', () => {
    it('displays watch folders list', async () => {
      renderTab();
      expect(await screen.findByText('/home/user/documents')).toBeInTheDocument();
      expect(screen.getByText('/home/user/downloads')).toBeInTheDocument();
    });

    it('shows empty state when no folders watched', async () => {
      mockGetConfig.mockResolvedValue({
        ok: true,
        data: { ...baseConfig, indexedPaths: [] },
      });
      renderTab();
      expect(await screen.findByText('No folders are being watched')).toBeInTheDocument();
    });

    it('adds folder via Tauri dialog', async () => {
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

    it('removes folder when remove button clicked', async () => {
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

    it('adds pattern via add button', async () => {
      const user = userEvent.setup();
      mockGetConfig.mockResolvedValue({
        ok: true,
        data: { ...baseConfig, excludePatterns: [] },
      });
      renderTab();

      const input = await screen.findByPlaceholderText('*.tmp, node_modules, .git');
      await user.type(input, '*.log');

      const addButton = screen.getByRole('button', { name: /^add$/i });
      await waitFor(() => expect(addButton).not.toBeDisabled());
      await user.click(addButton);

      await waitFor(() => {
        expect(mockSaveConfig).toHaveBeenCalledWith(
          expect.objectContaining({ excludePatterns: ['*.log'] })
        );
      });
    });

    it('does not add empty pattern (button disabled)', async () => {
      mockGetConfig.mockResolvedValue({
        ok: true,
        data: { ...baseConfig, excludePatterns: [] },
      });
      const user = userEvent.setup();
      renderTab();

      const input = await screen.findByPlaceholderText('*.tmp, node_modules, .git');
      await user.type(input, '   ');

      const addButton = screen.getByRole('button', { name: /^add$/i });
      expect(addButton).toBeDisabled();
    });

    it('removes pattern when remove button clicked', async () => {
      const user = userEvent.setup();
      renderTab();

      const removeButtons = await screen.findAllByTitle('Remove pattern');
      await user.click(removeButtons[0]);

      await waitFor(() => {
        expect(mockSaveConfig).toHaveBeenCalledWith(
          expect.objectContaining({ excludePatterns: ['node_modules', '*.tmp'] })
        );
      });
    });
  });
});
