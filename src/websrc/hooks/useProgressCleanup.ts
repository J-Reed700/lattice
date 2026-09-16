/**
 * Progress Cleanup Hook
 *
 * Provides automatic cleanup of stale progress store entries to prevent memory leaks.
 * Use this hook in your app's root component to enable periodic cleanup.
 *
 * @example
 * ```tsx
 * function App() {
 *   useProgressCleanup();
 *   return <YourApp />;
 * }
 * ```
 */

import { useEffect } from 'react';

import { useProgressStore } from '../stores/progressStore';

/**
 * Cleanup interval (5 minutes)
 */
const CLEANUP_INTERVAL_MS = 5 * 60 * 1000;

/**
 * Hook that sets up periodic cleanup of stale progress store entries.
 *
 * This hook:
 * - Runs cleanup every 5 minutes
 * - Removes stale throttle timers for non-existent operations
 * - Cleans up on component unmount
 *
 * @returns void
 */
export function useProgressCleanup(): void {
  const cleanupStaleEntries = useProgressStore((state) => state.cleanupStaleEntries);

  useEffect(() => {
    console.debug('[useProgressCleanup] Starting periodic cleanup interval');

    cleanupStaleEntries();

    const intervalId = setInterval(() => {
      console.debug('[useProgressCleanup] Running periodic cleanup');
      cleanupStaleEntries();
    }, CLEANUP_INTERVAL_MS);

    return () => {
      console.debug('[useProgressCleanup] Stopping periodic cleanup interval');
      clearInterval(intervalId);
    };
  }, [cleanupStaleEntries]);
}
