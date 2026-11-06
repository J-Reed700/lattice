import type { ReactNode } from 'react';

import { Button } from '@/components/ui/button';

import type { Failure, Retry } from './drillState';

const RETRY_LABELS: Record<Retry, string> = {
  start: 'Try again',
  restore: 'Try again',
  submit: 'Try handing in again',
  regrade: 'Try grading again',
  follow: 'Check again',
};

/** What the backend said when a call failed, and the action that asks again. */
export function DrillFailure({ failure, onRetry, children }: { failure: Failure; onRetry: () => void; children?: ReactNode }) {
  return (
    <div role="alert" className="rounded-xl bg-danger-muted p-4 text-sm text-text-primary">
      <p>{failure.message}</p>
      <div className="mt-3 flex flex-wrap items-center gap-2">
        <Button type="button" variant="outline" onClick={onRetry}>{RETRY_LABELS[failure.retry]}</Button>
        {children}
      </div>
    </div>
  );
}
