/**
 * Bootstrap: clear frozen localStorage entries BEFORE any store imports run.
 *
 * Tauri's freezePrototype setting can freeze objects stored in localStorage,
 * which makes Zustand's persist middleware throw during rehydration and leaves
 * a blank window. This check runs first so the stores always start clean.
 */
import { lazy, Suspense, useEffect } from 'react'

import { isTauri } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import ReactDOM from 'react-dom/client'

import App from './App'
import { AppProviders } from './AppProviders'
import { RootErrorBoundary } from './components/ErrorBoundary'
import { installGlobalDiagnostics } from './utils/globalDiagnostics'
import '@fontsource-variable/inter/opsz.css'
import '@fontsource-variable/inter/opsz-italic.css'
import '@fontsource-variable/source-serif-4/opsz.css'
import '@fontsource-variable/source-serif-4/opsz-italic.css'
import '@fontsource-variable/jetbrains-mono/wght.css'
import './index.css'

function bootstrapLocalStorage() {
  const keysToCheck = ['lattice-settings', 'vault_onboarding_complete', 'vault_tour_dismissed']

  keysToCheck.forEach((key) => {
    try {
      const item = localStorage.getItem(key)
      if (!item) return
      JSON.parse(item)
    } catch (error) {
      console.warn(`[bootstrap] Cleared unreadable localStorage key "${key}":`, error)
      localStorage.removeItem(key)
    }
  })
}

const disposeDiagnostics = installGlobalDiagnostics()
if (import.meta.hot) import.meta.hot.dispose(disposeDiagnostics)

bootstrapLocalStorage()

// The stylesheet reserves room for the macOS traffic lights; tell it where it is.
const platform = /Mac/i.test(navigator.userAgent) ? 'macos' : /Win/i.test(navigator.userAgent) ? 'windows' : 'linux'
document.documentElement.dataset.platform = platform

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Root element not found. Ensure index.html contains <div id="root"></div>')
}

// Reports the first commit. A sibling of the boundary, not a child, so a caught error's
// fallback still counts as a first screen while a root-level crash (which commits nothing) does not.
function BootSignal() {
  useEffect(() => {
    window.__LATTICE_BOOT__?.markMounted()
    // Emitted here, not by boot-guard.js, so the native handshake survives that script failing to load.
    if (isTauri()) {
      void emit('lattice:boot-mounted').catch(() => {})
    }
  }, [])
  return null
}

const ReactQueryDevtools = import.meta.env.DEV
  ? lazy(() =>
      import('@tanstack/react-query-devtools').then((mod) => ({ default: mod.ReactQueryDevtools }))
    )
  : null;

// StrictMode stays off: its double-invoked effects fire IPC commands twice.
ReactDOM.createRoot(rootElement).render(
  <>
    <RootErrorBoundary>
      <AppProviders>
        <App />
        {ReactQueryDevtools ? (
          <Suspense fallback={null}>
            <ReactQueryDevtools initialIsOpen={false} />
          </Suspense>
        ) : null}
      </AppProviders>
    </RootErrorBoundary>
    <BootSignal />
  </>,
)

// Last statement: the module graph loaded and evaluated. From here React's boundaries and
// installGlobalDiagnostics own errors, so the guard stops listening for stray rejections.
window.__LATTICE_BOOT__?.markLoaded()
