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
import { fetchFirstRunStatus } from '@/features/model/hooks/useFirstRunSetup';

import { ModelSetupModal } from './ModelSetupModal';

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
        const result = await fetchFirstRunStatus();
        if (!result.ok) {
          console.error('Failed to check first-run status:', result.error);
          return;
        }
        if (result.data.needs_setup) setOpen(true);
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
