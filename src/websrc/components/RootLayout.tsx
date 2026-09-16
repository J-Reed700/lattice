import { useCallback, useMemo, useState } from 'react';

import { PenLine } from 'lucide-react';
import { Outlet, useLocation, useNavigate } from 'react-router';

import { CommandPalette } from './CommandPalette';
import { QuickCaptureDialog, readClipboardText } from './QuickCapture';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import { useRegisterPaletteCommands } from '../hooks/useRegisterPaletteCommands';

import type { PaletteCommand } from '../stores/paletteCommandsStore';

/**
 * Fired on ⌘N. Surfaces that can create something (Chat, Journal) listen for
 * it and create in place; everywhere else ⌘N opens Chat with `?new=1`, which
 * the Chat surface turns into a fresh conversation.
 */
export const NEW_ITEM_EVENT = 'lattice:new';

/**
 * RootLayout — wraps every route. Owns the command palette, the quick-capture
 * sheet and the global keyboard shortcuts. The ⌘-number order here matches the
 * left rail.
 */
export function RootLayout() {
  const navigate = useNavigate();
  const location = useLocation();
  const [capture, setCapture] = useState<{ open: boolean; clipboard: string | null }>({
    open: false,
    clipboard: null,
  });

  const openCapture = useCallback(() => {
    setCapture({ open: true, clipboard: null });
    // The promise is created inside the key handler so the read still counts
    // as a user gesture; the hint fills in when it resolves.
    void readClipboardText().then((clipboard) =>
      setCapture((prev) => (prev.open ? { open: true, clipboard } : prev)),
    );
  }, []);

  const closeCapture = useCallback(() => setCapture({ open: false, clipboard: null }), []);

  const globalShortcuts = useMemo(
    () => [
      {
        id: 'global.new',
        keys: 'Mod+N',
        description: 'New conversation or entry',
        category: 'global' as const,
        handler: () => {
          const here = location.pathname;
          if (here.startsWith('/chat') || here.startsWith('/journals')) {
            window.dispatchEvent(new CustomEvent(NEW_ITEM_EVENT, { detail: { pathname: here } }));
            return;
          }
          navigate('/chat?new=1');
        },
        global: true,
      },
      { id: 'global.quickCapture', keys: 'Mod+Shift+N', description: 'Quick capture', category: 'global' as const, handler: openCapture, global: true },
      { id: 'global.settings', keys: 'Mod+,', description: 'Settings', category: 'global' as const, handler: () => navigate('/settings'), global: true },
      { id: 'navigation.home', keys: 'Mod+0', description: 'Home', category: 'navigation' as const, handler: () => navigate('/home'), global: true },
      { id: 'navigation.search', keys: 'Mod+1', description: 'Search', category: 'navigation' as const, handler: () => navigate('/search'), global: true },
      { id: 'navigation.library', keys: 'Mod+2', description: 'Library', category: 'navigation' as const, handler: () => navigate('/files'), global: true },
      { id: 'navigation.journal', keys: 'Mod+3', description: 'Journal', category: 'navigation' as const, handler: () => navigate('/journals'), global: true },
      { id: 'navigation.chat', keys: 'Mod+4', description: 'Chat', category: 'navigation' as const, handler: () => navigate('/chat'), global: true },
      { id: 'navigation.references', keys: 'Mod+5', description: 'References', category: 'navigation' as const, handler: () => navigate('/references'), global: true },
      { id: 'navigation.import', keys: 'Mod+I', description: 'Import', category: 'navigation' as const, handler: () => navigate('/ingest'), global: true },
    ],
    [location.pathname, navigate, openCapture],
  );

  useKeyboardShortcuts(globalShortcuts, { enabled: true });

  const paletteCommands = useMemo<PaletteCommand[]>(
    () => [
      {
        id: 'vault.quickCapture',
        label: 'Quick capture',
        group: 'Vault',
        icon: PenLine,
        shortcut: '⌘⇧N',
        run: openCapture,
      },
    ],
    [openCapture],
  );
  useRegisterPaletteCommands(paletteCommands);

  return (
    <>
      <Outlet />
      <CommandPalette />
      <QuickCaptureDialog
        open={capture.open}
        clipboard={capture.clipboard}
        onClose={closeCapture}
        onOpenPage={(noteId) =>
          navigate(`/journals?${new URLSearchParams({ noteId }).toString()}`)
        }
      />
    </>
  );
}
