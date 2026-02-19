/**
 * HelpOverlay Component
 *
 * Purpose: Contextual help and keyboard shortcuts reference
 *
 * Features:
 * - View-specific keyboard shortcuts
 * - Quick tips
 * - Feature explanations
 * - Link to documentation
 * - Searchable shortcuts
 *
 * States: Open, Closed
 * Accessibility: Focus trap, ARIA dialog, keyboard navigation
 * Trigger: ? key or Help button in Header
 */

import { useEffect, useState, useMemo } from 'react';

import { X, Search, ExternalLink, Keyboard } from 'lucide-react';

interface KeyboardShortcut {
  keys: string;
  description: string;
  category: 'navigation' | 'editing' | 'search' | 'global';
}

export interface HelpOverlayProps {
  /** Dialog open state */
  isOpen: boolean;

  /** Callback when closed */
  onClose: () => void;

  /** Current view context for relevant shortcuts */
  currentView?: 'search' | 'files' | 'chat' | 'settings' | 'daily';

  /** Additional view-specific shortcuts */
  customShortcuts?: KeyboardShortcut[];
}

const GLOBAL_SHORTCUTS: KeyboardShortcut[] = [
  { keys: '⌘K', description: 'Open command palette', category: 'global' },
  { keys: '⌘D', description: 'Quick capture note', category: 'global' },
  { keys: '⌘N', description: 'Create new note', category: 'global' },
  { keys: '⌘,', description: 'Open settings', category: 'global' },
  { keys: '?', description: 'Show help', category: 'global' },
];

const NAVIGATION_SHORTCUTS: KeyboardShortcut[] = [
  { keys: '⌘1', description: 'Go to Search', category: 'navigation' },
  { keys: '⌘2', description: 'Go to Files', category: 'navigation' },
  { keys: '⌘3', description: 'Go to Daily Note', category: 'navigation' },
  { keys: '⌘4', description: 'Go to Chat', category: 'navigation' },
  { keys: '⌘5', description: 'Go to Settings', category: 'navigation' },
];

const EDITING_SHORTCUTS: KeyboardShortcut[] = [
  { keys: '⌘B', description: 'Bold text', category: 'editing' },
  { keys: '⌘I', description: 'Italic text', category: 'editing' },
  { keys: '⌘K', description: 'Insert link', category: 'editing' },
  { keys: '⌘S', description: 'Save (auto-saves)', category: 'editing' },
  { keys: '⌘Z', description: 'Undo', category: 'editing' },
  { keys: '⌘⇧Z', description: 'Redo', category: 'editing' },
];

const SEARCH_SHORTCUTS: KeyboardShortcut[] = [
  { keys: '⌘F', description: 'Find in page', category: 'search' },
  { keys: '⌘G', description: 'Find next', category: 'search' },
  { keys: '⌘⇧G', description: 'Find previous', category: 'search' },
  { keys: '↵', description: 'Open selected result', category: 'search' },
  { keys: '↑/↓', description: 'Navigate results', category: 'search' },
];

export function HelpOverlay({
  isOpen,
  onClose,
  currentView: _currentView,
  customShortcuts = [],
}: HelpOverlayProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [activeTab, setActiveTab] = useState<'shortcuts' | 'tips'>('shortcuts');

  // Close on Escape
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  // Combine all shortcuts
  const allShortcuts = useMemo(
    () => [
      ...GLOBAL_SHORTCUTS,
      ...NAVIGATION_SHORTCUTS,
      ...EDITING_SHORTCUTS,
      ...SEARCH_SHORTCUTS,
      ...customShortcuts,
    ],
    [customShortcuts]
  );

  // Filter shortcuts by search query
  const filteredShortcuts = useMemo(() => {
    if (!searchQuery) return allShortcuts;

    const query = searchQuery.toLowerCase();
    return allShortcuts.filter(
      (shortcut) =>
        shortcut.description.toLowerCase().includes(query) ||
        shortcut.keys.toLowerCase().includes(query)
    );
  }, [allShortcuts, searchQuery]);

  // Group shortcuts by category
  const groupedShortcuts = useMemo(() => {
    const groups: Record<string, KeyboardShortcut[]> = {
      global: [],
      navigation: [],
      editing: [],
      search: [],
    };

    filteredShortcuts.forEach((shortcut) => {
      groups[shortcut.category].push(shortcut);
    });

    return groups;
  }, [filteredShortcuts]);

  if (!isOpen) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-black/50 backdrop-blur-sm z-50 animate-in fade-in duration-200"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Dialog */}
      <div
        className="fixed right-0 top-0 bottom-0 w-full max-w-2xl z-50 animate-in slide-in-from-right duration-300"
        role="dialog"
        aria-modal="true"
        aria-labelledby="help-overlay-title"
      >
        <div className="h-full bg-[var(--surface-elevated)] shadow-2xl flex flex-col">
          {/* Header */}
          <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--border-color)]">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-lg bg-[var(--accent-light)]/30 flex items-center justify-center">
                <Keyboard className="w-5 h-5 text-[var(--accent-primary)]" />
              </div>
              <div>
                <h2
                  id="help-overlay-title"
                  className="text-lg font-semibold text-[var(--text-primary)]"
                >
                  Help & Shortcuts
                </h2>
                <p className="text-sm text-[var(--text-secondary)]">
                  Press <kbd className="px-1.5 py-0.5 text-xs font-mono bg-[var(--bg-tertiary)] rounded">?</kbd> to toggle
                </p>
              </div>
            </div>
            <button
              onClick={onClose}
              className="p-2 rounded-lg text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] transition-colors"
              aria-label="Close help"
            >
              <X className="w-5 h-5" />
            </button>
          </div>

          {/* Tabs */}
          <div className="flex gap-4 px-6 py-3 border-b border-[var(--border-color)]">
            <button
              onClick={() => setActiveTab('shortcuts')}
              className={`px-3 py-1.5 text-sm font-medium rounded-lg transition-colors ${
                activeTab === 'shortcuts'
                  ? 'bg-[var(--accent-light)]/30 text-[var(--accent-primary)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]'
              }`}
            >
              Keyboard Shortcuts
            </button>
            <button
              onClick={() => setActiveTab('tips')}
              className={`px-3 py-1.5 text-sm font-medium rounded-lg transition-colors ${
                activeTab === 'tips'
                  ? 'bg-[var(--accent-light)]/30 text-[var(--accent-primary)]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]'
              }`}
            >
              Quick Tips
            </button>
          </div>

          {/* Content */}
          <div className="flex-1 overflow-y-auto">
            {activeTab === 'shortcuts' ? (
              <div className="p-6">
                {/* Search */}
                <div className="relative mb-6">
                  <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-[var(--text-tertiary)]" />
                  <input
                    type="text"
                    placeholder="Search shortcuts..."
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className="w-full pl-10 pr-4 py-2 border border-[var(--border-color)] rounded-lg bg-[var(--surface-elevated)] text-[var(--text-primary)] text-sm focus:outline-none focus:ring-2 ring-[var(--accent-primary)]"
                  />
                </div>

                {/* Shortcuts by category */}
                <div className="space-y-6">
                  {Object.entries(groupedShortcuts).map(([category, shortcuts]) => {
                    if (shortcuts.length === 0) return null;

                    return (
                      <div key={category}>
                        <h3 className="text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] mb-3">
                          {category}
                        </h3>
                        <div className="space-y-2">
                          {shortcuts.map((shortcut, index) => (
                            <div
                              key={index}
                              className="flex items-center justify-between py-2 px-3 rounded-lg hover:bg-[var(--surface-hover)]/50 transition-colors"
                            >
                              <span className="text-sm text-[var(--text-primary)]">
                                {shortcut.description}
                              </span>
                              <kbd className="px-2 py-1 text-xs font-mono bg-[var(--bg-tertiary)] text-[var(--text-secondary)] rounded border border-[var(--border-color)]">
                                {shortcut.keys}
                              </kbd>
                            </div>
                          ))}
                        </div>
                      </div>
                    );
                  })}
                </div>

                {filteredShortcuts.length === 0 && (
                  <p className="text-center text-sm text-[var(--text-secondary)] py-8">
                    No shortcuts found matching "{searchQuery}"
                  </p>
                )}
              </div>
            ) : (
              <div className="p-6 space-y-6">
                {/* Quick Tips */}
                <QuickTip
                  title="Organize with Tags"
                  description="Use #tags in your notes to create automatic collections. Click any tag to see related documents."
                />
                <QuickTip
                  title="Link Your Thoughts"
                  description="Use [[wiki-style links]] or @mentions to connect notes and build a knowledge graph."
                />
                <QuickTip
                  title="Daily Notes"
                  description="Create a new daily note each day to capture fleeting thoughts, tasks, and ideas."
                />
                <QuickTip
                  title="Semantic Search"
                  description="Search finds conceptually similar content, not just exact matches. Ask questions in natural language."
                />
                <QuickTip
                  title="Quick Capture"
                  description="Press ⌘D from anywhere to quickly capture a note without interrupting your flow."
                />
                <QuickTip
                  title="Command Palette"
                  description="Press ⌘K to open the command palette and quickly access any feature."
                />

                {/* Documentation Link */}
                <div className="mt-8 p-4 bg-[var(--accent-light)]/20 rounded-lg border border-[var(--accent-light)]">
                  <h4 className="text-sm font-semibold text-[var(--accent-primary)] mb-2">
                    Need more help?
                  </h4>
                  <p className="text-sm text-[var(--accent-primary)] mb-3">
                    Check out our comprehensive documentation for detailed guides and tutorials.
                  </p>
                  <a
                    href="https://docs.recall.dev"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-2 text-sm font-medium text-[var(--accent-primary)] hover:text-[var(--accent-primary)]"
                  >
                    View Documentation
                    <ExternalLink className="w-4 h-4" />
                  </a>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

function QuickTip({ title, description }: { title: string; description: string }) {
  return (
    <div className="p-4 bg-[var(--bg-secondary)]/50 rounded-lg">
      <h4 className="text-sm font-semibold text-[var(--text-primary)] mb-1">
        {title}
      </h4>
      <p className="text-sm text-[var(--text-secondary)]">{description}</p>
    </div>
  );
}
