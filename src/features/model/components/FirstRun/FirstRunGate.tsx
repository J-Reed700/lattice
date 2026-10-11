/**
 * Decides whether the first-run model bundle is offered.
 *
 * Two questions, two sources: "has the user already dismissed this?" comes
 * from the settings repository, "does this machine still need models?" from
 * the backend's own check. The status call only runs when the first answer is
 * no, so a returning user never pays for it.
 */

import { useEffect, useRef, useState } from 'react';

import { useFirstRunDismissal } from '@/features/model/hooks/useFirstRunDismissal';
import { VaultAPI } from '@/lib/api';

import { ModelSetupModal } from './ModelSetupModal';

interface FirstRunStatusResponse {
  needs_setup: boolean;
}

export function FirstRunGate() {
  const { dismissed, isPending, dismiss } = useFirstRunDismissal();
  const [open, setOpen] = useState(false);
  const asked = useRef(false);

  useEffect(() => {
    // `undefined` means the settings read hasn't landed (or failed). Staying
    // quiet is the right failure mode — the catalog is in Settings either way.
    if (isPending || dismissed !== false || asked.current) return;
    asked.current = true;

    void (async () => {
      try {
        // Returns a JSON string to keep the Future small (stack overflow fix).
        const result = await VaultAPI.checkFirstRunStatus();
        if (!result.ok) {
          console.error('Failed to check first-run status:', result.error);
          return;
        }
        const status: FirstRunStatusResponse = JSON.parse(result.data);
        if (status.needs_setup) setOpen(true);
      } catch (error) {
        console.error('Failed to check first-run status:', error);
      }
    })();
  }, [dismissed, isPending]);

  return (
    <ModelSetupModal
      open={open}
      onOpenChange={setOpen}
      onComplete={() => setOpen(false)}
      onDismiss={dismiss}
    />
  );
}
