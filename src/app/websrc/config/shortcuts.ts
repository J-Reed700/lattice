/**
 * Default Keyboard Shortcuts Configuration
 *
 * Purpose: Centralized configuration for all keyboard shortcuts in the application
 *
 * Note: 'Mod' is automatically translated to:
 * - ⌘ (Command) on macOS
 * - Ctrl on Windows/Linux
 */

import { type ShortcutsConfig } from '../types/shortcuts';

export const DEFAULT_SHORTCUTS: ShortcutsConfig = {
  global: {
    search: 'Mod+K',
    quickCapture: 'Mod+Shift+N',
    newNote: 'Mod+N',
    settings: 'Mod+,',
    commandPalette: 'Mod+P',
    toggleSidebar: 'Mod+B',
    toggleFavorite: 'Mod+D',
    openFavorites: 'Mod+Shift+F',
  },
  editor: {
    toggleEdit: 'Mod+E',
    save: 'Mod+S',
    bold: 'Mod+B',
    italic: 'Mod+I',
    link: 'Mod+K',
    codeBlock: 'Mod+Shift+C',
  },
  navigation: {
    nextNote: 'Alt+ArrowDown',
    prevNote: 'Alt+ArrowUp',
    closeNote: 'Escape',
    focusSearch: '/',
    goToFiles: 'Mod+1',
    goToSearch: 'Mod+2',
    goToDaily: 'Mod+3',
  },
  search: {
    selectResult1: '1',
    selectResult2: '2',
    selectResult3: '3',
    nextResult: 'ArrowDown',
    prevResult: 'ArrowUp',
    closeSearch: 'Escape',
  },
};

/**
 * Shortcut descriptions for UI display
 */
export const SHORTCUT_DESCRIPTIONS: Record<string, Record<string, string>> = {
  global: {
    search: 'Open global search',
    quickCapture: 'Quick capture note',
    newNote: 'Create new note',
    settings: 'Open settings',
    commandPalette: 'Open command palette',
    toggleSidebar: 'Toggle sidebar',
    toggleFavorite: 'Toggle favorite for current document',
    openFavorites: 'Open favorites panel',
  },
  editor: {
    toggleEdit: 'Toggle edit mode',
    save: 'Save current note',
    bold: 'Bold text',
    italic: 'Italic text',
    link: 'Insert link',
    codeBlock: 'Insert code block',
  },
  navigation: {
    nextNote: 'Go to next note',
    prevNote: 'Go to previous note',
    closeNote: 'Close current note',
    focusSearch: 'Focus search input',
    goToFiles: 'Go to files view',
    goToSearch: 'Go to search view',
    goToDaily: 'Go to journals view',
  },
  search: {
    selectResult1: 'Select first result',
    selectResult2: 'Select second result',
    selectResult3: 'Select third result',
    nextResult: 'Next search result',
    prevResult: 'Previous search result',
    closeSearch: 'Close search',
  },
};

/**
 * Category display names
 */
export const CATEGORY_NAMES: Record<string, string> = {
  global: 'Global',
  editor: 'Editor',
  navigation: 'Navigation',
  search: 'Search',
  file: 'File',
  view: 'View',
};

/**
 * Get platform-specific modifier key symbol
 */
export function getModifierSymbol(): string {
  const isMac = typeof navigator !== 'undefined' &&
    navigator.platform.toUpperCase().indexOf('MAC') >= 0;
  return isMac ? '⌘' : 'Ctrl';
}

/**
 * Get platform name
 */
export function getPlatform(): 'mac' | 'windows' | 'linux' {
  if (typeof navigator === 'undefined') return 'windows';

  const platform = navigator.platform.toUpperCase();
  if (platform.indexOf('MAC') >= 0) return 'mac';
  if (platform.indexOf('WIN') >= 0) return 'windows';
  return 'linux';
}

/**
 * Check if running on macOS
 */
export function isMacOS(): boolean {
  return getPlatform() === 'mac';
}

/**
 * Format shortcut for display
 * Converts "Mod+K" to "⌘K" on Mac or "Ctrl+K" on Windows/Linux
 */
export function formatShortcut(shortcut: string): string {
  const isMac = isMacOS();

  return shortcut
    .replace(/Mod/g, isMac ? '⌘' : 'Ctrl')
    .replace(/Shift/g, isMac ? '⇧' : 'Shift')
    .replace(/Alt/g, isMac ? '⌥' : 'Alt')
    .replace(/Meta/g, isMac ? '⌘' : 'Win')
    .replace(/\+/g, isMac ? '' : '+');
}

/**
 * Parse shortcut string into modifier keys and key
 */
export function parseShortcut(shortcut: string): {
  modifiers: Set<string>;
  key: string;
} {
  const parts = shortcut.split('+');
  const key = parts[parts.length - 1];
  const modifiers = new Set(parts.slice(0, -1).map(m => m.toLowerCase()));

  // Normalize 'Mod' to platform-specific key
  if (modifiers.has('mod')) {
    modifiers.delete('mod');
    modifiers.add(isMacOS() ? 'meta' : 'ctrl');
  }

  return { modifiers, key: key.toLowerCase() };
}

/**
 * Check if event matches shortcut
 */
export function matchesShortcut(event: KeyboardEvent, shortcut: string): boolean {
  const { modifiers, key } = parseShortcut(shortcut);

  const eventKey = event.key.toLowerCase();
  const eventModifiers = new Set<string>();

  if (event.ctrlKey) eventModifiers.add('ctrl');
  if (event.metaKey) eventModifiers.add('meta');
  if (event.shiftKey) eventModifiers.add('shift');
  if (event.altKey) eventModifiers.add('alt');

  // Check if key matches
  const keyMatches = eventKey === key ||
    event.code.toLowerCase() === key.toLowerCase() ||
    (key === 'escape' && eventKey === 'escape') ||
    (key === 'enter' && eventKey === 'enter');

  // Check if modifiers match exactly
  if (modifiers.size !== eventModifiers.size) return false;

  for (const mod of modifiers) {
    if (!eventModifiers.has(mod)) return false;
  }

  return keyMatches;
}
