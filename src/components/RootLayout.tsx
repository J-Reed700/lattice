import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { Pencil, PenLine } from 'lucide-react';
import { Outlet, useLocation } from 'react-router';

import { useAppNavigate } from '@/hooks/useAppNavigate';

import { useRestoreLockoutStore } from '../features/backup/restoreLockoutStore';
import { useDeepResearchJobs } from '../features/jobs/research';
import { useJournalSyntheses } from '../features/journal/synthesis/api';
import { useSynthesisPanel } from '../features/journal/synthesis/synthesisPanel';
import { CommandPalette } from '../features/palette/components/CommandPalette';
import { useRegisterPaletteCommands } from '../features/palette/hooks/useRegisterPaletteCommands';
import { useStopSpeechOnChange } from '../features/speech/hooks/useSpeech';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import { useEditMode } from '../shared/edits/editMode';
import { EditModeBar } from '../shared/edits/EditModeBar';
import { useEditorStore } from '../shared/edits/editorStore';
import { NEW_ITEM_EVENT } from '../shared/newItemEvent';
import { readClipboardText } from '../utils/clipboard';

import type { PaletteCommand } from '../features/palette/stores/paletteCommandsStore';

const ContentEditor = lazy(async () => ({
  default: (await import('../shared/edits/ContentEditor')).ContentEditor,
}));

const QuickCaptureDialog = lazy(async () => ({
  default: (await import('../features/journal/components/QuickCaptureDialog')).QuickCaptureDialog,
}));

const SynthesisProgress = lazy(async () => ({
  default: (await import('../features/journal/synthesis/SynthesisProgress')).SynthesisProgress,
}));

/**
 * RootLayout — wraps every route. Owns the command palette, the quick-capture
 * sheet and the global keyboard shortcuts. The ⌘-number order here matches the
 * left rail.
 */
export function RootLayout() {
  const navigate = useAppNavigate();
  const location = useLocation();
  const toggleEditMode = useEditMode((state) => state.toggle);
  const editorOpen = useEditorStore((state) => state.current !== null);
  const [editorMounted, setEditorMounted] = useState(editorOpen);
  useEffect(() => {
    if (editorOpen) setEditorMounted(true);
  }, [editorOpen]);
  useStopSpeechOnChange(location.pathname);
  // Syntheses are jobs: one that finished while the app was closed shows too.
  const syntheses = useJournalSyntheses();
  const savedSynthesis = useSynthesisPanel(state => state.saved !== null);
  const hasSynthesis = savedSynthesis || (syntheses.data?.length ?? 0) > 0;
  const isDatabaseRestoredRestartRequired = useRestoreLockoutStore(
    (state) => state.isDatabaseRestoredRestartRequired,
  );
  const restoreFailed = useRestoreLockoutStore((state) => state.restoreFailed);
  const lockoutRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!isDatabaseRestoredRestartRequired) return;

    lockoutRef.current?.focus();

    // Mark every sibling portal root on document.body (e.g. Radix Dialog/Popover
    // portals) inert so portaled UI outside <Outlet> cannot stay interactive.
    const modified = new Set<Element>();
    const markBodyChildrenInert = () => {
      for (const child of Array.from(document.body.children)) {
        if (lockoutRef.current && child.contains(lockoutRef.current)) continue;
        if (!child.hasAttribute('inert')) {
          child.setAttribute('inert', '');
          modified.add(child);
        }
      }
    };
    markBodyChildrenInert();

    const observer =
      typeof MutationObserver !== 'undefined'
        ? new MutationObserver(markBodyChildrenInert)
        : null;
    observer?.observe(document.body, { childList: true });

    // Capture-phase window listeners intercept before any window/document
    // keydown/keyup handlers (e.g. FlashcardSession, palette, reader Esc).
    const blockKeyEvent = (event: KeyboardEvent) => {
      if (event.key === 'Tab' || event.key === 'Escape') {
        event.preventDefault();
      }
      event.stopImmediatePropagation();
    };
    const keepFocusInLockout = (event: FocusEvent) => {
      const target = event.target;
      if (
        lockoutRef.current &&
        target instanceof Node &&
        !lockoutRef.current.contains(target)
      ) {
        lockoutRef.current.focus();
      }
    };

    window.addEventListener('keydown', blockKeyEvent, true);
    window.addEventListener('keyup', blockKeyEvent, true);
    document.addEventListener('focusin', keepFocusInLockout, true);

    return () => {
      observer?.disconnect();
      for (const child of modified) {
        child.removeAttribute('inert');
      }
      window.removeEventListener('keydown', blockKeyEvent, true);
      window.removeEventListener('keyup', blockKeyEvent, true);
      document.removeEventListener('focusin', keepFocusInLockout, true);
    };
  }, [isDatabaseRestoredRestartRequired]);
  useDeepResearchJobs();
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
          navigate(here.startsWith('/studio') ? '/studio?new=1' : '/chat?new=1');
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
      { id: 'navigation.explorer', keys: 'Mod+6', description: 'Explorer', category: 'navigation' as const, handler: () => navigate('/explorer'), global: true },
      { id: 'navigation.studio', keys: 'Mod+7', description: 'Studio', category: 'navigation' as const, handler: () => navigate('/studio'), global: true },
      { id: 'global.editMode', keys: 'Mod+Shift+E', description: 'Edit mode', category: 'global' as const, handler: toggleEditMode, global: true },
      { id: 'navigation.import', keys: 'Mod+I', description: 'Import', category: 'navigation' as const, handler: () => navigate('/ingest'), global: true },
    ],
    [location.pathname, navigate, openCapture, toggleEditMode],
  );

  useKeyboardShortcuts(globalShortcuts, { enabled: !isDatabaseRestoredRestartRequired });

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
      {
        id: 'learning.editMode',
        label: 'Edit mode',
        group: 'Learning',
        icon: Pencil,
        shortcut: '⌘⇧E',
        run: toggleEditMode,
      },
    ],
    [openCapture, toggleEditMode],
  );
  useRegisterPaletteCommands(paletteCommands);

  return (
    <>
      <div className="contents" inert={isDatabaseRestoredRestartRequired || undefined}>
        <Outlet />
        <EditModeBar />
        {(editorOpen || editorMounted) && (
          <Suspense fallback={null}>
            <ContentEditor />
          </Suspense>
        )}
        {hasSynthesis && (
          <Suspense fallback={null}>
            <SynthesisProgress />
          </Suspense>
        )}
      </div>
      {!isDatabaseRestoredRestartRequired && <CommandPalette />}
      {capture.open && !isDatabaseRestoredRestartRequired && (
        <Suspense fallback={null}>
          <QuickCaptureDialog
            open={capture.open}
            clipboard={capture.clipboard}
            onClose={closeCapture}
            onOpenPage={(noteId) =>
              navigate(`/journals?${new URLSearchParams({ noteId }).toString()}`)
            }
          />
        </Suspense>
      )}
      {isDatabaseRestoredRestartRequired && (
        <div
          ref={lockoutRef}
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="restore-lockout-title"
          aria-describedby="restore-lockout-desc"
          tabIndex={-1}
          onKeyDown={(e) => {
            if (e.key === 'Tab' || e.key === 'Escape') {
              e.preventDefault();
            }
          }}
          className="fixed inset-0 z-[9999] flex items-center justify-center bg-bg/90 p-6 backdrop-blur-xs focus:outline-hidden"
        >
          <div className="w-full max-w-sm rounded-lg border border-border-subtle bg-surface-raised p-6 shadow-md">
            <h2 id="restore-lockout-title" className="text-base font-medium text-text-primary">
              Restart required
            </h2>
            <p id="restore-lockout-desc" className="mt-2 text-sm text-text-secondary">
              {restoreFailed
                ? 'Restore failed after closing the library connection. Quit Lattice and open it again to reconnect.'
                : 'Backup restored. Quit Lattice and open it again to use the restored library.'}
            </p>
          </div>
        </div>
      )}
    </>
  );
}
