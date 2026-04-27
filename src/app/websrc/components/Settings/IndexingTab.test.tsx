import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';

import { IndexingTab } from './IndexingTab';
import { useSettingsStore } from '../../stores/settingsStore';

vi.mock('../../stores/settingsStore');

const { mockDialogOpen } = vi.hoisted(() => ({
  mockDialogOpen: vi.fn(),
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: mockDialogOpen,
}));

describe('IndexingTab', () => {
  const mockUpdateIndexing = vi.fn();
  const mockAddWatchFolder = vi.fn();
  const mockRemoveWatchFolder = vi.fn();
  const mockAddExcludePattern = vi.fn();
  const mockRemoveExcludePattern = vi.fn();

  const mockSettings = {
    autoIndex: true,
    watchFolders: ['/home/user/documents', '/home/user/downloads'],
    excludePatterns: ['*.git', 'node_modules', '*.tmp'],
    batchSize: 32,
  };

  beforeEach(() => {
    vi.clearAllMocks();
    (useSettingsStore as any).mockImplementation((selector: any) => {
      const state = {
        settings: { indexing: mockSettings },
        updateIndexing: mockUpdateIndexing,
        addWatchFolder: mockAddWatchFolder,
        removeWatchFolder: mockRemoveWatchFolder,
        addExcludePattern: mockAddExcludePattern,
        removeExcludePattern: mockRemoveExcludePattern,
      };
      return selector(state);
    });
  });

  it('renders indexing settings header', () => {
    render(<IndexingTab />);
    expect(screen.getByText('Indexing Settings')).toBeInTheDocument();
    expect(screen.getByText('Manage file watching and indexing behavior')).toBeInTheDocument();
  });

  it('displays auto-index checkbox with current state', () => {
    render(<IndexingTab />);
    const checkbox = screen.getByLabelText('Auto-index New Files') as HTMLInputElement;
    expect(checkbox).toBeChecked();
  });

  it('toggles auto-index when checkbox clicked', async () => {
    const user = userEvent.setup();
    render(<IndexingTab />);

    const checkbox = screen.getByLabelText('Auto-index New Files');
    await user.click(checkbox);

    expect(mockUpdateIndexing).toHaveBeenCalledWith({ autoIndex: false });
  });

  it('displays current batch size value', () => {
    render(<IndexingTab />);
    expect(screen.getByText('Batch Size: 32')).toBeInTheDocument();
  });

  it('updates batch size when slider changed', async () => {
    render(<IndexingTab />);

    const slider = screen.getByLabelText(/batch size/i) as HTMLInputElement;
    fireEvent.change(slider, { target: { value: '64' } });

    expect(mockUpdateIndexing).toHaveBeenCalledWith({ batchSize: 64 });
  });

  describe('Watch Folders', () => {
    it('displays watch folders list', () => {
      render(<IndexingTab />);

      expect(screen.getByText('/home/user/documents')).toBeInTheDocument();
      expect(screen.getByText('/home/user/downloads')).toBeInTheDocument();
    });

    it('shows empty state when no folders watched', () => {
      (useSettingsStore as any).mockImplementation((selector: any) => {
        const state = {
          settings: { indexing: { ...mockSettings, watchFolders: [] } },
          updateIndexing: mockUpdateIndexing,
          addWatchFolder: mockAddWatchFolder,
          removeWatchFolder: mockRemoveWatchFolder,
          addExcludePattern: mockAddExcludePattern,
          removeExcludePattern: mockRemoveExcludePattern,
        };
        return selector(state);
      });

      render(<IndexingTab />);

      expect(screen.getByText('No folders are being watched')).toBeInTheDocument();
      expect(screen.getByText('Click "Add Folder" to start indexing files')).toBeInTheDocument();
    });

    it('adds folder when add folder button clicked', async () => {
      const user = userEvent.setup();
      mockDialogOpen.mockResolvedValue('/home/user/projects');

      render(<IndexingTab />);

      const addButton = screen.getByRole('button', { name: /add folder/i });
      await user.click(addButton);

      await waitFor(() => {
        expect(mockDialogOpen).toHaveBeenCalledWith({
          directory: true,
          multiple: false,
          title: 'Select Folder to Watch',
        });
      });

      expect(mockAddWatchFolder).toHaveBeenCalledWith('/home/user/projects');
    });

    it('shows adding state during folder selection', async () => {
      const user = userEvent.setup();
      mockDialogOpen.mockImplementation(() => new Promise(() => {}));

      render(<IndexingTab />);

      const addButton = screen.getByRole('button', { name: /add folder/i });
      await user.click(addButton);

      expect(screen.getByText('Adding...')).toBeInTheDocument();
    });

    it('does not add folder when selection cancelled', async () => {
      const user = userEvent.setup();
      mockDialogOpen.mockResolvedValue(null);

      render(<IndexingTab />);

      const addButton = screen.getByRole('button', { name: /add folder/i });
      await user.click(addButton);

      await waitFor(() => {
        expect(mockAddWatchFolder).not.toHaveBeenCalled();
      });
    });

    it('removes folder when remove button clicked', async () => {
      const user = userEvent.setup();
      render(<IndexingTab />);

      const removeButtons = screen.getAllByTitle('Remove folder');
      await user.click(removeButtons[0]);

      expect(mockRemoveWatchFolder).toHaveBeenCalledWith('/home/user/documents');
    });
  });

  describe('Exclude Patterns', () => {
    it('displays exclude patterns list', () => {
      render(<IndexingTab />);

      expect(screen.getByText('*.git')).toBeInTheDocument();
      expect(screen.getByText('node_modules')).toBeInTheDocument();
      expect(screen.getByText('*.tmp')).toBeInTheDocument();
    });

    it('shows empty state when no patterns defined', () => {
      (useSettingsStore as any).mockImplementation((selector: any) => {
        const state = {
          settings: { indexing: { ...mockSettings, excludePatterns: [] } },
          updateIndexing: mockUpdateIndexing,
          addWatchFolder: mockAddWatchFolder,
          removeWatchFolder: mockRemoveWatchFolder,
          addExcludePattern: mockAddExcludePattern,
          removeExcludePattern: mockRemoveExcludePattern,
        };
        return selector(state);
      });

      render(<IndexingTab />);

      expect(screen.getByText('No exclusion patterns defined')).toBeInTheDocument();
    });

    it('adds pattern when add button clicked', async () => {
      (useSettingsStore as any).mockImplementation((selector: any) => {
        const state = {
          settings: { indexing: { ...mockSettings, excludePatterns: [] } },
          updateIndexing: mockUpdateIndexing,
          addWatchFolder: mockAddWatchFolder,
          removeWatchFolder: mockRemoveWatchFolder,
          addExcludePattern: mockAddExcludePattern,
          removeExcludePattern: mockRemoveExcludePattern,
        };
        return selector(state);
      });

      const user = userEvent.setup();
      render(<IndexingTab />);

      const input = screen.getByPlaceholderText('*.tmp, node_modules, .git');
      await user.type(input, '*.log');

      const addButton = screen.getByRole('button', { name: /^add$/i });
      await user.click(addButton);

      expect(mockAddExcludePattern).toHaveBeenCalledWith('*.log');
    });

    it('adds pattern when enter key pressed', async () => {
      (useSettingsStore as any).mockImplementation((selector: any) => {
        const state = {
          settings: { indexing: { ...mockSettings, excludePatterns: [] } },
          updateIndexing: mockUpdateIndexing,
          addWatchFolder: mockAddWatchFolder,
          removeWatchFolder: mockRemoveWatchFolder,
          addExcludePattern: mockAddExcludePattern,
          removeExcludePattern: mockRemoveExcludePattern,
        };
        return selector(state);
      });

      const user = userEvent.setup();
      render(<IndexingTab />);

      const input = screen.getByPlaceholderText('*.tmp, node_modules, .git');
      await user.type(input, '*.bak{Enter}');

      expect(mockAddExcludePattern).toHaveBeenCalledWith('*.bak');
    });

    it('trims whitespace when adding pattern', async () => {
      (useSettingsStore as any).mockImplementation((selector: any) => {
        const state = {
          settings: { indexing: { ...mockSettings, excludePatterns: [] } },
          updateIndexing: mockUpdateIndexing,
          addWatchFolder: mockAddWatchFolder,
          removeWatchFolder: mockRemoveWatchFolder,
          addExcludePattern: mockAddExcludePattern,
          removeExcludePattern: mockRemoveExcludePattern,
        };
        return selector(state);
      });

      const user = userEvent.setup();
      render(<IndexingTab />);

      const input = screen.getByPlaceholderText('*.tmp, node_modules, .git');
      await user.type(input, '  *.cache  ');

      const addButton = screen.getByRole('button', { name: /^add$/i });
      await user.click(addButton);

      expect(mockAddExcludePattern).toHaveBeenCalledWith('*.cache');
    });

    it('does not add empty pattern', async () => {
      (useSettingsStore as any).mockImplementation((selector: any) => {
        const state = {
          settings: { indexing: { ...mockSettings, excludePatterns: [] } },
          updateIndexing: mockUpdateIndexing,
          addWatchFolder: mockAddWatchFolder,
          removeWatchFolder: mockRemoveWatchFolder,
          addExcludePattern: mockAddExcludePattern,
          removeExcludePattern: mockRemoveExcludePattern,
        };
        return selector(state);
      });

      const user = userEvent.setup();
      render(<IndexingTab />);

      const input = screen.getByPlaceholderText('*.tmp, node_modules, .git');
      await user.type(input, '   ');

      const addButton = screen.getByRole('button', { name: /^add$/i });
      expect(addButton).toBeDisabled();
    });

    it('removes pattern when remove button clicked', async () => {
      const user = userEvent.setup();
      render(<IndexingTab />);

      const removeButtons = screen.getAllByTitle('Remove pattern');
      await user.click(removeButtons[0]);

      expect(mockRemoveExcludePattern).toHaveBeenCalledWith('*.git');
    });

    it('clears input after adding pattern', async () => {
      (useSettingsStore as any).mockImplementation((selector: any) => {
        const state = {
          settings: { indexing: { ...mockSettings, excludePatterns: [] } },
          updateIndexing: mockUpdateIndexing,
          addWatchFolder: mockAddWatchFolder,
          removeWatchFolder: mockRemoveWatchFolder,
          addExcludePattern: mockAddExcludePattern,
          removeExcludePattern: mockRemoveExcludePattern,
        };
        return selector(state);
      });

      const user = userEvent.setup();
      render(<IndexingTab />);

      const input = screen.getByPlaceholderText('*.tmp, node_modules, .git') as HTMLInputElement;
      await user.type(input, '*.log');

      const addButton = screen.getByRole('button', { name: /^add$/i });
      await user.click(addButton);

      expect(input.value).toBe('');
    });
  });
});
