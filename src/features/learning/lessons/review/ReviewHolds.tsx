import type { ReactNode } from 'react';

import type { StopReasonDto } from '@/lib/bindings';

/** A stopped review's cause and fix, with the request's facts behind Details. */
export function StopReason({ reason }: { reason: StopReasonDto }) {
  const facts = reason.request;
  return (
    <div className="space-y-1 text-sm leading-6 text-text-secondary">
      <p><span className="font-semibold text-text-primary">Stopped: </span>{reason.message}</p>
      <p><span className="font-semibold text-text-primary">Fix: </span>{reason.fix}</p>
      {facts && <details className="text-xs">
        <summary className="cursor-pointer text-text-muted">Details</summary>
        <dl className="mt-1 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 tabular-nums">
          <dt>Step</dt><dd>{facts.step}</dd>
          <dt>Prompt</dt><dd>{facts.promptTokensExact ?? facts.promptTokensEstimated ?? '—'} tokens{facts.promptTokensExact == null ? ' (estimated)' : ''}</dd>
          <dt>Window</dt><dd>{facts.windowTokens} tokens · {facts.outputReserved} reserved for the answer</dd>
          <dt>Server</dt><dd>{facts.serverKind}{facts.serverMessageSanitized ? ` · ${facts.serverMessageSanitized}` : ''}</dd>
          <dt>Attempts</dt><dd>{facts.attempts}</dd>
        </dl>
      </details>}
    </div>
  );
}

/** The focus pane when no claim is in it: a heading, what it means, and what to do. */
export function FocusMessage({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section aria-label={title} className="space-y-3">
      <h2 className="font-serif text-xl text-text-primary">{title}</h2>
      <div className="space-y-2 text-sm leading-6 text-text-secondary">{children}</div>
    </section>
  );
}

/**
 * Held by the Verifier audit : its message and fix. How far it got is
 * the audit line in the screen's header; the Resume button is the screen's.
 */
export function AuditHold({ reason }: { reason: StopReasonDto | null }) {
  return (
    <FocusMessage title="Waiting for the independent check">
      {reason
        ? <><p><span className="font-semibold text-text-primary">It could not finish: </span>{reason.message}</p><p><span className="font-semibold text-text-primary">Fix: </span>{reason.fix}</p></>
        : <p>No claim needs you, but the independent check of this lesson has not finished, so the lesson is held.</p>}
      <p>Resume verification to run it again, or turn it off for this course on the Plan tab.</p>
    </FocusMessage>
  );
}
