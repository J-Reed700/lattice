import { forwardRef, useId, useState, type ReactNode } from 'react';

import { ChevronLeft, ChevronRight } from 'lucide-react';

import { Button } from '@/components/ui/button';
import { ClaimActions, type ActionForm, type ClaimActionRunner } from '@/features/learning/lessons/review/ClaimActions';
import { ClaimEvidence } from '@/features/learning/lessons/review/ClaimEvidence';
import { resolvedChip, triedLine, verifiedLabel, whyItWaits } from '@/features/learning/lessons/review/claimReviewCopy';
import { isWaitingForYou } from '@/features/learning/lessons/review/claimReviewPresentation';
import type { ClaimReviewItemDto, ClaimSupportDto, SourceRefDto } from '@/lib/bindings';

const LEGEND: Array<[string, string]> = [['A', 'accept'], ['E', 'edit'], ['R', 'remove'], ['S', 'add a source'], ['J/K', 'next/previous'], ['U', 'undo'], ['?', 'all keys']];

const siteOf = (url: string | null) => { try { return url ? new URL(url).host : null; } catch { return null; } };

/** The checked statement, marked, inside its sentence and paragraph (§3.3 item 2). */
const StatementInContext = forwardRef<HTMLParagraphElement, { item: ClaimReviewItemDto }>(({ item }, ref) => {
  const { statement, sentence, contextBefore, contextAfter } = item.presentation;
  const at = sentence.indexOf(statement);
  const marked = at >= 0
    ? <>{sentence.slice(0, at)}<span className="rounded-sm bg-accent/15 px-0.5 text-text-primary">{statement}</span>{sentence.slice(at + statement.length)}</>
    : <span className="rounded-sm bg-accent/15 px-0.5 text-text-primary">{sentence || statement}</span>;
  return (
    <p ref={ref} tabIndex={-1} data-claim-statement="" className="font-serif text-lg leading-8 text-text-secondary focus-visible:outline-2 focus-visible:outline-accent">
      {contextBefore && <>{contextBefore} </>}
      <span className="text-text-primary">{marked}</span>
      {contextAfter && <> {contextAfter}</>}
    </p>
  );
});
StatementInContext.displayName = 'StatementInContext';

function Tried({ item, onOpen }: { item: ClaimReviewItemDto; onOpen: (source: SourceRefDto) => void }) {
  const line = triedLine(item.tried);
  const [open, setOpen] = useState(false);
  const id = useId();
  if (!line) return null;
  return (
    <div className="text-sm leading-6 text-text-secondary">
      <span>{line}</span>
      {item.tried.relevant.length > 0 && <>
        {' '}<button type="button" aria-expanded={open} aria-controls={id} onClick={() => setOpen((value) => !value)}
          className="inline-flex items-center gap-1 text-xs font-medium text-text-secondary hover:text-text-primary">
          Details <span aria-hidden="true" className={`transition-transform ${open ? 'rotate-90' : ''}`}>▸</span>
        </button>
        {open && <ul id={id} aria-label="Pages that mention it" className="mt-2 space-y-1 text-xs">
          {item.tried.relevant.map((source) => (
            <li key={source.sourceVersionId} className="flex flex-wrap items-center gap-2">
              <span className="min-w-0 text-text-primary">{[source.title, siteOf(source.url)].filter(Boolean).join(' · ')}</span>
              <button type="button" onClick={() => onOpen(source)} className="text-accent underline underline-offset-2">Open</button>
            </li>
          ))}
        </ul>}
      </>}
    </div>
  );
}

/**
 * One claim in focus (§3.3), top to bottom: where it is, the statement in
 * its sentence, why it waits, the best evidence, what was tried, the
 * actions and the keyboard legend. Nothing else.
 */
export const ClaimInFocus = forwardRef<HTMLParagraphElement, {
  programId: string;
  item: ClaimReviewItemDto;
  position: number;
  total: number;
  onPrevious: (() => void) | null;
  onNext: (() => void) | null;
  /** Waiting claims on the same source, this one included. */
  sharing: number;
  busy: boolean;
  error: string | null;
  form: ActionForm | null;
  onFormChange: (form: ActionForm | null) => void;
  onAct: ClaimActionRunner;
  onAcceptSource: () => void;
  onAddSource: () => void;
  /** The closest passage in what the user just added, when it does not say this. */
  closest: ClaimSupportDto | null;
  onOpenSource: (support: ClaimSupportDto) => void;
  onOpenRelevant: (source: SourceRefDto) => void;
  /** The Add a source sheet, open on this claim. */
  sheet?: ReactNode;
  /** A back link for the stacked narrow layout. */
  back?: ReactNode;
  /** The action row is a sticky bar at narrow widths. */
  stickyActions?: boolean;
}>(({ programId, item, position, total, onPrevious, onNext, sharing, busy, error, form, onFormChange, onAct, onAcceptSource, onAddSource, closest, onOpenSource, onOpenRelevant, sheet, back, stickyActions = false }, statementRef) => {
  const waiting = isWaitingForYou(item);
  const why = whyItWaits(item);
  const support = item.support;
  const label = waiting ? null : verifiedLabel(item.sufficiency);
  return (
    <article aria-label="Claim in focus" className="space-y-5">
      <header className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          {back}
          <p className="min-w-0 truncate text-xs font-medium uppercase tracking-[.08em] text-text-muted">{item.presentation.place}</p>
        </div>
        <div className="flex items-center gap-1 text-xs tabular-nums text-text-muted">
          <span aria-live="off">{position} of {total}</span>
          <Button type="button" size="icon" variant="ghost" aria-label="Previous claim" disabled={!onPrevious} onClick={() => onPrevious?.()}><ChevronLeft aria-hidden="true" /></Button>
          <Button type="button" size="icon" variant="ghost" aria-label="Next claim" disabled={!onNext} onClick={() => onNext?.()}><ChevronRight aria-hidden="true" /></Button>
        </div>
      </header>

      <StatementInContext ref={statementRef} item={item} />

      <div className="space-y-1">
        <p className="flex flex-wrap items-center gap-2 text-sm text-text-primary">
          <span className={`rounded-full px-2 py-0.5 text-xs font-semibold ${waiting ? 'bg-warning-muted text-warning-fg' : 'bg-success-muted text-success-fg'}`}>{waiting ? why.chip : resolvedChip(item)}</span>
          {waiting ? why.reason : label}
        </p>
        {waiting && why.detail && <p className="text-sm leading-6 text-text-secondary">{why.detail}</p>}
      </div>

      {closest
        ? <ClaimEvidence programId={programId} heading="CLOSEST PASSAGE IN WHAT YOU ADDED" support={closest} sufficiency={null} onOpenSource={onOpenSource} />
        : support
          ? <ClaimEvidence programId={programId} heading={support.role === 'contradicts' ? 'SAYS OTHERWISE' : 'BEST EVIDENCE'} support={support} also={item.alsoSupported} sufficiency={item.sufficiency} onOpenSource={onOpenSource} />
          : <p className="text-sm text-text-secondary">No saved source mentions this.</p>}

      <Tried item={item} onOpen={onOpenRelevant} />

      {sheet}

      <div className={stickyActions ? 'sticky bottom-0 -mx-4 border-t border-border bg-surface px-4 pb-3 pt-3' : undefined}>
        <ClaimActions item={item} sharing={sharing} suggested={why.fixes[0] ?? 'accept'} busy={busy} error={error} form={form}
          onFormChange={onFormChange} onAct={onAct} onAcceptSource={onAcceptSource} onAddSource={onAddSource} closest={Boolean(closest)} />
      </div>

      <p aria-hidden="true" className="flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-text-muted [@media(pointer:coarse)]:hidden">
        {LEGEND.map(([key, what]) => <span key={key}><kbd className="kbd">{key}</kbd> {what}</span>)}
      </p>
    </article>
  );
});
ClaimInFocus.displayName = 'ClaimInFocus';
