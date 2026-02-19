import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { Settings } from './Settings';
import { useSettingsStore } from '../../stores/settingsStore';
import { toast } from '../../stores/toastStore';

vi.mock('../../stores/settingsStore');
vi.mock('../../stores/toastStore');

const mockSave = vi.hoisted(() => vi.fn());
const mockOpen = vi.hoisted(() => vi.fn());
const mockWriteTextFile = vi.hoisted(() => vi.fn());
const mockReadTextFile = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: mockSave,
  open: mockOpen,
}));

vi.mock('@tauri-apps/plugin-fs', () => ({
  writeTextFile: mockWriteTextFile,
  readTextFile: mockReadTextFile,
}));

describe('Settings', () => {
  const mockSettings = {
    version: 1,
    search: {
      defaultMode: 'hybrid',
      resultsPerPage: 20,
      enableReranking: true,
      semanticWeight: 0.7,
      keywordWeight: 0.3,
    },
    indexing: {
      autoIndex: true,
      watchFolders: [],
      excludePatterns: [],
      batchSize: 32,
    },
    ai: {
      embeddingModel: 'bge-m3',
      ocrModel: 'qwen2.5-vl-2b',
      useQuantization: true,
      enableAgenticRAG: false,
    },
    display: {
      theme: 'system',
      fontSize: 'medium',
      compactMode: false,
      showPreviews: true,
    },
    privacy: {
      telemetryEnabled: true,
      crashReporting: true,
    },
  };

  const mockResetToDefaults = vi.fn();
  const mockExportSettings = vi.fn();
  const mockImportSettings = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    (useSettingsStore as any).mockImplementation((selector: any) => {
      const state = {
        settings: mockSettings,
        resetToDefaults: mockResetToDefaults,
        exportSettings: mockExportSettings,
        importSettings: mockImportSettings,
      };
      return selector(state);
    });
  });

  it('renders settings dialog with all tabs', () => {
    render(<Settings />);

    expect(screen.getByText('Settings')).toBeInTheDocument();
    expect(screen.getByText('Search')).toBeInTheDocument();
    expect(screen.getByText('Indexing')).toBeInTheDocument();
    expect(screen.getByText('AI Models')).toBeInTheDocument();
    expect(screen.getByText('Display')).toBeInTheDocument();
    expect(screen.getByText('Privacy')).toBeInTheDocument();
  });

  it('displays version number', () => {
    render(<Settings />);
    expect(screen.getByText('v1')).toBeInTheDocument();
  });

  it('switches between tabs when clicked', async () => {
    const user = userEvent.setup();
    render(<Settings />);

    const indexingTab = screen.getByRole('button', { name: /indexing/i });
    await user.click(indexingTab);

    expect(screen.getByText('Indexing Settings')).toBeInTheDocument();
  });

  it('shows active tab with highlighted style', async () => {
    const user = userEvent.setup();
    render(<Settings />);

    const searchTab = screen.getByRole('button', { name: /^search$/i });
    expect(searchTab).toHaveClass('bg-[var(--accent-primary)]');

    const indexingTab = screen.getByRole('button', { name: /indexing/i });
    await user.click(indexingTab);

    expect(indexingTab).toHaveClass('bg-[var(--accent-primary)]');
    expect(searchTab).not.toHaveClass('bg-[var(--accent-primary)]');
  });

  describe('Export functionality', () => {
    it('exports settings when export button clicked', async () => {
      const user = userEvent.setup();
      mockSave.mockResolvedValue('/path/to/settings.json');
      mockExportSettings.mockReturnValue('{"version":1}');

      render(<Settings />);

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      await waitFor(() => {
        expect(mockSave).toHaveBeenCalledWith({
          defaultPath: 'recall-settings.json',
          filters: [{ name: 'JSON', extensions: ['json'] }],
        });
      });

      expect(mockExportSettings).toHaveBeenCalled();
      expect(mockWriteTextFile).toHaveBeenCalledWith('/path/to/settings.json', '{"version":1}');
      expect(toast.success).toHaveBeenCalledWith('Settings exported successfully', { duration: 3000 });
    });

    it('shows exporting state during export', async () => {
      const user = userEvent.setup();
      mockSave.mockImplementation(() => new Promise(() => {}));

      render(<Settings />);

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      expect(screen.getByText('Exporting...')).toBeInTheDocument();
    });

    it('handles export cancellation', async () => {
      const user = userEvent.setup();
      mockSave.mockResolvedValue(null);

      render(<Settings />);

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      await waitFor(() => {
        expect(mockExportSettings).not.toHaveBeenCalled();
      });
    });

    it('handles export errors', async () => {
      const user = userEvent.setup();
      mockSave.mockRejectedValue(new Error('Permission denied'));

      render(<Settings />);

      const exportButton = screen.getByRole('button', { name: /export/i });
      await user.click(exportButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('Export failed', {
          message: 'Error: Permission denied',
          duration: 5000,
        });
      });
    });
  });

  describe('Import functionality', () => {
    it('imports settings when import button clicked', async () => {
      const user = userEvent.setup();
      mockOpen.mockResolvedValue('/path/to/settings.json');
      mockReadTextFile.mockResolvedValue('{"version":1}');
      mockImportSettings.mockReturnValue(true);

      render(<Settings />);

      const importButton = screen.getByRole('button', { name: /^import$/i });
      await user.click(importButton);

      await waitFor(() => {
        expect(mockOpen).toHaveBeenCalledWith({
          multiple: false,
          filters: [{ name: 'JSON', extensions: ['json'] }],
        });
      });

      expect(mockReadTextFile).toHaveBeenCalledWith('/path/to/settings.json');
      expect(mockImportSettings).toHaveBeenCalledWith('{"version":1}');
      expect(toast.success).toHaveBeenCalledWith('Settings imported successfully', { duration: 3000 });
    });

    it('shows importing state during import', async () => {
      const user = userEvent.setup();
      mockOpen.mockImplementation(() => new Promise(() => {}));

      render(<Settings />);

      const importButton = screen.getByRole('button', { name: /^import$/i });
      await user.click(importButton);

      expect(screen.getByText('Importing...')).toBeInTheDocument();
    });

    it('handles invalid settings file', async () => {
      const user = userEvent.setup();
      mockOpen.mockResolvedValue('/path/to/invalid.json');
      mockReadTextFile.mockResolvedValue('invalid json');
      mockImportSettings.mockReturnValue(false);

      render(<Settings />);

      const importButton = screen.getByRole('button', { name: /^import$/i });
      await user.click(importButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('Invalid settings file', { duration: 5000 });
      });
    });

    it('handles import cancellation', async () => {
      const user = userEvent.setup();
      mockOpen.mockResolvedValue(null);

      render(<Settings />);

      const importButton = screen.getByRole('button', { name: /^import$/i });
      await user.click(importButton);

      await waitFor(() => {
        expect(mockImportSettings).not.toHaveBeenCalled();
      });
    });

    it('handles import errors', async () => {
      const user = userEvent.setup();
      mockOpen.mockRejectedValue(new Error('File not found'));

      render(<Settings />);

      const importButton = screen.getByRole('button', { name: /^import$/i });
      await user.click(importButton);

      await waitFor(() => {
        expect(toast.error).toHaveBeenCalledWith('Import failed', {
          message: 'Error: File not found',
          duration: 5000,
        });
      });
    });
  });

  describe('Reset functionality', () => {
    it('shows confirmation dialog when reset button clicked', async () => {
      const user = userEvent.setup();
      render(<Settings />);

      const resetButton = screen.getByRole('button', { name: /reset all/i });
      await user.click(resetButton);

      expect(screen.getByText('Reset All Settings?')).toBeInTheDocument();
      expect(
        screen.getByText(/This will restore all settings to their default values/)
      ).toBeInTheDocument();
    });

    it('resets settings when confirmed', async () => {
      const user = userEvent.setup();
      render(<Settings />);

      const resetButton = screen.getByRole('button', { name: /reset all/i });
      await user.click(resetButton);

      const confirmButton = screen.getByRole('button', { name: /reset settings/i });
      await user.click(confirmButton);

      expect(mockResetToDefaults).toHaveBeenCalled();
      expect(toast.success).toHaveBeenCalledWith('Settings reset to defaults', { duration: 3000 });
      expect(screen.queryByText('Reset All Settings?')).not.toBeInTheDocument();
    });

    it('closes dialog when cancelled', async () => {
      const user = userEvent.setup();
      render(<Settings />);

      const resetButton = screen.getByRole('button', { name: /reset all/i });
      await user.click(resetButton);

      const cancelButton = screen.getByRole('button', { name: /cancel/i });
      await user.click(cancelButton);

      expect(mockResetToDefaults).not.toHaveBeenCalled();
      expect(screen.queryByText('Reset All Settings?')).not.toBeInTheDocument();
    });
  });

  it('shows auto-save indicator', () => {
    render(<Settings />);
    expect(screen.getByText('Settings saved automatically')).toBeInTheDocument();
  });

  it('renders all tab icons', () => {
    render(<Settings />);

    const icons = screen.getAllByRole('button').filter(button =>
      button.querySelector('svg')
    );

    expect(icons.length).toBeGreaterThan(0);
  });
});
