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
  currentView?: 'search' | 'files' | 'chat' | 'settings' | 'journals';

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
  { keys: '⌘3', description: 'Go to Journals', category: 'navigation' },
  { keys: '⌘4', description: 'Go to Chat', category: 'navigation' },
  { keys: '⌘5', description: 'Go to Reference Inbox', category: 'navigation' },
  { keys: '⌘,', description: 'Go to Settings', category: 'navigation' },
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
        className="fixed inset-0 bg-[hsl(var(--overlay))] z-50 animate-in fade-in duration-base"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Dialog */}
      <div
        className="fixed right-0 top-0 bottom-0 w-full max-w-2xl z-50 animate-in slide-in-from-right duration-slow"
        role="dialog"
        aria-modal="true"
        aria-labelledby="help-overlay-title"
      >
        <div className="h-full bg-[hsl(var(--surface-raised))] shadow-md flex flex-col border-l border-[hsl(var(--border-subtle))]">
          {/* Header */}
          <div className="flex items-center justify-between px-6 py-4 border-b border-[hsl(var(--border-subtle))]">
            <div className="flex items-center gap-3">
              <div className="w-10 h-10 rounded-md bg-[hsl(var(--accent-muted))] flex items-center justify-center">
                <Keyboard className="w-4 h-4 text-[hsl(var(--accent))]" strokeWidth={1.75} />
              </div>
              <div>
                <h2
                  id="help-overlay-title"
                  className="text-lg font-semibold text-[hsl(var(--text-primary))]"
                >
                  Help & Shortcuts
                </h2>
                <p className="text-sm text-[hsl(var(--text-secondary))]">
                  Press <kbd className="px-1.5 py-0.5 text-xs font-mono bg-[hsl(var(--surface))] rounded border border-[hsl(var(--border-subtle))]">?</kbd> to toggle
                </p>
              </div>
            </div>
            <button
              onClick={onClose}
              className="p-2 rounded-md text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface))] transition-colors duration-fast"
              aria-label="Close help"
            >
              <X className="w-4 h-4" strokeWidth={1.75} />
            </button>
          </div>

          {/* Tabs */}
          <div className="flex gap-4 px-6 py-3 border-b border-[hsl(var(--border-subtle))]">
            <button
              onClick={() => setActiveTab('shortcuts')}
              className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors duration-fast ${
                activeTab === 'shortcuts'
                  ? 'bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))]'
                  : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface))]'
              }`}
            >
              Keyboard shortcuts
            </button>
            <button
              onClick={() => setActiveTab('tips')}
              className={`px-3 py-1.5 text-sm font-medium rounded-md transition-colors duration-fast ${
                activeTab === 'tips'
                  ? 'bg-[hsl(var(--accent-muted))] text-[hsl(var(--accent))]'
                  : 'text-[hsl(var(--text-secondary))] hover:bg-[hsl(var(--surface))]'
              }`}
            >
              Quick tips
            </button>
          </div>

          {/* Content */}
          <div className="flex-1 overflow-y-auto">
            {activeTab === 'shortcuts' ? (
              <div className="p-6">
                {/* Search */}
                <div className="relative mb-6">
                  <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-[hsl(var(--text-tertiary))]" strokeWidth={1.75} />
                  <input
                    type="text"
                    placeholder="Search shortcuts..."
                    value={searchQuery}
                    onChange={(e) => setSearchQuery(e.target.value)}
                    className="w-full pl-10 pr-4 py-2 border border-[hsl(var(--border-default))] rounded-md bg-[hsl(var(--surface))] text-[hsl(var(--text-primary))] text-sm focus:outline-none focus:ring-2 focus:ring-[hsl(var(--ring))]"
                  />
                </div>

                {/* Shortcuts by category */}
                <div className="space-y-6">
                  {Object.entries(groupedShortcuts).map(([category, shortcuts]) => {
                    if (shortcuts.length === 0) return null;

                    return (
                      <div key={category}>
                        <h3 className="text-xs font-semibold uppercase tracking-wider text-[hsl(var(--text-secondary))] mb-3">
                          {category}
                        </h3>
                        <div className="space-y-2">
                          {shortcuts.map((shortcut, index) => (
                            <div
                              key={index}
                              className="flex items-center justify-between py-2 px-3 rounded-md hover:bg-[hsl(var(--surface))] transition-colors duration-fast"
                            >
                              <span className="text-sm text-[hsl(var(--text-primary))]">
                                {shortcut.description}
                              </span>
                              <kbd className="px-2 py-1 text-xs font-mono bg-[hsl(var(--surface))] text-[hsl(var(--text-secondary))] rounded border border-[hsl(var(--border-subtle))]">
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
                  <p className="text-center text-sm text-[hsl(var(--text-secondary))] py-8">
                    No shortcuts found matching "{searchQuery}"
                  </p>
                )}
              </div>
            ) : (
              <div className="p-6 space-y-6">
                {/* Quick Tips */}
                <QuickTip
                  title="Organize with tags"
                  description="Use #tags in your notes to create automatic collections. Click any tag to see related documents."
                />
                <QuickTip
                  title="Link your thoughts"
                  description="Use [[wiki-style links]] or @mentions to connect notes and build a knowledge graph."
                />
                <QuickTip
                  title="Journals"
                  description="Capture entries, synthesize threads, and keep highlights and sources in one notebook flow."
                />
                <QuickTip
                  title="Semantic search"
                  description="Search finds conceptually similar content, not just exact matches. Ask questions in natural language."
                />
                <QuickTip
                  title="Quick capture"
                  description="Press ⌘D from anywhere to quickly capture a note without interrupting your flow."
                />
                <QuickTip
                  title="Command palette"
                  description="Press ⌘K to open the command palette and quickly access any feature."
                />

                {/* Documentation Link */}
                <div className="mt-8 p-4 bg-[hsl(var(--accent-muted))] rounded-md border border-[hsl(var(--accent-muted))]">
                  <h4 className="text-sm font-semibold text-[hsl(var(--accent))] mb-2">
                    Need more help?
                  </h4>
                  <p className="text-sm text-[hsl(var(--accent))] mb-3">
                    Check out our comprehensive documentation for detailed guides and tutorials.
                  </p>
                  <a
                    href="https://docs.recall.dev"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-2 text-sm font-medium text-[hsl(var(--accent))] hover:text-[hsl(var(--accent-hover))]"
                  >
                    View documentation
                    <ExternalLink className="w-4 h-4" strokeWidth={1.75} />
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
    <div className="p-4 bg-[hsl(var(--surface))] rounded-md">
      <h4 className="text-sm font-semibold text-[hsl(var(--text-primary))] mb-1">
        {title}
      </h4>
      <p className="text-sm text-[hsl(var(--text-secondary))]">{description}</p>
    </div>
  );
}
