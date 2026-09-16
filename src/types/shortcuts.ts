/**
 * Keyboard Shortcuts Type Definitions
 *
 * Purpose: Type definitions for the keyboard shortcuts system
 */

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
