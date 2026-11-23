import { lazy, Suspense, useEffect, useState } from 'react'

import { useCommandPalette } from '@/features/palette/hooks/useCommandPalette'

const CommandPaletteDialog = lazy(async () => ({
  default: (await import('./CommandPaletteDialog')).CommandPaletteDialog,
}))

const KeyboardShortcutsModal = lazy(async () => ({
  default: (await import('../KeyboardShortcutsModal')).KeyboardShortcutsModal,
}))

/**
 * CommandPalette — ⌘K. Keeps only the global shortcut listener in the initial
 * route bundle; the palette UI, cmdk, and shortcuts sheet load on first open.
 */
export function CommandPalette() {
  const palette = useCommandPalette()
  const [hasOpened, setHasOpened] = useState(false)
  const [showShortcuts, setShowShortcuts] = useState(false)

  useEffect(() => {
    if (palette.isOpen) setHasOpened(true)
  }, [palette.isOpen])

  return (
    <>
      {hasOpened && (
        <Suspense fallback={null}>
          <CommandPaletteDialog
            isOpen={palette.isOpen}
            searchMode={palette.searchMode}
            recentSearches={palette.recentSearches}
            recentDocuments={palette.recentDocuments}
            close={palette.close}
            enterSearchMode={palette.enterSearchMode}
            addRecentSearch={palette.addRecentSearch}
            addRecentDocument={palette.addRecentDocument}
            clearRecentSearches={palette.clearRecentSearches}
            onShowShortcuts={() => setShowShortcuts(true)}
          />
        </Suspense>
      )}

      {showShortcuts && (
        <Suspense fallback={null}>
          <KeyboardShortcutsModal isOpen={showShortcuts} onClose={() => setShowShortcuts(false)} />
        </Suspense>
      )}
    </>
  )
}
