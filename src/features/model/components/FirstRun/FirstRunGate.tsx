import { useEffect, useState } from 'react';

import { useQuery } from '@tanstack/react-query';

import { useFirstRunDismissal } from '@/features/model/hooks/useFirstRunDismissal';
import { firstRunStatusQueryOptions } from '@/features/model/hooks/useFirstRunSetup';

import { ModelSetupModal } from './ModelSetupModal';

export function FirstRunGate() {
  const { dismissed, isPending, dismiss } = useFirstRunDismissal();
  const statusQuery = useQuery({ ...firstRunStatusQueryOptions(), enabled: !isPending && dismissed === false });
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
