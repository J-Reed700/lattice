import { OpenModelSettingsButton } from '@/components/OpenModelSettingsButton';
import { waitingSentence } from '@/lib/errorHandling';
import type { ErrorCode } from '@/types/api/errorCodes';

/** The work is waiting out a model outage and retrying by itself: the cause in a plain sentence from its code, the raw text under Details. */
export function WaitingNotice({ code, raw, retryInSeconds }: { code?: ErrorCode | null; raw?: string | null; retryInSeconds?: number }) {
  return <div role="alert" className="mt-4 flex flex-wrap items-start justify-between gap-3 rounded-lg border border-warning/40 bg-warning-muted p-3 text-sm text-text-primary">
    <div className="min-w-0">
      <p className="font-semibold">{waitingSentence(code)}</p>
      {retryInSeconds !== undefined && <p className="mt-1 text-xs text-text-secondary">Your work is kept. Retrying in {retryInSeconds}s.</p>}
      {raw && <details className="mt-2 text-xs">
        <summary className="cursor-pointer font-medium">Details</summary>
        <p className="mt-1 wrap-break-word">{raw}</p>
      </details>}
    </div>
    <OpenModelSettingsButton />
  </div>;
}
