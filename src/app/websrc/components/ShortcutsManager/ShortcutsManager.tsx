/**
 * ShortcutsManager Component
 *
 * Purpose: UI to view and customize all keyboard shortcuts
 *
 * Features:
 * - View all shortcuts categorized by function
 * - Customize shortcuts with key capture
 * - Validate conflicts (warn if shortcut already used)
 * - Reset individual shortcuts or all to defaults
 * - Search/filter shortcuts
 * - Export/import custom shortcuts
 * - Platform-aware display (⌘ for Mac, Ctrl for Windows/Linux)
 *
 * States: default, editing, capturing, conflict warning
 * Accessibility: Keyboard navigation, ARIA labels, screen reader support
 */

import { useState, useCallback, useRef, useEffect } from 'react';

import {
  Search,
  Download,
  Upload,
  RotateCcw,
  X,
  AlertTriangle,
  Keyboard,
} from 'lucide-react';

import {
  DEFAULT_SHORTCUTS,
  SHORTCUT_DESCRIPTIONS,
  CATEGORY_NAMES,
  formatShortcut,
  isMacOS,
} from '../../config/shortcuts';
import { type KeyCombination } from '../../types/shortcuts';
import { logger } from '../../utils/logger';

interface ShortcutEntry {
  id: string;
  category: string;
  keys: KeyCombination;
  description: string;
  isCustom: boolean;
}

interface ShortcutsManagerProps {
  onClose?: () => void;
}

/**
 * Load custom shortcuts from localStorage
 */
function loadCustomShortcuts(): Record<string, string> {
  try {
    const stored = localStorage.getItem('vault-custom-shortcuts');
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
    localStorage.setItem('vault-custom-shortcuts', JSON.stringify(shortcuts));
    logger.info('Saved custom shortcuts', { component: 'shortcuts', count: Object.keys(shortcuts).length });
  } catch (error) {
    logger.error('Failed to save custom shortcuts:', { error });
  }
}

/**
 * Get all shortcuts with custom overrides applied
 */
function getAllShortcuts(): ShortcutEntry[] {
  const customShortcuts = loadCustomShortcuts();
  const shortcuts: ShortcutEntry[] = [];

  // Flatten all shortcuts from config
  for (const [category, categoryShortcuts] of Object.entries(DEFAULT_SHORTCUTS)) {
    for (const [id, defaultKeys] of Object.entries(categoryShortcuts)) {
      const fullId = `${category}.${id}`;
      const keys = customShortcuts[fullId] || defaultKeys;
      const description = SHORTCUT_DESCRIPTIONS[category]?.[id] || id;

      shortcuts.push({
        id: fullId,
        category,
        keys: keys as string,
        description,
        isCustom: fullId in customShortcuts,
      });
    }
  }

  return shortcuts;
}

export function ShortcutsManager({ onClose }: ShortcutsManagerProps) {
  const [shortcuts, setShortcuts] = useState<ShortcutEntry[]>(getAllShortcuts());
  const [searchQuery, setSearchQuery] = useState('');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [capturingKeys, setCapturingKeys] = useState<string[]>([]);
  const [conflictWarning, setConflictWarning] = useState<string | null>(null);
  const [selectedCategory, setSelectedCategory] = useState<string | 'all'>('all');
  const captureRef = useRef<HTMLDivElement>(null);

  // Refresh shortcuts when component mounts or after changes
  const refreshShortcuts = useCallback(() => {
    setShortcuts(getAllShortcuts());
  }, []);

  // Filter shortcuts based on search and category
  const filteredShortcuts = shortcuts.filter((shortcut) => {
    const matchesSearch = searchQuery
      ? shortcut.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
        shortcut.keys.toLowerCase().includes(searchQuery.toLowerCase()) ||
        shortcut.category.toLowerCase().includes(searchQuery.toLowerCase())
      : true;

    const matchesCategory = selectedCategory === 'all' || shortcut.category === selectedCategory;

    return matchesSearch && matchesCategory;
  });

  // Group shortcuts by category
  const groupedShortcuts = filteredShortcuts.reduce((acc, shortcut) => {
    if (!acc[shortcut.category]) {
      acc[shortcut.category] = [];
    }
    acc[shortcut.category].push(shortcut);
    return acc;
  }, {} as Record<string, ShortcutEntry[]>);

  // Start editing a shortcut
  const startEditing = useCallback((id: string) => {
    setEditingId(id);
    setCapturingKeys([]);
    setConflictWarning(null);
  }, []);

  // Cancel editing
  const cancelEditing = useCallback(() => {
    setEditingId(null);
    setCapturingKeys([]);
    setConflictWarning(null);
  }, []);

  // Check for conflicts
  const checkConflict = useCallback((keys: string, excludeId: string): string | null => {
    const conflicting = shortcuts.find(
      (s) => s.id !== excludeId && s.keys === keys
    );
    return conflicting ? conflicting.description : null;
  }, [shortcuts]);

  // Save the edited shortcut
  const saveShortcut = useCallback((id: string, newKeys: string) => {
    const conflict = checkConflict(newKeys, id);
    if (conflict) {
      setConflictWarning(`This shortcut is already used by "${conflict}"`);
      return;
    }

    const customShortcuts = loadCustomShortcuts();
    customShortcuts[id] = newKeys;
    saveCustomShortcuts(customShortcuts);

    refreshShortcuts();
    setEditingId(null);
    setCapturingKeys([]);
    setConflictWarning(null);

    logger.info(`Updated shortcut: ${id}`, { component: 'shortcuts', newKeys });
  }, [checkConflict, refreshShortcuts]);

  // Reset a single shortcut to default
  const resetShortcut = useCallback((id: string) => {
    const customShortcuts = loadCustomShortcuts();
    delete customShortcuts[id];
    saveCustomShortcuts(customShortcuts);

    refreshShortcuts();
    logger.info(`Reset shortcut: ${id}`, { component: 'shortcuts' });
  }, [refreshShortcuts]);

  // Reset all shortcuts to defaults
  const resetAllShortcuts = useCallback(() => {
    localStorage.removeItem('vault-custom-shortcuts');
    refreshShortcuts();
    logger.info('Reset all shortcuts to defaults', { component: 'shortcuts' });
  }, [refreshShortcuts]);

  // Export custom shortcuts
  const exportShortcuts = useCallback(() => {
    const customShortcuts = loadCustomShortcuts();
    const data = JSON.stringify(customShortcuts, null, 2);
    const blob = new Blob([data], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'vault-shortcuts.json';
    a.click();
    URL.revokeObjectURL(url);
    logger.info('Exported custom shortcuts', { component: 'shortcuts' });
  }, []);

  // Import custom shortcuts
  const importShortcuts = useCallback(() => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'application/json';
    input.onchange = (e) => {
      // Runtime type guard instead of type cast
      if (!(e.target instanceof HTMLInputElement)) return;
      const file = e.target.files?.[0];
      if (!file) return;

      const reader = new FileReader();
      reader.onload = (event) => {
        try {
          // Runtime type guard instead of type cast
          const result = event.target?.result;
          if (typeof result !== 'string') {
            logger.error('FileReader result is not a string', { component: 'shortcuts' });
            return;
          }
          const imported = JSON.parse(result);
          saveCustomShortcuts(imported);
          refreshShortcuts();
          logger.info('Imported custom shortcuts', { component: 'shortcuts' });
        } catch (error) {
          logger.error('Failed to import shortcuts:', { error });
          alert('Failed to import shortcuts. Please check the file format.');
        }
      };
      reader.readAsText(file);
    };
    input.click();
  }, [refreshShortcuts]);

  // Handle key capture
  useEffect(() => {
    if (editingId === null) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      const modifiers: string[] = [];
      if (event.ctrlKey || event.metaKey) modifiers.push(isMacOS() ? 'Meta' : 'Ctrl');
      if (event.shiftKey) modifiers.push('Shift');
      if (event.altKey) modifiers.push('Alt');

      // Get the key (excluding modifiers)
      let key = event.key;

      // Normalize special keys
      if (key === 'Control' || key === 'Meta' || key === 'Shift' || key === 'Alt') {
        setCapturingKeys(modifiers);
        return;
      }

      // Convert to proper case
      if (key.length === 1) {
        key = key.toUpperCase();
      }

      // Build shortcut string
      const shortcutParts = [...modifiers, key];
      const shortcut = shortcutParts.join('+').replace(/Meta/g, 'Mod').replace(/Ctrl/g, 'Mod');

      setCapturingKeys(shortcutParts);

      // Auto-save after a short delay
      setTimeout(() => {
        saveShortcut(editingId, shortcut);
      }, 500);
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [editingId, saveShortcut]);

  // Get unique categories
  const categories = ['all', ...Object.keys(DEFAULT_SHORTCUTS)];

  return (
    <div className="shortcuts-manager">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <Keyboard className="w-6 h-6 text-[hsl(var(--accent))]" />
          <h2 className="text-2xl font-semibold text-[hsl(var(--text-primary))]">
            Keyboard Shortcuts
          </h2>
        </div>
        {onClose && (
          <button
            onClick={onClose}
            className="p-2 rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors"
            aria-label="Close"
          >
            <X className="w-5 h-5 text-[hsl(var(--text-tertiary))]" />
          </button>
        )}
      </div>

      {/* Toolbar */}
      <div className="flex flex-col sm:flex-row gap-3 mb-6">
        {/* Search */}
        <div className="flex-1 relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[hsl(var(--text-tertiary))]" />
          <input
            type="text"
            placeholder="Search shortcuts..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-10 pr-4 py-2 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-lg text-sm focus:outline-none focus:ring-2 ring-[hsl(var(--accent))]"
          />
        </div>

        {/* Category Filter */}
        <select
          value={selectedCategory}
          onChange={(e) => setSelectedCategory(e.target.value)}
          className="px-4 py-2 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-lg text-sm focus:outline-none focus:ring-2 ring-[hsl(var(--accent))]"
        >
          {categories.map((cat) => (
            <option key={cat} value={cat}>
              {cat === 'all' ? 'All Categories' : CATEGORY_NAMES[cat] || cat}
            </option>
          ))}
        </select>

        {/* Actions */}
        <div className="flex gap-2">
          <button
            onClick={importShortcuts}
            className="px-3 py-2 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-lg text-sm hover:bg-[hsl(var(--surface))] transition-colors flex items-center gap-2"
            title="Import shortcuts"
          >
            <Upload className="w-4 h-4" />
          </button>
          <button
            onClick={exportShortcuts}
            className="px-3 py-2 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-lg text-sm hover:bg-[hsl(var(--surface))] transition-colors flex items-center gap-2"
            title="Export shortcuts"
          >
            <Download className="w-4 h-4" />
          </button>
          <button
            onClick={resetAllShortcuts}
            className="px-3 py-2 bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-lg text-sm hover:bg-[hsl(var(--surface))] transition-colors flex items-center gap-2"
            title="Reset all to defaults"
          >
            <RotateCcw className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Shortcuts List */}
      <div className="space-y-6 max-h-[600px] overflow-y-auto pr-2">
        {Object.entries(groupedShortcuts).map(([category, categoryShortcuts]) => (
          <div key={category} className="bg-[hsl(var(--surface-raised))] rounded-lg border border-[hsl(var(--border-subtle))] overflow-hidden">
            {/* Category Header */}
            <div className="px-4 py-3 bg-[hsl(var(--surface))]/50 border-b border-[hsl(var(--border-subtle))]">
              <h3 className="text-sm font-semibold text-[hsl(var(--text-secondary))] uppercase tracking-wide">
                {CATEGORY_NAMES[category] || category}
              </h3>
            </div>

            {/* Shortcuts */}
            <div className="divide-y divide-gray-200">
              {categoryShortcuts.map((shortcut) => (
                <div
                  key={shortcut.id}
                  className="px-4 py-3 hover:bg-[hsl(var(--surface-raised))]/50 transition-colors"
                >
                  <div className="flex items-center justify-between gap-4">
                    {/* Description */}
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium text-[hsl(var(--text-primary))]">
                        {shortcut.description}
                      </p>
                      {shortcut.isCustom && (
                        <p className="text-xs text-[hsl(var(--accent))] mt-0.5">
                          Custom
                        </p>
                      )}
                    </div>

                    {/* Shortcut Keys */}
                    <div className="flex items-center gap-2">
                      {editingId === shortcut.id ? (
                        <div ref={captureRef} className="flex items-center gap-2">
                          <div className="px-3 py-2 bg-[hsl(var(--accent-muted))]/30 border-2 border-[hsl(var(--accent))] rounded-lg min-w-[120px]">
                            <p className="text-xs text-[hsl(var(--accent))] font-mono text-center">
                              {capturingKeys.length > 0
                                ? capturingKeys.join('+')
                                : 'Press keys...'}
                            </p>
                          </div>
                          <button
                            onClick={cancelEditing}
                            className="p-1.5 rounded hover:bg-[hsl(var(--surface-raised))] transition-colors"
                            title="Cancel"
                          >
                            <X className="w-4 h-4 text-[hsl(var(--text-tertiary))]" />
                          </button>
                        </div>
                      ) : (
                        <>
                          <button
                            onClick={() => startEditing(shortcut.id)}
                            className="px-3 py-1.5 bg-[hsl(var(--surface-raised))] hover:bg-[hsl(var(--surface-raised))] rounded-lg transition-colors"
                          >
                            <span className="text-xs font-mono text-[hsl(var(--text-secondary))]">
                              {formatShortcut(shortcut.keys)}
                            </span>
                          </button>
                          {shortcut.isCustom && (
                            <button
                              onClick={() => resetShortcut(shortcut.id)}
                              className="p-1.5 rounded hover:bg-[hsl(var(--surface-raised))] transition-colors"
                              title="Reset to default"
                            >
                              <RotateCcw className="w-4 h-4 text-[hsl(var(--text-tertiary))]" />
                            </button>
                          )}
                        </>
                      )}
                    </div>
                  </div>

                  {/* Conflict Warning */}
                  {editingId === shortcut.id && conflictWarning && (
                    <div className="mt-2 flex items-center gap-2 text-xs text-[hsl(var(--danger-fg))] animate-in fade-in slide-in-from-top-2 duration-200">
                      <AlertTriangle className="w-3 h-3" />
                      {conflictWarning}
                    </div>
                  )}
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>

      {/* No Results */}
      {filteredShortcuts.length === 0 && (
        <div className="text-center py-12">
          <Search className="w-12 h-12 text-[hsl(var(--text-tertiary))] mx-auto mb-3" />
          <p className="text-[hsl(var(--text-secondary))]">No shortcuts found</p>
        </div>
      )}
    </div>
  );
}
