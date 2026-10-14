import { useCallback, useEffect, useState } from 'react';

export type ActionTone = 'info' | 'success' | 'error';

export interface Notice {
  tone: ActionTone;
  message: string;
  /** One verb, when the message names somewhere the user may want to go. */
  action?: { label: string; run: () => void };
}

export type JournalNotify = (tone: ActionTone, message: string, action?: Notice['action']) => void;

/** The journal workspace's one transient status line. */
export function useJournalNotice() {
  const [notice, setNotice] = useState<Notice | null>(null);

  const notify = useCallback(
    (tone: ActionTone, message: string, action?: Notice['action']) => {
      setNotice({ tone, message, action });
    },
    [],
  );

  // Auto-dismiss notice after 3.5s
  useEffect(() => {
    if (!notice) return;
    const timer = setTimeout(() => setNotice(null), 3500);
    return () => clearTimeout(timer);
  }, [notice]);

  return { notice, setNotice, notify };
}
