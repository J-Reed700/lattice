import { actorLabel, describeEvent } from '@/features/learning/lessons/review/claimReviewPresentation';
import type { ClaimEventDto } from '@/lib/bindings';

/** The claim's append-only audit trail , oldest first. */
export function ClaimHistory({ history }: { history: ClaimEventDto[] }) {
  if (history.length === 0) return <p className="text-xs text-text-muted">Nothing has been recorded for this claim yet.</p>;
  return (
    <ol aria-label="Claim history" className="space-y-2 text-xs leading-5">
      {history.map((entry) => (
        <li key={entry.ordinal} className="grid gap-x-3 sm:grid-cols-[7.5rem_1fr]">
          <span className="tabular-nums text-text-muted">
            <time dateTime={new Date(entry.at).toISOString()}>{new Date(entry.at).toLocaleString([], { day: 'numeric', month: 'short', hour: 'numeric', minute: '2-digit' })}</time>
            {' · '}{actorLabel(entry.actor)}
          </span>
          <span className="wrap-anywhere text-text-secondary">
            {describeEvent(entry.event)}
            {entry.checkerModel && entry.event.kind !== 'checked' && <span className="text-text-muted"> · checker {entry.checkerModel}</span>}
          </span>
        </li>
      ))}
    </ol>
  );
}
