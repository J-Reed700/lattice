import { useEffect, useRef, useState } from "react";

import { useBlocker } from "react-router";

import { flushPendingSaves } from "@/lib/pendingSaves";

/** Sidebar, history, and keyboard navigation must wait for acknowledged drafts. */
export function useStudioNavigationGuard(enabled: boolean) {
  const blocker = useBlocker(enabled);
  const saving = useRef(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (blocker.state !== "blocked" || saving.current) return;
    saving.current = true;
    setError(null);
    void flushPendingSaves().then((saved) => {
      if (saved) blocker.proceed();
      else {
        setError("Your work could not be saved. You are still in Studio; retry saving before leaving.");
        blocker.reset();
      }
    }).finally(() => { saving.current = false; });
  }, [blocker]);

  return { error };
}
