import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { toast } from '@/stores/toastStore';

import { Settings } from './Settings';


const mockSaveTextFile = vi.hoisted(() => vi.fn());
const mockOpenTextFile = vi.hoisted(() => vi.fn());
const mockExportSettings = vi.hoisted(() => vi.fn());
const mockImportSettings = vi.hoisted(() => vi.fn());
const mockResetSettings = vi.hoisted(() => vi.fn());

const JSON_FILTERS = [{ name: 'JSON', extensions: ['json'] }];

vi.mock('@/stores/toastStore');

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeTextFile: vi.fn(),
  readTextFile: vi.fn(),
}));

vi.mock('@/lib/api', () => ({
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
    saveTextFile: mockSaveTextFile,
    openTextFile: mockOpenTextFile,
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
    saveTextFile: mockSaveTextFile,
    openTextFile: mockOpenTextFile,
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
    mockSaveTextFile.mockReset();
    mockOpenTextFile.mockReset();
    mockExportSettings.mockResolvedValue({ ok: true, data: '{"foo":"bar"}' });
    mockImportSettings.mockResolvedValue({ ok: true, data: {} });
    mockResetSettings.mockResolvedValue({ ok: true, data: {} });
  });

  it('renders the grouped settings navigation', () => {
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
      'Sources',
      'Source trust',
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
    it('saves the exported settings through the backend save dialog', async () => {
      const user = userEvent.setup();
      mockSaveTextFile.mockResolvedValue({ ok: true, data: true });

      renderSettings();

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith('Settings exported successfully', { duration: 3000 });
      });

      expect(mockExportSettings).toHaveBeenCalled();
      expect(mockSaveTextFile).toHaveBeenCalledWith('lattice-settings.json', JSON_FILTERS, '{"foo":"bar"}');
    });

    it('says nothing when the user cancels the save dialog', async () => {
      const user = userEvent.setup();
      mockSaveTextFile.mockResolvedValue({ ok: true, data: false });

      renderSettings();

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      await waitFor(() => expect(mockSaveTextFile).toHaveBeenCalled());
      await waitFor(() => expect(screen.getByRole('button', { name: /^export$/i })).toBeEnabled());
      expect(toast.success).not.toHaveBeenCalled();
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('reports why the file could not be written', async () => {
      const user = userEvent.setup();
      mockSaveTextFile.mockResolvedValue({
        ok: false,
        error: 'I/O error',
        details: { code: 'FILE_SYSTEM_ERROR', message: 'I/O error', details: 'Access is denied.' },
      });

      renderSettings();

      await user.click(screen.getByRole('button', { name: /export/i }));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          "Couldn't export settings",
          expect.objectContaining({ message: expect.stringContaining('I/O error: Access is denied.'), duration: 5000 })
        );
      });
      expect(toast.success).not.toHaveBeenCalled();
    });

    it('does not open the save dialog when the settings cannot be exported', async () => {
      const user = userEvent.setup();
      mockExportSettings.mockResolvedValue({ ok: false, error: 'settings are locked' });

      renderSettings();

      await user.click(screen.getByRole('button', { name: /export/i }));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          "Couldn't export settings",
          expect.objectContaining({ message: expect.stringContaining('settings are locked') })
        );
      });
      expect(mockSaveTextFile).not.toHaveBeenCalled();
    });
  });

  describe('Import functionality', () => {
    it('imports the settings file chosen in the backend open dialog', async () => {
      const user = userEvent.setup();
      mockOpenTextFile.mockResolvedValue({ ok: true, data: '{"foo":"bar"}' });

      renderSettings();

      const importButton = screen.getByRole('button', { name: /^import$/i });
      await user.click(importButton);

      await waitFor(() => {
        expect(mockImportSettings).toHaveBeenCalledWith('{"foo":"bar"}', false);
      });

      expect(mockOpenTextFile).toHaveBeenCalledWith(JSON_FILTERS);
      expect(toast.success).toHaveBeenCalledWith('Settings imported successfully', { duration: 3000 });
    });

    it('reports invalid settings file when backend rejects', async () => {
      const user = userEvent.setup();
      mockOpenTextFile.mockResolvedValue({ ok: true, data: 'not valid' });
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

    it('imports nothing when the user cancels the open dialog', async () => {
      const user = userEvent.setup();
      mockOpenTextFile.mockResolvedValue({ ok: true, data: null });

      renderSettings();

      await user.click(screen.getByRole('button', { name: /^import$/i }));

      await waitFor(() => expect(mockOpenTextFile).toHaveBeenCalled());
      await waitFor(() => expect(screen.getByRole('button', { name: /^import$/i })).toBeEnabled());
      expect(mockImportSettings).not.toHaveBeenCalled();
      expect(toast.success).not.toHaveBeenCalled();
      expect(toast.error).not.toHaveBeenCalled();
    });

    it('reports why the file could not be read and imports nothing', async () => {
      const user = userEvent.setup();
      mockOpenTextFile.mockResolvedValue({
        ok: false,
        error: 'Failed to read file: /path/to/settings.json',
        details: { code: 'FILE_READ_ERROR', message: 'Failed to read file: /path/to/settings.json', details: 'The file is not valid UTF-8 text.' },
      });

      renderSettings();

      await user.click(screen.getByRole('button', { name: /^import$/i }));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          "Couldn't import settings",
          expect.objectContaining({
            message: expect.stringContaining('The file is not valid UTF-8 text.'),
            duration: 5000,
          })
        );
      });
      expect(mockImportSettings).not.toHaveBeenCalled();
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

    it('closes the reset confirmation on Escape without resetting', async () => {
      const user = userEvent.setup();
      renderSettings();

      await user.click(screen.getByRole('button', { name: /reset all/i }));
      expect(screen.getByRole('dialog', { name: 'Reset all settings?' })).toBeInTheDocument();
      await user.keyboard('{Escape}');

      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
      expect(mockResetSettings).not.toHaveBeenCalled();
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
