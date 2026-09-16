import { useEffect, useRef } from 'react';

import { useBlocker } from 'react-router';

/** Keep the editor mounted until the repository acknowledges its draft. */
export function useJournalNavigationGuard(dirty: boolean, saveNow: () => Promise<boolean>) {
  const blocker = useBlocker(dirty);
  const saving = useRef(false);

  useEffect(() => {
    if (blocker.state !== 'blocked' || saving.current) return;
    saving.current = true;
    void saveNow().then(
      (saved) => { if (saved) blocker.proceed(); else blocker.reset(); },
      () => blocker.reset(),
    ).finally(() => { saving.current = false; });
  }, [blocker, saveNow]);
}
