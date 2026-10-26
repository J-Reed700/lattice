/**
 * Decides whether the first-run model bundle is offered.
 *
 * Two questions, two sources: "has the user already dismissed this?" comes
 * from the settings repository, "does this machine still need models?" from
 * the backend's own check. The status call only runs when the first answer is
 * no, so a returning user never pays for it.
 */

import { useEffect, useState } from 'react';

import { useQuery } from '@tanstack/react-query';

import { useApi } from '@/contexts/ApiContext';
import { useFirstRunDismissal } from '@/features/model/hooks/useFirstRunDismissal';
import { firstRunStatusQueryOptions } from '@/features/model/hooks/useFirstRunSetup';

import { ModelSetupModal } from './ModelSetupModal';

export function FirstRunGate() {
  const api = useApi();
  const { dismissed, isPending, dismiss } = useFirstRunDismissal();
  // `undefined` means the settings read hasn't landed (or failed). Staying
  // quiet is the right failure mode — the catalog is in Settings either way,
  // and so is a failed status check.
  const statusQuery = useQuery({ ...firstRunStatusQueryOptions(api), enabled: !isPending && dismissed === false });
  const statusError = statusQuery.error;
  useEffect(() => {
    if (statusError) console.error('Failed to check first-run status:', statusError.message);
  }, [statusError]);
  const [closed, setClosed] = useState(false);
  const open = !closed && statusQuery.data?.needs_setup === true;
  const setOpen = (next: boolean) => setClosed(!next);

  return (
    <ModelSetupModal
      open={open}
      onOpenChange={setOpen}
      onComplete={() => setClosed(true)}
      onDismiss={dismiss}
    />
  );
}
