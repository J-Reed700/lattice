import { X } from 'lucide-react'

interface KeyboardShortcutsModalProps {
  isOpen: boolean
  onClose: () => void
}

interface Shortcut {
  keys: string[]
  description: string
}

interface ShortcutGroup {
  title: string
  shortcuts: Shortcut[]
}

/**
 * KeyboardShortcutsModal
 *
 * Purpose: Display all available keyboard shortcuts in a modal
 *
 * Features:
 * - Organized by category
 * - Platform-aware (Mac vs Windows/Linux)
 * - Escape to close
 * - Print-friendly layout
 */
export function KeyboardShortcutsModal({ isOpen, onClose }: KeyboardShortcutsModalProps) {
  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0
  const cmd = isMac ? '⌘' : 'Ctrl'
  const shift = isMac ? '⇧' : 'Shift'

  const shortcutGroups: ShortcutGroup[] = [
    {
      title: 'General',
      shortcuts: [
        { keys: [cmd, 'K'], description: 'Open command palette' },
        { keys: ['Esc'], description: 'Close command palette' },
        { keys: ['?'], description: 'Show keyboard shortcuts' },
        { keys: [cmd, ','], description: 'Open preferences' },
      ],
    },
    {
      title: 'Search',
      shortcuts: [
        { keys: [cmd, 'F'], description: 'Search documents' },
        { keys: [cmd, shift, 'F'], description: 'Search by filename' },
        { keys: ['↑', '↓'], description: 'Navigate results' },
        { keys: ['Enter'], description: 'Open selected result' },
      ],
    },
    {
      title: 'Upload',
      shortcuts: [
        { keys: [cmd, 'U'], description: 'Upload document' },
        { keys: [cmd, shift, 'U'], description: 'Upload folder' },
      ],
    },
    {
      title: 'Navigation',
      shortcuts: [
        { keys: [cmd, '1'], description: 'View all documents' },
        { keys: [cmd, '2'], description: 'Recent documents' },
        { keys: [cmd, '3'], description: 'Settings' },
      ],
    },
    {
      title: 'Command Palette',
      shortcuts: [
        { keys: ['↑', '↓'], description: 'Navigate commands' },
        { keys: ['Enter'], description: 'Execute command' },
        { keys: ['Esc'], description: 'Close palette' },
        { keys: ['Type'], description: 'Filter commands' },
      ],
    },
  ]

  return (
    <>
      {isOpen && (
        <>
          {/* Backdrop */}
          <div
            className="fixed inset-0 bg-[hsl(var(--overlay))] z-[9998] animate-in fade-in duration-150"
            onClick={onClose}
          />

          {/* Modal */}
          <div className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-2xl z-[9999] max-h-[80vh] overflow-y-auto animate-in fade-in zoom-in-95 slide-in-from-top-4 duration-150">
            <div className="bg-[hsl(var(--surface-raised))] rounded-lg shadow-md">
              {/* Header */}
              <div className="flex items-center justify-between p-6 border-b border-[hsl(var(--border-subtle))]">
                <h2 className="text-2xl font-semibold text-[hsl(var(--text-primary))]">
                  Keyboard shortcuts
                </h2>
                <button
                  onClick={onClose}
                  className="p-2 rounded-lg hover:bg-[hsl(var(--surface-raised))] transition-colors"
                  aria-label="Close"
                >
                  <X className="w-4 h-4 text-[hsl(var(--text-tertiary))]" strokeWidth={1.75} />
                </button>
              </div>

              {/* Content */}
              <div className="p-6 space-y-8">
                {shortcutGroups.map((group) => (
                  <div key={group.title}>
                    <h3 className="text-sm font-semibold text-[hsl(var(--text-secondary))] uppercase tracking-wide mb-3">
                      {group.title}
                    </h3>
                    <div className="space-y-2">
                      {group.shortcuts.map((shortcut, index) => (
                        <div
                          key={index}
                          className="flex items-center justify-between py-2 px-3 rounded-lg hover:bg-[hsl(var(--surface-raised))]/50 transition-colors"
                        >
                          <span className="text-sm text-[hsl(var(--text-secondary))]">
                            {shortcut.description}
                          </span>
                          <div className="flex gap-1">
                            {shortcut.keys.map((key, keyIndex) => (
                              <kbd
                                key={keyIndex}
                                className="inline-flex items-center justify-center px-2.5 py-1.5 text-xs font-semibold text-[hsl(var(--text-secondary))] bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded-md min-w-[32px]"
                              >
                                {key}
                              </kbd>
                            ))}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>

              {/* Footer */}
              <div className="px-6 py-4 border-t border-[hsl(var(--border-subtle))] bg-[hsl(var(--surface))]/50 rounded-b-lg">
                <p className="text-xs text-[hsl(var(--text-secondary))] text-center">
                  Press <kbd className="px-2 py-1 text-xs font-semibold bg-[hsl(var(--surface-raised))] border border-[hsl(var(--border-subtle))] rounded">Esc</kbd> to close
                </p>
              </div>
            </div>
          </div>
        </>
      )}
    </>
  )
}
