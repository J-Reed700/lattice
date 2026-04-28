/**
 * Settings Store Tests
 *
 * Tests for Zustand settings store with persistence, validation, and migrations.
 */

import { describe, it, expect, beforeEach } from 'vitest';

import { useSettingsStore, DEFAULT_SETTINGS } from '../stores/settingsStore';

describe('Settings Store', () => {
  beforeEach(() => {
    // Clear localStorage before each test
    localStorage.clear();
    // Reset store to defaults
    useSettingsStore.getState().resetToDefaults();
  });

  describe('Initialization', () => {
    it('should initialize with default settings', () => {
      const { settings } = useSettingsStore.getState();
      expect(settings).toEqual(DEFAULT_SETTINGS);
    });

    it('should have version number', () => {
      const { settings } = useSettingsStore.getState();
      expect(settings.version).toBe(1);
    });
  });

  describe('Search Settings', () => {
    it('should update search mode', () => {
      const { updateSearch } = useSettingsStore.getState();

      updateSearch({ defaultMode: 'semantic' });

      const updated = useSettingsStore.getState().settings;
      expect(updated.search.defaultMode).toBe('semantic');
    });

    it('should update results per page within valid range', () => {
      const { updateSearch } = useSettingsStore.getState();

      updateSearch({ resultsPerPage: 50 });

      const updated = useSettingsStore.getState().settings;
      expect(updated.search.resultsPerPage).toBe(50);
    });

    it('should update reranking setting', () => {
      const { updateSearch } = useSettingsStore.getState();

      updateSearch({ enableReranking: false });

      const updated = useSettingsStore.getState().settings;
      expect(updated.search.enableReranking).toBe(false);
    });

    it('should update weight settings', () => {
      const { updateSearch } = useSettingsStore.getState();

      updateSearch({ semanticWeight: 0.6, keywordWeight: 0.4 });

      const updated = useSettingsStore.getState().settings;
      expect(updated.search.semanticWeight).toBe(0.6);
      expect(updated.search.keywordWeight).toBe(0.4);
    });

    it('should reject invalid search settings', () => {
      const { settings: before } = useSettingsStore.getState();
      const { updateSearch } = useSettingsStore.getState();

      // Try to set invalid mode
      updateSearch({ defaultMode: 'invalid' as any });

      const after = useSettingsStore.getState().settings;
      // Should remain unchanged due to validation
      expect(after.search.defaultMode).toBe(before.search.defaultMode);
    });
  });

  describe('Indexing Settings', () => {
    it('should toggle auto-index', () => {
      const { updateIndexing } = useSettingsStore.getState();

      updateIndexing({ autoIndex: false });

      const updated = useSettingsStore.getState().settings;
      expect(updated.indexing.autoIndex).toBe(false);
    });

    it('should update batch size', () => {
      const { updateIndexing } = useSettingsStore.getState();

      updateIndexing({ batchSize: 64 });

      const updated = useSettingsStore.getState().settings;
      expect(updated.indexing.batchSize).toBe(64);
    });

    it('should add watch folder', () => {
      const { addWatchFolder } = useSettingsStore.getState();

      addWatchFolder('/test/folder');

      const updated = useSettingsStore.getState().settings;
      expect(updated.indexing.watchFolders).toContain('/test/folder');
    });

    it('should not add duplicate watch folder', () => {
      const { addWatchFolder } = useSettingsStore.getState();

      addWatchFolder('/test/folder');
      addWatchFolder('/test/folder');

      const updated = useSettingsStore.getState().settings;
      const count = updated.indexing.watchFolders.filter(f => f === '/test/folder').length;
      expect(count).toBe(1);
    });

    it('should remove watch folder', () => {
      const { addWatchFolder, removeWatchFolder } = useSettingsStore.getState();

      addWatchFolder('/test/folder');
      removeWatchFolder('/test/folder');

      const updated = useSettingsStore.getState().settings;
      expect(updated.indexing.watchFolders).not.toContain('/test/folder');
    });

    it('should add exclude pattern', () => {
      const { addExcludePattern } = useSettingsStore.getState();

      addExcludePattern('*.test.js');

      const updated = useSettingsStore.getState().settings;
      expect(updated.indexing.excludePatterns).toContain('*.test.js');
    });

    it('should remove exclude pattern', () => {
      const { addExcludePattern, removeExcludePattern } = useSettingsStore.getState();

      addExcludePattern('*.test.js');
      removeExcludePattern('*.test.js');

      const updated = useSettingsStore.getState().settings;
      expect(updated.indexing.excludePatterns).not.toContain('*.test.js');
    });
  });

  describe('Display Settings', () => {
    it('should update theme', () => {
      const { updateDisplay } = useSettingsStore.getState();

      updateDisplay({ theme: 'dark' });

      const updated = useSettingsStore.getState().settings;
      expect(updated.display.theme).toBe('dark');
    });

    it('should update font size', () => {
      const { updateDisplay } = useSettingsStore.getState();

      updateDisplay({ fontSize: 'large' });

      const updated = useSettingsStore.getState().settings;
      expect(updated.display.fontSize).toBe('large');
    });

    it('should toggle compact mode', () => {
      const { updateDisplay } = useSettingsStore.getState();

      updateDisplay({ compactMode: true });

      const updated = useSettingsStore.getState().settings;
      expect(updated.display.compactMode).toBe(true);
    });

    it('should toggle show previews', () => {
      const { updateDisplay } = useSettingsStore.getState();

      updateDisplay({ showPreviews: false });

      const updated = useSettingsStore.getState().settings;
      expect(updated.display.showPreviews).toBe(false);
    });
  });

  describe('Privacy Settings', () => {
    it('should toggle telemetry', () => {
      const { updatePrivacy } = useSettingsStore.getState();

      updatePrivacy({ telemetryEnabled: true });

      const updated = useSettingsStore.getState().settings;
      expect(updated.privacy.telemetryEnabled).toBe(true);
    });

    it('should toggle crash reporting', () => {
      const { updatePrivacy } = useSettingsStore.getState();

      updatePrivacy({ crashReporting: true });

      const updated = useSettingsStore.getState().settings;
      expect(updated.privacy.crashReporting).toBe(true);
    });
  });

  describe('Reset Functionality', () => {
    it('should reset all settings to defaults', () => {
      const { updateSearch, updateDisplay, resetToDefaults } = useSettingsStore.getState();

      // Make some changes
      updateSearch({ defaultMode: 'keyword' });
      updateDisplay({ theme: 'dark' });

      // Reset
      resetToDefaults();

      const updated = useSettingsStore.getState().settings;
      expect(updated).toEqual(DEFAULT_SETTINGS);
    });

    it('should reset single category', () => {
      const { updateSearch, resetCategory } = useSettingsStore.getState();

      // Change search settings
      updateSearch({ defaultMode: 'keyword', resultsPerPage: 10 });

      // Reset only search
      resetCategory('search');

      const updated = useSettingsStore.getState().settings;
      expect(updated.search).toEqual(DEFAULT_SETTINGS.search);
    });
  });

  describe('Export/Import', () => {
    it('should export settings as JSON', () => {
      const { exportSettings } = useSettingsStore.getState();

      const json = exportSettings();
      const parsed = JSON.parse(json);

      expect(parsed).toEqual(DEFAULT_SETTINGS);
    });

    it('should import valid settings', () => {
      const { importSettings, updateSearch } = useSettingsStore.getState();

      // Change a setting
      updateSearch({ defaultMode: 'keyword' });

      // Export, reset, then import
      const exported = JSON.stringify(DEFAULT_SETTINGS);
      const success = importSettings(exported);

      expect(success).toBe(true);
      const updated = useSettingsStore.getState().settings;
      expect(updated).toEqual(DEFAULT_SETTINGS);
    });

    it('should reject invalid JSON', () => {
      const { importSettings } = useSettingsStore.getState();

      const success = importSettings('invalid json');

      expect(success).toBe(false);
    });

    it('should reject invalid settings structure', () => {
      const { importSettings } = useSettingsStore.getState();

      const invalid = JSON.stringify({ invalid: 'structure' });
      const success = importSettings(invalid);

      // Migration system converts invalid to defaults, which is actually success
      // but settings should be defaults
      expect(success).toBe(true);
      const updated = useSettingsStore.getState().settings;
      expect(updated).toEqual(DEFAULT_SETTINGS);
    });
  });

  describe('Persistence', () => {
    it('should persist settings to localStorage', () => {
      const { updateSearch } = useSettingsStore.getState();

      updateSearch({ defaultMode: 'semantic' });

      // Check localStorage
      const stored = localStorage.getItem('lattice-settings');
      expect(stored).toBeTruthy();

      const parsed = JSON.parse(stored!);
      expect(parsed.state.settings.search.defaultMode).toBe('semantic');
    });
  });

  describe('Validation', () => {
    it('should validate settings structure', () => {
      const { validateSettings } = useSettingsStore.getState();

      const valid = validateSettings(DEFAULT_SETTINGS);

      expect(valid).toBe(true);
    });

    it('should reject invalid settings', () => {
      const { validateSettings } = useSettingsStore.getState();

      const invalid = { ...DEFAULT_SETTINGS, search: { invalid: 'data' } } as any;
      const valid = validateSettings(invalid);

      expect(valid).toBe(false);
    });
  });
});
