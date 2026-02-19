/**
 * CRITICAL: Bootstrap localStorage cleanup BEFORE any React imports
 *
 * This checks for frozen objects in localStorage and clears them BEFORE Zustand
 * attempts to rehydrate state. Tauri's freezePrototype setting can freeze objects
 * stored in localStorage, causing Zustand persist middleware to fail during
 * initialization, resulting in a blank white window.
 *
 * This MUST run before any store imports to prevent the race condition.
 */
// NOW import React and stores (after localStorage cleanup)
// NOTE: React 17+ doesn't require explicit React import for JSX
import { QueryClientProvider } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'
import ReactDOM from 'react-dom/client'

import App from './App'
import { RootErrorBoundary } from './components/ErrorBoundary'
import { queryClient } from './lib/queryClient'
import './index.css'

(function bootstrapLocalStorage() {
  console.log('[BOOTSTRAP] Checking localStorage for frozen objects...');

  const keysToCheck = ['recall-settings', 'vault_onboarding_complete', 'vault_tour_dismissed'];
  let clearedCount = 0;

  keysToCheck.forEach(key => {
    try {
      const item = localStorage.getItem(key);
      if (item) {
        const parsed = JSON.parse(item);

        // Check if the parsed object or any nested objects are frozen
        const isFrozen = Object.isFrozen(parsed) ||
                        (parsed.state && Object.isFrozen(parsed.state)) ||
                        (parsed.state?.settings && Object.isFrozen(parsed.state.settings));

        if (isFrozen) {
          console.warn(`[BOOTSTRAP] ❌ Detected frozen object in localStorage key: ${key}`);
          console.warn(`[BOOTSTRAP] 🧹 Clearing to prevent Zustand initialization failure`);
          localStorage.removeItem(key);
          clearedCount++;
        } else {
          console.log(`[BOOTSTRAP] ✅ Key "${key}" is clean (not frozen)`);
        }
      }
    } catch (e) {
      console.error(`[BOOTSTRAP] ⚠️  Error checking localStorage key "${key}":`, e);
      // On error, clear it to be safe
      console.warn(`[BOOTSTRAP] 🧹 Clearing "${key}" due to parse error`);
      localStorage.removeItem(key);
      clearedCount++;
    }
  });

  if (clearedCount > 0) {
    console.warn(`[BOOTSTRAP] 🔄 Cleared ${clearedCount} frozen/corrupted localStorage keys`);
    console.warn(`[BOOTSTRAP] App will initialize with default settings`);
  } else {
    console.log(`[BOOTSTRAP] ✅ All localStorage keys are healthy`);
  }
})();

// Debug logging
console.log('[MAIN] React entry point loading...');
console.log('[MAIN] Document ready state:', document.readyState);

const rootElement = document.getElementById('root');
console.log('[MAIN] Root element found:', !!rootElement);

if (!rootElement) {
  console.error('[MAIN] FATAL: Root element not found!');
  throw new Error('Root element not found. Ensure index.html contains <div id="root"></div>');
}

console.log('[MAIN] Creating React root...');
const root = ReactDOM.createRoot(rootElement);

console.log('[MAIN] Rendering React app...');
// TEMPORARILY DISABLED: Oracle diagnosis - StrictMode double-invokes effects, may trigger commands
root.render(
  // <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <RootErrorBoundary>
        <App />
      </RootErrorBoundary>
      <ReactQueryDevtools initialIsOpen={false} />
    </QueryClientProvider>
  // </React.StrictMode>,
)
console.log('[MAIN] React render call completed');
