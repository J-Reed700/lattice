/**
 * useKeyboardShortcuts Hook
 *
 * Purpose: Register and manage keyboard shortcuts with conflict detection
 *
 * Features:
 * - Platform detection (⌘ for Mac, Ctrl for Windows/Linux)
 * - Modifier key combinations (Ctrl/⌘, Shift, Alt)
 * - Conflict detection and warnings
 * - Context-aware shortcuts (different shortcuts per view)
 * - Automatic cleanup on unmount
 * - Prevent default browser shortcuts
 * - Custom shortcuts storage and persistence
 *
 * Usage:
 * ```tsx
 * useKeyboardShortcuts([
 *   {
 *     id: 'save',
 *     keys: 'Mod+S',
 *     description: 'Save document',
 *     handler: handleSave,
 *     category: 'editor'
 *   }
 * ], { context: 'editor', preventDefault: true });
 * ```
 */

import { useEffect, useRef, useCallback, useState } from 'react';

import { matchesShortcut} from '../config/shortcuts';
import {
  type KeyboardShortcutRegistration,
  type UseKeyboardShortcutsOptions,
  type ShortcutConflict,
} from '../types/shortcuts';
import { logger } from '../utils/logger';

// Global registry of all active shortcuts
const globalShortcutRegistry = new Map<string, KeyboardShortcutRegistration>();

/**
 * Load custom shortcuts from localStorage
 */
function loadCustomShortcuts(): Record<string, string> {
  try {
    const stored = localStorage.getItem('lattice-custom-shortcuts');
    return stored ? JSON.parse(stored) : {};
  } catch (error) {
    logger.error('Failed to load custom shortcuts:', { error });
    return {};
  }
}

/**
 * Save custom shortcuts to localStorage
 */
function saveCustomShortcuts(shortcuts: Record<string, string>): void {
  try {
    localStorage.setItem('lattice-custom-shortcuts', JSON.stringify(shortcuts));
  } catch (error) {
    logger.error('Failed to save custom shortcuts:', { error });
  }
}

/**
 * Detect conflicts between shortcuts
 */
function detectConflicts(
  newShortcut: KeyboardShortcutRegistration,
  registry: Map<string, KeyboardShortcutRegistration>
): ShortcutConflict[] {
  const conflicts: ShortcutConflict[] = [];

  for (const [id, existing] of registry) {
    if (id === newShortcut.id) continue;

    if (existing.keys === newShortcut.keys) {
      const existingContext = existing.options?.context;
      const newContext = newShortcut.options?.context;

      // Conflict if both are global or in the same context
      if (!existingContext || !newContext || existingContext === newContext) {
        conflicts.push({
          shortcut1: existing.id,
          shortcut2: newShortcut.id,
          keys: newShortcut.keys,
        });
      }
    }
  }

  return conflicts;
}

function areConflictsEqual(
  current: ShortcutConflict[],
  next: ShortcutConflict[]
): boolean {
  if (current.length !== next.length) return false;
  for (let i = 0; i < current.length; i += 1) {
    const currentConflict = current[i];
    const nextConflict = next[i];
    if (
      currentConflict.shortcut1 !== nextConflict.shortcut1 ||
      currentConflict.shortcut2 !== nextConflict.shortcut2 ||
      currentConflict.keys !== nextConflict.keys
    ) {
      return false;
    }
  }
  return true;
}

/**
 * Get the effective shortcut (custom or default)
 */
function getEffectiveShortcut(id: string, defaultKeys: string): string {
  const customShortcuts = loadCustomShortcuts();
  return customShortcuts[id] || defaultKeys;
}

/**
 * Main hook for registering keyboard shortcuts
 */
export function useKeyboardShortcuts(
  shortcuts: KeyboardShortcutRegistration[],
  options: UseKeyboardShortcutsOptions = {}
): {
  conflicts: ShortcutConflict[];
  updateShortcut: (id: string, newKeys: string) => void;
  resetShortcut: (id: string) => void;
  resetAllShortcuts: () => void;
} {
  const {
    context,
    enabled = true,
    preventDefault = true,
  } = options;

  const shortcutsRef = useRef(shortcuts);
  const [conflicts, setConflicts] = useState<ShortcutConflict[]>([]);

  useEffect(() => {
    shortcutsRef.current = shortcuts;
  }, [shortcuts]);

  // Register shortcuts in global registry
  useEffect(() => {
    if (!enabled) return;

    const registeredIds: string[] = [];
    const newConflicts: ShortcutConflict[] = [];

    // Register each shortcut
    for (const shortcut of shortcuts) {
      const effectiveKeys = getEffectiveShortcut(shortcut.id, shortcut.keys);
      const registration: KeyboardShortcutRegistration = {
        ...shortcut,
        keys: effectiveKeys,
        options: { ...shortcut.options, context },
      };

      const conflicts = detectConflicts(registration, globalShortcutRegistry);
      if (conflicts.length > 0) {
        newConflicts.push(...conflicts);
        logger.warn(`Shortcut conflict detected for "${shortcut.id}":`, { conflicts });
      }

      // Register the shortcut
      globalShortcutRegistry.set(shortcut.id, registration);
      registeredIds.push(shortcut.id);
    }

    setConflicts((current) =>
      areConflictsEqual(current, newConflicts) ? current : newConflicts
    );

    return () => {
      for (const id of registeredIds) {
        globalShortcutRegistry.delete(id);
      }
    };
  }, [shortcuts, enabled, context]);

  useEffect(() => {
    if (!enabled) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      // Don't trigger shortcuts when typing in inputs (unless it's a global shortcut)
      // Runtime type guard instead of type cast
      const target = event.target;
      if (!(target instanceof HTMLElement)) return;
      const isInput = target.tagName === 'INPUT' ||
        target.tagName === 'TEXTAREA' ||
        target.isContentEditable;

      for (const shortcut of shortcutsRef.current) {
        const effectiveKeys = getEffectiveShortcut(shortcut.id, shortcut.keys);

        if (matchesShortcut(event, effectiveKeys)) {
          // Skip if typing in input and not a global shortcut
          if (isInput && !shortcut.global) {
            continue;
          }

          const shortcutContext = shortcut.options?.context || shortcut.contexts?.[0];
          if (shortcutContext && context && shortcutContext !== context) {
            continue;
          }

          // Prevent default if requested
          if (preventDefault || shortcut.options?.preventDefault !== false) {
            event.preventDefault();
            event.stopPropagation();
          }

          try {
            shortcut.handler(event);
            logger.info(`Executed: ${shortcut.id}`, {
              component: 'shortcuts',
              keys: effectiveKeys,
              context,
            });
          } catch (error) {
            logger.error(`Error executing shortcut "${shortcut.id}":`, { error });
          }

          break; // Only execute first matching shortcut
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [enabled, context, preventDefault]);

  const updateShortcut = useCallback((id: string, newKeys: string) => {
    const customShortcuts = loadCustomShortcuts();
    customShortcuts[id] = newKeys;
    saveCustomShortcuts(customShortcuts);

    const existing = globalShortcutRegistry.get(id);
    if (existing) {
      globalShortcutRegistry.set(id, { ...existing, keys: newKeys });
    }

    logger.info(`Updated shortcut: ${id}`, { component: 'shortcuts', newKeys });
  }, []);

  const resetShortcut = useCallback((id: string) => {
    const customShortcuts = loadCustomShortcuts();
    delete customShortcuts[id];
    saveCustomShortcuts(customShortcuts);

    const existing = globalShortcutRegistry.get(id);
    if (existing) {
      const defaultShortcut = shortcuts.find(s => s.id === id);
      if (defaultShortcut) {
        globalShortcutRegistry.set(id, { ...existing, keys: defaultShortcut.keys });
      }
    }

    logger.info(`Reset shortcut: ${id}`, { component: 'shortcuts' });
  }, [shortcuts]);

  const resetAllShortcuts = useCallback(() => {
    localStorage.removeItem('lattice-custom-shortcuts');

    for (const shortcut of shortcuts) {
      const existing = globalShortcutRegistry.get(shortcut.id);
      if (existing) {
        globalShortcutRegistry.set(shortcut.id, { ...existing, keys: shortcut.keys });
      }
    }

    logger.info('Reset all shortcuts to defaults', { component: 'shortcuts' });
  }, [shortcuts]);

  return {
    conflicts,
    updateShortcut,
    resetShortcut,
    resetAllShortcuts,
  };
}

/**
 * Hook to get all registered shortcuts (for display in UI)
 */
export function useRegisteredShortcuts(): KeyboardShortcutRegistration[] {
  const shortcuts = Array.from(globalShortcutRegistry.values());
  return shortcuts;
}

/**
 * Hook to check if a shortcut key combination is available
 */
export function useIsShortcutAvailable(keys: string, excludeId?: string): boolean {
  for (const [id, shortcut] of globalShortcutRegistry) {
    if (id === excludeId) continue;
    if (shortcut.keys === keys) return false;
  }
  return true;
}
