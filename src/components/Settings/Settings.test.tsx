import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { Settings } from './Settings';
import { toast } from '../../stores/toastStore';


const mockSave = vi.hoisted(() => vi.fn());
const mockOpen = vi.hoisted(() => vi.fn());
const mockWriteTextFile = vi.hoisted(() => vi.fn());
const mockReadTextFile = vi.hoisted(() => vi.fn());
const mockExportSettings = vi.hoisted(() => vi.fn());
const mockImportSettings = vi.hoisted(() => vi.fn());
const mockResetSettings = vi.hoisted(() => vi.fn());

vi.mock('../../stores/toastStore');

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: mockSave,
  open: mockOpen,
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeTextFile: mockWriteTextFile,
  readTextFile: mockReadTextFile,
}));

vi.mock('../../lib/api', () => ({
  __esModule: true,
  VaultAPI: {
    getSettings: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    getModelDownloadPath: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    getConfig: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    saveConfig: vi.fn().mockResolvedValue({ ok: true, data: undefined }),
    updateSettings: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    exportSettings: mockExportSettings,
    importSettings: mockImportSettings,
    resetSettings: mockResetSettings,
  },
  default: {
    getSettings: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    getModelDownloadPath: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    getConfig: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    saveConfig: vi.fn().mockResolvedValue({ ok: true, data: undefined }),
    updateSettings: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    exportSettings: mockExportSettings,
    importSettings: mockImportSettings,
    resetSettings: mockResetSettings,
  },
}));

function renderSettings() {
  // Each test gets a fresh client so cache state doesn't leak.
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <MemoryRouter>
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    </MemoryRouter>
  );
  return render(<Settings />, { wrapper });
}

describe('Settings', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockExportSettings.mockResolvedValue({ ok: true, data: '{"foo":"bar"}' });
    mockImportSettings.mockResolvedValue({ ok: true, data: {} });
    mockResetSettings.mockResolvedValue({ ok: true, data: {} });
  });

  it('renders the tab list, grouped, without icons', () => {
    renderSettings();

    expect(screen.getByRole('heading', { name: 'Settings' })).toBeInTheDocument();
    expect(screen.getByText('General')).toBeInTheDocument();
    expect(screen.getByText('AI')).toBeInTheDocument();

    for (const label of [
      'Search',
      'Indexing',
      'Vault',
      'Spaces',
      'Display',
      'Privacy',
      'Chat',
      'Models',
      'Downloaded',
      'Prompts',
      'Tuning',
      'Tools',
    ]) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }
  });

  it('switches between tabs when clicked', async () => {
    const user = userEvent.setup();
    renderSettings();

    const indexingTab = screen.getByRole('button', { name: 'Indexing' });
    await user.click(indexingTab);

    expect(screen.getByRole('heading', { level: 1, name: 'Indexing' })).toBeInTheDocument();
  });

  describe('Export functionality', () => {
    it('exports settings via backend command when export button clicked', async () => {
      const user = userEvent.setup();
      mockSave.mockResolvedValue('/path/to/settings.json');

      renderSettings();

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      await waitFor(() => {
        expect(mockSave).toHaveBeenCalled();
      });

      expect(mockExportSettings).toHaveBeenCalled();
      expect(mockWriteTextFile).toHaveBeenCalledWith('/path/to/settings.json', '{"foo":"bar"}');
      expect(toast.success).toHaveBeenCalledWith('Settings exported successfully', { duration: 3000 });
    });

    it('skips write when user cancels save dialog', async () => {
      const user = userEvent.setup();
      mockSave.mockResolvedValue(null);

      renderSettings();

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      await waitFor(() => expect(mockSave).toHaveBeenCalled());
      expect(mockExportSettings).not.toHaveBeenCalled();
      expect(mockWriteTextFile).not.toHaveBeenCalled();
    });
  });

  describe('Import functionality', () => {
    it('imports settings via backend command when import button clicked', async () => {
      const user = userEvent.setup();
      mockOpen.mockResolvedValue('/path/to/settings.json');
      mockReadTextFile.mockResolvedValue('{"foo":"bar"}');

      renderSettings();

      const importButton = screen.getByRole('button', { name: /^import$/i });
      await user.click(importButton);

      await waitFor(() => {
        expect(mockImportSettings).toHaveBeenCalledWith('{"foo":"bar"}', false);
      });

      expect(toast.success).toHaveBeenCalledWith('Settings imported successfully', { duration: 3000 });
    });

    it('reports invalid settings file when backend rejects', async () => {
      const user = userEvent.setup();
      mockOpen.mockResolvedValue('/path/to/invalid.json');
      mockReadTextFile.mockResolvedValue('not valid');
      mockImportSettings.mockResolvedValue({ ok: false, error: 'parse error' });

      renderSettings();

      const importButton = screen.getByRole('button', { name: /^import$/i });
      await user.click(importButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          'Invalid settings file',
          expect.objectContaining({ message: 'parse error', duration: 5000 })
        );
      });
    });
  });

  describe('Reset functionality', () => {
    it('shows confirmation dialog when reset button clicked', async () => {
      const user = userEvent.setup();
      renderSettings();

      const resetButton = screen.getByRole('button', { name: /reset all/i });
      await user.click(resetButton);

      expect(screen.getByText('Reset all settings?')).toBeInTheDocument();
    });

    it('calls backend reset and reports success when confirmed', async () => {
      const user = userEvent.setup();
      renderSettings();

      const resetButton = screen.getByRole('button', { name: /reset all/i });
      await user.click(resetButton);

      const confirmButton = screen.getByRole('button', { name: /^reset$/i });
      await user.click(confirmButton);

      await waitFor(() => {
        expect(mockResetSettings).toHaveBeenCalled();
      });
      expect(toast.success).toHaveBeenCalledWith('Settings reset to defaults', { duration: 3000 });
    });

    it('closes dialog when cancelled and does NOT touch backend', async () => {
      const user = userEvent.setup();
      renderSettings();

      const resetButton = screen.getByRole('button', { name: /reset all/i });
      await user.click(resetButton);

      const cancelButton = screen.getByRole('button', { name: /cancel/i });
      await user.click(cancelButton);

      expect(mockResetSettings).not.toHaveBeenCalled();
      expect(screen.queryByText('Reset all settings?')).not.toBeInTheDocument();
    });
  });

  it('does not announce autosave', () => {
    renderSettings();
    expect(screen.queryByText('Saved automatically')).not.toBeInTheDocument();
  });
});
