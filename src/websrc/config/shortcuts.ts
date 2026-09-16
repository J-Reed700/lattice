/**
 * Keyboard shortcut key-matching helpers.
 *
 * The bindings themselves live in `components/RootLayout.tsx`; the reference sheet
 * lives in `components/KeyboardShortcutsModal/KeyboardShortcutsModal.tsx`.
 *
 * Note: 'Mod' is automatically translated to:
 * - ⌘ (Command) on macOS
 * - Ctrl on Windows/Linux
 */

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

  const keyMatches = eventKey === key ||
    event.code.toLowerCase() === key.toLowerCase() ||
    (key === 'escape' && eventKey === 'escape') ||
    (key === 'enter' && eventKey === 'enter');

  if (modifiers.size !== eventModifiers.size) return false;

  for (const mod of modifiers) {
    if (!eventModifiers.has(mod)) return false;
  }

  return keyMatches;
}
