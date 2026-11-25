import type { ReactNode } from 'react';

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, useLocation, useNavigate } from 'react-router';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { toast } from '@/stores/toastStore';

import { Settings, tabGroups } from './Settings';
import {
  ALL_SETTINGS_TABS,
  getDomainForTab,
  resolveSettingsTab,
  searchSettings,
  SETTINGS_DOMAINS,
} from './settingsRegistry';

const mockSaveTextFile = vi.hoisted(() => vi.fn());
const mockOpenTextFile = vi.hoisted(() => vi.fn());
const mockExportSettings = vi.hoisted(() => vi.fn());
const mockImportSettings = vi.hoisted(() => vi.fn());
const mockResetSettings = vi.hoisted(() => vi.fn());

const JSON_FILTERS = [{ name: 'JSON', extensions: ['json'] }];

/** The error toasts besides the page's own chat-settings load, which fails at a time of its own here. */
const errorsBesidesTheSettingsLoad = () =>
  vi.mocked(toast.error).mock.calls.filter(([message]) => message !== 'Failed to load chat settings');

vi.mock('@/stores/toastStore');

vi.mock('@/lib/api', () => ({
  __esModule: true,
  VaultAPI: {
    getSettings: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    getModelDownloadPath: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    getConfig: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    saveConfig: vi.fn().mockResolvedValue({ ok: true, data: null }),
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
    saveConfig: vi.fn().mockResolvedValue({ ok: true, data: null }),
    updateSettings: vi.fn().mockResolvedValue({ ok: false, error: 'not mocked' }),
    exportSettings: mockExportSettings,
    importSettings: mockImportSettings,
    resetSettings: mockResetSettings,
    saveTextFile: mockSaveTextFile,
    openTextFile: mockOpenTextFile,
  },
}));

function renderSettings(initialEntries: string[] = ['/settings']) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <MemoryRouter initialEntries={initialEntries}>
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

  it('renders the 6 minimalist sidebar domains and search input', () => {
    renderSettings();

    expect(screen.getByRole('heading', { name: 'Settings' })).toBeInTheDocument();
    expect(screen.getByRole('searchbox', { name: 'Search settings' })).toBeInTheDocument();

    for (const label of [
      'General',
      'Models & AI',
      'Search & Indexing',
      'Storage & Sync',
      'Learning Studio',
      'System & Diagnostics',
    ]) {
      expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
    }

    // Lands on General -> Display with underline sub-tabs for General
    expect(screen.getByRole('heading', { level: 1, name: 'Display' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Display' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: 'Spaces' })).toBeInTheDocument();
  });

  it('switches between domains and underline sub-tabs when clicked', async () => {
    const user = userEvent.setup();
    renderSettings();

    await user.click(screen.getByRole('button', { name: 'Search & Indexing' }));
    expect(screen.getByRole('heading', { level: 1, name: 'Search' })).toBeInTheDocument();

    await user.click(screen.getByRole('tab', { name: 'Indexing' }));
    expect(screen.getByRole('heading', { level: 1, name: 'Indexing' })).toBeInTheDocument();
  });

  it('maps all 16 legacy ?tab= deep links to their exact domain and sub-tab', () => {
    expect(SETTINGS_DOMAINS).toHaveLength(6);
    expect(ALL_SETTINGS_TABS).toHaveLength(16);

    const legacyIds = [
      'search',
      'indexing',
      'vault',
      'spaces',
      'chat',
      'models',
      'downloaded-models',
      'prompts',
      'tuning',
      'tools',
      'source-trust',
      'sources',
      'display',
      'read-aloud',
      'privacy',
      'logs',
    ] as const;

    for (const legacyId of legacyIds) {
      const resolved = resolveSettingsTab(legacyId);
      expect(resolved).toBe(legacyId);
      const domain = getDomainForTab(resolved!);
      expect(domain.tabs.some((tab) => tab.id === legacyId)).toBe(true);
    }

    // Domain IDs also resolve cleanly to their first sub-tab
    expect(resolveSettingsTab('general')).toBe('display');
    expect(resolveSettingsTab('system-diagnostics')).toBe('tuning');
    expect(resolveSettingsTab('nonexistent')).toBeNull();
  });

  it('consumes ?tab= while keeping /settings clean so repeated /settings?tab=indexing deep-links re-activate Indexing', async () => {
    const user = userEvent.setup();
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });

    function Harness() {
      const navigate = useNavigate();
      const location = useLocation();
      return (
        <>
          <button type="button" onClick={() => navigate('/settings?tab=indexing')}>
            Jump to indexing
          </button>
          <output aria-label="settings-location">{location.pathname + location.search}</output>
          <Settings />
        </>
      );
    }

    render(
      <MemoryRouter initialEntries={['/settings?tab=indexing']}>
        <QueryClientProvider client={client}>
          <Harness />
        </QueryClientProvider>
      </MemoryRouter>,
    );

    expect(screen.getByRole('heading', { level: 1, name: 'Indexing' })).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole('status', { name: 'settings-location' })).toHaveTextContent('/settings'),
    );

    await user.click(screen.getByRole('button', { name: 'Storage & Sync' }));
    expect(screen.getByRole('heading', { level: 1, name: 'Vault' })).toBeInTheDocument();
    expect(screen.getByRole('status', { name: 'settings-location' })).toHaveTextContent('/settings');

    await user.click(screen.getByRole('button', { name: 'Jump to indexing' }));
    expect(await screen.findByRole('heading', { level: 1, name: 'Indexing' })).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole('status', { name: 'settings-location' })).toHaveTextContent('/settings'),
    );
  });

  describe('Instant settings search', () => {
    it('focuses search on "/" shortcut, filters domains and settings, and jumps on click', async () => {
      const user = userEvent.setup();
      renderSettings();

      const searchInput = screen.getByRole('searchbox', { name: 'Search settings' });
      expect(searchInput).not.toHaveFocus();

      await user.keyboard('/');
      expect(searchInput).toHaveFocus();

      await user.type(searchInput, 'reranker');

      expect(screen.getByRole('heading', { level: 1, name: 'Search results' })).toBeInTheDocument();
      // Only Search & Indexing remains in the filtered sidebar
      const nav = screen.getByRole('navigation', { name: 'Settings sections' });
      expect(within(nav).getByRole('button', { name: /Search & Indexing/ })).toBeInTheDocument();
      expect(within(nav).queryByRole('button', { name: /^General$/ })).not.toBeInTheDocument();

      // Clicking the matching setting jumps to Search & Indexing -> Search and clears search
      await user.click(screen.getByRole('button', { name: /Reranker/i }));
      expect(screen.getByRole('heading', { level: 1, name: 'Search' })).toBeInTheDocument();
      expect(searchInput).toHaveValue('');
    });

    it('jumps to the top match on Enter and clears search on Escape', async () => {
      const user = userEvent.setup();
      renderSettings();

      const searchInput = screen.getByRole('searchbox', { name: 'Search settings' });
      await user.type(searchInput, 'temperature');
      expect(screen.getByRole('heading', { level: 1, name: 'Search results' })).toBeInTheDocument();

      await user.keyboard('{Escape}');
      expect(searchInput).toHaveValue('');
      expect(screen.getByRole('heading', { level: 1, name: 'Display' })).toBeInTheDocument();

      await user.type(searchInput, 'telemetry{Enter}');
      expect(screen.getByRole('heading', { level: 1, name: 'Privacy' })).toBeInTheDocument();
    });

    it('shows a calm empty state when nothing matches', async () => {
      const user = userEvent.setup();
      renderSettings();

      const searchInput = screen.getByRole('searchbox', { name: 'Search settings' });
      await user.type(searchInput, 'xyznonexistent');

      expect(screen.getByText(/No settings match/)).toBeInTheDocument();
      expect(searchSettings('xyznonexistent')).toEqual([]);
    });
  });

  describe('Export functionality', () => {
    it('saves the exported settings through the backend save dialog', async () => {
      const user = userEvent.setup();
      mockSaveTextFile.mockResolvedValue({ ok: true, data: true });

      renderSettings();

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith('Settings exported successfully', {
          duration: 3000,
        });
      });

      expect(mockExportSettings).toHaveBeenCalled();
      expect(mockSaveTextFile).toHaveBeenCalledWith(
        'lattice-settings.json',
        JSON_FILTERS,
        '{"foo":"bar"}',
      );
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
      expect(errorsBesidesTheSettingsLoad()).toEqual([]);
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
          expect.objectContaining({
            message: expect.stringContaining('I/O error: Access is denied.'),
            duration: 5000,
          }),
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
          expect.objectContaining({ message: expect.stringContaining('settings are locked') }),
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
      expect(toast.success).toHaveBeenCalledWith('Settings imported successfully', {
        duration: 3000,
      });
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
          expect.objectContaining({ message: 'parse error', duration: 5000 }),
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
      expect(errorsBesidesTheSettingsLoad()).toEqual([]);
    });

    it('reports why the file could not be read and imports nothing', async () => {
      const user = userEvent.setup();
      mockOpenTextFile.mockResolvedValue({
        ok: false,
        error: 'Failed to read file: /path/to/settings.json',
        details: {
          code: 'FILE_READ_ERROR',
          message: 'Failed to read file: /path/to/settings.json',
          details: 'The file is not valid UTF-8 text.',
        },
      });

      renderSettings();

      await user.click(screen.getByRole('button', { name: /^import$/i }));

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith(
          "Couldn't import settings",
          expect.objectContaining({
            message: expect.stringContaining('The file is not valid UTF-8 text.'),
            duration: 5000,
          }),
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

  it('keeps the model role pickers on the tab labelled Downloaded, which the guidance sentences name', () => {
    const tabs = tabGroups.flatMap((group) => group.tabs);

    expect(tabs.find((tab) => tab.id === 'downloaded-models')?.label).toBe('Downloaded');
    expect(tabs.find((tab) => tab.id === 'models')?.label).toBe('Models');
  });

  it('does not announce autosave', () => {
    renderSettings();
    expect(screen.queryByText('Saved automatically')).not.toBeInTheDocument();
  });
});
