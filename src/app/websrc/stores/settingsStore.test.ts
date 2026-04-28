import { act } from '@testing-library/react';
import { describe, it, expect, beforeEach } from 'vitest';

import { useSettingsStore, DEFAULT_SETTINGS } from './settingsStore';

describe('settingsStore', () => {
  beforeEach(() => {
    const store = useSettingsStore.getState();
    store.resetToDefaults();
  });

  describe('initial state', () => {
    it('starts with default settings', () => {
      const { settings } = useSettingsStore.getState();

      expect(settings).toEqual(DEFAULT_SETTINGS);
    });
  });

  describe('updateSearch', () => {
    it('updates search settings', () => {
      const { updateSearch } = useSettingsStore.getState();

      act(() => {
        updateSearch({ defaultMode: 'semantic' });
      });

      const newSettings = useSettingsStore.getState().settings;
      expect(newSettings.search.defaultMode).toBe('semantic');
    });

    it('updates multiple search properties', () => {
      const { updateSearch } = useSettingsStore.getState();

      act(() => {
        updateSearch({
          defaultMode: 'keyword',
          resultsPerPage: 50,
          enableReranking: false,
        });
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.search.defaultMode).toBe('keyword');
      expect(settings.search.resultsPerPage).toBe(50);
      expect(settings.search.enableReranking).toBe(false);
    });

    it('rejects invalid search settings', () => {
      const { updateSearch, settings: initialSettings } = useSettingsStore.getState();

      act(() => {
        updateSearch({ resultsPerPage: 1000 } as any);
      });

      const { settings } = useSettingsStore.getState();
      expect(settings).toEqual(initialSettings);
    });
  });

  describe('updateIndexing', () => {
    it('updates indexing settings', () => {
      const { updateIndexing } = useSettingsStore.getState();

      act(() => {
        updateIndexing({ autoIndex: false, batchSize: 64 });
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.indexing.autoIndex).toBe(false);
      expect(settings.indexing.batchSize).toBe(64);
    });

    it('rejects invalid batch size', () => {
      const { updateIndexing, settings: initialSettings } = useSettingsStore.getState();

      act(() => {
        updateIndexing({ batchSize: 200 } as any);
      });

      const { settings } = useSettingsStore.getState();
      expect(settings).toEqual(initialSettings);
    });
  });

  describe('updateDisplay', () => {
    it('updates display settings', () => {
      const { updateDisplay } = useSettingsStore.getState();

      act(() => {
        updateDisplay({
          theme: 'dark',
          fontSize: 'large',
          compactMode: true,
        });
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.display.theme).toBe('dark');
      expect(settings.display.fontSize).toBe('large');
      expect(settings.display.compactMode).toBe(true);
    });
  });

  describe('updatePrivacy', () => {
    it('updates privacy settings', () => {
      const { updatePrivacy } = useSettingsStore.getState();

      act(() => {
        updatePrivacy({
          telemetryEnabled: true,
          crashReporting: true,
        });
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.privacy.telemetryEnabled).toBe(true);
      expect(settings.privacy.crashReporting).toBe(true);
    });
  });

  describe('watch folder management', () => {
    it('adds watch folder', () => {
      const { addWatchFolder } = useSettingsStore.getState();

      act(() => {
        addWatchFolder('/path/to/folder');
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.indexing.watchFolders).toContain('/path/to/folder');
    });

    it('does not add duplicate folders', () => {
      const { addWatchFolder } = useSettingsStore.getState();

      act(() => {
        addWatchFolder('/path/to/folder');
        addWatchFolder('/path/to/folder');
      });

      const { settings } = useSettingsStore.getState();
      const count = settings.indexing.watchFolders.filter(
        (f) => f === '/path/to/folder'
      ).length;
      expect(count).toBe(1);
    });

    it('removes watch folder', () => {
      const { addWatchFolder, removeWatchFolder } = useSettingsStore.getState();

      act(() => {
        addWatchFolder('/path/to/folder');
        removeWatchFolder('/path/to/folder');
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.indexing.watchFolders).not.toContain('/path/to/folder');
    });

    it('removes only specified folder', () => {
      const { addWatchFolder, removeWatchFolder } = useSettingsStore.getState();

      act(() => {
        addWatchFolder('/path/to/folder1');
        addWatchFolder('/path/to/folder2');
        removeWatchFolder('/path/to/folder1');
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.indexing.watchFolders).not.toContain('/path/to/folder1');
      expect(settings.indexing.watchFolders).toContain('/path/to/folder2');
    });
  });

  describe('exclude pattern management', () => {
    it('adds exclude pattern', () => {
      const { addExcludePattern } = useSettingsStore.getState();

      act(() => {
        addExcludePattern('*.log');
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.indexing.excludePatterns).toContain('*.log');
    });

    it('does not add duplicate patterns', () => {
      const { addExcludePattern } = useSettingsStore.getState();

      act(() => {
        addExcludePattern('*.tmp');
        addExcludePattern('*.tmp');
      });

      const { settings } = useSettingsStore.getState();
      const count = settings.indexing.excludePatterns.filter((p) => p === '*.tmp').length;
      expect(count).toBe(1);
    });

    it('removes exclude pattern', () => {
      const { addExcludePattern, removeExcludePattern } = useSettingsStore.getState();

      act(() => {
        addExcludePattern('*.bak');
        removeExcludePattern('*.bak');
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.indexing.excludePatterns).not.toContain('*.bak');
    });
  });

  describe('resetToDefaults', () => {
    it('resets all settings to defaults', () => {
      const { updateSearch, updateDisplay, resetToDefaults } = useSettingsStore.getState();

      act(() => {
        updateSearch({ defaultMode: 'semantic' });
        updateDisplay({ theme: 'dark' });
        resetToDefaults();
      });

      const { settings } = useSettingsStore.getState();
      expect(settings).toEqual(DEFAULT_SETTINGS);
    });
  });

  describe('resetCategory', () => {
    it('resets search category', () => {
      const { updateSearch, resetCategory } = useSettingsStore.getState();

      act(() => {
        updateSearch({ defaultMode: 'semantic', resultsPerPage: 50 });
        resetCategory('search');
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.search).toEqual(DEFAULT_SETTINGS.search);
    });

    it('does not affect other categories', () => {
      const { updateSearch, updateDisplay, resetCategory } = useSettingsStore.getState();

      act(() => {
        updateSearch({ defaultMode: 'semantic' });
        updateDisplay({ theme: 'dark' });
        resetCategory('search');
      });

      const { settings } = useSettingsStore.getState();
      expect(settings.search).toEqual(DEFAULT_SETTINGS.search);
      expect(settings.display.theme).toBe('dark');
    });
  });

  describe('exportSettings', () => {
    it('exports settings as JSON string', () => {
      const { exportSettings } = useSettingsStore.getState();

      const json = exportSettings();
      const parsed = JSON.parse(json);

      expect(parsed).toEqual(DEFAULT_SETTINGS);
    });

    it('exports modified settings', () => {
      const { updateSearch, exportSettings } = useSettingsStore.getState();

      act(() => {
        updateSearch({ defaultMode: 'semantic' });
      });

      const json = exportSettings();
      const parsed = JSON.parse(json);

      expect(parsed.search.defaultMode).toBe('semantic');
    });
  });

  describe('importSettings', () => {
    it('imports valid settings', () => {
      const { importSettings } = useSettingsStore.getState();

      const settingsToImport = {
        ...DEFAULT_SETTINGS,
        search: {
          ...DEFAULT_SETTINGS.search,
          defaultMode: 'keyword' as const,
        },
      };

      const success = importSettings(JSON.stringify(settingsToImport));

      expect(success).toBe(true);
      const { settings } = useSettingsStore.getState();
      expect(settings.search.defaultMode).toBe('keyword');
    });

    it('rejects invalid JSON', () => {
      const { importSettings, settings: initialSettings } = useSettingsStore.getState();

      const success = importSettings('invalid json');

      expect(success).toBe(false);
      const { settings } = useSettingsStore.getState();
      expect(settings).toEqual(initialSettings);
    });

    it('migrates settings without version', () => {
      const { importSettings } = useSettingsStore.getState();

      const success = importSettings(JSON.stringify({ someOldField: 'value' }));

      expect(success).toBe(true);
      const { settings } = useSettingsStore.getState();
      expect(settings).toEqual(DEFAULT_SETTINGS);
    });
  });

  describe('validateSettings', () => {
    it('validates correct settings', () => {
      const { validateSettings } = useSettingsStore.getState();

      const isValid = validateSettings(DEFAULT_SETTINGS);

      expect(isValid).toBe(true);
    });

    it('rejects invalid settings', () => {
      const { validateSettings } = useSettingsStore.getState();

      const invalidSettings = {
        ...DEFAULT_SETTINGS,
        search: {
          ...DEFAULT_SETTINGS.search,
          resultsPerPage: 1000,
        },
      };

      const isValid = validateSettings(invalidSettings);

      expect(isValid).toBe(false);
    });

    it('rejects settings with missing fields', () => {
      const { validateSettings } = useSettingsStore.getState();

      const incompleteSettings = {
        version: 1,
        search: DEFAULT_SETTINGS.search,
      };

      const isValid = validateSettings(incompleteSettings as any);

      expect(isValid).toBe(false);
    });
  });
});
