import { act } from '@testing-library/react';
import { describe, it, expect, beforeEach } from 'vitest';

import { useSettingsStore, DEFAULT_DISPLAY_SETTINGS } from './settingsStore';

describe('settingsStore (UI prefs only — Phase 4b)', () => {
  beforeEach(() => {
    useSettingsStore.getState().resetDisplay();
  });

  describe('initial state', () => {
    it('starts with default display settings', () => {
      expect(useSettingsStore.getState().display).toEqual(DEFAULT_DISPLAY_SETTINGS);
    });
  });

  describe('updateDisplay', () => {
    it('updates a single display field', () => {
      act(() => {
        useSettingsStore.getState().updateDisplay({ theme: 'dark' });
      });
      expect(useSettingsStore.getState().display.theme).toBe('dark');
    });

    it('merges multiple updates', () => {
      act(() => {
        useSettingsStore.getState().updateDisplay({
          theme: 'light',
          fontSize: 'large',
          compactMode: true,
        });
      });
      const { display } = useSettingsStore.getState();
      expect(display.theme).toBe('light');
      expect(display.fontSize).toBe('large');
      expect(display.compactMode).toBe(true);
      expect(display.showPreviews).toBe(DEFAULT_DISPLAY_SETTINGS.showPreviews);
    });

    it('rejects invalid theme values via Zod', () => {
      const before = useSettingsStore.getState().display;
      act(() => {
        useSettingsStore.getState().updateDisplay({ theme: 'neon' as never });
      });
      expect(useSettingsStore.getState().display).toEqual(before);
    });

    it('rejects invalid fontSize values via Zod', () => {
      const before = useSettingsStore.getState().display;
      act(() => {
        useSettingsStore.getState().updateDisplay({ fontSize: 'gigantic' as never });
      });
      expect(useSettingsStore.getState().display).toEqual(before);
    });
  });

  describe('resetDisplay', () => {
    it('returns the display slice to defaults', () => {
      act(() => {
        useSettingsStore.getState().updateDisplay({ theme: 'dark', compactMode: true });
        useSettingsStore.getState().resetDisplay();
      });
      expect(useSettingsStore.getState().display).toEqual(DEFAULT_DISPLAY_SETTINGS);
    });
  });
});
