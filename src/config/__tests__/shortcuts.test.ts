import { describe, expect, it } from 'vitest';

import { formatShortcut } from '../shortcuts';

describe('formatShortcut', () => {
  it('formats shortcuts idiomatically for macOS', () => {
    expect(formatShortcut('Mod+K', 'mac')).toBe('⌘K');
    expect(formatShortcut('⌘K', 'mac')).toBe('⌘K');
    expect(formatShortcut('⌘⇧N', 'mac')).toBe('⌘⇧N');
    expect(formatShortcut('Mod+Shift+N', 'mac')).toBe('⌘⇧N');
    expect(formatShortcut('⇧⌘N', 'mac')).toBe('⌘⇧N');
    expect(formatShortcut('Ctrl+Shift+N', 'mac')).toBe('⌘⇧N');
    expect(formatShortcut('Ctrl+N', 'mac')).toBe('⌘N');
    expect(formatShortcut('Mod+,', 'mac')).toBe('⌘,');
    expect(formatShortcut('⌘,', 'mac')).toBe('⌘,');
    expect(formatShortcut('Mod+\\', 'mac')).toBe('⌘\\');
    expect(formatShortcut('Ctrl+\\', 'mac')).toBe('⌘\\');
  });

  it('formats shortcuts idiomatically for Windows and Linux', () => {
    for (const platform of ['windows', 'linux'] as const) {
      expect(formatShortcut('Mod+K', platform)).toBe('Ctrl+K');
      expect(formatShortcut('⌘K', platform)).toBe('Ctrl+K');
      expect(formatShortcut('⌘⇧N', platform)).toBe('Ctrl+Shift+N');
      expect(formatShortcut('Mod+Shift+N', platform)).toBe('Ctrl+Shift+N');
      expect(formatShortcut('Ctrl+N', platform)).toBe('Ctrl+N');
      expect(formatShortcut('Ctrl+Shift+P', platform)).toBe('Ctrl+Shift+P');
      expect(formatShortcut('Mod+,', platform)).toBe('Ctrl+,');
      expect(formatShortcut('⌘,', platform)).toBe('Ctrl+,');
      expect(formatShortcut('Mod+\\', platform)).toBe('Ctrl+\\');
      expect(formatShortcut('⌘0', platform)).toBe('Ctrl+0');
    }
  });

  it('handles special and literal keys cleanly', () => {
    expect(formatShortcut('Mod++', 'mac')).toBe('⌘+');
    expect(formatShortcut('Mod++', 'windows')).toBe('Ctrl++');
    expect(formatShortcut('⌘↵', 'windows')).toBe('Ctrl+Enter');
    expect(formatShortcut('Mod+Enter', 'mac')).toBe('⌘Enter');
    expect(formatShortcut('Alt+Shift+f', 'mac')).toBe('⌥⇧F');
    expect(formatShortcut('Alt+Shift+f', 'windows')).toBe('Alt+Shift+F');
    expect(formatShortcut('', 'mac')).toBe('');
  });
});
