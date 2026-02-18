/**
 * Keyboard Shortcuts Type Definitions
 *
 * Purpose: Type definitions for the keyboard shortcuts system
 */

export type ModifierKey = 'Mod' | 'Ctrl' | 'Shift' | 'Alt' | 'Meta';
export type KeyCombination = string; // e.g., "Mod+K", "Shift+Alt+N"

export interface KeyboardShortcut {
  id: string;
  keys: KeyCombination;
  description: string;
  category: ShortcutCategory;
  handler: (event?: KeyboardEvent) => void;
  enabled?: boolean;
  global?: boolean; // Works anywhere in the app
  contexts?: ShortcutContext[]; // Specific contexts where this shortcut works
}

export type ShortcutCategory =
  | 'global'
  | 'editor'
  | 'navigation'
  | 'search'
  | 'file'
  | 'view';

export type ShortcutContext =
  | 'search'
  | 'editor'
  | 'files'
  | 'settings'
  | 'daily'
  | 'qa';

export interface ShortcutsConfig {
  global: {
    search: KeyCombination;
    quickCapture: KeyCombination;
    newNote: KeyCombination;
    settings: KeyCombination;
    commandPalette: KeyCombination;
    toggleSidebar: KeyCombination;
    toggleFavorite?: KeyCombination;
    openFavorites?: KeyCombination;
  };
  editor: {
    toggleEdit: KeyCombination;
    save: KeyCombination;
    bold: KeyCombination;
    italic: KeyCombination;
    link: KeyCombination;
    codeBlock: KeyCombination;
  };
  navigation: {
    nextNote: KeyCombination;
    prevNote: KeyCombination;
    closeNote: KeyCombination;
    focusSearch: KeyCombination;
    goToFiles: KeyCombination;
    goToSearch: KeyCombination;
    goToDaily: KeyCombination;
  };
  search: {
    selectResult1: KeyCombination;
    selectResult2: KeyCombination;
    selectResult3: KeyCombination;
    nextResult: KeyCombination;
    prevResult: KeyCombination;
    closeSearch: KeyCombination;
  };
}

export interface ShortcutConflict {
  shortcut1: string;
  shortcut2: string;
  keys: KeyCombination;
}

export interface UseKeyboardShortcutsOptions {
  context?: ShortcutContext;
  enabled?: boolean;
  preventDefault?: boolean;
}

export interface KeyboardShortcutRegistration {
  id: string;
  keys: KeyCombination;
  handler: (event?: KeyboardEvent) => void;
  description: string;
  category?: ShortcutCategory;
  global?: boolean; // Works anywhere in the app
  contexts?: ShortcutContext[]; // Specific contexts where this shortcut works
  options?: UseKeyboardShortcutsOptions;
}
