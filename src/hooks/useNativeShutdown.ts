import { useCallback, useEffect, useRef, useState } from 'react';

import { isTauri } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';

import { flushPendingSaves } from '@/lib/pendingSaves';

/** Install before making the editor available; native quit waits for this reply. */
export function useNativeShutdown() {
  const [ready, setReady] = useState(!isTauri());
  const [isQuitting, setIsQuitting] = useState(false);
  const [canCancelQuit, setCanCancelQuit] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeRequest = useRef<{ requestId: number } | null>(null);

  const cancelQuit = useCallback(async () => {
    const pending = activeRequest.current;
    if (pending === null) return;
    const { requestId } = pending;
    // Invalidate before IPC so a late save completion cannot approve this quit.
    activeRequest.current = null;
    setIsQuitting(false);
    setCanCancelQuit(false);
    try {
      await emit('lattice:shutdown-response', { requestId, saved: false });
    } catch {
      setError('Could not cancel the quit request. Your work is still open.');
    }
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<number>('lattice:shutdown-requested', async ({ payload: requestId }) => {
      if (disposed || activeRequest.current !== null) return;
      const attempt = { requestId };
      activeRequest.current = attempt;
      setIsQuitting(true);
      setCanCancelQuit(true);
      setError(null);
      const saved = await flushPendingSaves();
      if (disposed || activeRequest.current !== attempt) return;
      setCanCancelQuit(false);
      try {
        await emit('lattice:shutdown-response', { requestId, saved });
        if (!saved) {
          setError('Could not save your work. The app is still open. Try saving again before quitting.');
          setIsQuitting(false);
        }
      } catch {
        setError('Could not finish quitting. Your work is still open. Try again.');
        setIsQuitting(false);
      } finally {
        if (activeRequest.current === attempt) activeRequest.current = null;
      }
    }).then(async (stop) => {
      if (disposed) { stop(); return; }
      unlisten = stop;
      await emit('lattice:renderer-ready');
      if (!disposed) setReady(true);
    }).catch(() => {
      if (!disposed) setError('Could not connect safe shutdown. Restart Lattice before editing.');
    });
    return () => { disposed = true; unlisten?.(); };
  }, []);

  return { ready, isQuitting, error, cancelQuit, canCancelQuit };
}
