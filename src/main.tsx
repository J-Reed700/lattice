/**
 * Bootstrap: clear frozen localStorage entries BEFORE any store imports run.
 *
 * Tauri's freezePrototype setting can freeze objects stored in localStorage,
 * which makes Zustand's persist middleware throw during rehydration and leaves
 * a blank window. This check runs first so the stores always start clean.
 */
import { QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'
import ReactDOM from 'react-dom/client'

import App from './App'
import { RootErrorBoundary } from './components/ErrorBoundary'
import { queryClient } from './lib/queryClient'
import { ConversationsProvider } from './stores/conversationsStore'
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
      const parsed = JSON.parse(item)
      const isFrozen =
        Object.isFrozen(parsed) ||
        (parsed.state && Object.isFrozen(parsed.state)) ||
        (parsed.state?.settings && Object.isFrozen(parsed.state.settings))
      if (isFrozen) {
        console.warn(`[bootstrap] Cleared frozen localStorage key "${key}"`)
        localStorage.removeItem(key)
      }
    } catch (error) {
      console.warn(`[bootstrap] Cleared unreadable localStorage key "${key}":`, error)
      localStorage.removeItem(key)
    }
  })
}

bootstrapLocalStorage()

// The stylesheet reserves room for the macOS traffic lights; tell it where it is.
const platform = /Mac/i.test(navigator.userAgent) ? 'macos' : /Win/i.test(navigator.userAgent) ? 'windows' : 'linux'
document.documentElement.dataset.platform = platform

const rootElement = document.getElementById('root')
if (!rootElement) {
  throw new Error('Root element not found. Ensure index.html contains <div id="root"></div>')
}

// StrictMode stays off: its double-invoked effects fire IPC commands twice.
ReactDOM.createRoot(rootElement).render(
  <QueryClientProvider client={queryClient}>
    <ConversationsProvider>
      <RootErrorBoundary>
        <App />
      </RootErrorBoundary>
    </ConversationsProvider>
    <ReactQueryDevtools initialIsOpen={false} />
  </QueryClientProvider>,
)
