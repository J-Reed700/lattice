import { useEffect } from 'react';

import { X } from 'lucide-react';

interface KeyboardShortcutsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface Shortcut {
  keys: string[];
  description: string;
}

interface ShortcutGroup {
  title: string;
  shortcuts: Shortcut[];
}

/**
 * KeyboardShortcutsModal — the reference sheet for every shortcut the app
 * actually binds. Keep this in sync with RootLayout, ChatView, and the
 * sidebars; a shortcut listed here that does nothing is a bug.
 */
export function KeyboardShortcutsModal({ isOpen, onClose }: KeyboardShortcutsModalProps) {
  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().includes('MAC');
  const cmd = isMac ? '⌘' : 'Ctrl';
  const shift = isMac ? '⇧' : 'Shift';

  useEffect(() => {
    if (!isOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  const groups: ShortcutGroup[] = [
    {
      title: 'Everywhere',
      shortcuts: [
        { keys: [cmd, 'K'], description: 'Command palette' },
        { keys: [cmd, 'N'], description: 'New conversation or entry' },
        { keys: [cmd, shift, 'N'], description: 'Quick capture' },
        { keys: [cmd, ','], description: 'Settings' },
        { keys: ['Esc'], description: 'Close' },
      ],
    },
    {
      title: 'Go to',
      shortcuts: [
        { keys: [cmd, '0'], description: 'Home' },
        { keys: [cmd, '1'], description: 'Search' },
        { keys: [cmd, '2'], description: 'Library' },
        { keys: [cmd, '3'], description: 'Journal' },
        { keys: [cmd, '4'], description: 'Chat' },
        { keys: [cmd, '5'], description: 'References' },
        { keys: [cmd, 'I'], description: 'Import' },
      ],
    },
    {
      title: 'Chat',
      shortcuts: [
        { keys: ['Enter'], description: 'Send' },
        { keys: [shift, 'Enter'], description: 'New line' },
        { keys: [cmd, shift, 'K'], description: 'Find in conversations and references' },
        { keys: [cmd, '\\'], description: 'Hide or show the sidebar' },
      ],
    },
  ];

  return (
    <>
      <div
        className="fixed inset-0 z-[9998] bg-overlay animate-in fade-in duration-fast"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcuts-title"
        className="fixed left-1/2 top-1/2 z-[9999] w-full max-w-lg -translate-x-1/2 -translate-y-1/2 animate-in fade-in zoom-in-95 duration-fast"
      >
        <div className="max-h-[80vh] overflow-y-auto rounded-lg border border-border-subtle bg-surface-raised shadow-md">
          <div className="flex items-center justify-between border-b border-border-subtle px-5 py-3">
            <h2 id="shortcuts-title" className="font-serif text-base font-semibold text-text-primary">
              Keyboard shortcuts
            </h2>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="inline-flex h-7 w-7 items-center justify-center rounded-sm text-text-muted transition-colors duration-fast hover:bg-surface hover:text-text-primary"
            >
              <X className="h-4 w-4" strokeWidth={1.75} />
            </button>
          </div>

          <div className="space-y-6 px-5 py-4">
            {groups.map((group) => (
              <section key={group.title}>
                <h3 className="pb-1 text-xxs uppercase tracking-[0.08em] text-text-muted">{group.title}</h3>
                <div className="border-t border-border-subtle">
                  {group.shortcuts.map((shortcut) => (
                    <div
                      key={shortcut.description}
                      className="flex items-center justify-between border-b border-border-subtle py-2"
                    >
                      <span className="text-sm text-text-secondary">{shortcut.description}</span>
                      <span className="flex gap-1">
                        {shortcut.keys.map((key) => (
                          <kbd
                            key={key}
                            className="inline-flex min-w-[24px] items-center justify-center rounded-sm border border-border-default bg-surface px-1.5 py-0.5 font-mono text-xs text-text-secondary"
                          >
                            {key}
                          </kbd>
                        ))}
                      </span>
                    </div>
                  ))}
                </div>
              </section>
            ))}
          </div>
        </div>
      </div>
    </>
  );
}
